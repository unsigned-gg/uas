//! Spec 0014 live probe gate. Operator-run on ceres: `make ebpf-test`.
//!
//! The cases load the real kernel program, so they need `CAP_BPF` and
//! `CAP_PERFMON`. Without them they print `skipped` and pass.

use std::process::Command;
use std::time::{Duration, Instant};

use cas_ebpf::{handle::ProbeHandle, loader::Probes, wire::ExecEvent, EbpfError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// `true` when this host cannot load BPF. Prints the skip line.
fn skip() -> bool {
    let report = cas_ebpf::probe::probe();
    if report.can_load() {
        return false;
    }
    eprintln!("skipped: cannot load BPF here ({})", report.summary());
    true
}

/// Reads events until `window` passes. Sleeps 20 ms between empty polls.
fn collect(probes: &mut Probes, window: Duration) -> Vec<ExecEvent> {
    let deadline = Instant::now() + window;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        match probes.next_event() {
            Some(event) => events.push(event),
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    events
}

#[test]
fn load_without_capabilities_is_a_typed_error() {
    if cas_ebpf::probe::probe().can_load() {
        return;
    }
    assert!(matches!(
        Probes::load(),
        Err(EbpfError::PermissionDenied | EbpfError::BtfMissing)
    ));
}

#[test]
fn load_attaches_three_tracepoints() -> TestResult {
    if skip() {
        return Ok(());
    }
    Probes::load()?;
    Ok(())
}

#[test]
fn exec_of_a_tracked_child_is_reported() -> TestResult {
    if skip() {
        return Ok(());
    }
    let me = std::process::id();
    let mut probes = Probes::load()?;
    probes.track(me, 1)?;
    let mut child = Command::new("/bin/true").spawn()?;
    let child_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_secs(3));
    let event = events
        .iter()
        .find(|e| e.pid == child_pid)
        .ok_or("no exec event for the tracked child")?;
    assert!(
        event.filename_str().ends_with("/true"),
        "{}",
        event.filename_str()
    );
    assert_eq!(event.comm_str(), "true");
    assert_eq!(event.ppid, me);
    probes.untrack(me)?;
    assert!(!probes.is_tracked(me)?);
    Ok(())
}

#[test]
fn fork_propagates_tracking() -> TestResult {
    if skip() {
        return Ok(());
    }
    let me = std::process::id();
    let mut probes = Probes::load()?;
    probes.track(me, 1)?;
    // Two commands, so the shell forks instead of exec-ing the last one.
    let mut child = Command::new("sh")
        .args(["-c", "/bin/true; /bin/true"])
        .spawn()?;
    let sh_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_secs(3));
    assert!(
        events.iter().any(|e| e.pid == sh_pid),
        "no exec event for the shell"
    );
    assert!(
        events
            .iter()
            .any(|e| e.ppid == sh_pid && e.filename_str().ends_with("/true")),
        "no exec event for a true forked by the shell"
    );
    probes.untrack(me)?;
    Ok(())
}

#[test]
fn untracked_processes_are_silent() -> TestResult {
    if skip() {
        return Ok(());
    }
    let mut probes = Probes::load()?;
    let mut child = Command::new("/bin/true").spawn()?;
    let child_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_millis(300));
    assert!(events.iter().all(|e| e.pid != child_pid));
    Ok(())
}

#[test]
fn dropped_counter_starts_at_zero() -> TestResult {
    if skip() {
        return Ok(());
    }
    let probes = Probes::load()?;
    assert_eq!(probes.dropped_events()?, 0);
    Ok(())
}
