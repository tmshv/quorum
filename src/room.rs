//! Per-room Yjs protocol state: one shared doc plus relayed awareness.
//! Pure and IO-free; the hub owns locking and fan-out.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::sync::{Message, SyncMessage};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, ReadTxn, StateVector, Transact, Update};

use crate::limits::{MAX_AWARENESS_IDS_PER_CONN, MAX_AWARENESS_STATE, MAX_DOC};

pub type ConnId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    ToSender(Vec<u8>),
    ToOthers(Vec<u8>),
    /// Everyone including the sender (awareness: keeps lone clients alive).
    ToAll(Vec<u8>),
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
                let accepted = self.apply_awareness(from, update)?;
                if accepted.is_empty() {
                    return Ok(Vec::new());
                }
                let relay = Message::Awareness(AwarenessUpdate { clients: accepted }).encode_v1();
                Ok(vec![Reply::ToAll(relay)])
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

    /// Applies newer entries; returns only the accepted ones. A connection
    /// owns a client ID only if it introduced it (echoes of other peers'
    /// states must not make the echoer an owner).
    fn apply_awareness(
        &mut self,
        from: ConnId,
        update: AwarenessUpdate,
    ) -> Result<HashMap<ClientID, AwarenessUpdateEntry>, RoomError> {
        let is_accepted =
            |this: &Self, id: &ClientID, e: &AwarenessUpdateEntry| match this.awareness.get(id) {
                Some((clock, _)) => e.clock > *clock,
                None => &*e.json != "null",
            };
        let added = update
            .clients
            .iter()
            .filter(|(id, e)| !self.awareness.contains_key(id) && is_accepted(self, id, e))
            .count();
        let owned = self.owners.get(&from).map_or(0, |s| s.len());
        if owned + added > MAX_AWARENESS_IDS_PER_CONN {
            return Err(RoomError::Protocol);
        }

        let mut accepted = HashMap::new();
        for (id, entry) in update.clients {
            if !is_accepted(self, &id, &entry) {
                continue;
            }
            if &*entry.json == "null" {
                self.awareness.remove(&id);
                for set in self.owners.values_mut() {
                    set.remove(&id);
                }
            } else {
                if !self.awareness.contains_key(&id) {
                    self.owners.entry(from).or_default().insert(id);
                }
                self.awareness.insert(id, (entry.clock, entry.json.clone()));
            }
            accepted.insert(id, entry);
        }
        Ok(accepted)
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

    fn aw_ids(bytes: &[u8]) -> Vec<u64> {
        let Message::Awareness(u) = Message::decode_v1(bytes).unwrap() else {
            panic!("expected awareness")
        };
        let mut v: Vec<u64> = u.clients.keys().map(|c| c.get()).collect();
        v.sort();
        v
    }

    fn aw_multi(entries: &[(u64, u32, &str)]) -> Vec<u8> {
        let clients = entries
            .iter()
            .map(|(id, clock, json)| {
                (
                    ClientID::new(*id),
                    AwarenessUpdateEntry {
                        clock: *clock,
                        json: Arc::from(*json),
                    },
                )
            })
            .collect();
        Message::Awareness(AwarenessUpdate { clients }).encode_v1()
    }

    #[test]
    fn echo_does_not_make_echoer_owner() {
        let mut room = Room::new();
        room.handle(1, &awareness_msg(7, 3, "{}")).unwrap();
        let r = room.handle(2, &awareness_msg(7, 3, "{}")).unwrap();
        assert!(r.is_empty());
        assert!(room.remove_conn(2).is_none());
        assert_eq!(room.greeting().len(), 2);
    }

    #[test]
    fn mixed_update_broadcasts_only_newer_entries() {
        let mut room = Room::new();
        room.handle(1, &awareness_msg(7, 3, "{}")).unwrap();
        let r = room
            .handle(2, &aw_multi(&[(7, 3, "{}"), (8, 1, "{}")]))
            .unwrap();
        assert_eq!(r.len(), 1);
        let Reply::ToAll(b) = &r[0] else { panic!() };
        assert_eq!(aw_ids(b), vec![8]);
    }

    #[test]
    fn stale_clock_is_ignored() {
        let mut room = Room::new();
        room.handle(1, &awareness_msg(7, 3, "{\"a\":1}")).unwrap();
        let r = room.handle(1, &awareness_msg(7, 2, "{\"a\":2}")).unwrap();
        assert!(r.is_empty());
        let e = entry_for(&room.greeting()[1], 7);
        assert_eq!(&*e.json, "{\"a\":1}");
    }

    #[test]
    fn incoming_null_removes_entry() {
        let mut room = Room::new();
        room.handle(1, &awareness_msg(7, 3, "{}")).unwrap();
        let r = room.handle(1, &awareness_msg(7, 4, "null")).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(room.greeting().len(), 1);
        assert!(room.remove_conn(1).is_none());
    }

    #[test]
    fn too_many_awareness_ids_is_protocol_error() {
        let mut room = Room::new();
        let ids: Vec<(u64, u32, &str)> = (1..=9).map(|i| (i, 1, "{}")).collect();
        assert_eq!(room.handle(1, &aw_multi(&ids)), Err(RoomError::Protocol));
        assert_eq!(room.greeting().len(), 1);
        let ok: Vec<(u64, u32, &str)> = (1..=8).map(|i| (i, 1, "{}")).collect();
        assert!(room.handle(1, &aw_multi(&ok)).is_ok());
        assert_eq!(
            room.handle(1, &awareness_msg(99, 1, "{}")),
            Err(RoomError::Protocol)
        );
    }

    #[test]
    fn awareness_query_returns_all_entries() {
        let mut room = Room::new();
        let q = Message::AwarenessQuery.encode_v1();
        assert!(room.handle(2, &q).unwrap().is_empty());
        room.handle(1, &aw_multi(&[(7, 1, "{}"), (8, 1, "{}")]))
            .unwrap();
        let r = room.handle(2, &q).unwrap();
        assert_eq!(r.len(), 1);
        let Reply::ToSender(b) = &r[0] else { panic!() };
        assert_eq!(aw_ids(b), vec![7, 8]);
    }

    #[test]
    fn auth_and_custom_are_ignored() {
        let mut room = Room::new();
        let a = Message::Auth(None).encode_v1();
        let c = Message::Custom(9, vec![1, 2]).encode_v1();
        assert!(room.handle(1, &a).unwrap().is_empty());
        assert!(room.handle(1, &c).unwrap().is_empty());
    }

    #[test]
    fn sync_step2_is_applied_and_broadcast_as_update() {
        let mut room = Room::new();
        let m = Message::Sync(SyncMessage::SyncStep2(insert_update("hi"))).encode_v1();
        let r = room.handle(1, &m).unwrap();
        assert_eq!(r.len(), 1);
        let Reply::ToOthers(b) = &r[0] else { panic!() };
        assert!(matches!(
            Message::decode_v1(b).unwrap(),
            Message::Sync(SyncMessage::Update(_))
        ));
        assert_eq!(room.text("code"), "hi");
    }
}
