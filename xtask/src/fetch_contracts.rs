// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `cargo xtask fetch-contracts [--lock <path>]` (spec 0048 D9, REQ-0048-11).
//!
//! Reads `contracts.lock`, fetches each pinned upstream commit with `git`, hashes every file and
//! writes it under `.contracts/` only when the sha256 matches the lock. Fails closed: a bad lock,
//! an unreachable commit, a commit that differs from the pin or a hash mismatch exits non-zero
//! and names the file, which is left out of `.contracts/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// Distinguishes scratch repositories when several fetches run in one process.
static SCRATCH: AtomicUsize = AtomicUsize::new(0);

/// One lock line: a file at an upstream commit and the sha256 of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    sha256: String,
    commit: String,
    repo: String,
    path: String,
    dest: String,
}

pub fn run(root: &Path, args: &[String]) -> ExitCode {
    match fetch(root, args) {
        Ok(msg) => {
            println!("fetch-contracts: {msg}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("fetch-contracts: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn parse_lock(text: &str) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let n = i + 1;
        let f: Vec<&str> = line.split_whitespace().collect();
        let [sha256, commit, repo, path, dest] = f[..] else {
            bail!("lock line {n}: want 5 fields (sha256 commit repository path destination)");
        };
        if !is_hex(sha256, 64) {
            bail!("lock line {n}: sha256 is not 64 lowercase hex characters");
        }
        if !is_hex(commit, 40) {
            bail!("lock line {n}: commit must be a full 40-hex pin, not a branch or a short id");
        }
        let safe = |p: &str| {
            Path::new(p)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        };
        if !safe(dest) || !safe(path) {
            bail!("lock line {n}: path and destination must be relative with no '..'");
        }
        entries.push(Entry {
            sha256: sha256.into(),
            commit: commit.into(),
            repo: repo.into(),
            path: path.into(),
            dest: dest.into(),
        });
    }
    if entries.is_empty() {
        bail!("lock has no entries");
    }
    Ok(entries)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Compare the hash of `bytes` with the lock entry; the error names the file.
fn check_hash(entry: &Entry, bytes: &[u8]) -> Result<()> {
    let got = hex_digest(bytes);
    if got == entry.sha256 {
        Ok(())
    } else {
        bail!(
            "{}: sha256 mismatch ({}@{}:{}): lock {}, upstream {got}",
            entry.dest,
            entry.repo,
            &entry.commit[..12],
            entry.path,
            entry.sha256
        )
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    // A hook exports GIT_DIR, GIT_INDEX_FILE and more; the child must see none of them.
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().starts_with("GIT_")) {
        cmd.env_remove(key);
    }
    let out = cmd
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// Shallow-fetch exactly `commit` into a throwaway repository and prove FETCH_HEAD equals it.
fn fetch_commit(scratch: &Path, url: &str, repo: &str, commit: &str) -> Result<()> {
    fs::create_dir_all(scratch)?;
    git(scratch, &["init", "-q"])?;
    git(scratch, &["fetch", "-q", "--depth", "1", url, commit])
        .with_context(|| format!("commit {commit} of {repo} is unreachable"))?;
    let head = String::from_utf8(git(scratch, &["rev-parse", "FETCH_HEAD"])?)?;
    if head.trim() != commit {
        bail!("{repo}: fetched {} but the lock pins {commit}", head.trim());
    }
    Ok(())
}

fn fetch(root: &Path, args: &[String]) -> Result<String> {
    fetch_from(root, args, &|repo| format!("git@github.com:{repo}.git"))
}

/// `fetch` with the remote URL of a repository injected, so tests can use a local repository.
fn fetch_from(root: &Path, args: &[String], remote: &dyn Fn(&str) -> String) -> Result<String> {
    let lock = match args {
        [] => root.join("contracts.lock"),
        [flag, path] if flag == "--lock" => PathBuf::from(path),
        _ => bail!("usage: cargo xtask fetch-contracts [--lock <path>]"),
    };
    let text = fs::read_to_string(&lock)
        .with_context(|| format!("cannot read lock {}", lock.display()))?;
    let entries = parse_lock(&text)?;
    let out_root = root.join(".contracts");

    let mut groups: BTreeMap<(&str, &str), Vec<&Entry>> = BTreeMap::new();
    for e in &entries {
        groups.entry((&e.repo, &e.commit)).or_default().push(e);
    }
    let mut errors = Vec::new();
    let mut written = 0usize;
    for ((repo, commit), files) in &groups {
        let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let scratch =
            std::env::temp_dir().join(format!("fetch-contracts-{}-{n}", std::process::id()));
        let result = fetch_commit(&scratch, &remote(repo), repo, commit);
        match result {
            Err(err) => errors.push(format!("{err:#}")),
            Ok(()) => {
                println!("fetch-contracts: {repo} resolved {commit}");
                for e in files {
                    let dest = out_root.join(&e.dest);
                    // A failed file is left out, never kept from an earlier run.
                    let _ = fs::remove_file(&dest);
                    let spec = format!("{commit}:{}", e.path);
                    let step = git(&scratch, &["show", &spec])
                        .with_context(|| format!("{}: cannot read {spec}", e.dest))
                        .and_then(|bytes| check_hash(e, &bytes).map(|()| bytes))
                        .and_then(|bytes| {
                            if let Some(dir) = dest.parent() {
                                fs::create_dir_all(dir)?;
                            }
                            fs::write(&dest, bytes)
                                .with_context(|| format!("{}: cannot write", e.dest))
                        });
                    match step {
                        Ok(()) => written += 1,
                        Err(err) => errors.push(format!("{err:#}")),
                    }
                }
            }
        }
        let _ = fs::remove_dir_all(&scratch);
    }
    if errors.is_empty() {
        Ok(format!("wrote {written} files into .contracts/"))
    } else {
        bail!(
            "{} problem(s), those files are left out:\n  {}",
            errors.len(),
            errors.join("\n  ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const COMMIT: &str = "a63d8588a5d27c98505f7e8822befebe78eaa8cb";

    fn entry() -> Entry {
        Entry {
            sha256: SHA.into(),
            commit: COMMIT.into(),
            repo: "o/r".into(),
            path: "p/f".into(),
            dest: "x/f".into(),
        }
    }

    #[test]
    fn fetch_contracts_parse_lock_reads_a_line() {
        let text = format!("# c\n\n{SHA} {COMMIT} o/r p/f x/f\n");
        assert_eq!(parse_lock(&text).unwrap(), vec![entry()]);
    }

    #[test]
    fn fetch_contracts_parse_lock_rejects_bad_lines() {
        for bad in [
            String::new(),
            format!("{SHA} {COMMIT} o/r p/f"),
            format!("{SHA} main o/r p/f x/f"),
            format!("{SHA} {} o/r p/f x/f", &COMMIT[..7]),
            format!("{} {COMMIT} o/r p/f x/f", &SHA[..63]),
            format!("{SHA} {COMMIT} o/r p/f ../x"),
            format!("{SHA} {COMMIT} o/r p/f /abs"),
        ] {
            assert!(parse_lock(&bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn fetch_contracts_hash_compare_names_the_file() {
        assert!(check_hash(&entry(), b"hello").is_ok());
        let err = check_hash(&entry(), b"hellp").unwrap_err().to_string();
        assert!(err.contains("x/f") && err.contains("mismatch"), "{err}");
    }

    #[test]
    fn fetch_contracts_missing_lock_fails() {
        let args = [
            "--lock".to_string(),
            "/nonexistent/contracts.lock".to_string(),
        ];
        let err = fetch(Path::new("/tmp"), &args).unwrap_err();
        assert!(format!("{err:#}").contains("cannot read lock"));
    }

    /// A local repository with one commit holding `p/f` = "hello", and an empty output root.
    fn fixture(tag: &str) -> (PathBuf, PathBuf, String) {
        let base = std::env::temp_dir().join(format!("fc-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (up, root) = (base.join("up"), base.join("root"));
        fs::create_dir_all(up.join("p")).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::write(up.join("p/f"), "hello").unwrap();
        git(&up, &["init", "-q"]).unwrap();
        git(&up, &["add", "p/f"]).unwrap();
        let id = [
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ];
        let commit = [&id[..], &["commit", "-q", "-m", "x"]].concat();
        git(&up, &commit).unwrap();
        let sha = String::from_utf8(git(&up, &["rev-parse", "HEAD"]).unwrap()).unwrap();
        (up, root, sha.trim().to_string())
    }

    fn run_fetch(up: &Path, root: &Path, sha256: &str, commit: &str) -> Result<String> {
        let lock = root.join("contracts.lock");
        fs::write(&lock, format!("{sha256} {commit} o/r p/f x/f\n")).unwrap();
        let args = ["--lock".to_string(), lock.display().to_string()];
        fetch_from(root, &args, &|_| format!("file://{}", up.display()))
    }

    #[test]
    fn fetch_contracts_fetch_refuses_a_hash_mismatch() {
        let (up, root, sha) = fixture("bad");
        let err = run_fetch(&up, &root, &SHA.replace("2cf2", "2cf3"), &sha).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("x/f") && msg.contains("mismatch"), "{msg}");
        assert!(!root.join(".contracts/x/f").exists());
        let _ = fs::remove_dir_all(up.parent().unwrap());
    }

    #[test]
    fn fetch_contracts_fetch_places_a_matching_file() {
        let (up, root, sha) = fixture("good");
        let good = hex_digest(b"hello");
        run_fetch(&up, &root, &good, &sha).unwrap();
        assert_eq!(fs::read(root.join(".contracts/x/f")).unwrap(), b"hello");
        let _ = fs::remove_dir_all(up.parent().unwrap());
    }
}
