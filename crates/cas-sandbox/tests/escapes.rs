//! Spec 0012 gate: `cargo test -p cas-sandbox -j 2 --test escapes`.
//!
//! The worker is this test binary, re-executed with `helper_attacks` as the only
//! test and `CAS_ESCAPES_HELPER` set, inside the Landlock jail, the launcher and,
//! where the host allows it, the network namespace. It runs every attack in the
//! matrix and prints `check=<name> result=<value>` lines; the parent asserts them.
//! Expected errnos: EACCES 13, EXDEV 18, ENETUNREACH 101.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use cas_sandbox::{launch, netns, LandlockRulesetBuilder, NETNS_REFUSED_ERRNO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "CAS_ESCAPES_HELPER";
const WS_ENV: &str = "CAS_ESCAPES_WS";
const OUTSIDE_ENV: &str = "CAS_ESCAPES_OUTSIDE";
const NETNS_ENV: &str = "CAS_ESCAPES_NETNS";

const EACCES: i32 = 13;
const EXDEV: i32 = 18;
const ENETUNREACH: i32 = 101;

fn describe<T>(result: io::Result<T>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(err) => match err.raw_os_error() {
            Some(errno) => format!("errno{errno}"),
            None => format!("{:?}", err.kind()),
        },
    }
}

fn report(name: &str, value: impl AsRef<str>) {
    println!("check={name} result={}", value.as_ref());
}

/// The attack matrix. Runs only in helper mode, inside the jail.
#[test]
fn helper_attacks() {
    if env::var_os(HELPER_ENV).is_none() {
        return;
    }
    let ws = PathBuf::from(env::var_os(WS_ENV).expect("workspace path"));
    let outside = PathBuf::from(env::var_os(OUTSIDE_ENV).expect("outside path"));
    let netns = env::var(NETNS_ENV).map(|v| v == "1").unwrap_or(false);

    // Filesystem: the positive control first, then every way out.
    report(
        "write_ws",
        describe(fs::write(ws.join("ok.txt"), b"inside")),
    );
    report(
        "write_home",
        describe(fs::write(outside.join("escape.txt"), b"x")),
    );
    report(
        "write_etc",
        describe(fs::write("/etc/cas-escape-test", b"x")),
    );
    report(
        "append_etc_passwd",
        describe(fs::OpenOptions::new().append(true).open("/etc/passwd")),
    );
    let ssh = Path::new(&env::var_os("HOME").unwrap_or_default()).join(".ssh");
    report(
        "read_home_ssh",
        if ssh.exists() {
            describe(fs::read_dir(&ssh))
        } else {
            "absent".into()
        },
    );
    report("write_root", describe(fs::write("/cas-escape-test", b"x")));
    report(
        "read_outside",
        describe(fs::read(outside.join("secret.txt"))),
    );
    let link = ws.join("link-out");
    report("symlink_create", describe(symlink(&outside, &link)));
    report(
        "write_via_symlink",
        describe(fs::write(link.join("escape.txt"), b"x")),
    );
    let inside_file = ws.join("movable.txt");
    report("prep_movable", describe(fs::write(&inside_file, b"x")));
    report(
        "rename_out",
        describe(fs::rename(&inside_file, outside.join("moved.txt"))),
    );
    report(
        "hardlink_out",
        describe(fs::hard_link(&inside_file, outside.join("linked.txt"))),
    );
    // Execute: the same copy of /usr/bin/true runs from the workspace and not
    // from /tmp, the write-only scratch class (spec 0010 D7). /tmp is literal:
    // the preset names /tmp, not $TMPDIR.
    let ws_bin = ws.join("true-copy");
    let staged = fs::copy("/usr/bin/true", &ws_bin)
        .and_then(|_| fs::set_permissions(&ws_bin, fs::Permissions::from_mode(0o755)));
    report("stage_ws_binary", describe(staged));
    report("exec_from_ws", describe(Command::new(&ws_bin).status()));
    let tmp_bin = PathBuf::from(format!("/tmp/cas-escape-true-{}", std::process::id()));
    let staged = fs::copy("/usr/bin/true", &tmp_bin)
        .and_then(|_| fs::set_permissions(&tmp_bin, fs::Permissions::from_mode(0o755)));
    report("stage_tmp_binary", describe(staged));
    report("exec_from_tmp", describe(Command::new(&tmp_bin).status()));
    let _ = fs::remove_file(&tmp_bin);

    // A pathname Unix socket outside the workspace (spec 0012 known gap, measured
    // for spec 0013). The parent listens on it; the jailed worker connects.
    report(
        "uds_connect_out",
        describe(std::os::unix::net::UnixStream::connect(
            outside.join("denied.sock"),
        )),
    );

    // Privilege: no_new_privs here and in a grandchild, Landlock in a grandchild.
    report("nnp", read_nnp("/proc/self/status"));
    report(
        "grandchild_nnp",
        sh_stdout("grep NoNewPrivs /proc/self/status"),
    );
    let grand_ws = format!("echo x > '{}'", ws.join("grand-ws.txt").display());
    report("grandchild_write_ws", sh_status(&grand_ws));
    let grand = format!("echo x > '{}'", outside.join("grand.txt").display());
    report("grandchild_write_home", sh_status(&grand));
    report("setuid_sudo", sudo_probe());

    // Network: only when the parent could give us a namespace.
    if netns {
        report(
            "tcp_wan",
            describe(TcpStream::connect_timeout(
                &SocketAddr::from(([1, 1, 1, 1], 80)),
                Duration::from_secs(3),
            )),
        );
        let udp = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .and_then(|s| s.send_to(b"x", SocketAddr::from(([1, 1, 1, 1], 53))));
        report("udp_wan", describe(udp));
        report("ping_raw", ping_probe());
        report("interfaces", interfaces());
    } else {
        report("network", "skipped");
    }
}

