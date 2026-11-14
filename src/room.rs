//! Per-room Yjs protocol state: one shared doc plus relayed awareness.
//! Pure and IO-free; the hub owns locking and fan-out.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, ReadTxn, StateVector, Transact, Update};

use crate::limits::{MAX_AWARENESS_STATE, MAX_DOC};

pub type ConnId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    ToSender(Vec<u8>),
    ToOthers(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomError {
    DocTooLarge,
    Protocol,
}

pub struct Room {
    doc: Doc,
    approx_size: usize,
    awareness: HashMap<ClientID, (u32, Arc<str>)>,
    owners: HashMap<ConnId, HashSet<ClientID>>,
}

impl Default for Room {
    fn default() -> Self {
        Self::new()
    }
}

impl Room {
    pub fn new() -> Room {
        Room {
            doc: Doc::new(),
            approx_size: 0,
            awareness: HashMap::new(),
            owners: HashMap::new(),
        }
    }

    /// Messages to send a newly connected client: our state vector, then
    /// all known awareness states (if any).
    pub fn greeting(&self) -> Vec<Vec<u8>> {
        let sv = self.doc.transact().state_vector();
        let mut out = vec![Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1()];
        if let Some(msg) = self.awareness_message() {
            out.push(msg);
        }
        out
    }

    pub fn handle(&mut self, from: ConnId, data: &[u8]) -> Result<Vec<Reply>, RoomError> {
        let msg = Message::decode_v1(data).map_err(|_| RoomError::Protocol)?;
        match msg {
            Message::Sync(SyncMessage::SyncStep1(sv)) => {
                let diff = self.doc.transact().encode_diff_v1(&sv);
                let reply = Message::Sync(SyncMessage::SyncStep2(diff)).encode_v1();
                Ok(vec![Reply::ToSender(reply)])
            }
            Message::Sync(SyncMessage::SyncStep2(u)) | Message::Sync(SyncMessage::Update(u)) => {
                self.apply_update(&u)?;
                let relay = Message::Sync(SyncMessage::Update(u)).encode_v1();
                Ok(vec![Reply::ToOthers(relay)])
            }
            Message::Awareness(update) => {
                if update
                    .clients
                    .values()
                    .any(|e| e.json.len() > MAX_AWARENESS_STATE)
                {
                    return Err(RoomError::Protocol);
                }
                self.apply_awareness(from, update);
                Ok(vec![Reply::ToOthers(data.to_vec())])
            }
            Message::AwarenessQuery => Ok(self
                .awareness_message()
                .map(Reply::ToSender)
                .into_iter()
                .collect()),
            Message::Auth(_) | Message::Custom(_, _) => Ok(Vec::new()),
        }
    }

    /// Awareness message marking every state owned by `conn` as removed,
    /// or `None` if there is nothing to remove.
    pub fn remove_conn(&mut self, conn: ConnId) -> Option<Vec<u8>> {
        let ids = self.owners.remove(&conn)?;
        let mut clients = HashMap::new();
        for id in ids {
            if let Some((clock, _)) = self.awareness.remove(&id) {
                clients.insert(
                    id,
                    AwarenessUpdateEntry {
                        clock: clock.wrapping_add(1),
                        json: Arc::from("null"),
                    },
                );
            }
        }
        if clients.is_empty() {
            return None;
        }
        Some(Message::Awareness(AwarenessUpdate { clients }).encode_v1())
    }

    fn apply_update(&mut self, bytes: &[u8]) -> Result<(), RoomError> {
        if self.approx_size + bytes.len() > MAX_DOC {
            self.approx_size = self
                .doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default())
                .len();
            if self.approx_size + bytes.len() > MAX_DOC {
                return Err(RoomError::DocTooLarge);
            }
        }
        let update = Update::decode_v1(bytes).map_err(|_| RoomError::Protocol)?;
        self.doc
            .transact_mut()
            .apply_update(update)
            .map_err(|_| RoomError::Protocol)?;
        self.approx_size += bytes.len();
        Ok(())
    }

    fn apply_awareness(&mut self, from: ConnId, update: AwarenessUpdate) {
        for (id, entry) in update.clients {
            self.owners.entry(from).or_default().insert(id);
            let newer = match self.awareness.get(&id) {
                Some((clock, _)) => entry.clock > *clock,
                None => true,
            };
            if !newer {
                continue;
            }
            if &*entry.json == "null" {
                self.awareness.remove(&id);
            } else {
                self.awareness.insert(id, (entry.clock, entry.json));
            }
        }
    }

    fn awareness_message(&self) -> Option<Vec<u8>> {
        if self.awareness.is_empty() {
            return None;
        }
        let clients = self
            .awareness
            .iter()
            .map(|(id, (clock, json))| {
                (
                    *id,
                    AwarenessUpdateEntry {
                        clock: *clock,
                        json: json.clone(),
                    },
                )
            })
            .collect();
        Some(Message::Awareness(AwarenessUpdate { clients }).encode_v1())
    }

    #[cfg(test)]
    pub fn text(&self, name: &str) -> String {
        use yrs::GetString;
        let text = self.doc.get_or_insert_text(name);
        text.get_string(&self.doc.transact())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::Text;

    /// Build a client-side update that inserts `s` at the start of "code".
    fn insert_update(s: &str) -> Vec<u8> {
        let doc = Doc::new();
        let text = doc.get_or_insert_text("code");
        let sv = doc.transact().state_vector();
        text.insert(&mut doc.transact_mut(), 0, s);
        doc.transact().encode_diff_v1(&sv)
    }

    fn update_msg(u: Vec<u8>) -> Vec<u8> {
        Message::Sync(SyncMessage::Update(u)).encode_v1()
    }

    fn awareness_msg(client: u64, clock: u32, json: &str) -> Vec<u8> {
        let mut clients = HashMap::new();
        clients.insert(
            ClientID::new(client),
            AwarenessUpdateEntry {
                clock,
                json: Arc::from(json),
            },
        );
        Message::Awareness(AwarenessUpdate { clients }).encode_v1()
    }

    #[test]
    fn sync_step1_gets_step2_with_content() {
        let mut room = Room::new();
        room.handle(1, &update_msg(insert_update("hello"))).unwrap();
        let q = Message::Sync(SyncMessage::SyncStep1(StateVector::default())).encode_v1();
        let replies = room.handle(2, &q).unwrap();
        assert_eq!(replies.len(), 1);
        let Reply::ToSender(bytes) = &replies[0] else {
            panic!("expected ToSender")
        };
        let Message::Sync(SyncMessage::SyncStep2(u)) = Message::decode_v1(bytes).unwrap() else {
            panic!("expected SyncStep2")
        };
        let fresh = Doc::new();
        let text = fresh.get_or_insert_text("code");
        fresh
            .transact_mut()
            .apply_update(Update::decode_v1(&u).unwrap())
            .unwrap();
        use yrs::GetString;
        assert_eq!(text.get_string(&fresh.transact()), "hello");
    }

    #[test]
    fn update_is_applied_and_broadcast() {
        let mut room = Room::new();
        let replies = room.handle(1, &update_msg(insert_update("hello"))).unwrap();
        assert_eq!(replies.len(), 1);
        assert!(matches!(replies[0], Reply::ToOthers(_)));
        assert_eq!(room.text("code"), "hello");
    }

    #[test]
    fn update_past_doc_cap_is_rejected() {
        let mut room = Room::new();
        let big = "a".repeat(MAX_DOC + 1);
        let r = room.handle(1, &update_msg(insert_update(&big)));
        assert_eq!(r, Err(RoomError::DocTooLarge));
        assert_eq!(room.text("code"), "");
    }

    #[test]
    fn many_small_updates_hit_cap() {
        let mut room = Room::new();
        let doc = Doc::new();
        let text = doc.get_or_insert_text("code");
        let chunk = "x".repeat(1000);
        let mut hit = false;
        for _ in 0..1100 {
            let sv = doc.transact().state_vector();
            let len = text.len(&doc.transact());
            text.insert(&mut doc.transact_mut(), len, &chunk);
            let u = doc.transact().encode_diff_v1(&sv);
            match room.handle(1, &update_msg(u)) {
                Ok(_) => {}
                Err(RoomError::DocTooLarge) => {
                    hit = true;
                    break;
                }
                Err(e) => panic!("unexpected {e:?}"),
            }
        }
        assert!(hit);
        assert!(room.text("code").len() <= MAX_DOC);
    }

    #[test]
    fn oversized_awareness_is_protocol_error() {
        let mut room = Room::new();
        let json = format!("\"{}\"", "a".repeat(5000));
        let r = room.handle(1, &awareness_msg(7, 1, &json));
        assert_eq!(r, Err(RoomError::Protocol));
    }

    fn entry_for(bytes: &[u8], client: u64) -> AwarenessUpdateEntry {
        let Message::Awareness(u) = Message::decode_v1(bytes).unwrap() else {
            panic!("expected awareness")
        };
        u.clients.get(&ClientID::new(client)).unwrap().clone()
    }

    #[test]
    fn greeting_includes_awareness() {
        let mut room = Room::new();
        assert_eq!(room.greeting().len(), 1);
        room.handle(1, &awareness_msg(7, 3, "{\"a\":1}")).unwrap();
        let g = room.greeting();
        assert_eq!(g.len(), 2);
        let e = entry_for(&g[1], 7);
        assert_eq!(e.clock, 3);
        assert_eq!(&*e.json, "{\"a\":1}");
    }

    #[test]
    fn remove_conn_marks_states_null() {
        let mut room = Room::new();
        room.handle(1, &awareness_msg(7, 3, "{\"a\":1}")).unwrap();
        let msg = room.remove_conn(1).expect("removal message");
        let e = entry_for(&msg, 7);
        assert_eq!(&*e.json, "null");
        assert!(e.clock > 3);
        assert_eq!(room.greeting().len(), 1);
        assert!(room.remove_conn(2).is_none());
    }

    #[test]
    fn garbage_is_protocol_error() {
        let mut room = Room::new();
        assert_eq!(room.handle(1, &[0xff, 0xff]), Err(RoomError::Protocol));
    }
}
