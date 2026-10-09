//! Spec 0013 gate: `cargo test -p cas-sandbox -j 2 --test uds_proxy`.
//!
//! A fake gateway listens on 127.0.0.1 and echoes the `host` and `authorization`
//! it received. The proxy (`cas-router`) serves `<workspace>/.ipc/llm.sock` with a
//! key file the worker cannot read. The worker is this test binary re-executed in
//! helper mode inside the jail (and the network namespace where the host allows
//! it); it talks raw HTTP/1.1 over the socket and prints one line per check.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cas_router::{ProxyConfig, ProxyHandle};
use cas_sandbox::{ipc, launch, netns, LandlockRulesetBuilder, NETNS_REFUSED_ERRNO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "CAS_UDS_HELPER";
const SOCKET_ENV: &str = "CAS_UDS_SOCKET";
const KEY_ENV: &str = "CAS_UDS_KEY_FILE";
const KEY: &str = "test-gateway-key-0123";
const UPSTREAM_HOST: &str = "llm.llm:4000";
const BODY: &str = r#"{"model":"glm-5.2","messages":[{"role":"user","content":"hi"}]}"#;

// ---------------------------------------------------------------- worker side

/// Send one request over the socket and return `(status, body)`.
fn request(socket: &Path, raw: &str) -> io::Result<(u16, String)> {
    let mut stream = UnixStream::connect(socket)?;
    stream.write_all(raw.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
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
fn helper_requests() {
    if env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let socket = PathBuf::from(env::var_os(SOCKET_ENV).expect("socket path"));
    let key_file = PathBuf::from(env::var_os(KEY_ENV).expect("key file path"));

    report(
        "completion",
        request(
            &socket,
            &format!(
                "POST /v1/chat/completions HTTP/1.1\r\nhost: worker.invalid\r\nauthorization: Bearer worker-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{BODY}",
                BODY.len()
            ),
        ),
    );
    report(
        "models",
        request(&socket, "GET /v1/models HTTP/1.1\r\nhost: x\r\n\r\n"),
    );
    report(
        "connect",
        request(
            &socket,
            "CONNECT llm.unsigned.gg:443 HTTP/1.1\r\nhost: llm.unsigned.gg:443\r\n\r\n",
        ),
    );
    report(
        "admin",
        request(&socket, "GET /admin/keys HTTP/1.1\r\nhost: x\r\n\r\n"),
    );
    report(
        "passwd",
        request(&socket, "GET /etc/passwd HTTP/1.1\r\nhost: x\r\n\r\n"),
    );
    report("traversal", request(&socket, "POST /v1/chat/completions/../../admin HTTP/1.1\r\nhost: x\r\ncontent-length: 0\r\n\r\n"));
    report(
        "put_models",
        request(
            &socket,
            "PUT /v1/models HTTP/1.1\r\nhost: x\r\ncontent-length: 0\r\n\r\n",
        ),
    );
    let huge = format!(
        "GET /v1/models HTTP/1.1\r\nhost: x\r\nx-pad: {}\r\n\r\n",
        "a".repeat(20 * 1024)
    );
    report("oversized", request(&socket, &huge));
    // The key file sits outside the workspace; the jail must deny it.
    println!(
        "check=read_key_file status=0 body={}",
        match fs::read(&key_file) {
            Ok(_) => "readable".to_string(),
            Err(err) => format!("errno{}", err.raw_os_error().unwrap_or(-1)),
        }
    );
}

// ---------------------------------------------------------------- parent side

/// A gateway stand-in: answers every request with the `host` and `authorization`
/// it saw, so the parent can check injection and replacement.
fn fake_upstream() -> io::Result<(std::net::SocketAddr, std::thread::JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let thread = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream);
            let mut host = String::new();
            let mut authorization = String::new();
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                if let Some((name, value)) = trimmed.split_once(':') {
                    match name.trim().to_ascii_lowercase().as_str() {
                        "host" => host = value.trim().to_string(),
                        "authorization" => authorization = value.trim().to_string(),
                        "content-length" => content_length = value.trim().parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let reply = format!(
                "host={host};authorization={authorization};body_len={}",
                body.len()
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let mut stream = reader.into_inner();
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    Ok((addr, thread))
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

fn home_scratch(name: &str) -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    fresh(
        Path::new(&home)
            .join(".cache")
            .join("cas-sandbox-tests")
            .join(format!("{name}-{}", std::process::id())),
    )
    .ok()
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
fn a_jailed_worker_reaches_the_gateway_only_through_the_allowlist() -> TestResult {
    let probe = cas_sandbox::probe()?;
    if !probe.is_available() {
        eprintln!("skipped: landlock not available ({:?})", probe.status);
        return Ok(());
    }
    let Some(outside) = home_scratch("uds-home") else {
        eprintln!("skipped: HOME is unset or not writable");
        return Ok(());
    };
    let key_file = outside.join("gateway.key");
    fs::write(&key_file, format!("{KEY}\n"))?;
    fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600))?;

    let ws = scratch("uds-ws")?;
    ipc::ensure_ipc_dir(&ws)?;
    let socket = ipc::socket_path(&ws);
    let (upstream, _upstream_thread) = fake_upstream()?;
    let proxy = ProxyHandle::start(
        ProxyConfig::new(socket.clone(), upstream, UPSTREAM_HOST.to_string())
            .with_key_file(&key_file)?,
    )?;
    assert!(socket.exists(), "the proxy bound its socket");

    let netns_ok = namespace_available()?;
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .build()?;
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["helper_requests", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .env(SOCKET_ENV, &socket)
        .env(KEY_ENV, &key_file)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if netns_ok {
        netns::isolate(&mut command);
    } else {
        eprintln!("network namespace unavailable on this host; worker runs in the jail only");
    }
    let output = launch(ruleset, &mut command)?.wait_with_output()?;
    assert!(
        output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = parse(&String::from_utf8_lossy(&output.stdout));
    for (name, (status, body)) in &results {
        println!("[netns={netns_ok}] {name}: {status} {body}");
    }
    let get = |name: &str| results.get(name).cloned().unwrap_or((0, "missing".into()));

    let (status, body) = get("completion");
    assert_eq!(status, 200, "completion: {body}");
    assert!(
        body.contains(&format!("host={UPSTREAM_HOST}")),
        "host rewritten: {body}"
    );
    assert!(
        body.contains(&format!("authorization=Bearer {KEY}")),
        "key injected: {body}"
    );
    assert!(!body.contains("worker-key"), "worker key replaced: {body}");
    assert!(
        body.contains(&format!("body_len={}", BODY.len())),
        "body forwarded whole: {body}"
    );
    assert_eq!(get("models").0, 200);
    for name in ["connect", "admin", "passwd", "traversal", "put_models"] {
        assert_eq!(get(name).0, 403, "{name}: {:?}", get(name));
    }
    assert_eq!(get("oversized").0, 431);
    assert_eq!(
        get("read_key_file").1,
        "errno13",
        "the worker must not read the key file"
    );

    proxy.shutdown();
    assert!(!socket.exists(), "the socket file is removed on shutdown");
    fs::remove_dir_all(&outside)?;
    Ok(())
}
