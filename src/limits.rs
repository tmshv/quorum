//! Server-wide caps, WebSocket close codes and room-name validation.

use std::time::Duration;

pub const MAX_CONNS: usize = 1000;
pub const MAX_CONNS_PER_IP: usize = 8;
pub const MAX_ROOMS: usize = 200;
pub const MAX_PER_ROOM: usize = 16;
/// Larger than `MAX_DOC`, so an oversized paste is read and refused by the
/// room as "document too large" instead of failing the socket read.
pub const MAX_MESSAGE: usize = 2 << 20;
pub const MAX_DOC: usize = 1 << 20;
pub const MAX_AWARENESS_STATE: usize = 4 << 10;
pub const MAX_AWARENESS_IDS_PER_CONN: usize = 8;
pub const RATE_PER_SEC: f64 = 120.0;
pub const RATE_BURST: f64 = 360.0;
/// Refused messages within one strike window before the connection is closed.
pub const MAX_STRIKES: u32 = 100;
/// Strikes reset after this long without a refusal.
pub const STRIKE_RESET: Duration = Duration::from_secs(10);
/// Client IDs whose removal each room remembers (stale echoes are refused).
pub const MAX_TOMBSTONES: usize = 1024;
pub const OUTBOX: usize = 512;

/// A socket write that takes longer than this ends the connection.
pub const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// The server pings this often.
pub const PING_INTERVAL: Duration = Duration::from_secs(30);
/// A connection with no inbound frame (pongs count) for this long is closed.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// WebSocket close codes shared by server and client.
pub mod close {
    pub const BAD_ROOM: u16 = 4000;
    pub const ROOM_FULL: u16 = 4001;
    pub const BUSY: u16 = 4002;
    pub const DOC_TOO_LARGE: u16 = 4003;
    pub const RATE_LIMITED: u16 = 4004;
    pub const TOO_SLOW: u16 = 4005;
    pub const PROTOCOL: u16 = 4006;
}

/// Matches `^[a-z0-9][a-z0-9-]{0,39}$`.
pub fn valid_room(name: &str) -> bool {
    let b = name.as_bytes();
    if b.is_empty() || b.len() > 40 {
        return false;
    }
    let alnum = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    alnum(b[0]) && b[1..].iter().all(|&c| alnum(c) || c == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_rooms() {
        assert!(valid_room("kernel-panic-42"));
        assert!(valid_room("a"));
        assert!(valid_room("0"));
        assert!(valid_room(&"a".repeat(40)));
    }

    #[test]
    fn rejects_invalid_rooms() {
        assert!(!valid_room(""));
        assert!(!valid_room("-a"));
        assert!(!valid_room("A"));
        assert!(!valid_room("a_b"));
        assert!(!valid_room("a/b"));
        assert!(!valid_room(&"a".repeat(41)));
    }
}
