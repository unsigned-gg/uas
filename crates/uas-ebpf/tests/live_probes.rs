//! Spec 0014 live probe gate. Operator-run on ceres: `make ebpf-test`.
//!
//! The cases load the real kernel program, so they need `CAP_BPF` and
//! `CAP_PERFMON`. Without them they print `skipped` and pass.

use std::process::Command;
use std::time::{Duration, Instant};

use uas_ebpf::{handle::ProbeHandle, loader::Probes, wire::ExecEvent, EbpfError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// `true` when this host cannot load BPF. Prints the skip line.
fn skip() -> bool {
    let report = uas_ebpf::probe::probe();
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
    if uas_ebpf::probe::probe().can_load() {
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
        .find(|e| e.ns_pid == child_pid)
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
    // D13: every tracked key is a real pid, and the kernel map holds the offsets
    // the parser reads. The keys are checked first, and the list must not be
    // empty: `me` was promoted at the fork above.
    let pid_max: u32 = std::fs::read_to_string("/proc/sys/kernel/pid_max")?
        .trim()
        .parse()?;
    let tracked = probes.tracked_globals()?;
    assert!(
        !tracked.is_empty(),
        "no tracked key after a fork under tracking"
    );
    assert!(
        tracked.iter().all(|pid| (1..=pid_max).contains(pid)),
        "tracked keys outside 1..={pid_max}: {tracked:?}"
    );
    assert_eq!(probes.tp_offsets()?, uas_ebpf::tracefs::read()?.slots());
    assert!(
        events.iter().any(|e| e.ns_pid == sh_pid),
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
    let me = std::process::id();
    let mut probes = Probes::load()?;
    let mut child = Command::new("/bin/true").spawn()?;
    let silent_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_millis(300));
    assert!(
        events.iter().all(|e| e.ns_pid != silent_pid),
        "untracked exec reported"
    );
    // Positive control in the same test: once tracked, the next child is reported.
    probes.track(me, 1)?;
    let mut child = Command::new("/bin/true").spawn()?;
    let loud_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_secs(3));
    assert!(
        events.iter().any(|e| e.ns_pid == loud_pid),
        "control: tracked exec missing"
    );
    probes.untrack(me)?;
    Ok(())
}

#[test]
fn exit_evicts_the_child_from_the_map() -> TestResult {
    if skip() {
        return Ok(());
    }
    let me = std::process::id();
    let mut probes = Probes::load()?;
    probes.track(me, 1)?;
    let mut child = Command::new("sleep").arg("3").spawn()?;
    let child_ns_pid = child.id();
    // Read the child's global pid from its exec event. The assertions below
    // look at `TRACKED_PIDS` by that global pid. The namespace-keyed lookups
    // would pass on the `NS_TO_GLOBAL` removal alone and miss a missing
    // `TRACKED_PIDS` removal.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut global = None;
    while global.is_none() && Instant::now() < deadline {
        match probes.next_event() {
            Some(event) if event.ns_pid == child_ns_pid => global = Some(event.pid),
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let global = global.ok_or("no exec event for the sleeping child")?;
    assert!(
        probes.is_tracked_global(global)?,
        "child must be in TRACKED_PIDS while it runs"
    );
    child.wait()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while probes.is_tracked_global(global)? {
        assert!(
            Instant::now() < deadline,
            "exit did not evict the child from TRACKED_PIDS"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    probes.untrack(me)?;
    Ok(())
}

/// `true` when this host refuses `unshare -Upf --fork`. Prints the skip line.
fn nested_unshare_refused() -> bool {
    let ok = Command::new("unshare")
        .args(["-Upf", "--fork", "/bin/true"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("skipped: unshare -Upf --fork is refused here");
    }
    !ok
}

#[test]
fn nested_pid_namespace_exec_is_audited() -> TestResult {
    if skip() || nested_unshare_refused() {
        return Ok(());
    }
    let me = std::process::id();
    let mut probes = Probes::load()?;
    probes.track(me, 1)?;
    let mut child = Command::new("unshare")
        .args(["-Upf", "--fork", "sh", "-c", "/bin/true; /bin/true"])
        .spawn()?;
    let unshare_pid = child.id();
    child.wait()?;
    let events = collect(&mut probes, Duration::from_secs(3));

    // Control: `unshare` itself runs in the daemon's namespace, so it has a
    // namespace pid. Without it, a zero below could mean a broken harness.
    let unshare = events
        .iter()
        .find(|e| e.ns_pid == unshare_pid)
        .ok_or("control: no exec event for unshare with its namespace pid")?;
    // On ceres the daemon's namespace is itself nested under the host's, so
    // the global tgid can differ from the namespace pid (spec 0014 T13), and
    // the exit probe has already evicted unshare's map entry by now. The
    // control is the event itself: a real global pid and the right binary.
    assert_ne!(unshare.pid, 0, "unshare event has no global pid");
    assert!(
        unshare.filename_str().ends_with("/unshare"),
        "control event is not unshare: {}",
        unshare.filename_str()
    );

    // The nested shell and its `true` children run in a namespace the helper
    // cannot name. They are audited with `ns_pid` 0 and a real global pid.
    let nested: Vec<&ExecEvent> = events.iter().filter(|e| e.ns_pid == 0).collect();
    assert!(
        nested
            .iter()
            .any(|e| e.pid != 0 && e.filename_str().ends_with("/sh")),
        "no nested exec event for sh: {nested:?}"
    );
    assert!(
        nested
            .iter()
            .any(|e| e.pid != 0 && e.filename_str().ends_with("/true")),
        "no nested exec event for true: {nested:?}"
    );
    // Fork tracking from a nested parent: `true` runs in a child the nested
    // shell forked, so its global pid differs from the shell's. (dash forks
    // both commands; bash would exec the last one in place.)
    let sh_pid = nested
        .iter()
        .find(|e| e.filename_str().ends_with("/sh"))
        .map(|e| e.pid)
        .ok_or("nested sh event vanished")?;
    assert!(
        nested
            .iter()
            .any(|e| e.filename_str().ends_with("/true") && e.pid != sh_pid),
        "no forked nested child distinct from sh (pid {sh_pid}): {nested:?}"
    );

    // Exit eviction works by global pid for tasks the helper could not name.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut still = Vec::new();
        for event in &events {
            if probes.is_tracked_global(event.pid)? {
                still.push(event.pid);
            }
        }
        if still.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "exit did not evict global pids {still:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    probes.untrack(me)?;
    Ok(())
}

#[test]
fn tracking_failures_start_at_zero() -> TestResult {
    if skip() {
        return Ok(());
    }
    let probes = Probes::load()?;
    assert_eq!(probes.tracking_failures()?, 0);
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
