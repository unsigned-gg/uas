// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0012 gate: `cargo test -p uas-sandbox -j 2 --test escapes`.
//!
//! The worker is this test binary, re-executed with `helper_attacks` as the only
//! test and `UAS_ESCAPES_HELPER` set, inside the Landlock jail, the launcher and,
//! where the host allows it, the network namespace. It runs every attack in the
//! matrix and prints `check=<name> result=<value>` lines; the parent asserts them.
//! Expected errnos: EACCES 13, EXDEV 18, ENETUNREACH 101.
//!
//! Spec 0043 D6 adds eleven seccomp rows and an x32 row. Every one runs its raw
//! syscall in a forked probe, so an unfiltered success (a traced parent, a new user
//! namespace, a new process) never touches the matrix worker, and a kill by SIGSYS
//! is reported instead of ending the run. Expected errnos: EPERM 1, ENOSYS 38.

use std::collections::BTreeMap;
use std::env;
use std::ffi::CString;
use std::fs;
use std::io;
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use uas_sandbox::{jail, launch, netns, LandlockRulesetBuilder, NETNS_REFUSED_ERRNO};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HELPER_ENV: &str = "UAS_ESCAPES_HELPER";
const WS_ENV: &str = "UAS_ESCAPES_WS";
const OUTSIDE_ENV: &str = "UAS_ESCAPES_OUTSIDE";
const NETNS_ENV: &str = "UAS_ESCAPES_NETNS";
const SECCOMP_ENV: &str = "UAS_ESCAPES_SECCOMP";

const EACCES: i32 = 13;
const EXDEV: i32 = 18;
const ENETUNREACH: i32 = 101;
const EPERM: i32 = 1;
const ENOSYS: i32 = 38;
const SIGSYS: i32 = 31;

/// The seccomp rows an unprivileged worker would already get `EPERM` for from the
/// kernel (no `CAP_SYS_ADMIN`, `CAP_NET_RAW` or `CAP_DAC_READ_SEARCH`), so they pass
/// without the filter on such a host and cannot tell the mechanisms apart.
const ALSO_CAPABILITY: [&str; 4] = ["bpf_syscall", "mount_tmpfs", "open_by_handle", "raw_socket"];

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
    let seccomp = env::var(SECCOMP_ENV).map(|v| v == "1").unwrap_or(false);

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
        describe(fs::write("/etc/uas-escape-test", b"x")),
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
    report("write_root", describe(fs::write("/uas-escape-test", b"x")));
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
    let tmp_bin = PathBuf::from(format!("/tmp/uas-escape-true-{}", std::process::id()));
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

    // Syscall filter (spec 0043 D6): only when the parent launched us with it.
    if seccomp {
        seccomp_rows(&ws, &outside);
    } else {
        report("seccomp", "skipped");
    }

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

// ------------------------------------------------------- seccomp probes (D6)

/// Issue one raw syscall and return 0 on success or the errno.
#[allow(unsafe_code)]
fn sys(nr: libc::c_long, a: [usize; 5]) -> i32 {
    // SAFETY: every caller passes integers or pointers to buffers that live for the
    // whole call and are sized for the syscall; the probes run in a forked child
    // that exits right after, so a call that succeeds unfiltered harms nothing.
    let ret = unsafe { libc::syscall(nr, a[0], a[1], a[2], a[3], a[4]) };
    if ret == -1 {
        io::Error::last_os_error().raw_os_error().unwrap_or(255)
    } else {
        0
    }
}

