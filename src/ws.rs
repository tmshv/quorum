//! The `/ws/{room}` endpoint: one connection = one reader loop, one writer
//! loop, and exactly one `Hub::leave` after both are done.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};

use crate::hub::{Hub, Out, Peer};
use crate::limits::{
    IDLE_TIMEOUT, MAX_MESSAGE, MAX_STRIKES, OUTBOX, PING_INTERVAL, RATE_BURST, RATE_PER_SEC,
    SEND_TIMEOUT, close,
};
use crate::net::{client_ip, same_origin};
use crate::rate::{Limiter, Verdict};
use crate::room::RoomError;

/// How long the writer gets to flush a close frame once the reader is done.
const FLUSH_GRACE: Duration = Duration::from_secs(2);

/// Liveness timings; injectable so tests need not wait for real minutes.
#[derive(Clone, Copy)]
pub struct Timing {
    pub send_timeout: Duration,
    pub ping_interval: Duration,
    pub idle_timeout: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            send_timeout: SEND_TIMEOUT,
            ping_interval: PING_INTERVAL,
            idle_timeout: IDLE_TIMEOUT,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<Hub>,
    pub timing: Timing,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(room): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let ip = client_ip(&headers, addr);
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| connection(state, room, ip, socket))
}

fn close_message(code: u16) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: "".into(),
    }))
}

async fn connection(state: AppState, room: String, ip: std::net::IpAddr, mut socket: WebSocket) {
    let AppState { hub, timing } = state;
    let (tx, rx) = mpsc::channel(OUTBOX);
    // The hub holds the only Sender: when it drops a slow peer the writer's
    // `recv()` returns `None`.
    let peer = match hub.join(&room, ip, tx) {
        Ok(peer) => peer,
        Err(refusal) => {
            let _ = socket.send(close_message(refusal.close_code())).await;
            return;
        }
    };

    let (sink, stream) = socket.split();
    let (stop_tx, stop_rx) = oneshot::channel();
    let reader = read_loop(stream, &peer, stop_tx, timing.idle_timeout);
    let writer = write_loop(sink, rx, stop_rx, timing);
    tokio::pin!(reader, writer);

    tokio::select! {
        () = &mut reader => {
            // Let the writer flush a requested close frame, but not forever.
            let _ = tokio::time::timeout(FLUSH_GRACE, &mut writer).await;
        }
        () = &mut writer => {}
    }
    hub.leave(&peer);
}

async fn write_loop(
    mut sink: SplitSink<WebSocket, Message>,
    mut outbox: mpsc::Receiver<Out>,
    mut stop: oneshot::Receiver<u16>,
    timing: Timing,
) {
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + timing.ping_interval,
        timing.ping_interval,
    );
    loop {
        let msg = tokio::select! {
            out = outbox.recv() => match out {
                Some(Out::Data(data)) => Message::Binary(data.into()),
                Some(Out::Close(code)) => close_message(code),
                None => close_message(close::TOO_SLOW),
            },
            code = &mut stop => match code {
                Ok(code) => close_message(code),
                // Reader ended without asking for a close frame.
                Err(_) => return,
            },
            _ = ping.tick() => Message::Ping(Vec::new().into()),
        };
        let closing = matches!(msg, Message::Close(_));
        // A client that stops reading must not hold the connection forever.
        match tokio::time::timeout(timing.send_timeout, sink.send(msg)).await {
            Ok(Ok(())) if !closing => {}
            _ => return,
        }
    }
}

async fn read_loop(
    mut stream: SplitStream<WebSocket>,
    peer: &Peer,
    stop: oneshot::Sender<u16>,
    idle_timeout: Duration,
) {
    let mut limiter = Limiter::new(RATE_PER_SEC, RATE_BURST, MAX_STRIKES);
    let code = loop {
        // Any frame, pongs included, counts as activity.
        let Ok(Some(Ok(msg))) = tokio::time::timeout(idle_timeout, stream.next()).await else {
            return;
        };
        match msg {
            Message::Binary(data) => match limiter.check() {
                Verdict::Allow => match peer.handle(&data) {
                    Ok(()) => {}
                    Err(RoomError::DocTooLarge) => break close::DOC_TOO_LARGE,
                    Err(RoomError::Protocol) => break close::PROTOCOL,
                },
                Verdict::Refuse => {}
                Verdict::Disconnect => break close::RATE_LIMITED,
            },
            Message::Text(_) => break close::PROTOCOL,
            Message::Close(_) => return,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    };
    let _ = stop.send(code);
}
