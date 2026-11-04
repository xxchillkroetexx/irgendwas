//! Client IP behind a proxy chain (e.g. CDN → Traefik → app).
//!
//! Walks `X-Forwarded-For` from the right, starting at the TCP peer, and skips every hop in
//! `TRUSTED_PROXIES_CIDR`. The first untrusted address is the client. If the peer itself
//! is untrusted, the header is ignored (it could be forged).

use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use ipnet::IpNet;

/// Pure resolution logic.
pub fn resolve_client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[IpNet]) -> IpAddr {
    let is_trusted = |ip: &IpAddr| trusted.iter().any(|net| net.contains(ip));
    if !is_trusted(&peer) {
        return peer;
    }
    let forwarded: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|s| parse_hop(s.trim()))
        .collect();
    let mut client = peer;
    for ip in forwarded.into_iter().rev() {
        client = ip;
        if !is_trusted(&ip) {
            break;
        }
    }
    client
}

fn parse_hop(s: &str) -> Option<IpAddr> {
    s.parse::<IpAddr>()
        .ok()
        .or_else(|| s.parse::<SocketAddr>().ok().map(|a| a.ip()))
}

/// Client IP of the current request inside a server fn. Needs the server to run with
/// `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn client_ip() -> Option<IpAddr> {
    let parts = leptos::prelude::use_context::<axum::http::request::Parts>()?;
    let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>()?.0.ip();
    let trusted = super::state()
        .map(|s| s.config.trusted_proxies.clone())
        .unwrap_or_default();
    Some(resolve_client_ip(peer, &parts.headers, &trusted))
}
