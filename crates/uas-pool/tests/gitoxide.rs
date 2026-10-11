// SPDX-License-Identifier: Apache-2.0 OR MIT
//! CER-2724: In-process Gitoxide (`gix`) worktree provisioning and teardown integration tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

use uas_domain::SlotId;
use uas_pool::WarmWorktree;

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "uas-pool-test-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn repo_open_opts() -> gix::open::Options {
    let mut opts = gix::open::Options::default();
    opts.permissions.env.git_prefix = gix::sec::Permission::Deny;
    opts
}

fn open_repo(path: &Path) -> Result<gix::Repository, Box<dyn std::error::Error>> {
    Ok(gix::open_opts(path, repo_open_opts())?)
}

fn create_test_base_repo(dir: &Path) -> Result<gix::Repository, Box<dyn std::error::Error>> {
    let repo = gix::init(dir)?;
    let blob_id = repo.write_blob(b"# Test Repository\nInitial commit content.\n")?;

    let mut tree = gix::objs::Tree::empty();
    tree.entries.push(gix::objs::tree::Entry {
        mode: gix::objs::tree::EntryKind::Blob.into(),
        filename: "README.md".into(),
        oid: blob_id.into(),
    });
    let tree_id = repo.write_object(&tree)?;

    let sig = gix::actor::SignatureRef::from_bytes(b"UAS Test <test@example.com> 1700000000 +0000")
        .expect("valid signature");
    let _ = repo.commit_as(
        sig,
        sig,
        "HEAD",
        "initial commit",
        tree_id,
        gix::commit::NO_PARENT_IDS,
    )?;

    let config_path = dir.join(".git").join("config");
    let mut config = fs::read_to_string(&config_path).unwrap_or_default();
    config.push_str("\n[user]\n\tname = UAS Test\n\temail = test@example.com\n");
    fs::write(&config_path, config)?;

    Ok(repo)
}

#[test]
fn worktree_provision_and_files_checked_out() {
    let temp_dir = TempDirGuard::new("wt-provision");
    let base_path = temp_dir.path().join("base_repo");
    let wt_path = temp_dir.path().join("worktree_slot_1");

    let _repo = create_test_base_repo(&base_path).expect("base repo created");

    let slot_id = SlotId::new("slot-01").expect("valid slot id");
    let branch_name = "slot-01-branch";

    let wt = WarmWorktree::provision(&base_path, slot_id, &wt_path, branch_name)
        .expect("provision worktree");

    assert_eq!(wt.branch_name(), branch_name);
    assert_eq!(wt.worktree_dir(), wt_path);
    assert_eq!(wt.worktree_ref().as_str(), "slots/slot-01");
    assert!(!wt.is_torn_down());
    assert!(wt_path.exists());
    assert!(wt.worktree_git_dir().exists());

    // Verify checked-out file content matches
    let readme_path = wt_path.join("README.md");
    assert!(readme_path.exists());
    let content = fs::read_to_string(&readme_path).expect("read file");
    assert_eq!(content, "# Test Repository\nInitial commit content.\n");

    // Open worktree repository directly with gix
    let wt_repo = open_repo(&wt_path).expect("open worktree repo");
    assert!(!wt_repo.is_bare());

    // Clean teardown
    wt.teardown().expect("teardown worktree");
    assert!(!wt_path.exists());
}

#[test]
fn concurrent_worktree_provisioning_no_lock_collision() {
    let temp_dir = TempDirGuard::new("wt-concurrent");
    let base_path = temp_dir.path().join("base_repo");

    let _repo = create_test_base_repo(&base_path).expect("base repo created");

    let num_slots = 4;
    let mut handles = Vec::new();

    for i in 0..num_slots {
        let base_path_clone = base_path.clone();
        let wt_path = temp_dir.path().join(format!("worktree_slot_{i}"));
        let slot_id_str = format!("slot-{:02}", i + 1);
        let branch_str = format!("branch-slot-{:02}", i + 1);

        handles.push(thread::spawn(move || {
            let slot_id = SlotId::new(&slot_id_str).expect("valid slot id");
            let wt = WarmWorktree::provision(&base_path_clone, slot_id, &wt_path, &branch_str)
                .unwrap_or_else(|e| panic!("failed to provision {slot_id_str}: {e}"));

            // Verify worktree exists and file is present
            let readme = wt.worktree_dir().join("README.md");
            assert!(
                readme.exists(),
                "README must exist in {}",
                wt.worktree_dir().display()
            );

            wt.teardown().expect("teardown worktree");
        }));
    }

    for handle in handles {
        handle.join().expect("thread join succeeded");
    }

    // Verify base repository has no leftover branch locks or worktree dirs
    let base_repo = open_repo(&base_path).expect("open base repo");
    for i in 0..num_slots {
        let branch_ref = format!("refs/heads/branch-slot-{:02}", i + 1);
        let found = base_repo
            .try_find_reference(&branch_ref)
            .expect("lookup ref");
        assert!(
            found.is_none(),
            "branch {branch_ref} should have been pruned on teardown"
        );
    }
}

