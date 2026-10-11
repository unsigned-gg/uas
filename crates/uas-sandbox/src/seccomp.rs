//! The seccomp-BPF deny list for slot workers (spec 0043 D2, D3, D4, D5).
//!
//! Two programs, because a `seccompiler` filter has one match verdict: program A
//! returns `EPERM` for the D2 list and its `clone` and `socket` argument rules;
//! program B returns `ENOSYS` for `clone3` alone (its flags sit in a user-space
//! struct the filter cannot read, and glibc falls back to `clone`). Both allow
//! everything else. A syscall from a non-native architecture kills the process,
//! and on `x86_64` so does an x32 syscall number (bit 30 set), which carries the
//! native audit architecture and would otherwise miss every deny entry.
//!
//! [`build`] allocates and runs in the parent. [`install`] runs in the child's
//! `pre_exec`, applies both programs to the calling thread only (no `TSYNC`) and
//! does not allocate on success. Any failure becomes [`JAIL_REFUSED_ERRNO`].

use std::collections::BTreeMap;
use std::io;

use seccompiler::{
    apply_filter, BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition,
    SeccompFilter, SeccompRule,
};

use crate::error::SandboxError;
use crate::launcher::JAIL_REFUSED_ERRNO;

/// Syscalls denied with `EPERM`, by D2 name and number. Not to be reordered or
/// extended without a spec 0043 amendment. D2 lists `umount` and `umount2`; the
/// `x86_64` ABI has only `umount2`, so the `umount` entry maps to that number.
const DENY: &[(&str, libc::c_long)] = &[
    ("ptrace", libc::SYS_ptrace),
    ("process_vm_readv", libc::SYS_process_vm_readv),
    ("process_vm_writev", libc::SYS_process_vm_writev),
    ("kcmp", libc::SYS_kcmp),
    ("keyctl", libc::SYS_keyctl),
    ("add_key", libc::SYS_add_key),
    ("request_key", libc::SYS_request_key),
    ("bpf", libc::SYS_bpf),
    ("perf_event_open", libc::SYS_perf_event_open),
    ("mount", libc::SYS_mount),
    ("umount", libc::SYS_umount2),
    ("umount2", libc::SYS_umount2),
    ("pivot_root", libc::SYS_pivot_root),
    ("chroot", libc::SYS_chroot),
    ("open_by_handle_at", libc::SYS_open_by_handle_at),
    ("unshare", libc::SYS_unshare),
    ("setns", libc::SYS_setns),
    ("setgroups", libc::SYS_setgroups),
    ("io_uring_setup", libc::SYS_io_uring_setup),
    ("userfaultfd", libc::SYS_userfaultfd),
    ("kexec_load", libc::SYS_kexec_load),
    ("kexec_file_load", libc::SYS_kexec_file_load),
    ("init_module", libc::SYS_init_module),
    ("finit_module", libc::SYS_finit_module),
    ("delete_module", libc::SYS_delete_module),
    ("reboot", libc::SYS_reboot),
    ("swapon", libc::SYS_swapon),
    ("swapoff", libc::SYS_swapoff),
    ("acct", libc::SYS_acct),
    ("personality", libc::SYS_personality),
];

/// `clone3` gets `ENOSYS`, not `EPERM` (D2).
const ENOSYS_CALL: libc::c_long = libc::SYS_clone3;

/// The two programs of D2, built in the parent and installed in the child.
#[derive(Debug)]
pub struct Programs {
    errno: BpfProgram,
    enosys: BpfProgram,
}

impl Programs {
    /// The `EPERM` program (deny list plus argument rules and the x32 check).
    pub fn eperm_program(&self) -> &BpfProgram {
        &self.errno
    }

    /// The `ENOSYS` program (`clone3` only).
    pub fn enosys_program(&self) -> &BpfProgram {
        &self.enosys
    }
}

/// Build both programs for the build target's architecture. Allocates; call it in
/// the parent. Any failure is `JailRefused` (D4).
pub fn build() -> Result<Programs, SandboxError> {
    build_inner().map_err(|_| SandboxError::JailRefused)
}

fn build_inner() -> Result<Programs, seccompiler::Error> {
    let arch = std::env::consts::ARCH.try_into()?;

    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    for (_, nr) in DENY {
        rules.insert(*nr, Vec::new());
    }
    // `clone` with CLONE_NEWUSER in its flags argument (arg 0, 64 bits wide).
    let newuser = libc::CLONE_NEWUSER as u64;
    rules.insert(
        libc::SYS_clone,
        vec![SeccompRule::new(vec![SeccompCondition::new(
            0,
            SeccompCmpArgLen::Qword,
            SeccompCmpOp::MaskedEq(newuser),
            newuser,
        )?])?],
    );
    // `socket` with domain AF_PACKET, or type SOCK_RAW. SOCK_CLOEXEC and
    // SOCK_NONBLOCK are OR-ed into the type, so the type compare is masked.
    let raw = libc::SOCK_RAW as u64;
    rules.insert(
        libc::SYS_socket,
        vec![
            SeccompRule::new(vec![SeccompCondition::new(
                0,
                SeccompCmpArgLen::Dword,
                SeccompCmpOp::Eq,
                libc::AF_PACKET as u64,
            )?])?,
            SeccompRule::new(vec![SeccompCondition::new(
                1,
                SeccompCmpArgLen::Dword,
                SeccompCmpOp::MaskedEq(0xf),
                raw,
            )?])?,
        ],
    );
    let eperm = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EPERM as u32),
        arch,
    )?;
    let mut errno: BpfProgram = eperm.try_into()?;
    reject_x32(&mut errno);

    let mut only_clone3: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    only_clone3.insert(ENOSYS_CALL, Vec::new());
    let enosys: BpfProgram = SeccompFilter::new(
        only_clone3,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::ENOSYS as u32),
        arch,
    )?
    .try_into()?;

    Ok(Programs { errno, enosys })
}