/// Run `probe` in a forked child and describe how the child ended: `ok` for exit 0,
/// `errno<N>` for exit N, `signal<N>` when a signal killed it. The child makes raw
/// syscalls only, then `_exit`. The wait is bounded so a probe that hangs because the
/// filter is missing fails the row instead of the run.
#[allow(unsafe_code)]
fn forked(probe: impl FnOnce() -> i32) -> String {
    // SAFETY: fork has no preconditions here. The child runs `probe` (raw syscalls
    // over buffers built before the fork, no allocation, no lock) and calls `_exit`.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return "fork-failed".into();
    }
    if pid == 0 {
        let code = probe();
        // SAFETY: `_exit` ends the forked child without destructors or atexit hooks.
        unsafe { libc::_exit(code) };
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut status = 0;
    loop {
        // SAFETY: `pid` is our child and `status` is a valid out pointer.
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if waited == pid {
            break;
        }
        if waited < 0 || Instant::now() > deadline {
            // SAFETY: `pid` is our own child; kill then reap it.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
            return "hung".into();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if libc::WIFEXITED(status) {
        match libc::WEXITSTATUS(status) {
            0 => "ok".into(),
            code => format!("errno{code}"),
        }
    } else if libc::WIFSIGNALED(status) {
        format!("signal{}", libc::WTERMSIG(status))
    } else {
        format!("status{status:#x}")
    }
}

/// After a raw `clone` or `clone3` that succeeded unfiltered: the new process exits
/// at once, and the probe child reaps it, so nothing outlives the row.
#[allow(unsafe_code)]
fn spawned(ret: libc::c_long) -> i32 {
    if ret == -1 {
        return io::Error::last_os_error().raw_os_error().unwrap_or(255);
    }
    if ret == 0 {
        // SAFETY: `_exit` ends the new process without destructors or atexit hooks.
        unsafe { libc::_exit(0) };
    }
    let mut status = 0;
    // SAFETY: `ret` is the pid of our child; `__WALL` also reaps a child that
    // `clone3` made with no exit signal.
    unsafe { libc::waitpid(ret as libc::pid_t, &mut status, libc::__WALL) };
    0
}

/// The kernel's own answer for a zeroed file handle on a directory, or a zeroed
/// buffer when the filesystem has none. The filter denies the call either way.
#[allow(unsafe_code)]
fn handle_buffer(dir: &CString) -> Vec<u8> {
    // struct file_handle { u32 handle_bytes; i32 handle_type; u8 f_handle[]; }
    let mut buf = vec![0u8; 8 + 128];
    buf[..4].copy_from_slice(&128u32.to_ne_bytes());
    let mut mount_id: libc::c_int = 0;
    // SAFETY: `buf` holds a file_handle header plus 128 bytes, `dir` is a valid
    // C string and `mount_id` a valid out pointer. A failure leaves the buffer zeroed.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_name_to_handle_at,
            libc::AT_FDCWD,
            dir.as_ptr(),
            buf.as_mut_ptr(),
            &mut mount_id as *mut libc::c_int,
            0,
        )
    };
    if ret == -1 {
        buf.fill(0);
        buf[..4].copy_from_slice(&8u32.to_ne_bytes());
    }
    buf
}

fn raw_socket_row() -> String {
    // Both arms of the D2 rule: the AF_PACKET domain, and SOCK_RAW with
    // SOCK_CLOEXEC OR-ed in, which an unmasked compare of the type would miss.
    let packet = forked(|| {
        sys(
            libc::SYS_socket,
            [libc::AF_PACKET as usize, libc::SOCK_DGRAM as usize, 0, 0, 0],
        )
    });
    let raw = forked(|| {
        sys(
            libc::SYS_socket,
            [
                libc::AF_INET as usize,
                (libc::SOCK_RAW | libc::SOCK_CLOEXEC) as usize,
                libc::IPPROTO_ICMP as usize,
                0,
                0,
            ],
        )
    });
    if packet == raw {
        packet
    } else {
        format!("packet={packet},raw={raw}")
    }
}

