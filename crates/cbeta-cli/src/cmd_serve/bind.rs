//! Parse `--http ADDR` and bind a TCP listener.

use std::net::{SocketAddr, TcpListener, ToSocketAddrs};

/// Parse bind address for `serve --http`.
///
/// Accepts `127.0.0.1:1873`, `[::1]:1873`, `localhost:1873`.
/// Rejects bare port (`1873`), host-less `:1873`, and missing port.
pub fn parse_http_addr(raw: &str) -> Result<SocketAddr, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("empty --http address".into());
    }
    // Bare port or host-less :port — never implicit 0.0.0.0.
    if s.starts_with(':') || s.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "invalid --http address `{s}`: require host:port (e.g. 127.0.0.1:1873)"
        ));
    }
    if let Ok(addr) = s.parse::<SocketAddr>() {
        return Ok(addr);
    }
    // localhost / DNS names via ToSocketAddrs.
    let mut iter = s
        .to_socket_addrs()
        .map_err(|e| format!("invalid --http address `{s}`: {e}"))?;
    iter.next()
        .ok_or_else(|| format!("invalid --http address `{s}`: no resolved addresses"))
}

/// Bind `addr` (port 0 allowed). On failure print to stderr and return exit 2.
pub fn bind_listener(addr: SocketAddr) -> Result<TcpListener, i32> {
    match TcpListener::bind(addr) {
        Ok(l) => Ok(l),
        Err(e) => {
            eprintln!("failed to bind {addr}: {e}");
            Err(2)
        }
    }
}

/// Host strings for DNS-rebinding protection after the real port is known.
pub fn allowed_hosts_for(local: SocketAddr) -> Vec<String> {
    let port = local.port();
    let mut hosts = vec![
        "localhost".into(),
        format!("localhost:{port}"),
        "127.0.0.1".into(),
        format!("127.0.0.1:{port}"),
    ];
    match local {
        SocketAddr::V4(v4) => {
            let ip = v4.ip().to_string();
            if ip != "127.0.0.1" {
                hosts.push(ip.clone());
                hosts.push(format!("{ip}:{port}"));
            }
        }
        SocketAddr::V6(v6) => {
            let ip = v6.ip().to_string();
            hosts.push(format!("[{ip}]"));
            hosts.push(format!("[{ip}]:{port}"));
            if ip == "::1" {
                hosts.push("::1".into());
            }
        }
    }
    hosts.sort();
    hosts.dedup();
    hosts
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parse_rejects_bare_port_and_colon() {
        assert!(parse_http_addr("1873").is_err());
        assert!(parse_http_addr(":1873").is_err());
        assert!(parse_http_addr("").is_err());
    }

    #[test]
    fn parse_socket_addr_and_localhost() {
        let a = parse_http_addr("127.0.0.1:0").unwrap();
        assert_eq!(a.ip().to_string(), "127.0.0.1");
        let b = parse_http_addr("localhost:1873").unwrap();
        assert_eq!(b.port(), 1873);
    }

    #[test]
    fn allowed_hosts_include_loopback_variants() {
        let addr: SocketAddr = "127.0.0.1:1873".parse().unwrap();
        let h = allowed_hosts_for(addr);
        assert!(h.iter().any(|x| x == "127.0.0.1"));
        assert!(h.iter().any(|x| x == "127.0.0.1:1873"));
        assert!(h.iter().any(|x| x == "localhost:1873"));
    }
}
