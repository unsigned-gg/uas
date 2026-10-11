//! In-process Gitoxide (`gix`) warm worktree provisioning and teardown.
//!
//! CER-2724: Provisions ephemeral worktrees directly via the `gix` repository API,
//! eliminating external `git` CLI invocations and avoiding lock collisions
//! on `.git/config` via shared ODB references.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use uas_domain::{SlotId, WorktreeRef};

use crate::error::PoolError;
use crate::marker::{self, SlotMarker};

/// An ephemeral git worktree managed directly via in-process Gitoxide (`gix`).
///
/// Shares the object database (ODB) with the base repository without
/// duplicating storage on host ceres. Eliminates lock collisions on
/// `.git/config` by storing worktree-specific state and loose branch refs
/// independently.
#[derive(Debug)]
pub struct WarmWorktree {
    slot_id: SlotId,
    base_repo_path: PathBuf,
    worktree_dir: PathBuf,
    worktree_git_dir: PathBuf,
    branch_name: String,
    worktree_ref: WorktreeRef,
    torn_down: bool,
    /// Open, locked slot marker (spec 0040 D1); dropped last in teardown.
    marker: Option<File>,
}

pub(crate) fn repo_open_opts() -> gix::open::Options {
    let mut opts = gix::open::Options::default();
    opts.permissions.env.git_prefix = gix::sec::Permission::Deny;
    opts
}

impl WarmWorktree {
    /// Provision a new linked worktree directly from `base_repo_path`.
    ///
    /// Resolves HEAD commit in the base repo, creates a branch `branch_name`
    /// pointing to the commit, writes the gitdir pointer and commondir control
    /// files, and performs an in-process checkout into `worktree_dir`.
    pub fn provision(
        base_repo_path: impl AsRef<Path>,
        slot_id: SlotId,
        worktree_dir: impl AsRef<Path>,
        branch_name: impl Into<String>,
    ) -> Result<Self, PoolError> {
        let base_repo_path = base_repo_path.as_ref().to_path_buf();
        let worktree_dir = worktree_dir.as_ref().to_path_buf();
        let branch_name = branch_name.into();

        let mut base_repo = gix::open_opts(&base_repo_path, repo_open_opts())
            .map_err(|e| PoolError::Gitoxide(format!("failed to open base repository: {e}")))?;
        let _ = base_repo.committer_or_set_fallback("uas-pool", "uas-pool@localhost");

        let mut head = base_repo.head().map_err(|e| {
            PoolError::Gitoxide(format!("failed to resolve HEAD in base repository: {e}"))
        })?;
        let commit_obj = head
            .peel_to_object()
            .map_err(|e| PoolError::Gitoxide(format!("failed to peel HEAD to object: {e}")))?;
        let commit = commit_obj.into_commit();
        let commit_id = commit.id();
        let tree_id = commit
            .tree_id()
            .map_err(|e| PoolError::Gitoxide(format!("failed to get tree from commit: {e}")))?;

        let common_git_dir = base_repo.common_dir().to_path_buf();
        let worktree_git_dir = common_git_dir.join("worktrees").join(slot_id.as_str());

        std::fs::create_dir_all(&worktree_git_dir)
            .map_err(|e| PoolError::Gitoxide(format!("failed to create worktree git dir: {e}")))?;

        // Spec 0040 D1: the locked marker exists before the branch ref and the
        // worktree directory, so a crash after this point is reclaimable.
        let provisioned_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let marker_file = marker::create_locked(
            &worktree_git_dir,
            &SlotMarker {
                slot_id: slot_id.as_str().to_string(),
                worktree_dir: worktree_dir.clone(),
                cgroup: format!("slot-{}", slot_id.as_str()),
                branch: branch_name.clone(),
                pool_pid: std::process::id(),
                provisioned_at,
            },
        )
        .map_err(|e| PoolError::Gitoxide(format!("failed to write slot marker: {e}")))?;
        std::fs::create_dir_all(&worktree_dir)
            .map_err(|e| PoolError::Gitoxide(format!("failed to create worktree dir: {e}")))?;

        let branch_ref_name = format!("refs/heads/{}", branch_name);
        base_repo
            .reference(
                branch_ref_name.as_str(),
                commit_id,
                gix::refs::transaction::PreviousValue::Any,
                format!("uas-pool: provision worktree for slot {}", slot_id.as_str()),
            )
            .map_err(|e| {
                PoolError::Gitoxide(format!("failed to create branch {branch_name}: {e}"))
            })?;

        let dot_git_file = worktree_dir.join(".git");
        let canonical_worktree_git_dir = worktree_git_dir
            .canonicalize()
            .unwrap_or_else(|_| worktree_git_dir.clone());
        let canonical_common_git_dir = common_git_dir
            .canonicalize()
            .unwrap_or_else(|_| common_git_dir.clone());
        let canonical_dot_git_file = dot_git_file
            .canonicalize()
            .unwrap_or_else(|_| dot_git_file.clone());

        // Control files in worktree gitdir
        std::fs::write(
            worktree_git_dir.join("gitdir"),
            format!("{}\n", canonical_dot_git_file.display()),
        )
        .map_err(|e| PoolError::Gitoxide(format!("failed to write worktree gitdir: {e}")))?;

        std::fs::write(
            worktree_git_dir.join("commondir"),
            format!("{}\n", canonical_common_git_dir.display()),
        )
        .map_err(|e| PoolError::Gitoxide(format!("failed to write worktree commondir: {e}")))?;

        std::fs::write(
            worktree_git_dir.join("HEAD"),
            format!("ref: {branch_ref_name}\n"),
        )
        .map_err(|e| PoolError::Gitoxide(format!("failed to write worktree HEAD: {e}")))?;

        // Pointer file in worktree dir
        std::fs::write(
            &dot_git_file,
            format!("gitdir: {}\n", canonical_worktree_git_dir.display()),
        )
        .map_err(|e| PoolError::Gitoxide(format!("failed to write .git pointer file: {e}")))?;

        // Open linked worktree repository and populate working tree
        let wt_repo = gix::open_opts(&worktree_dir, repo_open_opts()).map_err(|e| {
            PoolError::Gitoxide(format!("failed to open linked worktree repo: {e}"))
        })?;

        let mut index_file = wt_repo
            .index_from_tree(&tree_id)
            .map_err(|e| PoolError::Gitoxide(format!("failed to create index from tree: {e}")))?;

        let mut opts = wt_repo
            .checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping)
            .map_err(|e| PoolError::Gitoxide(format!("failed to get checkout options: {e}")))?;
        opts.destination_is_initially_empty = true;
        opts.overwrite_existing = true;