/// Number of instructions `seccompiler` emits for its architecture check:
/// load arch, compare, kill.
const ARCH_CHECK_LEN: usize = 3;
const BPF_LD_W_ABS: u16 = 0x20;
const BPF_JSET_K: u16 = 0x45;
const BPF_RET_K: u16 = 0x06;
const X32_SYSCALL_BIT: u32 = 0x4000_0000;

/// On `x86_64`, kill any syscall number with bit 30 set (the x32 ABI). The three
/// instructions go right after `seccompiler`'s architecture check, which jumps
/// forward by a relative offset only, so nothing else needs fixing up. The rest of
/// the program reloads the syscall number itself.
fn reject_x32(program: &mut BpfProgram) {
    if std::env::consts::ARCH != "x86_64" {
        return;
    }
    let insn = |code, jt, jf, k| seccompiler::sock_filter { code, jt, jf, k };
    let check = [
        insn(BPF_LD_W_ABS, 0, 0, 0), // A = seccomp_data.nr
        insn(BPF_JSET_K, 0, 1, X32_SYSCALL_BIT),
        insn(BPF_RET_K, 0, 0, libc::SECCOMP_RET_KILL_PROCESS),
    ];
    program.splice(ARCH_CHECK_LEN..ARCH_CHECK_LEN, check);
}

/// Apply both programs to the calling thread, without `TSYNC` (D5). Runs in the
/// child's `pre_exec`. Sets `no_new_privs` itself, which Landlock already did.
pub fn install(programs: &Programs) -> io::Result<()> {
    let refused = |_| io::Error::from_raw_os_error(JAIL_REFUSED_ERRNO);
    apply_filter(&programs.errno).map_err(refused)?;
    apply_filter(&programs.enosys).map_err(refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D2, copied from the spec, not from `DENY`. `umount` has no `x86_64` number
    /// of its own and maps to `umount2`'s.
    const D2_NAMES: &[&str] = &[
        "ptrace",
        "process_vm_readv",
        "process_vm_writev",
        "kcmp",
        "keyctl",
        "add_key",
        "request_key",
        "bpf",
        "perf_event_open",
        "mount",
        "umount",
        "umount2",
        "pivot_root",
        "chroot",
        "open_by_handle_at",
        "unshare",
        "setns",
        "setgroups",
        "io_uring_setup",
        "userfaultfd",
        "kexec_load",
        "kexec_file_load",
        "init_module",
        "finit_module",
        "delete_module",
        "reboot",
        "swapon",
        "swapoff",
        "acct",
        "personality",
    ];

    /// Syscall numbers the program compares with `JEQ k`.
    fn compared(program: &BpfProgram) -> Vec<u32> {
        program
            .iter()
            .filter(|i| i.code == 0x15)
            .map(|i| i.k)
            .collect()
    }

    #[test]
    fn seccomp_programs_build_for_native_arch() {
        let p = build().expect("programs build");
        assert!(!p.eperm_program().is_empty());
        assert!(!p.enosys_program().is_empty());
    }

    #[test]
    fn seccomp_deny_list_names_every_d2_entry() {
        let names: Vec<&str> = DENY.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, D2_NAMES);
    }

    #[test]
    fn seccomp_program_compares_every_deny_number() {
        let p = build().expect("programs build");
        let in_program = compared(p.eperm_program());
        for (name, nr) in DENY {
            assert!(in_program.contains(&(*nr as u32)), "{name} missing");
        }
        for nr in [libc::SYS_clone, libc::SYS_socket] {
            assert!(in_program.contains(&(nr as u32)), "argument-rule call {nr}");
        }
        assert!(!in_program.contains(&(ENOSYS_CALL as u32)));
        assert!(compared(p.enosys_program()).contains(&(ENOSYS_CALL as u32)));
    }

    #[test]
    fn seccomp_foreign_arch_check_is_first() {
        let p = build().expect("programs build");
        for prog in [p.eperm_program(), p.enosys_program()] {
            // load arch (offset 4), compare, return KILL_PROCESS
            assert_eq!((prog[0].code, prog[0].k), (BPF_LD_W_ABS, 4));
            assert_eq!(prog[1].code, 0x15);
            assert_eq!(prog[2].code, BPF_RET_K);
            assert_eq!(prog[2].k, libc::SECCOMP_RET_KILL_PROCESS);
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn seccomp_x32_numbers_are_killed() {
        let p = build().expect("programs build");
        let prog = p.eperm_program();
        assert_eq!(prog[3].code, BPF_LD_W_ABS);
        assert_eq!((prog[4].code, prog[4].k), (BPF_JSET_K, X32_SYSCALL_BIT));
        assert_eq!(prog[5].code, BPF_RET_K);
        assert_eq!(prog[5].k, libc::SECCOMP_RET_KILL_PROCESS);
    }
}