fn read_nnp(path: &str) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("NoNewPrivs:").map(|v| v.trim().to_string()))
        })
        .unwrap_or_else(|| "unreadable".into())
}

fn sh_stdout(script: &str) -> String {
    match Command::new("/bin/sh").arg("-c").arg(script).output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .trim()
            .replace('\t', " "),
        Err(err) => describe::<()>(Err(err)),
    }
}

fn sh_status(script: &str) -> String {
    match Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => format!("exit{}", status.code().unwrap_or(-1)),
        Err(err) => describe::<()>(Err(err)),
    }
}

/// `sudo -n true`: under no_new_privs the setuid bit is ignored, so sudo sees a
/// non-root effective uid and says so. Without sudo on the host: `absent`.
fn sudo_probe() -> String {
    if !Path::new("/usr/bin/sudo").exists() {
        return "absent".into();
    }
    match Command::new("/usr/bin/sudo").args(["-n", "true"]).output() {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("no new privileges") || stderr.contains("effective uid is not 0") {
                // no_new_privs: the setuid bit was ignored at exec.
                "setuid-ignored".into()
            } else if stderr.contains("is owned by uid 65534") {
                // User namespace: root is unmapped, so sudo's own files look foreign.
                "root-unmapped".into()
            } else {
                format!("exit{} {}", out.status.code().unwrap_or(-1), stderr.trim())
            }
        }
        Err(err) => describe::<()>(Err(err)),
    }
}

/// `ping -c1 -W1 1.1.1.1`: an ICMP or raw socket with nowhere to go.
fn ping_probe() -> String {
    if !Path::new("/usr/bin/ping").exists() {
        return "absent".into();
    }
    match Command::new("/usr/bin/ping")
        .args(["-c", "1", "-W", "1", "1.1.1.1"])
        .output()
    {
        Ok(out) if out.status.success() => "reached".into(),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("socktype: SOCK_RAW") {
                // The raw socket could not be opened at all.
                "raw-socket-denied".into()
            } else if stderr.to_ascii_lowercase().contains("unreachable") {
                "no-route".into()
            } else {
                format!("exit{} {}", out.status.code().unwrap_or(-1), stderr.trim())
            }
        }
        Err(err) => describe::<()>(Err(err)),
    }
}

