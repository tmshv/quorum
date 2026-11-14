use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message as Frame};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, GetString, ReadTxn, Text, Transact, Update};

use quorum::hub::Hub;
use quorum::rate::Budget;
use quorum::web::MapAssets;
use quorum::ws::Timing;

const WAIT: Duration = Duration::from_secs(5);

struct Server {
    addr: SocketAddr,
    hub: Arc<Hub>,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl Server {
    async fn start() -> Server {
        Server::start_with(Timing::default()).await
    }

    async fn start_with(timing: Timing) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hub = Hub::new();
        let assets = Arc::new(MapAssets(HashMap::from([(
            "index.html",
            b"<!doctype html>".as_slice(),
        )])));
        let (stop, rx) = oneshot::channel::<()>();
        let task = tokio::spawn({
            let hub = hub.clone();
            async move {
                quorum::serve_with(listener, hub, assets, timing, async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
            }
        });
        Server {
            addr,
            hub,
            stop: Some(stop),
            task,
        }
    }

    async fn shutdown(mut self) {
        let _ = self.stop.take().unwrap().send(());
        timeout(WAIT, self.task).await.unwrap().unwrap();
    }
}

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Client {
    ws: Ws,
    doc: Doc,
}

fn text_of(doc: &Doc) -> String {
    doc.get_or_insert_text("code").get_string(&doc.transact())
}

impl Client {
    /// Connects and consumes the server's greeting (its SyncStep1), so the
    /// server has certainly registered us once this returns.
    async fn connect(addr: SocketAddr, room: &str) -> Client {
        Client::connect_with_doc(addr, room, Doc::new()).await
    }

    async fn connect_with_doc(addr: SocketAddr, room: &str, doc: Doc) -> Client {
        let (ws, _) = timeout(WAIT, connect_async(format!("ws://{addr}/ws/{room}")))
            .await
            .unwrap()
            .unwrap();
        let mut c = Client { ws, doc };
        assert!(matches!(
            c.recv().await,
            Message::Sync(SyncMessage::SyncStep1(_))
        ));
        c
    }

    async fn send(&mut self, msg: Message) {
        self.ws.send(Frame::binary(msg.encode_v1())).await.unwrap();
    }

    async fn recv_frame(&mut self) -> Frame {
        loop {
            match timeout(WAIT, self.ws.next()).await.expect("recv timed out") {
                Some(Ok(Frame::Ping(_) | Frame::Pong(_))) => continue,
                Some(Ok(f)) => return f,
                Some(Err(e)) => panic!("socket error: {e}"),
                None => panic!("socket closed without a close frame"),
            }
        }
    }

    async fn recv(&mut self) -> Message {
        match self.recv_frame().await {
            Frame::Binary(b) => Message::decode_v1(&b).unwrap(),
            f => panic!("expected binary frame, got {f:?}"),
        }
    }

    /// Reads until a close frame; returns its code.
    async fn recv_close(&mut self) -> u16 {
        loop {
            if let Frame::Close(Some(cf)) = self.recv_frame().await {
                return cf.code.into();
            }
        }
    }

    /// Sends our state vector and applies the server's SyncStep2.
    async fn sync(&mut self) {
        let sv = self.doc.transact().state_vector();
        self.send(Message::Sync(SyncMessage::SyncStep1(sv))).await;
        loop {
            if let Message::Sync(SyncMessage::SyncStep2(u)) = self.recv().await {
                self.apply(&u);
                return;
            }
        }
    }

    fn apply(&self, bytes: &[u8]) {
        let u = Update::decode_v1(bytes).unwrap();
        self.doc.transact_mut().apply_update(u).unwrap();
    }

    /// Inserts text at the end and returns the update bytes for it.
    fn append(&self, s: &str) -> Vec<u8> {
        let before = self.doc.transact().state_vector();
        let text = self.doc.get_or_insert_text("code");
        let len = text.get_string(&self.doc.transact()).chars().count() as u32;
        text.insert(&mut self.doc.transact_mut(), len, s);
        self.doc.transact().encode_diff_v1(&before)
    }

    async fn recv_update(&mut self) {
        loop {
            if let Message::Sync(SyncMessage::Update(u)) = self.recv().await {
                self.apply(&u);
                return;
            }
        }
    }
}

fn awareness(client: ClientID, clock: u32, json: &str) -> Message {
    Message::Awareness(AwarenessUpdate {
        clients: HashMap::from([(
            client,
            AwarenessUpdateEntry {
                clock,
                json: Arc::from(json),
            },
        )]),
    })
}

#[tokio::test]
async fn two_clients_converge() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    let u = a.append("print(42)");
    a.send(Message::Sync(SyncMessage::Update(u))).await;
    b.recv_update().await;
    assert_eq!(text_of(&b.doc), "print(42)");
    assert_eq!(text_of(&b.doc), text_of(&a.doc));
}