/// The twelve rows of spec 0043 D6 plus the x32 row. Each prints its own result.
fn seccomp_rows(ws: &Path, outside: &Path) {
    report(
        "ptrace_self",
        forked(|| {
            sys(
                libc::SYS_ptrace,
                [libc::PTRACE_TRACEME as usize, 0, 0, 0, 0],
            )
        }),
    );
    // KEYCTL_GET_KEYRING_ID (0) for KEY_SPEC_SESSION_KEYRING (-3), no create.
    report(
        "keyctl_session",
        forked(|| sys(libc::SYS_keyctl, [0, (-3isize) as usize, 0, 0, 0])),
    );
    // BPF_MAP_CREATE with a one-entry array map: the attr is map_type, key_size,
    // value_size, max_entries, then zeros.
    let mut bpf_attr = [0u32; 32];
    bpf_attr[..4].copy_from_slice(&[2, 4, 4, 1]);
    report(
        "bpf_syscall",
        forked(|| sys(libc::SYS_bpf, [0, bpf_attr.as_ptr() as usize, 128, 0, 0])),
    );
    // A software dummy event on this process, kernel and hypervisor excluded, so
    // the paranoid sysctl alone would not refuse it.
    let mut perf_attr = [0u32; 32];
    perf_attr[0] = 1; // PERF_TYPE_SOFTWARE
    perf_attr[1] = 128; // attr.size
    perf_attr[2] = 9; // PERF_COUNT_SW_DUMMY
    perf_attr[10] = 0x60; // exclude_kernel | exclude_hv
    report(
        "perf_open",
        forked(|| {
            sys(
                libc::SYS_perf_event_open,
                [perf_attr.as_ptr() as usize, 0, usize::MAX, usize::MAX, 0],
            )
        }),
    );
    let dir = CString::new(ws.as_os_str().as_encoded_bytes()).unwrap_or_default();
    let tmpfs = CString::new("tmpfs").unwrap_or_default();
    report(
        "mount_tmpfs",
        forked(|| {
            sys(
                libc::SYS_mount,
                [
                    tmpfs.as_ptr() as usize,
                    dir.as_ptr() as usize,
                    tmpfs.as_ptr() as usize,
                    0,
                    0,
                ],
            )
        }),
    );
    report(
        "unshare_user",
        forked(|| {
            sys(
                libc::SYS_unshare,
                [libc::CLONE_NEWUSER as usize, 0, 0, 0, 0],
            )
        }),
    );
    report(
        "clone_newuser",
        forked(|| {
            // SAFETY: a raw clone with no stack and no VM sharing behaves as fork;
            // `spawned` ends the new process at once and reaps it.
            #[allow(unsafe_code)]
            let ret = unsafe {
                libc::syscall(
                    libc::SYS_clone,
                    libc::CLONE_NEWUSER | libc::SIGCHLD,
                    0usize,
                    0usize,
                    0usize,
                    0usize,
                )
            };
            spawned(ret)
        }),
    );
    report("raw_socket", raw_socket_row());
    // io_uring_params is 120 bytes of zeros for a request of one entry.
    let uring_params = [0u8; 120];
    report(
        "io_uring",
        forked(|| {
            sys(
                libc::SYS_io_uring_setup,
                [1, uring_params.as_ptr() as usize, 0, 0, 0],
            )
        }),
    );
    let handle = handle_buffer(&dir);
    #[allow(unsafe_code)]
    // SAFETY: `dir` is a valid C string; the fd is closed on drop of the raw value below.
    let mount_fd = unsafe { libc::open(dir.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
    report(
        "open_by_handle",
        forked(|| {
            sys(
                libc::SYS_open_by_handle_at,
                [
                    mount_fd as usize,
                    handle.as_ptr() as usize,
                    libc::O_RDONLY as usize,
                    0,
                    0,
                ],
            )
        }),
    );
    #[allow(unsafe_code)]
    // SAFETY: closing the descriptor opened above, once.
    unsafe {
        libc::close(mount_fd)
    };
    // clone3 with a zeroed 64-byte clone_args: on the unfiltered path it makes a child.
    let clone_args = [0u8; 64];
    report(
        "clone3_enosys",
        forked(|| {
            // SAFETY: `clone_args` is the 64-byte zeroed struct clone3 reads; zero
            // flags make a plain child, which `spawned` ends and reaps.
            #[allow(unsafe_code)]
            let ret =
                unsafe { libc::syscall(libc::SYS_clone3, clone_args.as_ptr(), clone_args.len()) };
            spawned(ret)
        }),
    );
    // x32 (spec 0043 D3 and the critic's row): a number with bit 30 set carries the
    // native audit arch and would miss every deny entry; the filter must kill it.
    if cfg!(target_arch = "x86_64") {
        report(
            "x32_syscall",
            forked(|| sys(0x4000_0000 | libc::SYS_getpid, [0; 5])),
        );
    } else {
        report("x32_syscall", "skipped");
    }
    // pre_exec order in one worker (REQ-0043-01). The netns hook calls `unshare`
    // and the filter denies `unshare`, so a worker that has both a private network
    // and the filter proves the namespace was entered first. Landlock and the filter
    // are both visible from inside the same process here.
    report("layers", layers(outside));
}

/// The three layers as seen from inside one worker: the network (`lo` only means the
/// namespace is in place), Landlock (a write under `$HOME` is refused) and the
/// seccomp mode and filter count from `/proc/self/status`.
fn layers(outside: &Path) -> String {
    let landlock = describe(fs::write(outside.join("layers-probe.txt"), b"x"));
    let net = interfaces();
    let (mode, filters) = seccomp_status("/proc/self/status");
    format!("net={net} landlock={landlock} seccomp={mode}/{filters}")
}

/// `Seccomp:` and `Seccomp_filters:` from a status file, `?` when absent.
fn seccomp_status(path: &str) -> (String, String) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
            .unwrap_or_else(|| "?".into())
    };
    (field("Seccomp:"), field("Seccomp_filters:"))
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
            .join("uas-sandbox-tests")
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

fn attack_command(
    ws: &Path,
    outside: &Path,
    netns: bool,
    seccomp: bool,
) -> Result<Command, io::Error> {
    let mut command = Command::new(env::current_exe()?);
    command
        .args(["helper_attacks", "--exact", "--nocapture"])
        .env(HELPER_ENV, "1")
        .env(WS_ENV, ws)
        .env(OUTSIDE_ENV, outside)
        .env(NETNS_ENV, if netns { "1" } else { "0" })
        .env(SECCOMP_ENV, if seccomp { "1" } else { "0" })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

/// REQ-0043-08: the filter's `errno` verdict is listed in `actions_avail`.
fn errno_action_available() -> bool {
    fs::read_to_string("/proc/sys/kernel/seccomp/actions_avail")
        .map(|s| s.split_whitespace().any(|a| a == "errno"))
        .unwrap_or(false)
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
    let report = uas_sandbox::probe()?;
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
    // `launch` installs the filter, and a host without the `errno` verdict refuses
    // that install. There the matrix runs under Landlock alone and the seccomp
    // assertions are skipped (REQ-0043-08).
    let seccomp_on = errno_action_available();
    if !seccomp_on {
        eprintln!("skipped: seccomp errno action not available");
    }
    let mut command = attack_command(&ws, &outside, netns_ok, seccomp_on)?;
    if netns_ok {
        netns::isolate(&mut command);
    }
    let base_filters = seccomp_status("/proc/self/status").1;
    let child = if seccomp_on {
        launch(ruleset, &mut command)?
    } else {
        jail(&mut command, ruleset).spawn()?
    };
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = parse(&String::from_utf8_lossy(&output.stdout));
    for (name, value) in &results {
        let note = if ALSO_CAPABILITY.contains(&name.as_str()) {
            " (also capability: the kernel denies it without the filter)"
        } else {
            ""
        };
        println!("[netns={netns_ok}] {name}: {value}{note}");
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

    // Syscall filter (spec 0043 D6, REQ-0043-02, -03, -07).
    if seccomp_on {
        for name in [
            "ptrace_self",
            "keyctl_session",
            "bpf_syscall",
            "perf_open",
            "mount_tmpfs",
            "unshare_user",
            "clone_newuser",
            "raw_socket",
            "io_uring",
            "open_by_handle",
        ] {
            assert_eq!(get(name), format!("errno{EPERM}"), "{name}");
        }
        assert_eq!(get("clone3_enosys"), format!("errno{ENOSYS}"));
        match get("x32_syscall") {
            "skipped" => assert_ne!(
                std::env::consts::ARCH,
                "x86_64",
                "x32 row skipped on x86_64"
            ),
            other => assert_eq!(other, format!("signal{SIGSYS}"), "x32_syscall"),
        }
        // One worker holds all three layers: the namespace, Landlock and the
        // filter. The filter denies the `unshare` the namespace hook needs, so a
        // worker that has the namespace entered it before the filter went on.
        let net = "lo";
        let filters = match base_filters.parse::<u32>() {
            // The two programs of spec 0043 D2 stack on whatever the parent has.
            Ok(n) => (n + 2).to_string(),
            Err(_) => "?".into(),
        };
        if netns_ok {
            assert_eq!(
                get("layers"),
                format!("net={net} landlock=errno{EACCES} seccomp=2/{filters}"),
                "netns, Landlock and seccomp in one worker"
            );
        } else if filters != "?" {
            assert!(
                get("layers").ends_with(&format!("landlock=errno{EACCES} seccomp=2/{filters}")),
                "layers: {}",
                get("layers")
            );
        }
    } else {
        assert_eq!(get("seccomp"), "skipped");
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

/// Spec 0043 D8: the cost of the filter is measured, not promised. Spawns
/// `/usr/bin/true` through `jail` (Landlock only) and through `launch` (Landlock plus
/// the filter) in alternation and prints the mean time `spawn` takes, which ends when
/// the child has called `exec`. `launch` also builds the filter programs in the
/// parent, so that cost is inside its figure and is printed on its own as well.
/// Nothing here asserts a number.
#[test]
fn seccomp_timing_print() -> TestResult {
    const RUNS: u32 = 50;
    if !uas_sandbox::probe()?.is_available() {
        eprintln!("skipped: landlock not available");
        return Ok(());
    }
    if !errno_action_available() {
        eprintln!("skipped: seccomp errno action not available");
        return Ok(());
    }
    let ws = scratch("escapes-timing")?;
    let fresh_ruleset = || -> Result<_, Box<dyn std::error::Error>> {
        Ok(LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new())
            .build()?
            .0)
    };
    let (mut without, mut with, mut build) = (Duration::ZERO, Duration::ZERO, Duration::ZERO);
    for _ in 0..RUNS {
        let ruleset = fresh_ruleset()?;
        let mut command = Command::new("/usr/bin/true");
        let start = Instant::now();
        let mut child = jail(&mut command, ruleset).spawn()?;
        without += start.elapsed();
        assert!(child.wait()?.success());

        let ruleset = fresh_ruleset()?;
        let mut command = Command::new("/usr/bin/true");
        let start = Instant::now();
        let mut child = launch(ruleset, &mut command)?;
        with += start.elapsed();
        assert!(child.wait()?.success());

        let start = Instant::now();
        let _programs = uas_sandbox::seccomp::build()?;
        build += start.elapsed();
    }
    let mean = |total: Duration| total.as_micros() / u128::from(RUNS);
    println!(
        "seccomp_timing runs={RUNS} mean_us fork_to_exec_without={} fork_to_exec_with={} (parent-side build_only={})",
        mean(without),
        mean(with),
        mean(build)
    );
    fs::remove_dir_all(&ws)?;
    Ok(())
}
