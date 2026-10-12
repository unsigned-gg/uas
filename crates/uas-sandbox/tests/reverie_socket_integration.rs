// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0032 gate: `cargo test -p uas-sandbox -j 2 --test reverie_socket_integration`.
//!
//! Verifies that a jailed worker inside a private network namespace can only
//! access Reverie memory through `<workspace>/.ipc/reverie.sock`, and that direct
//! TCP access to the host loopback is denied.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use uas_router::{ProxyConfig, ProxyHandle};
use uas_sandbox::{ipc, launch, netns, LandlockRulesetBuilder, NETNS_REFUSED_ERRNO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "UAS_REVERIE_HELPER";
const SOCKET_ENV: &str = "UAS_REVERIE_SOCKET";
const UPSTREAM_PORT_ENV: &str = "UAS_REVERIE_UPSTREAM_PORT";

// ---------------------------------------------------------------- worker side

fn request_socket(socket: &Path, raw: &str) -> io::Result<(u16, String)> {
    let mut stream = UnixStream::connect(socket)?;
    stream.write_all(raw.as_bytes())?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.trim().to_string())
        .unwrap_or_default();
    Ok((status, body))
}

fn report(name: &str, outcome: io::Result<(u16, String)>) {
    match outcome {
        Ok((status, body)) => println!(
            "check={name} status={status} body={}",
            body.replace('\n', " ")
        ),
        Err(err) => println!(
            "check={name} status=0 body=error:{}",
            err.raw_os_error().unwrap_or(-1)
        ),
    }
}

#[test]
fn helper_reverie() {
    if env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let socket = PathBuf::from(env::var_os(SOCKET_ENV).expect("socket path"));
    let upstream_port: u16 = env::var(UPSTREAM_PORT_ENV)
        .expect("upstream port")
        .parse()
        .expect("valid port");

    // REQ-0032-01 Negative control: Direct TCP connect from jailed slot to host loopback must fail.
    let direct = TcpStream::connect(format!("127.0.0.1:{upstream_port}"));
    match direct {
        Ok(_) => println!("check=direct_connect status=200 body=unexpected_success"),
        Err(err) => println!(
            "check=direct_connect status=0 body=refused:{}",
            err.raw_os_error().unwrap_or(-1)
        ),
    }

    // Approved socket requests
    report(
        "health",
        request_socket(&socket, "GET /health HTTP/1.1\r\nhost: reverie\r\n\r\n"),
    );
    report(
        "search",
        request_socket(
            &socket,
            "GET /search?project=uas&q=prior HTTP/1.1\r\nhost: reverie\r\n\r\n",
        ),
    );
    let obs_body = r#"{"project":"uas","content":"jailed attestation"}"#;
    report(
        "observation",
        request_socket(
            &socket,
            &format!(
                "POST /observations HTTP/1.1\r\nhost: reverie\r\ncontent-length: {}\r\n\r\n{obs_body}",
                obs_body.len()
            ),
        ),
    );
    // REQ-0032-03, REQ-0032-06 Negative control: DELETE returns HTTP 403
    report(
        "delete",
        request_socket(
            &socket,
            "DELETE /observations/1 HTTP/1.1\r\nhost: reverie\r\n\r\n",
        ),
    );
}

// ---------------------------------------------------------------- parent side

fn mock_reverie() -> io::Result<(u16, std::thread::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let thread = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.starts_with("GET /health") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
                let _ = stream.write_all(resp.as_bytes());
            } else if req.starts_with("GET /search") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]";
                let _ = stream.write_all(resp.as_bytes());
            } else if req.starts_with("POST /observations") {
                let resp = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let resp = "HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\nconnection: close\r\n\r\nnot found";
                let _ = stream.write_all(resp.as_bytes());
            }
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    Ok((port, thread))
}

fn fresh(dir: PathBuf) -> io::Result<PathBuf> {
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn scratch(name: &str) -> io::Result<PathBuf> {
    fresh(Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id())))
}

fn target_dir() -> io::Result<PathBuf> {
    env::current_exe()?
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("test binary is not under target/"))
}

fn namespace_available() -> io::Result<bool> {
    let mut command = Command::new("/bin/true");
    netns::isolate(&mut command);
    match command.status() {
        Ok(_) => Ok(true),
        Err(err) if err.raw_os_error() == Some(NETNS_REFUSED_ERRNO) => Ok(false),
        Err(err) => Err(err),
    }
}

fn parse(stdout: &str) -> BTreeMap<String, (u16, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("check=")?;
            let (name, rest) = rest.split_once(" status=")?;
            let (status, body) = rest.split_once(" body=")?;
            Some((name.to_string(), (status.parse().ok()?, body.to_string())))
        })
        .collect()
}

#[test]
fn reverie_socket_integration() -> TestResult {
    let probe = uas_sandbox::probe()?;
    if !probe.is_available() {
        eprintln!("skipped: landlock not available ({:?})", probe.status);
        return Ok(());
    }

    // Spec 0032 test environment rule: Test shall fail with an explicit diagnostic
    // rather than skipping silently if netns is unavailable.
    if !namespace_available()? {
        panic!(
            "environment lacks unprivileged namespace capabilities (CLONE_NEWUSER/CLONE_NEWNET)"
        );
    }

    let ws = scratch("reverie-ws")?;
    ipc::ensure_ipc_dir(&ws)?;
    let socket = ipc::reverie_socket_path(&ws);

    let (upstream_port, _upstream_thread) = mock_reverie()?;
    let upstream_addr = format!("127.0.0.1:{upstream_port}").parse()?;

    let config = ProxyConfig::reverie(socket.clone(), upstream_addr, "uas".to_string());
    let proxy = ProxyHandle::start(config)?;
    assert!(socket.exists(), "the reverie proxy bound its socket");

    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .build()?;
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["helper_reverie", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .env(SOCKET_ENV, &socket)
        .env(UPSTREAM_PORT_ENV, upstream_port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    netns::isolate(&mut command);

    let output = launch(ruleset, &mut command)?.wait_with_output()?;
    assert!(
        output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = parse(&String::from_utf8_lossy(&output.stdout));
    let get = |name: &str| results.get(name).cloned().unwrap_or((0, "missing".into()));

    // REQ-0032-01: Direct TCP connect from jailed slot failed
    assert_eq!(get("direct_connect").0, 0, "direct connect must fail");
    assert!(
        get("direct_connect").1.starts_with("refused:"),
        "direct connect refused"
    );

    // REQ-0032-01, REQ-0032-02, REQ-0032-04: Worker reads context through proxy
    assert_eq!(get("health").0, 200, "health must be 200");
    assert_eq!(get("search").0, 200, "search must be 200");

    // REQ-0032-05: Worker writes observation through proxy
    assert_eq!(get("observation").0, 200, "observation must be 200");

    // REQ-0032-03, REQ-0032-06: DELETE is rejected with HTTP 403
    assert_eq!(get("delete").0, 403, "delete must be 403");

    proxy.shutdown();
    assert!(!socket.exists(), "the socket file is removed on shutdown");
    let _ = fs::remove_dir_all(&ws);
    Ok(())
}
