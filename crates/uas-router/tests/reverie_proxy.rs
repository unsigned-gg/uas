// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0032 integration tests: `cargo test -p uas-router -j 2 --test reverie_proxy`.
//!
//! Verifies that `uas-router` serves the Reverie proxy socket under
//! `<workspace>/.ipc/reverie.sock` with strict method/route allowlisting,
//! project scoping, and error synthesis (502 refused, 504 timeout).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use uas_router::{ProxyConfig, ProxyHandle};

fn send_request(socket_path: &std::path::Path, raw: &str) -> (u16, String) {
    let mut stream = UnixStream::connect(socket_path).expect("connect to proxy socket");
    stream
        .write_all(raw.as_bytes())
        .expect("write request to proxy");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read response from proxy");
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.trim().to_string())
        .unwrap_or_default();
    (status, body)
}

#[test]
fn reverie_proxy_routes_scoping_and_error_synthesis() {
    let scratch = std::env::temp_dir().join(format!("uas-reverie-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");

    let socket_path = scratch.join("reverie.sock");

    // Spawn mock upstream TCP server simulating Reverie daemon on 127.0.0.1:7437
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock reverie");
    let upstream_addr = listener.local_addr().expect("upstream addr");

    let upstream_thread = std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /health") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
                let _ = stream.write_all(resp.as_bytes());
            } else if req.starts_with("GET /search") || req.starts_with("GET /context/smart") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]";
                let _ = stream.write_all(resp.as_bytes());
            } else if req.starts_with("POST /observations") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let resp = "HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\nconnection: close\r\n\r\nnot found";
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });

    let config = ProxyConfig::reverie(socket_path.clone(), upstream_addr, "uas".to_string());
    let proxy = ProxyHandle::start(config).expect("start reverie proxy");

    // 1. Approved GET /health returns 200
    let (status, body) = send_request(
        &socket_path,
        "GET /health HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 200);
    assert_eq!(body, "ok");

    // 2. Approved GET /search with matching project returns 200
    let (status, body) = send_request(
        &socket_path,
        "GET /search?project=uas&q=hello HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 200);
    assert_eq!(body, "[]");

    // 3. REQ-0032-12 Negative control: GET /search with mismatched project returns 403
    let (status, _) = send_request(
        &socket_path,
        "GET /search?project=other&q=hello HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 403);

    // 4. REQ-0032-12 Negative control: GET /search without project parameter returns 403
    let (status, _) = send_request(
        &socket_path,
        "GET /search?q=hello HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 403);

    // 5. REQ-0032-05 Approved POST /observations with matching project in body returns 200
    let valid_body = r#"{"project":"uas","content":"sample observation"}"#;
    let (status, body) = send_request(
        &socket_path,
        &format!(
            "POST /observations HTTP/1.1\r\nhost: reverie\r\ncontent-length: {}\r\n\r\n{valid_body}",
            valid_body.len()
        ),
    );
    assert_eq!(status, 200);
    assert_eq!(body, "{}");

    // 6. REQ-0032-13 Negative control: POST /observations with mismatched project returns 403
    let wrong_body = r#"{"project":"other","content":"sample observation"}"#;
    let (status, _) = send_request(
        &socket_path,
        &format!(
            "POST /observations HTTP/1.1\r\nhost: reverie\r\ncontent-length: {}\r\n\r\n{wrong_body}",
            wrong_body.len()
        ),
    );
    assert_eq!(status, 403);

    // 7. REQ-0032-03, REQ-0032-06 Negative control: DELETE /observations/1 returns 403
    let (status, _) = send_request(
        &socket_path,
        "DELETE /observations/1 HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 403);

    // 8. REQ-0032-03 Negative control: path traversal returns 403
    let (status, _) = send_request(
        &socket_path,
        "GET /search/../../etc/passwd HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 403);

    // 9. REQ-0032-06 Negative control: administrative endpoint returns 403
    let (status, _) = send_request(
        &socket_path,
        "GET /admin/reset HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 403);

    proxy.shutdown();
    drop(upstream_thread);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn reverie_proxy_refused_upstream_returns_502() {
    let scratch = std::env::temp_dir().join(format!("uas-reverie-502-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");

    let socket_path = scratch.join("reverie.sock");

    // Pick an unused port with no listener
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    drop(listener);

    let config = ProxyConfig::reverie(
        socket_path.clone(),
        format!("127.0.0.1:{port}").parse().expect("addr"),
        "uas".to_string(),
    );
    let proxy = ProxyHandle::start(config).expect("start proxy");

    // REQ-0032-08 Negative control: Refused upstream connection returns HTTP 502 Bad Gateway
    let (status, _) = send_request(
        &socket_path,
        "GET /health HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 502);

    proxy.shutdown();
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn reverie_proxy_stalled_connect_returns_504() {
    let scratch = std::env::temp_dir().join(format!("uas-reverie-504-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch dir");

    let socket_path = scratch.join("reverie.sock");

    let mut config = ProxyConfig::reverie(
        socket_path.clone(),
        "192.0.2.1:81".parse().expect("addr"),
        "uas".to_string(),
    );
    config.connect_timeout = Duration::from_millis(50);
    let proxy = ProxyHandle::start(config).expect("start proxy");

    // REQ-0032-07, REQ-0032-09 Negative control: Stalled connect returns HTTP 504 Gateway Timeout
    let (status, _) = send_request(
        &socket_path,
        "GET /health HTTP/1.1\r\nhost: reverie\r\n\r\n",
    );
    assert_eq!(status, 504);

    proxy.shutdown();
    let _ = std::fs::remove_dir_all(&scratch);
}