fn interfaces() -> String {
    fs::read_to_string("/proc/net/dev")
        .map(|text| {
            text.lines()
                .skip(2)
                .filter_map(|line| line.split(':').next())
                .map(|name| name.trim().to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|err| format!("unreadable:{err}"))
}

// ---------------------------------------------------------------- parent side

fn fresh(dir: PathBuf) -> Result<PathBuf, io::Error> {
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn scratch(name: &str) -> Result<PathBuf, io::Error> {
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

fn target_dir() -> Result<PathBuf, io::Error> {
    env::current_exe()?
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("test binary is not under target/"))
}

fn attack_command(ws: &Path, outside: &Path, netns: bool) -> Result<Command, io::Error> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["helper_attacks", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .env(WS_ENV, ws)
        .env(OUTSIDE_ENV, outside)
        .env(NETNS_ENV, if netns { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

fn namespace_available() -> Result<bool, io::Error> {
    let mut command = Command::new("/bin/true");
    netns::isolate(&mut command);
    match command.status() {
        Ok(_) => Ok(true),
        Err(err) if err.raw_os_error() == Some(NETNS_REFUSED_ERRNO) => {
            eprintln!("network checks skipped: this host cannot create a network namespace");
            Ok(false)
        }
        Err(err) => Err(err),
    }
}

fn parse(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("check=")?;
            let (name, value) = rest.split_once(" result=")?;
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

/// Run the matrix once. `with_netns` adds the namespace when the host allows it.
fn run_matrix(with_netns: bool) -> TestResult {
    let report = cas_sandbox::probe()?;
    if !report.is_available() {
        eprintln!("skipped: landlock not available ({:?})", report.status);
        return Ok(());
    }
    let Some(outside) = home_scratch(if with_netns {
        "escapes-home-ns"
    } else {
        "escapes-home"
    }) else {
        eprintln!("skipped: HOME is unset or not writable");
        return Ok(());
    };
    fs::write(outside.join("secret.txt"), b"host secret")?;
    // A listening pathname socket outside the workspace, for the uds_connect_out row.
    let _listener = std::os::unix::net::UnixListener::bind(outside.join("denied.sock"))?;
    let ws = scratch(if with_netns {
        "escapes-ws-ns"
    } else {
        "escapes-ws"
    })?;
    let netns_ok = with_netns && namespace_available()?;
    if with_netns && !netns_ok {
        eprintln!("skipped: the first matrix run already covers a host without a namespace");
        return Ok(());
    }
    // Landlock's refer check runs only when the rename or link stays on one
    // mount; across mounts the kernel answers EXDEV before any LSM hook.
    let same_mount = fs::metadata(&ws)?.dev() == fs::metadata(&outside)?.dev();

    let (ruleset, _) = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
        .allow_read_exec(target_dir()?)
        .allow_read("/proc")
        .build()?;
    let mut command = attack_command(&ws, &outside, netns_ok)?;
    if netns_ok {
        netns::isolate(&mut command);
    }
    let output = launch(ruleset, &mut command)?.wait_with_output()?;
    assert!(
        output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = parse(&String::from_utf8_lossy(&output.stdout));
    for (name, value) in &results {
        println!("[netns={netns_ok}] {name}: {value}");
    }
    let get = |name: &str| results.get(name).map(String::as_str).unwrap_or("missing");
    let denied = |name: &str| {
        let value = get(name).to_string();
        if same_mount {
            assert_eq!(value, format!("errno{EACCES}"), "{name} on one mount");
        } else {
            assert!(
                value == format!("errno{EACCES}") || value == format!("errno{EXDEV}"),
                "{name} across mounts: {value}"
            );
            println!("{name}: cross-mount, Landlock's refer check not exercised");
        }
    };

    // Filesystem.
    assert_eq!(
        get("write_ws"),
        "ok",
        "positive control inside the workspace"
    );
    assert_eq!(get("write_home"), format!("errno{EACCES}"));
    assert_eq!(get("write_etc"), format!("errno{EACCES}"));
    assert_eq!(get("append_etc_passwd"), format!("errno{EACCES}"));
    match get("read_home_ssh") {
        "absent" => {}
        other => assert_eq!(other, format!("errno{EACCES}"), "read_home_ssh"),
    }
    assert_eq!(get("write_root"), format!("errno{EACCES}"));
    assert_eq!(get("read_outside"), format!("errno{EACCES}"));
    assert_eq!(
        get("symlink_create"),
        "ok",
        "a symlink inside the workspace is allowed"
    );
    assert_eq!(get("write_via_symlink"), format!("errno{EACCES}"));
    assert_eq!(get("prep_movable"), "ok");
    denied("rename_out");
    denied("hardlink_out");
    assert_eq!(get("stage_ws_binary"), "ok");
    assert_eq!(
        get("exec_from_ws"),
        "ok",
        "the same copy runs from the workspace"
    );
    assert_eq!(
        get("stage_tmp_binary"),
        "ok",
        "writing into /tmp is allowed"
    );
    assert_eq!(
        get("exec_from_tmp"),
        format!("errno{EACCES}"),
        "no execute from /tmp"
    );
    for name in ["escape.txt", "moved.txt", "linked.txt", "grand.txt"] {
        assert!(
            !outside.join(name).exists(),
            "{name} appeared outside the workspace"
        );
    }
    assert!(
        ws.join("movable.txt").exists(),
        "the file stayed in the workspace"
    );

    // Measurement, not a gate: Landlock below ABI v6 does not mediate connect on
    // pathname Unix sockets, so this row is expected to succeed. Spec 0013 cites it.
    println!("uds_connect_out (measurement): {}", get("uds_connect_out"));
    assert_ne!(get("uds_connect_out"), "missing");

    // Privilege.
    assert_eq!(get("nnp"), "1");
    assert_eq!(get("grandchild_nnp"), "NoNewPrivs: 1");
    assert_eq!(
        get("grandchild_write_ws"),
        "exit0",
        "the grandchild shell works"
    );
    assert!(ws.join("grand-ws.txt").exists());
    assert_ne!(
        get("grandchild_write_home"),
        "exit0",
        "the grandchild stays jailed"
    );
    match (get("setuid_sudo"), netns_ok) {
        ("absent", _) | ("setuid-ignored", _) | ("root-unmapped", true) => {}
        (other, _) => panic!("setuid_sudo: {other}"),
    }

    // Network.
    if netns_ok {
        assert_eq!(get("tcp_wan"), format!("errno{ENETUNREACH}"));
        assert_eq!(get("udp_wan"), format!("errno{ENETUNREACH}"));
        assert_eq!(get("interfaces"), "lo");
        match get("ping_raw") {
            "absent" | "raw-socket-denied" | "no-route" => {}
            other => panic!("ping_raw: {other}"),
        }
    } else {
        assert_eq!(get("network"), "skipped");
    }
    fs::remove_dir_all(&outside)?;
    Ok(())
}

/// Landlock and the launcher alone: what no_new_privs does by itself.
#[test]
fn the_jail_resists_the_attack_matrix() -> TestResult {
    run_matrix(false)
}

/// The full jail with the network namespace, where the host can make one.
#[test]
fn the_jail_with_a_namespace_resists_the_attack_matrix() -> TestResult {
    run_matrix(true)
}
