//! The process-group clamp, measured on a real child (spec 0014 D4, ruling R2).
//!
//! Lowering priority needs no privilege, so this gate runs unprivileged in CI.

use std::{
    os::unix::process::CommandExt,
    process::{Command, Stdio},
};

use cas_ebpf::clamp::{enter_clamped_group, nice_of, pgid_of, WORKER_NICE};

#[test]
fn child_spawned_with_enter_clamped_group_runs_at_nice_19_in_its_own_group() {
    let mut command = Command::new("sleep");
    command
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the hook runs in the child between fork and exec. It makes two
    // syscalls (setpgid, setpriority) that touch only the child's own process
    // and allocate nothing, which is what pre_exec requires.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(enter_clamped_group);
    }
    let mut child = command.spawn().expect("spawn sleep");
    let pid = child.id();

    let nice = nice_of(pid).expect("getpriority on child");
    let pgid = pgid_of(pid).expect("getpgid on child");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(nice, WORKER_NICE, "child nice");
    assert_eq!(pgid, pid, "child leads its own process group");
}

#[test]
fn child_spawned_without_the_hook_keeps_the_parent_nice_and_group() {
    let mut child = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");
    let pid = child.id();
    let me = std::process::id();

    let child_nice = nice_of(pid).expect("getpriority on child");
    let child_pgid = pgid_of(pid).expect("getpgid on child");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(child_nice, nice_of(me).expect("own nice"));
    assert_eq!(child_pgid, pgid_of(me).expect("own pgid"));
}
