//! Spec 0010 gate: `cargo test -p uas-sandbox -j 2 --test landlock_probe`.
//!
//! The jail cases need a kernel with Landlock. Without one they print
//! `skipped` and pass, and the PR body says which kernel ran the gate.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use uas_sandbox::{probe, AbiReport, AbiStatus, LandlockRulesetBuilder, PathAccess, SandboxError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const EACCES: i32 = 13;

/// A fresh directory under `target/tmp`.
fn scratch(name: &str) -> Result<PathBuf, std::io::Error> {
    fresh(Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id())))
}

/// A fresh directory under `$HOME`, which no worker preset rule reaches.
/// `None` when `HOME` is unset or not writable.
fn home_scratch(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = Path::new(&home)
        .join(".cache")
        .join("uas-sandbox-tests")
        .join(format!("{name}-{}", std::process::id()));
    fresh(dir).ok()
}

fn fresh(dir: PathBuf) -> Result<PathBuf, std::io::Error> {
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The probe, or `None` with a printed reason when the kernel has no Landlock.
fn landlock_or_skip() -> Result<Option<AbiReport>, SandboxError> {
    let report = probe()?;
    if report.is_available() {
        Ok(Some(report))
    } else {
        eprintln!("skipped: landlock not available ({:?})", report.status);
        Ok(None)
    }
}

/// Write `bytes` to `path` and report the errno, if any.
fn write_errno(path: &Path, bytes: &[u8]) -> Option<i32> {
    fs::write(path, bytes).err().and_then(|e| e.raw_os_error())
}

#[test]
fn probe_reports_the_kernel() -> TestResult {
    let report = probe()?;
    println!(
        "landlock probe: status={:?} effective_abi={} kernel_abi={:?}",
        report.status, report.effective_abi, report.kernel_abi
    );
    match report.status {
        AbiStatus::Available => assert!(report.effective_abi >= 1, "available but ABI 0"),
        AbiStatus::Unsupported | AbiStatus::NotEnabled => {
            assert_eq!(report.effective_abi, 0);
            assert_eq!(report.kernel_abi, None);
        }
    }
    Ok(())
}

#[test]
fn probe_leaves_the_caller_unrestricted() -> TestResult {
    let dir = scratch("unrestricted")?;
    probe()?;
    fs::write(dir.join("after-probe.txt"), b"still writable")?;
    Ok(())
}

#[test]
fn builder_canonicalises_a_symlink() -> TestResult {
    let dir = scratch("symlink")?;
    let real = dir.join("real");
    fs::create_dir(&real)?;
    let link = dir.join("link");
    symlink(&real, &link)?;
    let plan = LandlockRulesetBuilder::new().allow_read(&link).plan()?;
    assert_eq!(plan.rules.len(), 1);
    assert_eq!(plan.rules[0].path, fs::canonicalize(&real)?);
    assert_eq!(plan.rules[0].access, PathAccess::Read);
    Ok(())
}

#[test]
fn builder_rejects_a_missing_path() -> TestResult {
    let dir = scratch("missing")?;
    let missing = dir.join("absent");
    let err = LandlockRulesetBuilder::new()
        .allow_read_write(&missing)
        .plan()
        .err();
    assert!(
        matches!(err, Some(SandboxError::PathMissing(ref p)) if *p == missing),
        "expected PathMissing, got {err:?}"
    );
    Ok(())
}

#[test]
fn worker_preset_records_optional_paths() -> TestResult {
    let ws = scratch("preset")?;
    let plan = LandlockRulesetBuilder::worker(&ws, Vec::<PathBuf>::new()).plan()?;
    assert_eq!(plan.rules[0].path, fs::canonicalize(&ws)?);
    assert_eq!(plan.rules[0].access, PathAccess::ReadWrite);
    for optional in ["/lib64", "/sbin"] {
        let path = Path::new(optional);
        let skipped = plan.skipped.iter().any(|p| p == path);
        assert_ne!(path.exists(), skipped, "{optional}: exists xor skipped");
    }
    let access_of = |p: &str| {
        plan.rules
            .iter()
            .find(|r| r.path == Path::new(p))
            .map(|r| r.access)
    };
    assert_eq!(access_of("/etc"), Some(PathAccess::Read));
    assert_eq!(access_of("/tmp"), Some(PathAccess::Write));
    assert_eq!(access_of("/usr"), Some(PathAccess::ReadExec));
    Ok(())
}

/// What a jailed thread managed to do.
struct JailOutcome {
    inside_errno: Option<i32>,
    home_errno: Option<i32>,
    via_tmp_link_errno: Option<i32>,
}

#[test]
fn jail_writes_inside_the_workspace_only() -> TestResult {
    if landlock_or_skip()?.is_none() {
        return Ok(());
    }
    let Some(home_dir) = home_scratch("jail-home") else {
        eprintln!("skipped: HOME is unset or not writable");
        return Ok(());
    };
    let ws = scratch("jail-ws")?;
    let inside_file = ws.join("ok.txt");
    let home_file = home_dir.join("nope.txt");

    // A symlink inside the allowed /tmp that points at the denied $HOME directory.
    let tmp_link = std::env::temp_dir().join(format!("uas-sandbox-link-{}", std::process::id()));
    let _ = fs::remove_file(&tmp_link);
    symlink(&home_dir, &tmp_link)?;
    let via_tmp_link = tmp_link.join("via-link.txt");

    // Positive control: unjailed, every write succeeds.
    fs::write(&home_file, b"control")?;
    fs::remove_file(&home_file)?;
    fs::write(&via_tmp_link, b"control")?;
    fs::remove_file(&via_tmp_link)?;

    let (ws_t, inside_t, home_t, link_t) = (
        ws.clone(),
        inside_file.clone(),
        home_file.clone(),
        via_tmp_link.clone(),
    );
    // Landlock confines the calling thread only, so the jail lives and dies here.
    let handle = std::thread::spawn(move || -> Result<JailOutcome, SandboxError> {
        let (ruleset, _plan) =
            LandlockRulesetBuilder::worker(&ws_t, Vec::<PathBuf>::new()).build()?;
        ruleset.restrict_self()?;
        Ok(JailOutcome {
            inside_errno: write_errno(&inside_t, b"inside"),
            home_errno: write_errno(&home_t, b"home"),
            via_tmp_link_errno: write_errno(&link_t, b"link"),
        })
    });
    let outcome = handle.join().map_err(|_| "jail thread panicked")??;
    let _ = fs::remove_file(&tmp_link);

    assert_eq!(
        outcome.inside_errno, None,
        "write inside the workspace must succeed"
    );
    assert_eq!(
        outcome.home_errno,
        Some(EACCES),
        "write under HOME must fail with EACCES"
    );
    assert_eq!(
        outcome.via_tmp_link_errno,
        Some(EACCES),
        "write through a /tmp symlink into HOME must fail with EACCES"
    );
    assert!(inside_file.exists());
    assert!(!home_file.exists(), "no file may appear under HOME");
    assert!(!via_tmp_link.exists());
    fs::remove_dir_all(&home_dir)?;
    Ok(())
}
