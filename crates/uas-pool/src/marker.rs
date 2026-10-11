//! The locked slot marker (spec 0040 D1, REQ-0040-01, REQ-0040-12).
//!
//! `provision` writes `uas-pool.json.tmp` with `O_CREAT | O_EXCL | O_CLOEXEC`,
//! takes an exclusive advisory lock on it, writes the record, then renames it
//! to `uas-pool.json` with `RENAME_NOREPLACE` so a live marker is never
//! overwritten. The lock follows the inode across the rename and lives as long
//! as the returned [`File`] stays open (the kernel drops it on process death).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rustix::fs::{renameat_with, RenameFlags, CWD};
use serde::{Deserialize, Serialize};

/// File name of a published marker inside the worktree control directory.
pub(crate) const MARKER_NAME: &str = "uas-pool.json";
/// File name of the marker while it is being written.
pub(crate) const MARKER_TMP_NAME: &str = "uas-pool.json.tmp";

/// The record a pool writes for each slot it provisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SlotMarker {
    pub slot_id: String,
    pub worktree_dir: PathBuf,
    /// Relative cgroup name, `slot-<slot_id>` (spec 0015 D3).
    pub cgroup: String,
    pub branch: String,
    pub pool_pid: u32,
    /// Seconds since the Unix epoch.
    pub provisioned_at: u64,
}

/// Create, lock, fill and publish the marker in `control_dir`.
///
/// Returns the open, locked file. A failure leaves no `.tmp` behind and never
/// touches an existing `uas-pool.json`.
pub(crate) fn create_locked(control_dir: &Path, marker: &SlotMarker) -> io::Result<File> {
    let tmp = control_dir.join(MARKER_TMP_NAME);
    let dest = control_dir.join(MARKER_NAME);

    // `std` opens with O_CLOEXEC; `create_new` adds O_EXCL.
    let mut file = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&tmp)?;

    let published = (|| {
        file.try_lock()
            .map_err(|e| io::Error::other(format!("lock marker: {e}")))?;
        let body = serde_json::to_vec(marker).map_err(io::Error::other)?;
        file.write_all(&body)?;
        file.flush()?;
        renameat_with(CWD, &tmp, CWD, &dest, RenameFlags::NOREPLACE).map_err(io::Error::from)
    })();

    if let Err(e) = published {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(file)
}
