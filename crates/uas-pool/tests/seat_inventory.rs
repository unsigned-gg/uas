// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Spec 0058 wave 1: the read-only seat readers, driven by fixture trees.

mod common;

use std::fs;
use std::path::Path;

use common::TempDirGuard;
use uas_domain::{CpuMax, Limit, ScopeKind};
use uas_pool::seats::{read_headroom, read_scopes, scope_of_pid, user_service_dir};

const GIB: u64 = 1024 * 1024 * 1024;

fn write(dir: &Path, file: &str, text: &str) {
    fs::create_dir_all(dir).expect("dir");
    fs::write(dir.join(file), text).expect("file");
}

fn full_scope(service: &Path, name: &str) {
    let d = service.join(name);
    write(&d, "memory.current", "1048576\n");
    write(&d, "memory.peak", "2097152\n");
    write(&d, "memory.high", "max\n");
    write(&d, "memory.max", "4294967296\n");
    write(&d, "pids.current", "7\n");
    write(&d, "pids.max", "67473\n");
    write(&d, "cpu.max", "max 100000\n");
    write(&d, "cpu.weight", "100\n");
    write(
        &d,
        "memory.pressure",
        "some avg10=1.50 avg60=0.00 avg300=0.00 total=1\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n",
    );
    write(
        &d,
        "cpu.pressure",
        "some avg10=0.25 avg60=0.00 avg300=0.00 total=1\n",
    );
}

#[test]
fn seat_scopes_are_read_with_every_field() {
    let tmp = TempDirGuard::new("seat-full");
    let service = user_service_dir(tmp.path(), 1000);
    full_scope(&service, "tmux-spawn-aaa.scope");
    full_scope(&service, "app-other.scope");
    fs::create_dir_all(service.join("dbus.service")).unwrap();
    let scopes = read_scopes(&service).unwrap();
    assert_eq!(scopes.len(), 2, "the .service directory is not a scope");
    let pane = scopes
        .iter()
        .find(|s| s.scope == "tmux-spawn-aaa.scope")
        .unwrap();
    assert_eq!(pane.kind, ScopeKind::TmuxPane);
    assert_eq!(pane.memory_current, Some(1_048_576));
    assert_eq!(pane.memory_peak, Some(2_097_152));
    assert_eq!(pane.memory_high, Some(Limit::Max));
    assert_eq!(pane.memory_max, Some(Limit::Value(4_294_967_296)));
    assert_eq!(pane.pids_current, Some(7));
    assert_eq!(pane.pids_max, Some(Limit::Value(67473)));
    assert_eq!(pane.cpu_max, CpuMax::parse("max 100000"));
    assert_eq!(pane.cpu_weight, Some(100));
    assert_eq!(pane.memory_pressure_some_avg10, Some(1.5));
    assert_eq!(pane.cpu_pressure_some_avg10, Some(0.25));
    let other = scopes
        .iter()
        .find(|s| s.scope == "app-other.scope")
        .unwrap();
    assert_eq!(other.kind, ScopeKind::Other);
}

#[test]
fn seat_missing_memory_peak_is_absent_not_zero() {
    let tmp = TempDirGuard::new("seat-peak");
    let service = user_service_dir(tmp.path(), 1000);
    full_scope(&service, "tmux-spawn-bbb.scope");
    fs::remove_file(service.join("tmux-spawn-bbb.scope/memory.peak")).unwrap();
    let scopes = read_scopes(&service).unwrap();
    assert_eq!(scopes[0].memory_peak, None);
    assert_eq!(scopes[0].memory_current, Some(1_048_576));
}

#[test]
fn seat_garbage_value_is_absent_not_zero() {
    let tmp = TempDirGuard::new("seat-garbage");
    let service = user_service_dir(tmp.path(), 1000);
    full_scope(&service, "tmux-spawn-ccc.scope");
    write(
        &service.join("tmux-spawn-ccc.scope"),
        "pids.current",
        "oops\n",
    );
    let scopes = read_scopes(&service).unwrap();
    assert_eq!(scopes[0].pids_current, None);
}

#[test]
fn seat_pane_pid_maps_to_its_scope() {
    let tmp = TempDirGuard::new("seat-pid");
    let proc_root = tmp.path();
    write(
        &proc_root.join("4242"),
        "cgroup",
        "0::/user.slice/user-1000.slice/user@1000.service/tmux-spawn-ddd.scope\n",
    );
    write(
        &proc_root.join("4243"),
        "cgroup",
        "0::/user.slice/user-1000.slice/user@1000.service/tmux-spawn-ddd.scope/sub\n",
    );
    write(
        &proc_root.join("4244"),
        "cgroup",
        "0::/system.slice/ssh.service\n",
    );
    assert_eq!(
        scope_of_pid(proc_root, 4242).as_deref(),
        Some("tmux-spawn-ddd.scope")
    );
    assert_eq!(
        scope_of_pid(proc_root, 4243).as_deref(),
        Some("tmux-spawn-ddd.scope")
    );
    assert_eq!(scope_of_pid(proc_root, 4244), None);
    assert_eq!(scope_of_pid(proc_root, 9999), None);
}

#[test]
fn seat_headroom_reads_proc_files_and_fails_at_the_edges() {
    let tmp = TempDirGuard::new("seat-headroom");
    let p = tmp.path();
    write(p, "loadavg", "12.00 5.00 3.00 1/100 999\n");
    write(p, "meminfo", "MemTotal: 99 kB\nMemAvailable: 33554432 kB\n");
    let g = read_headroom(p, 12.0, 16 * GIB);
    assert_eq!(g.load_1m, Some(12.0));
    assert_eq!(g.mem_available_bytes, Some(32 * GIB));
    assert!(!g.passes, "load 12.0 fails");
    write(p, "loadavg", "1.00 5.00 3.00 1/100 999\n");
    write(p, "meminfo", "MemAvailable: 16675635 kB\n"); // about 15.9 GiB
    assert!(!read_headroom(p, 12.0, 16 * GIB).passes, "15.9 GiB fails");
    write(p, "meminfo", "MemAvailable: 16777216 kB\n");
    assert!(read_headroom(p, 12.0, 16 * GIB).passes);
    fs::remove_file(p.join("meminfo")).unwrap();
    let g = read_headroom(p, 12.0, 16 * GIB);
    assert_eq!(g.mem_available_bytes, None);
    assert!(!g.passes);
}

#[test]
fn seat_readers_source_has_no_write_and_no_environ() {
    let src = include_str!("../src/seats.rs");
    let environ = ["env", "iron"].concat();
    assert!(!src.contains(&environ), "no per-process environment path");
    for banned in [
        "fs::write",
        "File::create",
        "OpenOptions",
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
