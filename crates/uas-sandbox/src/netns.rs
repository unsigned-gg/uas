//! A private network namespace for a worker.
//!
//! Spec 0011. `isolate` installs a `pre_exec` hook that runs in the child before
//! the launcher's Landlock hook. The child gets a namespace with only `lo`, which
//! it brings up; with no other interface and no route, outbound networking is
//! denied by the namespace itself.

use std::ffi::CStr;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::os::unix::process::CommandExt;
use std::process::Command;

use rustix::fs::{open, Mode, OFlags};
use rustix::io::Errno;
use rustix::ioctl::{ioctl, Opcode, Updater};
use rustix::process::{getegid, geteuid};
use rustix::thread::{unshare_unsafe, UnshareFlags};

/// `ENONET`. The child reports a failed namespace with this errno; `exec` never
/// produces it, so the parent can tell it from every other spawn failure.
pub const NETNS_REFUSED_ERRNO: i32 = 64;

const SIOCGIFFLAGS: Opcode = 0x8913;
const SIOCSIFFLAGS: Opcode = 0x8914;
const IFF_UP: i16 = 0x1;

/// `struct ifreq` as the two flag ioctls read it: 16 bytes of interface name,
/// then the union, whose first member for these calls is the `short` flags.
/// 40 bytes on 64-bit Linux.
#[repr(C)]
struct IfReq {
    name: [u8; 16],
    flags: i16,
    _pad: [u8; 22],
}

/// Install the namespace hook on `command`. Call it before `launcher::jail`, so
/// the namespace exists when Landlock is applied.
pub fn isolate(command: &mut Command) -> &mut Command {
    // SAFETY: the closure runs in the child after fork and before exec. It makes
    // only syscalls through rustix and std's socket wrapper, formats into stack
    // buffers, opens procfs files by CStr literal, and returns raw errnos.
    // Nothing on the path allocates, so it is safe in a child forked from a
    // multi-threaded parent.
    #[allow(unsafe_code)]
    unsafe {
        command.pre_exec(enter_private_netns)
    }
}

/// Runs in the child. Spec 0011, "Child sequence".
fn enter_private_netns() -> io::Result<()> {
    let refused = || io::Error::from_raw_os_error(NETNS_REFUSED_ERRNO);
    // SAFETY: unshare changes only this child's namespaces. The child has one
    // thread after fork and execs next, so no other thread observes the change.
    #[allow(unsafe_code)]
    let direct = unsafe { unshare_unsafe(UnshareFlags::NEWNET) };
    match direct {
        // CAP_SYS_ADMIN got the namespace; CAP_NET_ADMIN may still be missing
        // for the lo ioctl. If it is, fall through to a user namespace, which
        // grants both over a fresh namespace.
        Ok(()) if loopback_up().is_ok() => return Ok(()),
        Ok(()) | Err(Errno::PERM) => {}
        Err(_) => return Err(refused()),
    }
    // The single-line maps may only name the writer's effective ids.
    let uid = geteuid().as_raw();
    let gid = getegid().as_raw();
    // SAFETY: as above; the user namespace is for this child alone.
    #[allow(unsafe_code)]
    let both = unsafe { unshare_unsafe(UnshareFlags::NEWUSER | UnshareFlags::NEWNET) };
    both.map_err(|_| refused())?;
    let mut line = [0u8; 32];
    write_proc(c"/proc/self/setgroups", b"deny").map_err(|_| refused())?;
    write_proc(c"/proc/self/gid_map", identity_map(&mut line, gid)).map_err(|_| refused())?;
    write_proc(c"/proc/self/uid_map", identity_map(&mut line, uid)).map_err(|_| refused())?;
    loopback_up().map_err(|_| refused())
}

/// Write `bytes` to a procfs file in one call. No allocation.
fn write_proc(path: &CStr, bytes: &[u8]) -> rustix::io::Result<()> {
    let fd = open(path, OFlags::WRONLY | OFlags::CLOEXEC, Mode::empty())?;
    let written = rustix::io::write(&fd, bytes)?;
    if written == bytes.len() {
        Ok(())
    } else {
        Err(Errno::IO)
    }
}

/// Format `<id> <id> 1\n` into `buf` and return the used slice. No allocation.
fn identity_map(buf: &mut [u8; 32], id: u32) -> &[u8] {
    let mut digits = [0u8; 10];
    let mut rest = id;
    let mut count = 0;
    loop {
        digits[count] = b'0' + (rest % 10) as u8;
        count += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    let mut pos = 0;
    for _ in 0..2 {
        for i in (0..count).rev() {
            buf[pos] = digits[i];
            pos += 1;
        }
        buf[pos] = b' ';
        pos += 1;
    }
    buf[pos] = b'1';
    buf[pos + 1] = b'\n';
    &buf[..pos + 2]
}

/// Set `IFF_UP` on `lo`. The kernel then assigns `127.0.0.1/8`.
fn loopback_up() -> io::Result<()> {
    let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    let mut req = IfReq {
        name: *b"lo\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        flags: 0,
        _pad: [0; 22],
    };
    // SAFETY: both opcodes take a `struct ifreq` by pointer and touch only its
    // name and flags fields. `IfReq` has that layout and size, and `req`
    // outlives both calls.
    #[allow(unsafe_code)]
    unsafe {
        ioctl(&socket, Updater::<SIOCGIFFLAGS, IfReq>::new(&mut req))?;
        req.flags |= IFF_UP;
        ioctl(&socket, Updater::<SIOCSIFFLAGS, IfReq>::new(&mut req))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_map_formats_without_allocation() {
        let mut buf = [0u8; 32];
        assert_eq!(identity_map(&mut buf, 0), b"0 0 1\n");
        assert_eq!(identity_map(&mut buf, 1000), b"1000 1000 1\n");
        assert_eq!(
            identity_map(&mut buf, u32::MAX),
            b"4294967295 4294967295 1\n"
        );
    }

    #[test]
    fn ifreq_has_the_kernel_layout() {
        assert_eq!(std::mem::size_of::<IfReq>(), 40);
        assert_eq!(std::mem::offset_of!(IfReq, flags), 16);
    }
}
