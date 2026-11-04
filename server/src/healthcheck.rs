//! `server --healthcheck`: a container healthcheck without curl. Sends `GET /health` to the
//! local server and exits 0 on `200`, 1 otherwise. Reads only `LEPTOS_SITE_ADDR` (no `.env`,
//! no config), so it works in the distroless runtime image.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// Command-line flag that switches `main` into healthcheck mode.
pub const FLAG: &str = "--healthcheck";

const TIMEOUT: Duration = Duration::from_secs(3);

/// Where the server listens: `LEPTOS_SITE_ADDR` (default `127.0.0.1:3000`), with an
/// unspecified bind address (`0.0.0.0` / `::`) replaced by loopback.
pub fn target_addr() -> SocketAddr {
    let mut addr = std::env::var("LEPTOS_SITE_ADDR")
        .ok()
        .and_then(|v| v.trim().parse::<SocketAddr>().ok())
        .unwrap_or_else(|| SocketAddr::from((Ipv4Addr::LOCALHOST, 3000)));
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        });
    }
    addr
}

/// `GET http://{addr}/health`; `Ok` iff the status is 200.
pub fn probe(addr: SocketAddr) -> Result<(), String> {
    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut head = [0u8; 64];
    let mut len = 0;
    // The status line ("HTTP/1.1 200 OK") fits easily in 64 bytes.
    while len < head.len() {
        match stream.read(&mut head[len..]) {
            Ok(0) => break,
            Ok(n) => len += n,
            Err(e) => return Err(e.to_string()),
        }
        if head[..len].contains(&b'\n') {
            break;
        }
    }
    let status_line = String::from_utf8_lossy(&head[..len]);
    let status = status_line.split_whitespace().nth(1).unwrap_or_default();
    if status == "200" {
        Ok(())
    } else {
        Err(format!("unexpected response: {}", status_line.trim()))
    }
}

/// Runs the check and returns the process exit code.
pub fn run() -> i32 {
    let addr = target_addr();
    match probe(addr) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("healthcheck http://{addr}/health failed: {err}");
            1
        }
    }
}
