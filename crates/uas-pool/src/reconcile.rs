// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Restart reconciliation of slot remains (spec 0040 D2, D4, D6).
//!
//! A starting pool scans `<common_git_dir>/worktrees/*` and reclaims what a dead
//! pool left behind. Ownership is the advisory lock on the slot marker, never a
//! pid: a marker whose lock can be taken is an orphan, a held lock is a live
//! pool. The filesystem half of the reclaim lives here; the cgroup half is
//! `freezer::reclaim_cgroup`.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::OFlags;
use uas_domain::ReconcileReport;

use crate::error::PoolError;
use crate::freezer;
use crate::marker::{SlotMarker, MARKER_NAME, MARKER_TMP_NAME};

/// The two roots a reconcile pass may touch (spec 0040 D2, D6).
#[derive(Debug, Clone)]
pub struct ReconcileRoots {
    /// Common git directory whose `worktrees/` holds the slot control directories.
    pub common_git_dir: PathBuf,
    /// Directory every slot `worktree_dir` must live under.
    pub worktree_root: PathBuf,
    /// Cgroup directory holding the `slot-<id>` cgroups. `None` skips the cgroup
    /// step. Production callers keep it `None` until the worktree is provisioned
    /// before the cgroup (plan P7).
    pub cgroup_root: Option<PathBuf>,
}

/// What one scanned entry came to.
enum Outcome {
    Reclaimed,
    Kept,
    /// Not a slot remain of ours; nothing touched, nothing counted.
    Ignored,
}

/// Reclaim the remains of dead pools under `roots` and count what happened.
///
/// Never fails: a refused or failed entry is logged at `warn`, counted in
/// `failed`, and retried at the next start (spec 0040 D4).
pub fn reconcile(roots: &ReconcileRoots) -> ReconcileReport {
    let mut report = ReconcileReport::default();
    let worktrees = roots.common_git_dir.join("worktrees");

    let mut names: Vec<OsString> = match fs::read_dir(&worktrees) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return report,
        Err(e) => {
            tracing::warn!("reconcile: cannot list {}: {e}", worktrees.display());
            report.failed += 1;
            return report;
        }
    };
    names.sort();

    for name in names {
        match reconcile_entry(roots, &worktrees.join(&name), &name) {
            Ok(Outcome::Reclaimed) => report.reclaimed += 1,
            Ok(Outcome::Kept) => report.kept += 1,
            Ok(Outcome::Ignored) => {}
            Err(e) => {
                tracing::warn!("reconcile: {e}");
                report.failed += 1;
            }
        }
    }
    report
}

fn refused(slot: &OsString, reason: impl Into<String>) -> PoolError {
    PoolError::ReconcileRefused {
        slot: slot.to_string_lossy().into_owned(),
        reason: reason.into(),
    }
}

fn step_failed(slot: &OsString, step: &str, e: io::Error) -> PoolError {
    PoolError::Gitoxide(format!(
        "reconcile slot {}: step {step} failed: {e}",
        slot.to_string_lossy()
    ))
}

fn reconcile_entry(
    roots: &ReconcileRoots,
    control: &Path,
    name: &OsString,
) -> Result<Outcome, PoolError> {
    let meta = fs::symlink_metadata(control).map_err(|e| step_failed(name, "stat control", e))?;
    if meta.file_type().is_symlink() {
        return Err(refused(name, "control directory is a symlink"));
    }
    if !meta.is_dir() {
        return Ok(Outcome::Ignored);
    }

    let marker_path = control.join(MARKER_NAME);
    match fs::symlink_metadata(&marker_path) {
        Ok(m) if m.file_type().is_symlink() => Err(refused(name, "marker is a symlink")),
        Ok(m) if !m.is_file() => Err(refused(name, "marker is not a regular file")),
        Ok(_) => reconcile_marker(roots, control, &marker_path, name),
        Err(e) if e.kind() == io::ErrorKind::NotFound => reconcile_unmarked(control, name),
        Err(e) => Err(step_failed(name, "stat marker", e)),
    }
}

