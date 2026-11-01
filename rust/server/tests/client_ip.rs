//! Client IP behind trusted proxies (`app::server::client_ip`).

use std::net::IpAddr;

use app::server::client_ip::resolve_client_ip;
use axum::http::HeaderMap;
use ipnet::IpNet;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn xff(value: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-forwarded-for", value.parse().unwrap());
    h
}

#[test]
fn walks_forwarded_for_from_the_right() {
    let trusted: Vec<IpNet> = vec![
        "172.18.0.0/16".parse().unwrap(),
        "10.0.0.5/32".parse().unwrap(),
    ];

    // Untrusted peer: header ignored (could be forged).
    assert_eq!(
        resolve_client_ip(ip("203.0.113.9"), &xff("1.2.3.4"), &trusted),
        ip("203.0.113.9")
    );
    // Caddy (trusted) → client.
    assert_eq!(
        resolve_client_ip(ip("172.18.0.2"), &xff("198.51.100.7"), &trusted),
        ip("198.51.100.7")
    );
    // Outer proxy + Caddy, client tried to spoof a left-most entry.
    assert_eq!(
        resolve_client_ip(
            ip("172.18.0.2"),
            &xff("6.6.6.6, 198.51.100.7, 10.0.0.5"),
            &trusted
        ),
        ip("198.51.100.7")
    );
    // Trusted peer without header.
    assert_eq!(
        resolve_client_ip(ip("172.18.0.2"), &HeaderMap::new(), &trusted),
        ip("172.18.0.2")
    );
}
