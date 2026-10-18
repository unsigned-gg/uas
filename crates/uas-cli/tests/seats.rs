// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0058 wave 1: `uas seats` against fixture trees and a fake tmux. No real /sys, no real
//! tmux server.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "uas-cli-seats-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&p).expect("tmp");
        Self(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A cgroup tree with two scopes (`aaa` has a pane, `bbb` has none) and a proc tree.
fn fixture(t: &Tmp) {
    let svc = t.0.join("cg/user.slice/user-1000.slice/user@1000.service");
    for n in ["aaa", "bbb", "ccc"] {
        let d = svc.join(format!("tmux-spawn-{n}.scope"));
        write(&d.join("memory.current"), "1048576\n");
        write(&d.join("memory.max"), "max\n");
        write(&d.join("pids.current"), "3\n");
    }
    write(
        &svc.join("tmux-spawn-aaa.scope/memory.pressure"),
        "some avg10=2.50 avg60=0.00 avg300=0.00 total=9\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
    );
    write(&svc.join("tmux-spawn-bbb.scope/pids.current"), "0\n");
    // A scope that is not a pane.
    write(&svc.join("app-other.scope/pids.current"), "2\n");
    // ccc holds two panes, one of them in a nested child cgroup.
    write(
        &t.0.join("proc/5001/cgroup"),
        "0::/user.slice/user-1000.slice/user@1000.service/tmux-spawn-ccc.scope\n",
    );
    write(
        &t.0.join("proc/5002/cgroup"),
        "0::/user.slice/user-1000.slice/user@1000.service/tmux-spawn-ccc.scope/child\n",
    );
    write(
        &t.0.join("proc/4242/cgroup"),
        "0::/user.slice/user-1000.slice/user@1000.service/tmux-spawn-aaa.scope\n",
    );
    write(&t.0.join("proc/loadavg"), "1.00 1.00 1.00 1/10 99\n");
    write(&t.0.join("proc/meminfo"), "MemAvailable: 33554432 kB\n");
}

/// A real unix socket file named `name` in the fixture's `tmux-1000` directory.
fn socket(t: &Tmp, name: &str) {
    let dir = t.0.join("tmp/tmux-1000");
    fs::create_dir_all(&dir).unwrap();
    drop(UnixListener::bind(dir.join(name)).expect("bind"));
}

fn fake_tmux(t: &Tmp, body: &str) -> PathBuf {
    let p = t.0.join("fake-tmux");
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn run(t: &Tmp, tmux: &Path) -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_uas"))
        .args(["seats", "--json", "--uid", "1000", "--cgroup-root"])
        .arg(t.0.join("cg"))
        .arg("--proc-root")
        .arg(t.0.join("proc"))
        .arg("--tmux-socket-dir")
        .arg(t.0.join("tmp"))
        .arg("--tmux-bin")
        .arg(tmux)
        .output()
        .expect("run uas");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

fn scope<'a>(v: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    v["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["scope"] == name)
        .unwrap_or_else(|| panic!("scope {name} missing"))
}

#[test]
fn seats_json_carries_the_schema() {
    let t = Tmp::new("schema");
    fixture(&t);
    socket(&t, "default");
    let v = run(&t, &fake_tmux(&t, "exit 0"));
    assert_eq!(v["schema"], "uas.seats.v1");
    assert_eq!(v["headroom"]["passes"], true);
    assert_eq!(v["summary"]["attributed"], 0);
}

#[test]
fn seats_pane_pid_maps_to_scope_and_session() {
    let t = Tmp::new("attrib");
    fixture(&t);
    socket(&t, "default");
    let v = run(
        &t,
        &fake_tmux(&t, "echo '4242 orch-lead@uas'; echo '5555 ghost@uas'"),
    );
    let aaa = scope(&v, "tmux-spawn-aaa.scope");
    assert_eq!(aaa["attributions"][0]["session"], "orch-lead@uas");
    assert!(aaa["attributions"][0]["socket"].is_null());
    assert_eq!(aaa["memory_pressure_some_avg10"], 2.5);
    assert_eq!(v["tmux"]["available"], true);
}

#[test]
fn seats_scope_without_a_pane_is_listed_unattributed() {
    let t = Tmp::new("unattrib");
    fixture(&t);
    socket(&t, "default");
    let v = run(&t, &fake_tmux(&t, "echo '4242 orch-lead@uas'"));
    let bbb = scope(&v, "tmux-spawn-bbb.scope");
    assert!(bbb["attributions"].as_array().unwrap().is_empty());
    assert_eq!(bbb["memory_current"], 1_048_576);
}

#[test]
fn seats_missing_files_are_absent_not_zero() {
    let t = Tmp::new("absent");
    fixture(&t);
    socket(&t, "default");
    let v = run(&t, &fake_tmux(&t, "exit 0"));
    let aaa = scope(&v, "tmux-spawn-aaa.scope");
    assert!(aaa["memory_peak"].is_null());
    assert!(aaa["cpu_max"].is_null());
}

#[test]
fn seats_failing_tmux_leaves_every_scope_unattributed_with_a_reason() {
    let t = Tmp::new("tmuxfail");
    fixture(&t);
    socket(&t, "default");
    for tmux in [
        t.0.join("no-such-tmux"),
        fake_tmux(&t, "echo 'no server running' >&2; exit 1"),
    ] {
        let v = run(&t, &tmux);
        assert_eq!(v["scopes"].as_array().unwrap().len(), 4);
        assert!(v["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["attributions"].as_array().unwrap().is_empty()));
        assert_eq!(v["tmux"]["available"], false);
        assert!(v["tmux"]["reason"].as_str().is_some_and(|r| !r.is_empty()));
    }
}

#[test]
fn seats_gate_fails_on_high_load() {
    let t = Tmp::new("gate");
    fixture(&t);
    write(&t.0.join("proc/loadavg"), "12.00 1.00 1.00 1/10 99\n");
    let v = run(&t, &fake_tmux(&t, "exit 0"));
    assert_eq!(v["headroom"]["passes"], false);
}

#[test]
fn seats_source_has_no_write_and_no_environ() {
    let src = include_str!("../src/seats.rs");
    let environ = ["env", "iron"].concat();
    assert!(!src.contains(&environ));
    for banned in [
        "fs::write",
        "File::create",
        ".write(true)",
        ".append(",
        ".create(",
        ".truncate(",
        "remove_file",
        "remove_dir",
        "create_dir",
        "set_permissions",
        "fs::rename",
        "fs::copy",
    ] {
        assert!(!src.contains(banned), "seats.rs uses {banned}");
    }
}

#[test]
fn seats_nested_cgroup_pid_and_two_panes_keep_every_session() {
    let t = Tmp::new("twopanes");
    fixture(&t);
    socket(&t, "default");
    let v = run(
        &t,
        &fake_tmux(
            &t,
            "echo '5002 zeta@uas'; echo '5001 alpha@uas'; echo '5001 alpha@uas'",
        ),
    );
    let ccc = scope(&v, "tmux-spawn-ccc.scope");
    let sessions: Vec<&str> = ccc["attributions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["session"].as_str().unwrap())
        .collect();
    assert_eq!(
        sessions,
        ["alpha@uas", "zeta@uas"],
        "sorted and deduplicated"
    );
}

#[test]
fn seats_non_pane_scope_is_kind_other() {
    let t = Tmp::new("other");
    fixture(&t);
    socket(&t, "default");
    let v = run(&t, &fake_tmux(&t, "exit 0"));
    let other = scope(&v, "app-other.scope");
    assert_eq!(other["kind"], "other");
    assert_eq!(scope(&v, "tmux-spawn-aaa.scope")["kind"], "tmux_pane");
}

#[test]
fn seats_every_socket_is_queried_labelled_and_a_dead_one_is_skipped() {
    let t = Tmp::new("sockets");
    fixture(&t);
    socket(&t, "default");
    socket(&t, "openrig-x");
    socket(&t, "dead");
    // A regular file in the socket directory is not a socket.
    fs::write(t.0.join("tmp/tmux-1000/not-a-socket"), "").unwrap();
    let v = run(
        &t,
        &fake_tmux(
            &t,
            r#"case "$2" in
*openrig-x) echo '5001 two@x';;
*dead) echo 'no server running on x' >&2; exit 1;;
*) echo '4242 one@default';;
esac"#,
        ),
    );
    let aaa = scope(&v, "tmux-spawn-aaa.scope");
    assert_eq!(aaa["attributions"][0]["session"], "one@default");
    assert!(aaa["attributions"][0]["socket"].is_null());
    let ccc = scope(&v, "tmux-spawn-ccc.scope");
    assert_eq!(ccc["attributions"][0]["socket"], "openrig-x");
    let socks = v["tmux"]["sockets"].as_array().unwrap();
    assert_eq!(socks.len(), 3, "the regular file is not listed");
    let dead = socks.iter().find(|s| s["socket"] == "dead").unwrap();
    assert_eq!(dead["queried"], false);
    assert!(dead["reason"]
        .as_str()
        .unwrap()
        .contains("no server running"));
    assert_eq!(v["tmux"]["available"], true);
    assert_eq!(v["summary"]["attributed"], 2);
}

#[test]
fn seats_summary_counts_empty_and_busy_unattributed() {
    let t = Tmp::new("summary");
    fixture(&t);
    socket(&t, "default");
    let v = run(&t, &fake_tmux(&t, "echo '4242 one@default'"));
    // aaa attributed, bbb empty (pids.current 0), ccc and app-other hold processes and have no pane.
    assert_eq!(v["summary"]["attributed"], 1);
    assert_eq!(v["summary"]["empty"], 1);
    assert_eq!(v["summary"]["unattributed_with_processes"], 2);
    assert_eq!(v["summary"]["unattributed_unknown"], 0);
}

#[test]
fn seats_no_socket_directory_reports_unavailable_with_a_reason() {
    let t = Tmp::new("nosock");
    fixture(&t);
    let v = run(&t, &fake_tmux(&t, "exit 0"));
    assert_eq!(v["tmux"]["available"], false);
    assert!(v["tmux"]["reason"]
        .as_str()
        .unwrap()
        .contains("no tmux sockets"));
    assert_eq!(v["scopes"].as_array().unwrap().len(), 4);
}