/// Open `path` without following a symlink and try the exclusive lock.
/// `Ok(None)` means another open file description holds it.
fn try_own(path: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn reconcile_marker(
    roots: &ReconcileRoots,
    control: &Path,
    marker_path: &Path,
    name: &OsString,
) -> Result<Outcome, PoolError> {
    reconcile_marker_with(roots, control, marker_path, name, &|| {})
}

/// `after_lock` runs between taking the lock and the inode check; tests use it
/// to replace the marker at that point.
fn reconcile_marker_with(
    roots: &ReconcileRoots,
    control: &Path,
    marker_path: &Path,
    name: &OsString,
    after_lock: &dyn Fn(),
) -> Result<Outcome, PoolError> {
    let mut file = match try_own(marker_path) {
        Ok(Some(f)) => f,
        Ok(None) => return Ok(Outcome::Kept),
        Err(e) => return Err(step_failed(name, "lock marker", e)),
    };

    after_lock();
    // The lock is on an inode. If the path now names a different inode, a new
    // marker replaced the one we locked and its owner is live.
    let locked = file
        .metadata()
        .map_err(|e| step_failed(name, "stat locked marker", e))?;
    match fs::symlink_metadata(marker_path) {
        Ok(now) if now.dev() == locked.dev() && now.ino() == locked.ino() => {}
        _ => return Ok(Outcome::Kept),
    }

    let mut body = String::new();
    file.read_to_string(&mut body)
        .map_err(|e| step_failed(name, "read marker", e))?;
    let marker: SlotMarker = serde_json::from_str(&body)
        .map_err(|e| refused(name, format!("unreadable marker: {e}")))?;

    validate(roots, name, &marker)?;
    reclaim_cgroup(roots, &marker)?;
    reclaim_filesystem(roots, control, name, &marker)?;
    // `file` drops here, after the control directory is gone.
    drop(file);
    Ok(Outcome::Reclaimed)
}

/// A control directory with no published marker.
fn reconcile_unmarked(control: &Path, name: &OsString) -> Result<Outcome, PoolError> {
    let tmp = control.join(MARKER_TMP_NAME);
    match fs::symlink_metadata(&tmp) {
        Ok(m) if m.file_type().is_symlink() => Err(refused(name, "tmp marker is a symlink")),
        Ok(_) => match try_own(&tmp) {
            Ok(None) => Ok(Outcome::Kept),
            Ok(Some(_held)) => {
                fs::remove_file(&tmp).map_err(|e| step_failed(name, "remove tmp marker", e))?;
                // Debris only if nothing else is in there; a non-empty dir stays.
                let _ = fs::remove_dir(control);
                Ok(Outcome::Reclaimed)
            }
            Err(e) => Err(step_failed(name, "lock tmp marker", e)),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // `provision` publishes the marker before it writes anything else, so
            // an empty control directory is a failed marker creation. A directory
            // with content is not ours (for example a user's own worktree).
            let empty = fs::read_dir(control)
                .map_err(|e| step_failed(name, "list control", e))?
                .next()
                .is_none();
            if !empty {
                return Ok(Outcome::Ignored);
            }
            fs::remove_dir(control).map_err(|e| step_failed(name, "remove empty control", e))?;
            Ok(Outcome::Reclaimed)
        }
        Err(e) => Err(step_failed(name, "stat tmp marker", e)),
    }
}

/// D6: the marker must describe this slot and name only paths under the roots.
fn validate(roots: &ReconcileRoots, name: &OsString, marker: &SlotMarker) -> Result<(), PoolError> {
    if name.to_string_lossy() != marker.slot_id {
        return Err(refused(name, "marker slot id does not match its directory"));
    }
    let dir = &marker.worktree_dir;
    if dir
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(refused(name, "worktree_dir has dot components"));
    }
    match fs::symlink_metadata(&roots.worktree_root) {
        Ok(m) if m.file_type().is_symlink() => {
            return Err(refused(name, "worktree root is a symlink"));
        }
        Ok(_) => {}
        Err(e) => return Err(step_failed(name, "stat worktree root", e)),
    }
    let rel = dir
        .strip_prefix(&roots.worktree_root)
        .map_err(|_| refused(name, "worktree_dir is outside the worktree root"))?;
    if rel.as_os_str().is_empty() {
        return Err(refused(name, "worktree_dir is the worktree root itself"));
    }
    // No component below the root may be a symlink.
    let mut walk = roots.worktree_root.clone();
    for part in rel.components() {
        walk.push(part);
        match fs::symlink_metadata(&walk) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(refused(name, "worktree_dir path contains a symlink"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(step_failed(name, "stat worktree_dir", e)),
        }
    }
    branch_ref_name(name, marker)?;
    Ok(())
}

fn branch_ref_name(name: &OsString, marker: &SlotMarker) -> Result<gix::refs::FullName, PoolError> {
    gix::refs::FullName::try_from(format!("refs/heads/{}", marker.branch))
        .map_err(|e| refused(name, format!("invalid branch name: {e}")))
}

/// Kill and remove the slot cgroup (spec 0040 D3). It runs before any file is
/// removed. A timeout or refusal fails the entry and leaves the marker for the
/// next start (D4).
fn reclaim_cgroup(roots: &ReconcileRoots, marker: &SlotMarker) -> Result<(), PoolError> {
    let Some(root) = &roots.cgroup_root else {
        return Ok(());
    };
    let mut parts = Path::new(&marker.cgroup).components();
    let (Some(Component::Normal(leaf)), None) = (parts.next(), parts.next()) else {
        return Err(PoolError::ReconcileRefused {
            slot: marker.slot_id.clone(),
            reason: "marker cgroup is not a single path component".to_string(),
        });
    };
    freezer::reclaim_cgroup(&root.join(leaf), freezer::DEFAULT_RECLAIM_TIMEOUT)?;
    Ok(())
}

/// Worktree directory, branch ref, then the control directory last. The marker
/// lives in the control directory, so a failed earlier step leaves the marker in
/// place and the entry is retried at the next start (spec 0040 D4).
///
/// This deliberately deviates from the order in spec 0040 D3, which removes the
/// control directory before the branch ref. It requests an amendment to D3 and
/// REQ-0040-03.
fn reclaim_filesystem(
    roots: &ReconcileRoots,
    control: &Path,
    name: &OsString,
    marker: &SlotMarker,
) -> Result<(), PoolError> {
    match fs::remove_dir_all(&marker.worktree_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(step_failed(name, "remove worktree dir", e)),
    }

    let branch = branch_ref_name(name, marker)?;
    let repo = gix::open_opts(&roots.common_git_dir, crate::worktree::repo_open_opts())
        .map_err(|e| PoolError::Gitoxide(format!("reconcile: open common git dir: {e}")))?;
    match repo.try_find_reference(branch.as_bstr()) {
        Ok(Some(reference)) => reference.delete().map_err(|e| {
            PoolError::Gitoxide(format!(
                "reconcile slot {}: step delete branch failed: {e}",
                marker.slot_id
            ))
        })?,
        Ok(None) => {}
        Err(e) => {
            return Err(PoolError::Gitoxide(format!(
                "reconcile slot {}: step find branch failed: {e}",
                marker.slot_id
            )))
        }
    }

    match fs::remove_dir_all(control) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(step_failed(name, "remove control dir", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_keeps_a_marker_replaced_after_the_lock() {
        let tmp = std::env::temp_dir().join(format!("uas-reconcile-inode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let root = tmp.join("root");
        let wt = root.join("wt");
        let control = tmp.join("worktrees").join("slot-x");
        fs::create_dir_all(&wt).unwrap();
        fs::create_dir_all(&control).unwrap();
        fs::write(wt.join("keep"), b"x").unwrap();
        let marker = SlotMarker {
            slot_id: "slot-x".into(),
            worktree_dir: wt.clone(),
            cgroup: "slot-slot-x".into(),
            branch: "uas/x".into(),
            pool_pid: 1,
            provisioned_at: 1,
        };
        let body = serde_json::to_vec(&marker).unwrap();
        let path = control.join(MARKER_NAME);
        fs::write(&path, &body).unwrap();
        let roots = ReconcileRoots {
            common_git_dir: tmp.clone(),
            worktree_root: root,
            cgroup_root: None,
        };

        // A new owner publishes a fresh marker (plain rename) after we lock the old inode.
        let fresh = control.join("fresh");
        let swap = || {
            fs::write(&fresh, &body).unwrap();
            fs::rename(&fresh, &path).unwrap();
        };
        let name = OsString::from("slot-x");
        let outcome = reconcile_marker_with(&roots, &control, &path, &name, &swap).unwrap();
        assert!(matches!(outcome, Outcome::Kept));
        assert!(wt.join("keep").is_file(), "worktree must stay");
        assert!(path.is_file(), "new marker must stay");
        let _ = fs::remove_dir_all(&tmp);
    }
}