        let files = gix::features::progress::Discard;
        let bytes = gix::features::progress::Discard;
        let should_interrupt = AtomicBool::new(false);

        let _outcome = gix::worktree::state::checkout(
            &mut index_file,
            &worktree_dir,
            wt_repo
                .objects
                .clone()
                .into_arc()
                .map_err(|e| PoolError::Gitoxide(format!("failed to clone objects: {e}")))?,
            &files,
            &bytes,
            &should_interrupt,
            opts,
        )
        .map_err(|e| PoolError::Gitoxide(format!("checkout into worktree failed: {e}")))?;

        index_file
            .write(Default::default())
            .map_err(|e| PoolError::Gitoxide(format!("failed to write index: {e}")))?;

        let worktree_ref_str = format!("slots/{}", slot_id.as_str());
        let worktree_ref = WorktreeRef::new(&worktree_ref_str)
            .map_err(|e| PoolError::Gitoxide(format!("invalid worktree reference: {e}")))?;

        Ok(Self {
            slot_id,
            base_repo_path,
            worktree_dir,
            worktree_git_dir,
            branch_name,
            worktree_ref,
            torn_down: false,
            marker: Some(marker_file),
        })
    }

    /// Reset the worktree back to clean HEAD commit state.
    ///
    /// Discards untracked files and reverts modified files, returning
    /// the worktree to a pristine checkout in under 100 milliseconds.
    pub fn reset_clean(&self) -> Result<(), PoolError> {
        if self.torn_down {
            return Err(PoolError::Gitoxide(
                "cannot reset a torn-down worktree".to_string(),
            ));
        }

        // Remove all untracked or modified files in worktree_dir except .git
        for entry in std::fs::read_dir(&self.worktree_dir)
            .map_err(|e| PoolError::Gitoxide(format!("failed to read worktree dir: {e}")))?
        {
            let entry =
                entry.map_err(|e| PoolError::Gitoxide(format!("read dir entry error: {e}")))?;
            let path = entry.path();
            if path.file_name() == Some(std::ffi::OsStr::new(".git")) {
                continue;
            }
            if path.is_dir() {
                std::fs::remove_dir_all(&path).map_err(|e| {
                    PoolError::Gitoxide(format!("failed to remove dir {}: {e}", path.display()))
                })?;
            } else {
                std::fs::remove_file(&path).map_err(|e| {
                    PoolError::Gitoxide(format!("failed to remove file {}: {e}", path.display()))
                })?;
            }
        }

        // Re-checkout the tree
        let wt_repo = gix::open_opts(&self.worktree_dir, repo_open_opts()).map_err(|e| {
            PoolError::Gitoxide(format!("failed to open worktree during reset: {e}"))
        })?;

        let mut head = wt_repo.head().map_err(|e| {
            PoolError::Gitoxide(format!("failed to resolve HEAD during reset: {e}"))
        })?;
        let commit_obj = head
            .peel_to_object()
            .map_err(|e| PoolError::Gitoxide(format!("failed to peel commit during reset: {e}")))?;
        let commit = commit_obj.into_commit();
        let tree_id = commit
            .tree_id()
            .map_err(|e| PoolError::Gitoxide(format!("failed to get tree during reset: {e}")))?;

        let mut index_file = wt_repo
            .index_from_tree(&tree_id)
            .map_err(|e| PoolError::Gitoxide(format!("failed to build index during reset: {e}")))?;

        let mut opts = wt_repo
            .checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping)
            .map_err(|e| PoolError::Gitoxide(format!("failed to get checkout options: {e}")))?;
        opts.destination_is_initially_empty = true;
        opts.overwrite_existing = true;

        let files = gix::features::progress::Discard;
        let bytes = gix::features::progress::Discard;
        let should_interrupt = AtomicBool::new(false);

        let _outcome = gix::worktree::state::checkout(
            &mut index_file,
            &self.worktree_dir,
            wt_repo
                .objects
                .clone()
                .into_arc()
                .map_err(|e| PoolError::Gitoxide(format!("failed to clone objects: {e}")))?,
            &files,
            &bytes,
            &should_interrupt,
            opts,
        )
        .map_err(|e| PoolError::Gitoxide(format!("checkout during reset failed: {e}")))?;

        index_file
            .write(Default::default())
            .map_err(|e| PoolError::Gitoxide(format!("failed to write index during reset: {e}")))?;

        Ok(())
    }

    /// Teardown the worktree, removing directories and pruning branch metadata.
    pub fn teardown(mut self) -> Result<(), PoolError> {
        self.teardown_inner()
    }

    fn teardown_inner(&mut self) -> Result<(), PoolError> {
        if self.torn_down {
            return Ok(());
        }
        self.torn_down = true;

        // 1. Remove working tree directory
        if self.worktree_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.worktree_dir);
        }

        // 2. Remove gitdir control directory
        if self.worktree_git_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.worktree_git_dir);
        }

        // 3. Prune branch reference in base repository
        if let Ok(base_repo) = gix::open_opts(&self.base_repo_path, repo_open_opts()) {
            let branch_ref = format!("refs/heads/{}", self.branch_name);
            if let Ok(Some(r)) = base_repo.try_find_reference(&branch_ref) {
                let _ = r.delete();
            }
        }

        // 4. Release the marker lock last (the control directory removal above
        // already deleted the file).
        drop(self.marker.take());

        Ok(())
    }

    /// The slot id associated with this worktree.
    pub fn slot_id(&self) -> &SlotId {
        &self.slot_id
    }

    /// The base repository path.
    pub fn base_repo_path(&self) -> &Path {
        &self.base_repo_path
    }

    /// The working tree directory path.
    pub fn worktree_dir(&self) -> &Path {
        &self.worktree_dir
    }

    /// The worktree git control directory path.
    pub fn worktree_git_dir(&self) -> &Path {
        &self.worktree_git_dir
    }

    /// The name of the checked-out branch.
    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    /// Domain `WorktreeRef` identifier.
    pub fn worktree_ref(&self) -> &WorktreeRef {
        &self.worktree_ref
    }

    /// True if teardown has already executed.
    pub fn is_torn_down(&self) -> bool {
        self.torn_down
    }
}

impl Drop for WarmWorktree {
    fn drop(&mut self) {
        let _ = self.teardown_inner();
    }
}
