//! Connection admission and per-room fan-out.
//!
//! Lock order is always hub -> slot. No `.await` while a lock is held.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::mpsc;

use crate::limits::{MAX_CONNS, MAX_CONNS_PER_IP, MAX_PER_ROOM, MAX_ROOMS, close, valid_room};
use crate::room::{ConnId, Reply, Room, RoomError};

/// What a connection's writer task sends to its socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    Data(Vec<u8>),
    Close(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    BadRoom,
    RoomFull,
    Busy,
}

impl Refusal {
    pub fn close_code(&self) -> u16 {
        match self {
            Refusal::BadRoom => close::BAD_ROOM,
            Refusal::RoomFull => close::ROOM_FULL,
            Refusal::Busy => close::BUSY,
        }
    }
}

struct SlotInner {
    room: Room,
    peers: HashMap<ConnId, mpsc::Sender<Out>>,
}

struct Slot {
    inner: Mutex<SlotInner>,
}

impl Slot {
    fn lock(&self) -> MutexGuard<'_, SlotInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl SlotInner {
    /// Non-blocking send; a connection whose outbox is full or closed loses
    /// its sender, so its writer sees the channel close.
    fn send(&mut self, to: ConnId, data: &[u8]) {
        let Some(tx) = self.peers.get(&to) else {
            return;
        };
        if tx.try_send(Out::Data(data.to_vec())).is_err() {
            self.peers.remove(&to);
        }
    }

    fn broadcast(&mut self, except: ConnId, data: &[u8]) {
        let ids: Vec<ConnId> = self
            .peers
            .keys()
            .copied()
            .filter(|&c| c != except)
            .collect();
        for id in ids {
            self.send(id, data);
        }
    }
}

struct RoomEntry {
    slot: Arc<Slot>,
    members: usize,
}

#[derive(Default)]
struct HubState {
    conns: usize,
    per_ip: HashMap<IpAddr, usize>,
    rooms: HashMap<String, RoomEntry>,
    next_id: ConnId,
}

pub struct Hub {
    state: Mutex<HubState>,
}

pub struct Peer {
    pub id: ConnId,
    pub room: String,
    pub ip: IpAddr,
    slot: Arc<Slot>,
}

