//! `cargo xtask pin-check` (spec 0004 D3).
//!
//! The `| Crate | Version |` tables in `specs/0004-dependency-and-licence-policy.md` bind the
//! lockfiles. A crate that a table pins and a manifest declares directly must be locked at the
//! pinned version. The workspace members are checked against `Cargo.lock`. The eBPF program
//! `crates/cas-ebpf/ebpf` is its own workspace and is checked against its own lockfile.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};

const SPEC: &str = "specs/0004-dependency-and-licence-policy.md";
const EBPF_DIR: &str = "crates/cas-ebpf/ebpf";

/// The result of one check.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    /// How many pinned crates a manifest declares, and a lock was asked about.
    pub checked: usize,
    /// One line per pinned crate that the lock does not hold at the pinned version.
    pub misses: Vec<String>,
}

pub fn run(root: &Path) -> ExitCode {
    match check_root(root) {
        Ok(report) if report.misses.is_empty() => {
            println!("pin-check: ok ({} pinned crates checked)", report.checked);
            ExitCode::SUCCESS
        }
        Ok(report) => {
            for miss in &report.misses {
                eprintln!("{miss}");
            }
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("pin-check: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

fn check_root(root: &Path) -> Result<Report> {
    let spec = read(&root.join(SPEC))?;
    let root_manifest = read(&root.join("Cargo.toml"))?;
    let mut manifests = vec![root_manifest.clone()];
    for dir in member_dirs(root, &root_manifest)? {
        manifests.push(read(&dir.join("Cargo.toml"))?);
    }
    let lock = read(&root.join("Cargo.lock"))?;
    let ebpf_manifest = read(&root.join(EBPF_DIR).join("Cargo.toml"))?;
    let ebpf_lock = read(&root.join(EBPF_DIR).join("Cargo.lock"))?;
    Ok(check_repo(
        &spec,
        &manifests,
        &lock,
        &ebpf_manifest,
        &ebpf_lock,
    ))
}

/// The member directories of the root workspace: one directory level under each `dir/*` entry.
fn member_dirs(root: &Path, root_manifest: &str) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for member in members(root_manifest) {
        if let Some(parent) = member.strip_suffix("/*") {
            let base = root.join(parent);
            let entries =
                fs::read_dir(&base).with_context(|| format!("cannot read {}", base.display()))?;
            for entry in entries {
                let path = entry?.path();
                if path.join("Cargo.toml").is_file() {
                    dirs.push(path);
                }
            }
        } else {
            dirs.push(root.join(&member));
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// The quoted strings of the `members = [...]` line of `[workspace]`.
pub fn members(manifest: &str) -> Vec<String> {
    let mut in_workspace = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_workspace = line == "[workspace]";
            continue;
        }
        if !in_workspace {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "members" {
                return value
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_owned)
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Check the workspace manifests against the workspace lock, and the eBPF manifest against its
/// own lock.
pub fn check_repo(
    spec: &str,
    manifests: &[String],
    lock: &str,
    ebpf_manifest: &str,
    ebpf_lock: &str,
) -> Report {
    let pins = match parse_pins(spec) {
        Ok(pins) => pins,
        Err(errors) => {
            return Report {
                checked: 0,
                misses: errors,
            }
        }
    };
    let workspace: BTreeSet<String> = manifests.iter().flat_map(|m| direct_deps(m)).collect();
    let ebpf = direct_deps(ebpf_manifest);
    let mut report = Report {
        checked: 0,
        misses: Vec::new(),
    };
    for (declared, lock_text, label) in [
        (&workspace, lock, "Cargo.lock"),
        (&ebpf, ebpf_lock, "crates/cas-ebpf/ebpf/Cargo.lock"),
    ] {
        let locked = parse_lock(lock_text);
        for (name, version) in &pins {
            if !declared.contains(name) {
                continue;
            }
            report.checked += 1;
            let have = locked.get(name).map_or(&[][..], Vec::as_slice);
            if !have.contains(version) {
                let list = if have.is_empty() {
                    "none".to_owned()
                } else {
                    have.join(", ")
                };
                report.misses.push(format!(
                    "pin-check: {name} pinned {version} (specs/0004), {label} has {list}"
                ));
            }
        }
    }
    report
}

/// The `(crate, version)` pairs of every `| Crate | Version |` table. A row with an empty crate
/// name, a version that is not `X.Y.Z`, or a different count of crates and versions is an error
/// that names the spec line and the row: nothing is skipped in silence (spec 0004 D3).
pub fn parse_pins(spec: &str) -> Result<Vec<(String, String)>, Vec<String>> {
    let mut pins: Vec<(String, String)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut in_table = false;
    for (index, raw) in spec.lines().enumerate() {
        let line = raw.trim();
        if !line.starts_with('|') {
            in_table = false;
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() >= 2 && cells[0] == "Crate" && cells[1] == "Version" {
            in_table = true;
            continue;
        }
        if !in_table || cells[0].starts_with("---") {
            continue;
        }
        // Echo the crate and version cells only: the other cells are long prose.
        let row = format!("| {} |", cells[..cells.len().min(2)].join(" | "));
        let fail =
            |problem: String| format!("pin-check: {SPEC} line {}: {problem}: {row}", index + 1);
        if cells.len() < 2 {
            errors.push(fail("the row has no version cell".to_owned()));
            continue;
        }
        let names: Vec<&str> = cells[0]
            .split(',')
            .map(|n| n.trim().trim_matches('`'))
            .collect();
        let versions: Vec<&str> = cells[1].split(',').map(str::trim).collect();
        if names.iter().any(|n| n.is_empty()) {
            errors.push(fail("empty crate name".to_owned()));
        } else if names.len() != versions.len() {
            errors.push(fail(format!(
                "{} crates but {} versions",
                names.len(),
                versions.len()
            )));
        } else if let Some(bad) = versions.iter().find(|v| !is_exact_version(v)) {
            errors.push(fail(format!("version {bad:?} is not X.Y.Z")));
        } else {
            for (name, version) in names.into_iter().zip(versions) {
                let pair = (name.to_owned(), version.to_owned());
                if !pins.contains(&pair) {
                    pins.push(pair);
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(pins)
    } else {
        Err(errors)
    }
}

/// True for `X.Y.Z` with digits only.
fn is_exact_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// The crate names a manifest declares directly.
pub fn direct_deps(manifest: &str) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_deps = false;
    for raw in manifest.lines() {
        let line = raw.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim();
            in_deps = is_dep_section(name);
            // `[dependencies.foo]` declares `foo` in the header.
            for prefix in ["dependencies.", "dev-dependencies.", "build-dependencies."] {
                if let Some(dep) = name.strip_prefix(prefix) {
                    deps.insert(dep.trim_matches('"').to_owned());
                }
            }
            continue;
        }
        if !in_deps || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            let key = key.trim().split('.').next().unwrap_or("").trim_matches('"');
            if !key.is_empty() {
                deps.insert(key.to_owned());
            }
        }
    }
    deps
}

/// True for a section whose keys are dependency names.
fn is_dep_section(name: &str) -> bool {
    matches!(
        name,
        "dependencies" | "dev-dependencies" | "build-dependencies" | "workspace.dependencies"
    ) || (name.starts_with("target.")
        && (name.ends_with(".dependencies")
            || name.ends_with(".dev-dependencies")
            || name.ends_with(".build-dependencies")))
}

/// The locked versions of each package name.
pub fn parse_lock(lock: &str) -> BTreeMap<String, Vec<String>> {
    let mut packages: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut name: Option<String> = None;
    for raw in lock.lines() {
        let line = raw.trim();
        if line == "[[package]]" {
            name = None;
        } else if let Some(value) = quoted_value(line, "name") {
            name = Some(value);
        } else if let Some(value) = quoted_value(line, "version") {
            if let Some(name) = name.take() {
                packages.entry(name).or_default().push(value);
            }
        }
    }
    packages
}

/// The value of `key = "value"`, or None.
fn quoted_value(line: &str, key: &str) -> Option<String> {
    let (k, v) = line.split_once('=')?;
    if k.trim() != key {
        return None;
    }
    v.trim()
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_TEXT: &str = "\
## Pins

| Crate | Version | Licence | Used by |
|---|---|---|---|
| `tokio` | 1.53.1 | MIT | cas-router |
| `aya-log`, `aya-log-ebpf` | 0.3.0, 0.2.0 | MIT | cas-ebpf |

| Component | Version |
|---|---|
| `ignored` | 9.9.9 |

| Crate | Version | Licence |
|---|---|---|
| `libc` | 0.2.169 | MIT |
";

    fn lock(pairs: &[(&str, &str)]) -> String {
        pairs
            .iter()
            .map(|(n, v)| format!("[[package]]\nname = \"{n}\"\nversion = \"{v}\"\n"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn parses_pin_rows() {
        let pins = parse_pins(SPEC_TEXT).unwrap();
        let want: Vec<(String, String)> = [
            ("tokio", "1.53.1"),
            ("aya-log", "0.3.0"),
            ("aya-log-ebpf", "0.2.0"),
            ("libc", "0.2.169"),
        ]
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
        .collect();
        assert_eq!(pins, want);
    }

    #[test]
    fn rejects_malformed_rows() {
        let table = |row: &str| format!("intro\n\n| Crate | Version |\n|---|---|\n{row}\n");
        let cases = [
            ("short", "| `serde` | 0.8 |", "0.8"),
            ("none", "| `thing` | none |", "none"),
            ("empty name", "| `` | 1.0.0 |", "empty crate name"),
            ("blank name", "|  | 1.0.0 |", "empty crate name"),
            ("count", "| `a`, `b` | 1.0.0 |", "2 crates but 1 versions"),
            ("one cell", "| `a` |", "`a`"),
        ];
        for (name, row, want) in cases {
            let errors = parse_pins(&table(row)).expect_err(name);
            assert_eq!(errors.len(), 1, "{name}: {errors:?}");
            assert!(errors[0].contains("line 5"), "{name}: {errors:?}");
            assert!(errors[0].contains(row), "{name}: {errors:?}");
            assert!(errors[0].contains(want), "{name}: {errors:?}");
        }
        // Control: a good row in the same shape parses.
        assert_eq!(
            parse_pins(&table("| `a`, `b` | 1.0.0, 2.0.0 |"))
                .unwrap()
                .len(),
            2
        );
        // A malformed row outside a pin table is not read.
        assert!(parse_pins("| Component | Version |\n|---|---|\n| `x` | none |\n").is_ok());
    }

    #[test]
    fn malformed_rows_fail_the_check() {
        let spec = "| Crate | Version |\n|---|---|\n| `tokio` | 1.53 |\n";
        let manifests = ["[dependencies]\ntokio = \"1\"\n".to_owned()];
        let got = check_repo(spec, &manifests, &lock(&[("tokio", "1.53.1")]), "", "");
        assert_eq!(got.checked, 0);
        assert_eq!(got.misses.len(), 1, "{:?}", got.misses);
        assert!(got.misses[0].contains("line 3"), "{:?}", got.misses);
    }

    fn temp_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-pc-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("xtask")).unwrap();
        fs::create_dir_all(dir.join(EBPF_DIR)).unwrap();
        fs::create_dir_all(dir.join("specs")).unwrap();
        fs::write(dir.join(SPEC), "no tables\n").unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"xtask\"]\n",
        )
        .unwrap();
        fs::write(dir.join("xtask/Cargo.toml"), "[dependencies]\n").unwrap();
        fs::write(dir.join(EBPF_DIR).join("Cargo.toml"), "[dependencies]\n").unwrap();
        dir
    }

    #[test]
    fn fails_on_missing_lockfile() {
        let dir = temp_repo("lock");
        let ws_lock = dir.join("Cargo.lock");
        let ebpf_lock = dir.join(EBPF_DIR).join("Cargo.lock");
        let (ws, ebpf) = (
            ws_lock.display().to_string(),
            ebpf_lock.display().to_string(),
        );
        // Both locks missing: the error names the root lock and not the eBPF lock.
        let err = format!("{:#}", check_root(&dir).unwrap_err());
        assert!(err.contains(&ws) && !err.contains(&ebpf), "{err}");
        // The eBPF lock only: the error still names the root lock.
        fs::write(&ebpf_lock, "").unwrap();
        let err = format!("{:#}", check_root(&dir).unwrap_err());
        assert!(err.contains(&ws) && !err.contains(&ebpf), "{err}");
        // The root lock only: the error names the eBPF lock and not the root lock.
        fs::remove_file(&ebpf_lock).unwrap();
        fs::write(&ws_lock, "").unwrap();
        let err = format!("{:#}", check_root(&dir).unwrap_err());
        assert!(err.contains(&ebpf) && !err.contains(&ws), "{err}");
        // Control: both locks present, the check runs.
        fs::write(&ebpf_lock, "").unwrap();
        assert!(check_root(&dir).is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_direct_deps() {
        let manifest = "\
[package]
name = \"member\"
version = \"0.1.0\"

[workspace.dependencies]
tokio = { version = \"=1.53.1\" }

[dependencies]
serde.workspace = true
clap = { workspace = true }

[dev-dependencies]
tempfile = \"3\"

[build-dependencies]
protox = { version = \"0.9.1\", optional = true }

[target.'cfg(unix)'.dependencies]
rustix = \"1\"

[dependencies.libc]
version = \"0.2\"

[features]
default = []
";
        let got: Vec<String> = direct_deps(manifest).into_iter().collect();
        assert_eq!(
            got,
            ["clap", "libc", "protox", "rustix", "serde", "tempfile", "tokio"]
        );
    }

    #[test]
    fn parses_workspace_members() {
        let manifest = "[workspace]\nresolver = \"2\"\nmembers = [\"crates/*\", \"xtask\"]\n";
        assert_eq!(members(manifest), ["crates/*", "xtask"]);
    }

    #[test]
    fn flags_lock_drift() {
        let spec = "| Crate | Version |\n|---|---|\n| `tokio` | 1.53.1 |\n";
        let manifests = ["[dependencies]\ntokio = \"1.53.1\"\n".to_owned()];
        let got = check_repo(
            spec,
            &manifests,
            &lock(&[("tokio", "1.53.2")]),
            "[dependencies]\n",
            "",
        );
        assert_eq!(got.checked, 1);
        assert_eq!(
            got.misses,
            ["pin-check: tokio pinned 1.53.1 (specs/0004), Cargo.lock has 1.53.2"]
        );
    }

    #[test]
    fn ignores_undeclared_pins() {
        let spec = "| Crate | Version |\n|---|---|\n| `libc` | 0.2.169 |\n";
        let manifests = ["[dependencies]\nserde = \"1\"\n".to_owned()];
        let got = check_repo(
            spec,
            &manifests,
            &lock(&[("libc", "0.2.190")]),
            "[dependencies]\n",
            "",
        );
        assert_eq!(
            got,
            Report {
                checked: 0,
                misses: Vec::new()
            }
        );
    }

    #[test]
    fn passes_when_locked_at_pin() {
        let spec = "| Crate | Version |\n|---|---|\n| `tokio` | 1.53.1 |\n";
        let manifests = ["[dependencies]\ntokio = \"=1.53.1\"\n".to_owned()];
        // A second locked version of the same crate does not hide the pinned one.
        let got = check_repo(
            spec,
            &manifests,
            &lock(&[("tokio", "1.53.1"), ("tokio", "0.2.0")]),
            "[dependencies]\n",
            "",
        );
        assert_eq!(
            got,
            Report {
                checked: 1,
                misses: Vec::new()
            }
        );
    }

    #[test]
    fn reports_a_pinned_crate_missing_from_the_lock() {
        let spec = "| Crate | Version |\n|---|---|\n| `tokio` | 1.53.1 |\n";
        let manifests = ["[dependencies]\ntokio = \"1\"\n".to_owned()];
        let got = check_repo(spec, &manifests, "", "[dependencies]\n", "");
        assert_eq!(
            got.misses,
            ["pin-check: tokio pinned 1.53.1 (specs/0004), Cargo.lock has none"]
        );
    }

    #[test]
    fn checks_ebpf_lock() {
        let spec = "| Crate | Version |\n|---|---|\n| `aya-ebpf` | 0.2.1 |\n";
        let ebpf_manifest = "[dependencies]\naya-ebpf = \"0.2.1\"\n";
        // The workspace lock holds the pin. The eBPF lock does not. The eBPF lock decides.
        let got = check_repo(
            spec,
            &["[dependencies]\n".to_owned()],
            &lock(&[("aya-ebpf", "0.2.1")]),
            ebpf_manifest,
            &lock(&[("aya-ebpf", "0.2.2")]),
        );
        assert_eq!(got.checked, 1);
        assert_eq!(
            got.misses,
            ["pin-check: aya-ebpf pinned 0.2.1 (specs/0004), crates/cas-ebpf/ebpf/Cargo.lock has 0.2.2"]
        );
        // Control: the same manifest passes when the eBPF lock holds the pin.
        let ok = check_repo(
            spec,
            &["[dependencies]\n".to_owned()],
            "",
            ebpf_manifest,
            &lock(&[("aya-ebpf", "0.2.1")]),
        );
        assert!(ok.misses.is_empty(), "{:?}", ok.misses);
        assert_eq!(ok.checked, 1);
    }
}
