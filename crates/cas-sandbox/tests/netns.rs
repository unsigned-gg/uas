//! Spec 0011 gate: `cargo test -p cas-sandbox -j 2 --test netns`.
//!
//! The worker is this test binary, re-executed with `helper_probe` as the only
//! test and `CAS_NETNS_HELPER` set. It prints two lines; the parent reads them.

use std::env;
use std::fs;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cas_sandbox::{jail, netns, LandlockRulesetBuilder};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "CAS_NETNS_HELPER";
const ENETUNREACH: i32 = 101;

/// The probe the worker runs. Outside the helper mode it does nothing.
#[test]
fn helper_probe() {
    if env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let lo = TcpListener::bind("127.0.0.1:0").and_then(|listener| {
        let addr = listener.local_addr()?;
        TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map(|_| ())
    });
    println!("lo_connect={}", describe(lo));
    let wan = TcpStream::connect_timeout(
        &SocketAddr::from(([1, 1, 1, 1], 80)),
        Duration::from_secs(3),
    )
    .map(|_| ());
    println!("wan_connect={}", describe(wan));
}

fn describe(result: std::io::Result<()>) -> String {
    match result {
        Ok(()) => "ok".to_string(),
        Err(err) => match err.raw_os_error() {
            Some(errno) => format!("errno{errno}"),
            None => format!("{:?}", err.kind()),
        },
    }
}

fn probe_command() -> Result<Command, std::io::Error> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["helper_probe", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

/// `(lo_connect, wan_connect)` from the probe's stdout.
fn parse(stdout: &str) -> (Option<String>, Option<String>) {
    let value = |key: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(key).map(str::to_string))
    };
    (value("lo_connect="), value("wan_connect="))
}

fn scratch(name: &str) -> Result<PathBuf, std::io::Error> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The `target` directory that holds this test binary, for the read-and-execute rule.
fn target_dir() -> Result<PathBuf, std::io::Error> {
    let exe = env::current_exe()?;
    // target/debug/deps/<binary>
    exe.ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| std::io::Error::other("test binary is not under target/"))
}

#[test]
fn unjailed_control_reaches_loopback() -> TestResult {
    let output = probe_command()?.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (lo, wan) = parse(&String::from_utf8_lossy(&output.stdout));
    assert_eq!(lo.as_deref(), Some("ok"), "control: loopback must work");
    println!("unjailed control: wan_connect={}", wan.unwrap_or_default());
    Ok(())
}

#[test]
fn jailed_worker_has_only_loopback() -> TestResult {
    let ws = scratch("netns-ws")?;
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .build()?;
    let mut command = probe_command()?;
    netns::isolate(&mut command);
    jail(&mut command, ruleset);
    let output = command.spawn()?.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "worker failed: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (lo, wan) = parse(&stdout);
    println!("jailed: lo_connect={:?} wan_connect={:?}", lo, wan);
    assert_eq!(
        lo.as_deref(),
        Some("ok"),
        "lo must be up inside the namespace"
    );
    assert_eq!(
        wan.as_deref(),
        Some(format!("errno{ENETUNREACH}").as_str()),
        "a WAN connect must fail with ENETUNREACH"
    );
    Ok(())
}