impl Hub {
    pub fn new() -> Arc<Hub> {
        Arc::new(Hub {
            state: Mutex::new(HubState::default()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, HubState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn join(
        self: &Arc<Self>,
        room: &str,
        ip: IpAddr,
        outbox: mpsc::Sender<Out>,
    ) -> Result<Peer, Refusal> {
        if !valid_room(room) {
            return Err(Refusal::BadRoom);
        }
        let mut st = self.lock();
        if st.conns >= MAX_CONNS {
            return Err(Refusal::Busy);
        }
        if st.per_ip.get(&ip).copied().unwrap_or(0) >= MAX_CONNS_PER_IP {
            return Err(Refusal::Busy);
        }
        match st.rooms.get(room) {
            None if st.rooms.len() >= MAX_ROOMS => return Err(Refusal::Busy),
            Some(e) if e.members >= MAX_PER_ROOM => return Err(Refusal::RoomFull),
            _ => {}
        }

        let id = st.next_id;
        st.next_id += 1;
        st.conns += 1;
        *st.per_ip.entry(ip).or_insert(0) += 1;
        let entry = st
            .rooms
            .entry(room.to_string())
            .or_insert_with(|| RoomEntry {
                slot: Arc::new(Slot {
                    inner: Mutex::new(SlotInner {
                        room: Room::new(),
                        peers: HashMap::new(),
                    }),
                }),
                members: 0,
            });
        entry.members += 1;
        let slot = entry.slot.clone();

        let mut inner = slot.lock();
        for msg in inner.room.greeting() {
            // A fresh outbox can't be full; if the writer is already gone
            // the connection is on its way out anyway.
            let _ = outbox.try_send(Out::Data(msg));
        }
        inner.peers.insert(id, outbox);
        drop(inner);

        Ok(Peer {
            id,
            room: room.to_string(),
            ip,
            slot,
        })
    }

    /// Must be called exactly once per successful `join`.
    pub fn leave(&self, peer: &Peer) {
        let mut st = self.lock();
        st.conns = st.conns.saturating_sub(1);
        if let Some(n) = st.per_ip.get_mut(&peer.ip) {
            *n -= 1;
            if *n == 0 {
                st.per_ip.remove(&peer.ip);
            }
        }
        let Some(entry) = st.rooms.get_mut(&peer.room) else {
            return;
        };
        entry.members -= 1;
        let empty = entry.members == 0;
        {
            let mut inner = entry.slot.lock();
            inner.peers.remove(&peer.id);
            if let Some(msg) = inner.room.remove_conn(peer.id) {
                inner.broadcast(peer.id, &msg);
            }
        }
        if empty {
            st.rooms.remove(&peer.room);
        }
    }

    pub fn room_count(&self) -> usize {
        self.lock().rooms.len()
    }

    pub fn conn_count(&self) -> usize {
        self.lock().conns
    }
}

impl Peer {
    pub fn handle(&self, data: &[u8]) -> Result<(), RoomError> {
        let mut inner = self.slot.lock();
        let replies = inner.room.handle(self.id, data)?;
        for reply in replies {
            match reply {
                Reply::ToSender(b) => inner.send(self.id, &b),
                Reply::ToOthers(b) => inner.broadcast(self.id, &b),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::OUTBOX;
    use yrs::sync::{Message, SyncMessage};
    use yrs::updates::encoder::Encode;
    use yrs::{Doc, ReadTxn, Text, Transact};

    type Rx = mpsc::Receiver<Out>;

    fn ip(n: u32) -> IpAddr {
        IpAddr::from(std::net::Ipv4Addr::from(n))
    }

    fn join(hub: &Arc<Hub>, room: &str, addr: IpAddr) -> Result<(Peer, Rx), Refusal> {
        let (tx, rx) = mpsc::channel(OUTBOX);
        hub.join(room, addr, tx).map(|p| (p, rx))
    }

    fn update_msg(s: &str) -> Vec<u8> {
        let doc = Doc::new();
        let text = doc.get_or_insert_text("code");
        let sv = doc.transact().state_vector();
        text.insert(&mut doc.transact_mut(), 0, s);
        let u = doc.transact().encode_diff_v1(&sv);
        Message::Sync(SyncMessage::Update(u)).encode_v1()
    }

    #[tokio::test]
    async fn join_sends_greeting() {
        let hub = Hub::new();
        let (_p, mut rx) = join(&hub, "r", ip(1)).unwrap();
        let Out::Data(b) = rx.try_recv().unwrap() else {
            panic!("expected data")
        };
        assert!(matches!(
            yrs::updates::decoder::Decode::decode_v1(&b),
            Ok(Message::Sync(SyncMessage::SyncStep1(_)))
        ));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn bad_room_refused() {
        let hub = Hub::new();
        let r = join(&hub, "A!", ip(1));
        assert_eq!(r.err(), Some(Refusal::BadRoom));
        assert_eq!(Refusal::BadRoom.close_code(), close::BAD_ROOM);
        assert_eq!(Refusal::RoomFull.close_code(), close::ROOM_FULL);
        assert_eq!(Refusal::Busy.close_code(), close::BUSY);
    }

    #[tokio::test]
    async fn room_full_at_16() {
        let hub = Hub::new();
        let mut keep = Vec::new();
        for i in 0..16 {
            keep.push(join(&hub, "r", ip(i + 1)).unwrap());
        }
        assert_eq!(join(&hub, "r", ip(99)).err(), Some(Refusal::RoomFull));
    }

    #[tokio::test]
    async fn per_ip_cap_8() {
        let hub = Hub::new();
        let mut keep = Vec::new();
        for i in 0..8 {
            keep.push(join(&hub, &format!("r{i}"), ip(1)).unwrap());
        }
        assert_eq!(join(&hub, "r9", ip(1)).err(), Some(Refusal::Busy));
    }

    #[tokio::test]
    async fn room_cap_200() {
        let hub = Hub::new();
        let mut keep = Vec::new();
        for i in 0..200 {
            keep.push(join(&hub, &format!("r{i}"), ip(i + 1)).unwrap());
        }
        assert_eq!(join(&hub, "new", ip(500)).err(), Some(Refusal::Busy));
        assert!(join(&hub, "r0", ip(500)).is_ok());
    }

    #[tokio::test]
    async fn leave_drops_empty_room() {
        let hub = Hub::new();
        let (p, _rx) = join(&hub, "r", ip(1)).unwrap();
        assert_eq!((hub.room_count(), hub.conn_count()), (1, 1));
        hub.leave(&p);
        assert_eq!((hub.room_count(), hub.conn_count()), (0, 0));
        // per-IP slot is released too
        for i in 0..8 {
            let (p, _rx) = join(&hub, &format!("r{i}"), ip(1)).unwrap();
            hub.leave(&p);
        }
    }

    #[tokio::test]
    async fn update_reaches_others_not_sender() {
        let hub = Hub::new();
        let (a, mut rx_a) = join(&hub, "r", ip(1)).unwrap();
        let (_b, mut rx_b) = join(&hub, "r", ip(2)).unwrap();
        rx_a.try_recv().unwrap();
        rx_b.try_recv().unwrap();
        let msg = update_msg("hi");
        a.handle(&msg).unwrap();
        let Out::Data(got) = rx_b.try_recv().unwrap() else {
            panic!("expected data")
        };
        assert_eq!(got, msg);
        assert!(rx_a.try_recv().is_err());
    }

    #[tokio::test]
    async fn slow_reader_is_dropped() {
        let hub = Hub::new();
        let (a, _rx_a) = join(&hub, "r", ip(1)).unwrap();
        let (_b, mut rx_b) = join(&hub, "r", ip(2)).unwrap();
        let msg = update_msg("hi");
        for _ in 0..=OUTBOX {
            a.handle(&msg).unwrap();
        }
        let mut n = 0;
        while rx_b.recv().await.is_some() {
            n += 1;
        }
        assert_eq!(n, OUTBOX);
    }
}
