use std::net::{IpAddr, SocketAddr};

use axum::http::{HeaderMap, header};

/// The client address: the first `X-Forwarded-For` entry when it parses
/// (the server runs behind a reverse proxy), otherwise the socket peer.
pub fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or_else(|| peer.ip())
}

/// True when the request has no `Origin`, or its authority equals `Host`
/// (case-insensitive).
pub fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Some((_, authority)) = origin.split_once("://") else {
        return false;
    };
    authority.eq_ignore_ascii_case(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> SocketAddr {
        "10.0.0.9:5555".parse().unwrap()
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn xff_first_entry_wins() {
        let h = headers(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1")]);
        assert_eq!(
            client_ip(&h, peer()),
            "203.0.113.7".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn bad_xff_falls_back_to_peer() {
        let h = headers(&[("x-forwarded-for", "not-an-ip, 10.0.0.1")]);
        assert_eq!(client_ip(&h, peer()), peer().ip());
        assert_eq!(client_ip(&HeaderMap::new(), peer()), peer().ip());
    }

    #[test]
    fn no_origin_is_allowed() {
        assert!(same_origin(&headers(&[("host", "quorum.example")])));
    }

    #[test]
    fn cross_origin_rejected() {
        let h = headers(&[
            ("host", "quorum.example"),
            ("origin", "https://evil.example"),
        ]);
        assert!(!same_origin(&h));
        let h = headers(&[("host", "quorum.example"), ("origin", "null")]);
        assert!(!same_origin(&h));
    }

    #[test]
    fn same_origin_case_insensitive() {
        let h = headers(&[
            ("host", "Quorum.Example:8080"),
            ("origin", "http://quorum.example:8080"),
        ]);
        assert!(same_origin(&h));
    }
}