#[tokio::test]
async fn late_joiner_gets_text() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let u = a.append("print(42)");
    a.send(Message::Sync(SyncMessage::Update(u))).await;
    // A round trip on A's own connection proves the update was processed.
    a.sync().await;

    let mut c = Client::connect(s.addr, "room-1").await;
    c.sync().await;
    assert_eq!(text_of(&c.doc), "print(42)");
}

#[tokio::test]
async fn awareness_removed_on_disconnect() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    let id = a.doc.client_id();
    a.send(awareness(id, 1, r#"{"user":"a"}"#)).await;
    match b.recv().await {
        Message::Awareness(u) => assert_eq!(&*u.clients[&id].json, r#"{"user":"a"}"#),
        m => panic!("expected awareness, got {m:?}"),
    }

    drop(a);
    match timeout(Duration::from_secs(1), b.recv()).await.unwrap() {
        Message::Awareness(u) => assert_eq!(&*u.clients[&id].json, "null"),
        m => panic!("expected awareness removal, got {m:?}"),
    }
}

#[tokio::test]
async fn room_dropped_when_empty() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let b = Client::connect(s.addr, "room-1").await;
    assert_eq!(s.hub.room_count(), 1);
    a.ws.close(None).await.unwrap();
    drop(a);
    drop(b);
    timeout(Duration::from_secs(1), async {
        while s.hub.room_count() != 0 || s.hub.conn_count() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("room not dropped");
}

#[tokio::test]
async fn bad_room_closes_4000() {
    let s = Server::start().await;
    let (ws, _) = connect_async(format!("ws://{}/ws/BAD_ROOM", s.addr))
        .await
        .unwrap();
    let mut c = Client {
        ws,
        doc: Doc::new(),
    };
    assert_eq!(c.recv_close().await, 4000);
    assert_eq!(s.hub.conn_count(), 0);
}

#[tokio::test]
async fn cross_origin_is_403() {
    let s = Server::start().await;
    let mut req = format!("ws://{}/ws/room-1", s.addr)
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("origin", "http://evil.example".parse().unwrap());
    match connect_async(req).await {
        Err(tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), 403),
        other => panic!("expected HTTP 403, got {:?}", other.map(|_| ())),
    }
}

#[tokio::test]
async fn doc_cap_closes_4003() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    // B keeps the room alive so its doc survives A being closed.
    let b = Client::connect(s.addr, "room-1").await;
    let chunk = "x".repeat(200 * 1024);
    let mut sent = 0;
    for _ in 0..6 {
        let u = a.append(&chunk);
        if a.ws
            .send(Frame::binary(
                Message::Sync(SyncMessage::Update(u)).encode_v1(),
            ))
            .await
            .is_err()
        {
            break;
        }
        sent += 1;
    }
    assert!(sent >= 5);
    assert_eq!(a.recv_close().await, 4003);

    let mut c = Client::connect(s.addr, "room-1").await;
    c.sync().await;
    let got = text_of(&c.doc);
    drop(b);
    assert!(
        got.len() >= 4 * chunk.len(),
        "earlier text lost: {}",
        got.len()
    );
    assert!(got.len() < 6 * chunk.len());
}

#[tokio::test]
async fn reconnect_after_restart_restores_text() {
    let s1 = Server::start().await;
    let mut a = Client::connect(s1.addr, "room-1").await;
    let u = a.append("hello");
    a.send(Message::Sync(SyncMessage::Update(u))).await;
    a.sync().await;
    let doc = a.doc.clone();
    a.ws.close(None).await.unwrap();
    drop(a);
    s1.shutdown().await;

    let s2 = Server::start().await;
    let (ws, _) = connect_async(format!("ws://{}/ws/room-1", s2.addr))
        .await
        .unwrap();
    let mut a = Client { ws, doc };
    // Like y-websocket: answer the server's SyncStep1 with our diff.
    let Message::Sync(SyncMessage::SyncStep1(sv)) = a.recv().await else {
        panic!("expected SyncStep1")
    };
    let diff = a.doc.transact().encode_diff_v1(&sv);
    a.send(Message::Sync(SyncMessage::SyncStep2(diff))).await;
    a.sync().await;

    let mut b = Client::connect(s2.addr, "room-1").await;
    b.sync().await;
    assert_eq!(text_of(&b.doc), "hello");
}

#[tokio::test]
async fn lone_client_gets_own_awareness_back() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let id = a.doc.client_id();
    a.send(awareness(id, 1, r#"{"user":"a"}"#)).await;
    match a.recv().await {
        Message::Awareness(u) => assert_eq!(&*u.clients[&id].json, r#"{"user":"a"}"#),
        m => panic!("expected awareness, got {m:?}"),
    }
}

fn fast_timing() -> Timing {
    Timing {
        send_timeout: Duration::from_millis(500),
        ping_interval: Duration::from_millis(100),
        idle_timeout: Duration::from_millis(600),
        ..Timing::default()
    }
}

async fn wait_conns(hub: &Hub, n: usize, within: Duration) {
    timeout(within, async {
        while hub.conn_count() != n {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("conn_count never reached {n} (now {})", hub.conn_count()));
}

#[tokio::test]
async fn stalled_reader_is_disconnected() {
    // Idle timeout is off so only the send timeout can end this connection.
    let s = Server::start_with(Timing {
        idle_timeout: Duration::from_secs(1000),
        ..fast_timing()
    })
    .await;
    // The stalled client announces itself, then never reads again.
    let mut stalled = Client::connect(s.addr, "room-1").await;
    let stalled_id = stalled.doc.client_id();
    stalled
        .send(awareness(stalled_id, 1, r#"{"user":"s"}"#))
        .await;
    let mut flooder = Client::connect(s.addr, "room-1").await;
    wait_conns(&s.hub, 2, WAIT).await;

    // Re-sending one 500 KiB update keeps the doc small (duplicates add no
    // state) while every copy is relayed to the stalled client.
    let big = flooder.append(&"x".repeat(500 * 1024));
    let frame = Message::Sync(SyncMessage::Update(big)).encode_v1();
    let flood = async {
        // A burst, then well under the rate limit; the loop ends once the
        // server has dropped the stalled connection.
        for i in 0u32.. {
            if s.hub.conn_count() == 1 {
                return;
            }
            flooder.ws.send(Frame::binary(frame.clone())).await.unwrap();
            if i >= 60 {
                tokio::time::sleep(Duration::from_millis(35)).await;
            }
        }
    };
    timeout(Duration::from_secs(20), flood)
        .await
        .expect("stalled client was never disconnected");
    wait_conns(&s.hub, 1, WAIT).await;

    // The flooder is told the stalled client is gone.
    loop {
        if let Message::Awareness(u) = flooder.recv().await
            && let Some(e) = u.clients.get(&stalled_id)
            && &*e.json == "null"
        {
            break;
        }
    }
    drop(stalled);
}

#[tokio::test]
async fn silent_peer_is_dropped_but_responsive_peer_stays() {
    let s = Server::start_with(fast_timing()).await;
    // Never polled, so it never answers pings.
    let silent = Client::connect(s.addr, "room-1").await;
    let mut live = Client::connect(s.addr, "room-1").await;
    wait_conns(&s.hub, 2, WAIT).await;

    // Reading answers pings automatically; stay connected well past the
    // idle timeout while the silent one is dropped.
    let stay = async {
        loop {
            match live.ws.next().await {
                Some(Ok(Frame::Close(_))) | None => panic!("live peer was closed"),
                Some(Err(e)) => panic!("live peer errored: {e}"),
                _ => {}
            }
        }
    };
    let _ = timeout(Duration::from_millis(2500), stay).await;
    assert_eq!(s.hub.conn_count(), 1);
    drop(silent);
}

/// A server whose connections get `burst` messages and (almost) no refill.
async fn start_with_budget(burst: f64, per_sec: f64) -> Server {
    Server::start_with(Timing {
        budget: Budget {
            per_sec,
            burst,
            ..Budget::default()
        },
        ..Timing::default()
    })
    .await
}

/// Applies every update that arrives until `quiet` passes without a frame;
/// returns how many updates were applied.
async fn drain_updates(c: &mut Client, quiet: Duration) -> usize {
    let mut n = 0;
    while let Ok(Some(Ok(f))) = timeout(quiet, c.ws.next()).await {
        if let Frame::Binary(b) = f
            && let Ok(Message::Sync(SyncMessage::Update(u))) = Message::decode_v1(&b)
        {
            c.apply(&u);
            n += 1;
        }
    }
    n
}

#[tokio::test]
async fn flood_of_updates_converges_exactly() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    for i in 0..300 {
        let ch = char::from(b'a' + (i % 26) as u8).to_string();
        let u = a.append(&ch);
        if a.ws
            .send(Frame::binary(
                Message::Sync(SyncMessage::Update(u)).encode_v1(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
    let sv = a.doc.transact().state_vector();
    let _ =
        a.ws.send(Frame::binary(
            Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1(),
        ))
        .await;
    // Either A is still connected (its SyncStep2 arrives) or it was told it
    // was rate limited; never connected with missing data.
    let connected = loop {
        match a.recv_frame().await {
            Frame::Binary(bytes) => {
                if let Ok(Message::Sync(SyncMessage::SyncStep2(_))) = Message::decode_v1(&bytes) {
                    break true;
                }
            }
            Frame::Close(Some(cf)) => {
                assert_eq!(u16::from(cf.code), 4004);
                break false;
            }
            f => panic!("unexpected frame {f:?}"),
        }
    };
    drain_updates(&mut b, Duration::from_millis(500)).await;
    let (got, sent) = (text_of(&b.doc), text_of(&a.doc));
    if connected {
        assert_eq!(got, sent);
    } else {
        assert!(sent.starts_with(&got), "B diverged: {got:?}");
    }
    // Nothing is waiting on a missing update.
    assert!(b.doc.transact().store().pending_update().is_none());
}

#[tokio::test]
async fn rate_limited_sync_closes_not_drops() {
    let s = start_with_budget(5.0, 0.0).await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    for i in 0..10u8 {
        let u = a.append(&char::from(b'a' + i).to_string());
        let frame = Frame::binary(Message::Sync(SyncMessage::Update(u)).encode_v1());
        if a.ws.send(frame).await.is_err() {
            break;
        }
    }
    assert_eq!(a.recv_close().await, 4004);
    let n = drain_updates(&mut b, Duration::from_millis(300)).await;
    assert_eq!(n, 5);
    assert_eq!(text_of(&b.doc), "abcde");
}

#[tokio::test]
async fn over_budget_awareness_is_dropped_not_closed() {
    let s = start_with_budget(2.0, 20.0).await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let id = a.doc.client_id();
    for clock in 1..=6 {
        a.send(awareness(id, clock, r#"{"user":"a"}"#)).await;
    }
    // The bucket refills, and A is still connected.
    tokio::time::sleep(Duration::from_millis(300)).await;
    a.sync().await;
}

#[tokio::test]
async fn awareness_echoes_are_not_charged() {
    let s = start_with_budget(2.0, 0.0).await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    let id = a.doc.client_id();
    a.send(awareness(id, 1, r#"{"user":"a"}"#)).await;
    assert!(matches!(b.recv().await, Message::Awareness(_)));
    // B echoes A's state back, like a stock y-websocket client does.
    for _ in 0..10 {
        b.send(awareness(id, 1, r#"{"user":"a"}"#)).await;
    }
    // B's budget is untouched: two syncs still fit.
    b.sync().await;
    b.sync().await;
}

#[tokio::test]
async fn oversize_update_closes_4003_and_keeps_text() {
    let s = Server::start().await;
    let mut a = Client::connect(s.addr, "room-1").await;
    let mut b = Client::connect(s.addr, "room-1").await;
    let u = a.append("hello");
    a.send(Message::Sync(SyncMessage::Update(u))).await;
    b.recv_update().await;

    let big = a.append(&"x".repeat(1536 * 1024));
    a.send(Message::Sync(SyncMessage::Update(big))).await;
    assert_eq!(a.recv_close().await, 4003);

    assert_eq!(drain_updates(&mut b, Duration::from_millis(300)).await, 0);
    assert_eq!(text_of(&b.doc), "hello");
    let mut c = Client::connect(s.addr, "room-1").await;
    c.sync().await;
    assert_eq!(text_of(&c.doc), "hello");
}

#[tokio::test]
async fn frame_over_message_cap_closes_4003() {
    let s = Server::start().await;
    let a = Client::connect(s.addr, "room-1").await;
    let mut frame = vec![0u8; (2 << 20) + 1024];
    frame[1] = 2; // sync update; the payload is never read
    // Like a browser: the send runs in the background while we read.
    let (mut tx, mut rx) = a.ws.split();
    let send = tokio::spawn(async move {
        let _ = tx.send(Frame::binary(frame)).await;
        tx
    });
    let code = loop {
        match timeout(WAIT, rx.next()).await.expect("recv timed out") {
            Some(Ok(Frame::Close(Some(cf)))) => break u16::from(cf.code),
            Some(Ok(_)) => continue,
            other => panic!("no close frame: {other:?}"),
        }
    };
    assert_eq!(code, 4003);
    drop(send);
}