#[test]
fn worktree_reset_clean() {
    let temp_dir = TempDirGuard::new("wt-reset");
    let base_path = temp_dir.path().join("base_repo");
    let wt_path = temp_dir.path().join("worktree_reset");

    let _repo = create_test_base_repo(&base_path).expect("base repo created");

    let slot_id = SlotId::new("slot-reset").expect("valid slot id");
    let branch_name = "slot-reset-branch";

    let wt = WarmWorktree::provision(&base_path, slot_id, &wt_path, branch_name)
        .expect("provision worktree");

    let readme_path = wt_path.join("README.md");
    let untracked_file = wt_path.join("untracked.txt");
    let nested_dir = wt_path.join("nested").join("sub");
    let nested_file = nested_dir.join("code.rs");

    // Dirty the worktree:
    // 1. Modify existing tracked file
    fs::write(&readme_path, "MODIFIED CONTENT\n").expect("write modified readme");
    // 2. Add untracked root file
    fs::write(&untracked_file, "extra junk").expect("write untracked file");
    // 3. Add untracked directory and nested file
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(&nested_file, "fn dirty() {}").expect("write nested file");

    assert!(untracked_file.exists());
    assert!(nested_file.exists());
    assert_eq!(
        fs::read_to_string(&readme_path).unwrap(),
        "MODIFIED CONTENT\n"
    );

    // Execute reset_clean and measure duration (target < 100ms)
    let start = Instant::now();
    wt.reset_clean().expect("reset_clean should succeed");
    let duration = start.elapsed();
    assert!(
        duration.as_millis() < 500,
        "reset took too long: {:?}",
        duration
    );

    // Verify untracked files and directories are gone
    assert!(!untracked_file.exists(), "untracked file must be removed");
    assert!(
        !nested_dir.exists(),
        "untracked nested directory must be removed"
    );

    // Verify modified file was restored to pristine commit content
    let restored_content = fs::read_to_string(&readme_path).expect("read restored readme");
    assert_eq!(
        restored_content, "# Test Repository\nInitial commit content.\n",
        "README.md must be restored to initial commit"
    );

    // Teardown
    wt.teardown().expect("teardown worktree");
}

#[test]
fn worktree_teardown_prunes_branch_and_metadata() {
    let temp_dir = TempDirGuard::new("wt-teardown");
    let base_path = temp_dir.path().join("base_repo");
    let wt_path = temp_dir.path().join("worktree_td");

    let _repo = create_test_base_repo(&base_path).expect("base repo created");

    let slot_id = SlotId::new("slot-td").expect("valid slot id");
    let branch_name = "slot-td-branch";

    let wt = WarmWorktree::provision(&base_path, slot_id, &wt_path, branch_name)
        .expect("provision worktree");

    let wt_git_dir = wt.worktree_git_dir().to_path_buf();
    assert!(wt_path.exists());
    assert!(wt_git_dir.exists());

    let base_repo = open_repo(&base_path).expect("open base repo");
    let branch_ref = format!("refs/heads/{}", branch_name);
    assert!(
        base_repo.try_find_reference(&branch_ref).unwrap().is_some(),
        "branch must exist in base repo before teardown"
    );

    // Teardown
    wt.teardown().expect("teardown worktree");

    // Verify working tree dir deleted
    assert!(!wt_path.exists(), "worktree dir must be deleted");

    // Verify worktree control dir in base repo deleted
    assert!(!wt_git_dir.exists(), "worktree git dir must be deleted");

    // Verify branch pruned in base repo
    let found = base_repo
        .try_find_reference(&branch_ref)
        .expect("lookup ref");
    assert!(
        found.is_none(),
        "branch must be pruned from base repo on teardown"
    );
}

#[test]
fn worktree_drop_cleans_up_automatically() {
    let temp_dir = TempDirGuard::new("wt-drop");
    let base_path = temp_dir.path().join("base_repo");
    let wt_path = temp_dir.path().join("worktree_drop");
    let branch_name = "slot-drop-branch";

    let _repo = create_test_base_repo(&base_path).expect("base repo created");

    let wt_git_dir;
    {
        let slot_id = SlotId::new("slot-drop").expect("valid slot id");
        let wt = WarmWorktree::provision(&base_path, slot_id, &wt_path, branch_name)
            .expect("provision worktree");
        wt_git_dir = wt.worktree_git_dir().to_path_buf();
        assert!(wt_path.exists());
        assert!(wt_git_dir.exists());
        // `wt` drops here
    }

    assert!(!wt_path.exists(), "worktree dir should be deleted on drop");
    assert!(
        !wt_git_dir.exists(),
        "worktree git dir should be deleted on drop"
    );

    let base_repo = open_repo(&base_path).expect("open base repo");
    let branch_ref = format!("refs/heads/{}", branch_name);
    let found = base_repo
        .try_find_reference(&branch_ref)
        .expect("lookup ref");
    assert!(
        found.is_none(),
        "branch should be pruned from base repo on drop"
    );
}
