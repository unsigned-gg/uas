//! Shared fixtures for the uas-pool integration tests (copied from `gitoxide.rs`).
#![allow(dead_code, missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

pub struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    pub fn new(name: &str) -> Self {
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

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn repo_open_opts() -> gix::open::Options {
    let mut opts = gix::open::Options::default();
    opts.permissions.env.git_prefix = gix::sec::Permission::Deny;
    opts
}

pub fn open_repo(path: &Path) -> Result<gix::Repository, Box<dyn std::error::Error>> {
    Ok(gix::open_opts(path, repo_open_opts())?)
}

pub fn create_test_base_repo(dir: &Path) -> Result<gix::Repository, Box<dyn std::error::Error>> {
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
