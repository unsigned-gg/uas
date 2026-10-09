//! Spec 0011 gate: `cargo test -p cas-sandbox -j 2 --test netns`.
//!
//! The worker is this test binary, re-executed with `helper_probe` as the only
//! test and `CAS_NETNS_HELPER` set. It prints three lines; the parent reads them.
//! On a host that cannot create a namespace the jailed case prints `skipped`
//! with the host diagnostics and passes; the PR body says which host ran it.

use std::env;
use std::fs;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cas_sandbox::{launch, netns, LandlockRulesetBuilder, NETNS_REFUSED_ERRNO};

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
    // The interfaces this process can see. /proc/net/dev is per network namespace.
    let interfaces = fs::read_to_string("/proc/net/dev")
        .map(|text| {
            text.lines()
                .skip(2)
                .filter_map(|line| line.split(':').next())
                .map(|name| name.trim().to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|err| format!("unreadable:{err}"));
    println!("interfaces={interfaces}");
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

/// `(lo_connect, wan_connect, interfaces)` from the probe's stdout.
fn parse(stdout: &str) -> (Option<String>, Option<String>, Option<String>) {
    let value = |key: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(key).map(str::to_string))
    };
    (
        value("lo_connect="),
        value("wan_connect="),
        value("interfaces="),
    )
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
    let (lo, wan, interfaces) = parse(&String::from_utf8_lossy(&output.stdout));
    assert_eq!(lo.as_deref(), Some("ok"), "control: loopback must work");
    let interfaces = interfaces.unwrap_or_default();
    println!(
        "unjailed control: wan_connect={} interfaces={interfaces}",
        wan.unwrap_or_default()
    );
    assert!(
        interfaces
            .split(',')
            .any(|name| name != "lo" && !name.is_empty()),
        "control: the host must have an interface besides lo, got {interfaces:?}"
    );
    Ok(())
}

/// Can this host give a child a private network namespace? Starts the probe
/// under `netns::isolate` alone. A refusal prints the host diagnostics.
fn namespace_available() -> Result<bool, std::io::Error> {
    let mut command = probe_command()?;
    netns::isolate(&mut command);
    match command.spawn() {
        Ok(mut child) => {
            child.wait()?;
            Ok(true)
        }
        Err(err) if err.raw_os_error() == Some(NETNS_REFUSED_ERRNO) => {
            eprintln!("skipped: this host cannot create a network namespace");
            print_host_diagnostics();
            Ok(false)
        }
        Err(err) => Err(err),
    }
}

#[test]
fn jailed_worker_has_only_loopback() -> TestResult {
    if !namespace_available()? {
        return Ok(());
    }
    let ws = scratch("netns-ws")?;
    // The preset has no /proc rule (spec 0010); the probe reads /proc/net/dev.
    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .allow_read("/proc")
        .build()?;
    let mut command = probe_command()?;
    netns::isolate(&mut command);
    let child = launch(ruleset, &mut command).inspect_err(|err| {
        eprintln!("launch failed: {err}");
        print_host_diagnostics();
    })?;
    let output = child.wait_with_output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "worker failed: {stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (lo, wan, interfaces) = parse(&stdout);
    println!("jailed: lo_connect={lo:?} wan_connect={wan:?} interfaces={interfaces:?}");
    assert_eq!(
        interfaces.as_deref(),
        Some("lo"),
        "the namespace must hold only lo"
    );
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

/// Facts about the host that decide whether a namespace can be made. Printed
/// only when the worker could not start, so a CI log says why.
fn print_host_diagnostics() {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("Cap")
                || line.starts_with("Seccomp")
                || line.starts_with("NoNewPrivs")
            {
                eprintln!("diag: {line}");
            }
        }
    }
    for path in [
        "/proc/sys/user/max_user_namespaces",
        "/proc/sys/kernel/unprivileged_userns_clone",
    ] {
        match fs::read_to_string(path) {
            Ok(value) => eprintln!("diag: {path} = {}", value.trim()),
            Err(err) => eprintln!("diag: {path}: {err}"),
        }
    }
    for args in [["-n", "true"], ["-Urn", "true"]] {
        match Command::new("unshare").args(args).output() {
            Ok(out) => eprintln!(
                "diag: unshare {} -> {} {}",
                args[0],
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(err) => eprintln!("diag: unshare {}: {err}", args[0]),
        }
    }
    if let Ok(release) = fs::read_to_string("/proc/sys/kernel/osrelease") {
        eprintln!("diag: kernel {}", release.trim());
    }
}
