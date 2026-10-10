---
status: planned
created: 2026-10-10
tags: [security, advisories, sbom, disclosure, release, ci]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0003-branch-model-and-promotion, 0004-dependency-and-licence-policy, 0005-ci-cache-and-runners, 0025-wave-execution-on-the-custody-chain, 0034-tool-pinning-beyond-cargo]
---

# 0045-security-disclosure-and-advisory-policy

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

`SECURITY.md` promises a private disclosure channel and says only the latest release on `main`
gets fixes. No spec owns that promise. Two tools judge RustSec advisories: `cargo deny check`
on every PR and `cargo audit` once a week. No spec says which one is authoritative, or what
happens when an advisory has no fixed version. `make sbom` writes a CycloneDX SBOM, and no spec
states its format or how long it is kept [reported: research lane w1X:p1, mesh 5871db01, ranked
second; integrating lane w25:p1 gap analysis, 2026-10-10].

This spec sets the advisory gate, the rule for an advisory with no fix, the SBOM format and
retention, the release block, and what `SECURITY.md` promises.

### Facts

Measured 2026-10-10 on ceres at development `6e7510e`.

- **F1 [verified]:** `SECURITY.md` (27 lines) names one channel, `chris@todie.io`, and the GitHub private vulnerability report "if it is enabled". It says "This repo sets no fixed response time", "Only the latest release on `main` gets security fixes", and that a secret found in the repo or its history is compromised and the owner rotates it. Command: `cat -n SECURITY.md`.
- **F2 [verified]:** Private vulnerability reporting (PVR) is unavailable on uas. `gh api repos/unsigned-gg/uas/private-vulnerability-reporting` returns HTTP 404, as it does for the private repos `unsigned-gg/allentech` and `unsigned-gg/cl`. Control: the public repos `unsigned-gg/herdr-dagr` and `unsigned-gg/herdr-norn` return `{"enabled":true}`. `gh api repos/unsigned-gg/uas --jq .private` prints `true`. GitHub offers PVR on public repos only [reported: GitHub docs]. The email is the only working channel today.
- **F3 [verified]:** Required checks, from `gh api repos/unsigned-gg/uas/branches/<branch>/protection --jq .required_status_checks.contexts`: `development` has `secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`; `main` has the first four. `audit` is required on neither (spec 0025 F21 agrees). The required `secrets` job runs gitleaks 8.30.1, pinned by version and sha256, over every reachable commit (`grep -n -A4 'name: gitleaks' .github/workflows/ci.yml`), as REQ-0034-03 requires (`grep -n 'REQ-0034-03' specs/0034-*.md`). It is the detector behind D3's secret rule.
- **F4 [verified]:** `.github/workflows/audit.yml` runs on cron `41 5 * * 1` (Monday 05:41 UTC) and on dispatch, with `contents: read`. It installs `cargo-audit` through `taiki-e/install-action` with no version, runs `make audit` (`cargo audit`), and fails the job on a finding. It opens nothing. `gh run list --workflow audit.yml` prints `[]`; control: the same command for `sbom.yml` lists three runs. The file is on `main` since `3a2e2d0` (2026-10-08, a Thursday; `git log -1 --format='%h %cs' origin/development -- .github/workflows/audit.yml` and `git ls-tree origin/main .github/workflows/`), so the first cron is Monday 2026-10-12. Zero runs is expected.
- **F5 [verified]:** CI runs cargo-deny 0.19.9 (`cargo deny --version` in the log of CI run 38093699822); ceres has 0.19.0. `deny.toml` `[advisories]` holds only `yanked = "deny"`. A scratch crate with `smallvec` 1.6.0 and `ansi_term` 0.12.1 under that section: `cargo deny check advisories` printed `error[vulnerability]` RUSTSEC-2021-0003, `error[unmaintained]` RUSTSEC-2021-0139 and `advisories FAILED` (0.19.0). The 0.19.9 source agrees: `vulnerability` is a deprecated key, so a vulnerability always fails, and `unmaintained` defaults to `Scope::All` (`src/advisories/cfg.rs` lines 107, 156, 283, fetched as in F6).
- **F6 [verified]:** cargo-deny cannot hold an expiry. An ignore entry with an `expires` key fails with `error[unexpected-keys]: found 1 unexpected keys, expected: ["id", "reason"]`, exit 1 (0.19.0). The 0.19.9 source has the same struct: `IgnoreId { id, reason }`, and `finalize` rejects other keys. Command: `gh api 'repos/EmbarkStudios/cargo-deny/contents/src/advisories/cfg.rs?ref=0.19.9'`. The 0.19.9 CHANGELOG has no "expir" (control: the same grep finds its `## [0.19.9]` heading).
- **F7 [verified]:** `cargo audit` 0.22.1 (ceres) on the same lockfile exits 1 for the vulnerability and reports the unmaintained crate as "1 allowed warning". It reads no `deny.toml`. `cargo audit --help` offers `--ignore <ADVISORY_ID>` and `-D, --deny` with `warnings`, `unmaintained`, `unsound`, `yanked`. On the uas lockfile, `cargo audit --deny warnings` exits 0 (1296 advisories loaded, 370 crates scanned). With `ansi_term = "=0.12.1"` added to `xtask`, plain `cargo audit` exits 0, `--deny warnings` exits 1 on RUSTSEC-2021-0139, and `--deny warnings --ignore RUSTSEC-2021-0139` exits 0.
- **F8 [verified]:** `sbom.yml` runs on push to `main` and uploads `sbom-<sha>` with `retention-days: 90`. `make sbom` runs `cargo cyclonedx --format json`. The log of run 38029204568 says `installing cargo-cyclonedx@latest`, which resolved to 0.5.9. Its artifact (`gh run download 38029204568`) holds 11 `*.cdx.json` files, one per package, and `jq .specVersion` prints `"1.3"`. The 0.5.9 README: `--spec-version` takes `1.3`, `1.4` or `1.5`, default 1.3. An `@latest` install breaks REQ-0034-03 and REQ-0034-04.
- **F9 [verified]:** `release.yml` runs release-plz on push to `main`. It has no advisory step and does not wait for CI. Release `v0.1.0` has no assets: `gh release view v0.1.0 --json assets` prints `[]`. The pinned release-plz action exposes `releases_created` (`action.yml` at `18641b6`). `make release-notes` runs no advisory check (Makefile lines 101-110).
- **F10 [verified]:** crates.io holds `uas` 0.1.0, published by `todie` at 2026-10-10T20:46Z, licence `MIT OR Apache-2.0`, repository `https://github.com/unsigned-gg/uas`. Its 621-byte README names no security contact. Command: `curl -s https://crates.io/api/v1/crates/uas | jq '.crate.max_version'`. No workspace package is named `uas` (`cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name'`), and `Cargo.toml` sets `publish = false` and `LicenseRef-Proprietary`. The crate is not built from this repo, and its repository link is private.

## Design

One tool gates: cargo-deny, on every PR, against one `deny.toml`. cargo-audit reads the same
advisory database once a week, with the same ignore list, so an advisory published between
pushes is seen before the next PR trips on it. An advisory with no fix gets a dated, ticketed
exception that expires. Releases stop while the gate is red. The SBOM ships with each release.

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | cargo-deny (`make deny`, required check `cargo-deny`, F3) is the authoritative advisory gate. The weekly `audit` job is a detector: `make audit` runs `cargo audit --deny warnings` with one `--ignore <id>` per `deny.toml` ignore entry. A red weekly run opens a tracked item (D7) and blocks no PR and no release by itself; the same advisory still stops a release through D5's own check. | **Adjusted:** cargo-audit reads no `deny.toml` and only warns on unmaintained crates (F7). Without the shared list and `--deny warnings`, the detector would stay red on every accepted exception and miss what the gate denies. | integrating lane w25:p1 brief, 2026-10-10; adjustment uas-engineer, 2026-10-10 (F7) |
| D2 | An advisory with no fixed version gets a `deny.toml` `[advisories] ignore` entry: one inline table per line, `{ id = "<RUSTSEC id>", reason = "expires YYYY-MM-DD; <ticket>; <why>" }`. The expiry is no more than 90 days after the day the entry is checked. `cargo xtask advisory-check` fails on a bare id, a malformed reason, a past expiry or one more than 90 days out. It runs in `make hygiene` (required on both branches, F3) and in the weekly `audit` job. | **Adjusted:** the brief had an expired entry fail cargo-deny. cargo-deny 0.19.9 accepts only `id` and `reason` and rejects any other key (F6), so the expiry lives in `reason` and an xtask enforces it. The weekly run catches an entry that expires while no PR lands. | integrating lane w25:p1 brief, 2026-10-10; adjustment uas-engineer, 2026-10-10 (F6) |
| D3 | `SECURITY.md` states: report privately to `chris@todie.io`, and through PVR where the repo offers it; acknowledgement within 5 business days [proposed: integrating lane w25:p1, 2026-10-10; awaits Christian]; fixes for the latest release on `main` only (`SECURITY.md` line 23; spec 0003 lines 13-14 and 39 make `main` the release branch); a secret found in the repo or its history is compromised and is rotated by its owner, through the operator gate for secrets (AGENTS.md, Approval routes). A fix ships through the normal spec 0003 flow. | **Adjusted:** the brief put PVR first and the email as fallback. PVR is unavailable while uas is private (F2), so the email is the channel and PVR is conditional. The acknowledgement target replaces "no fixed response time" (F1). | integrating lane w25:p1 brief, 2026-10-10; estate rule "exposed secret is compromised"; adjustment uas-engineer, 2026-10-10 (F2); the acknowledgement target is Christian's to rule |
| D4 | The SBOM is CycloneDX 1.5 JSON, one file per workspace package, written by `make sbom` with `--spec-version 1.5` and cargo-cyclonedx 0.5.9 pinned in spec 0004 (spec 0034). `sbom.yml` keeps its artifact 90 days. `release.yml` attaches the same files to every GitHub release it creates; a release asset lives as long as the release. | **Adjusted:** today the files are 1.3, from an unpinned `@latest` install, and `v0.1.0` has no assets (F8, F9). | integrating lane w25:p1 brief, 2026-10-10; adjustment uas-engineer, 2026-10-10 (F8) |
| D5 | A release is blocked while advisories fail on `main`. `release.yml` runs `cargo deny check advisories` before the release-plz step. `make release-notes` runs it first and refuses before it edits a file. | `main` already requires `cargo-deny` on the promotion PR (F3). `release.yml` does not wait for CI (F9), and the advisory database changes between the PR check and the push, so the step lives in the workflow. release-plz tags on the next green push, since it tags any untagged version. | integrating lane w25:p1 brief, 2026-10-10 |
| D6 | The disclosure policy applies now, to the private repo. Going public changes nothing in this spec: D3's PVR clause turns on once the repo offers it. | Christian keeps uas private. | Christian, 2026-10-10 [reported: cortex lane w10:p0 relay, mesh ead30203] |
| D7 | The tracked item of D1 is one GitHub issue in `unsigned-gg/uas` titled `cargo audit: advisories on main`. A red run comments on it if it is open and opens it if not. `audit.yml` gains `issues: write` and the pinned `gh` install step of `release.yml` (spec 0004, CI tools). | Linear is the tracker of record, but a Linear write from CI needs a new secret, and secrets are operator-gated. | proposed, uas-engineer, 2026-10-10; Christian may rule Linear instead |
| D8 | The crates.io crate `uas` 0.1.0 is not built from this workspace (F10). "Latest release on `main`" does not cover it, its repository link is private, and its README names no contact. | A reporter who finds the crate cannot reach `SECURITY.md`. | Christian to rule: publish a README with the reporting address, or keep it as a name hold |

### Requirements (EARS)

- **[REQ-0045-01]** (Ubiquitous): The branch protection of `development` and `main` shall list `cargo-deny` as a required check, and `make deny` shall run `cargo deny check` against the repo's `deny.toml`.
- **[REQ-0045-02]** (Event-driven): WHEN the weekly `audit` workflow runs, it shall run `cargo xtask advisory-check` and `make audit`, and `make audit` shall run `cargo audit --deny warnings` with one `--ignore` per `deny.toml` ignore id.
- **[REQ-0045-03]** (Unwanted): IF the weekly `audit` run fails, THEN the workflow shall open or comment on the D7 issue, and it shall block no PR and no release.
- **[REQ-0045-04]** (Ubiquitous): Every `[advisories] ignore` entry in `deny.toml` shall be one inline table on one line with an `id` and a `reason` of the form `expires YYYY-MM-DD; <ticket>; <why>`.
- **[REQ-0045-05]** (Unwanted): IF an ignore entry is a bare id, has a malformed reason, an expiry before the check date, or an expiry more than 90 days after it, THEN `cargo xtask advisory-check` shall exit non-zero and name the entry.
- **[REQ-0045-06]** (Event-driven): WHEN `make release-notes` runs, it shall run `cargo deny check advisories` first, and it shall exit non-zero without editing a file if that check fails.
- **[REQ-0045-07]** (State-driven): WHILE `cargo deny check advisories` fails on the `main` checkout, the `release` workflow shall create no tag and no GitHub release.
- **[REQ-0045-08]** (Event-driven): WHEN the `release` workflow creates a GitHub release, it shall attach the CycloneDX 1.5 JSON SBOM file of every workspace package to that release.
- **[REQ-0045-09]** (Ubiquitous): `make sbom` shall run the cargo-cyclonedx version pinned in spec 0004 with `--format json --spec-version 1.5`, and the `sbom` workflow shall keep its artifact for 90 days.
- **[REQ-0045-10]** (Ubiquitous): `SECURITY.md` shall name `chris@todie.io` as the private channel, the acknowledgement target D3 names, fixes for the latest `main` release only, and the rule that a secret found in the repo or its history is compromised and rotated.
- **[REQ-0045-11]** (Optional feature): WHERE the repository offers GitHub private vulnerability reporting, `SECURITY.md` shall name it as a second private channel beside the email.

## Plan

- [ ] `xtask`: an `advisory-check` subcommand. A line scanner over `deny.toml` `[advisories] ignore`, so no new dependency (xtask depends on `anyhow` only). Flags: `--today YYYY-MM-DD` (default: UTC date), `--config <path>` (default: `deny.toml`), `--ids` (print the ids, one per line). Fixtures under `xtask/tests/fixtures/advisory/`: `valid.toml`, `expired.toml`, `too-far.toml`, `bare-id.toml`, `no-ticket.toml` (D2).
- [ ] `Makefile`: `hygiene` gains `advisory-check`; `audit` becomes `cargo audit --deny warnings` plus `--ignore` for each id from `cargo xtask advisory-check --ids`; `release-notes` runs `cargo deny check advisories` as its first line; `sbom` gains `--spec-version 1.5` (D1, D2, D4, D5).
- [ ] `audit.yml`: pin `cargo-audit` by version; add `issues: write`, the pinned `gh` step from `release.yml`, `cargo xtask advisory-check`, and the D7 issue step on failure.
- [ ] `sbom.yml`: install `cargo-cyclonedx@0.5.9` (D4).
- [ ] `release.yml`: a `cargo deny check advisories` step before release-plz (D5). After it, `if: steps.<release-plz id>.outputs.releases_created == 'true'`: install pinned cargo-cyclonedx, `make sbom`, `gh release upload v<version>` with every `*.cdx.json` (D4).
- [ ] Spec 0004 CI tools table: rows for `cargo-audit` and `cargo-cyclonedx` 0.5.9 (spec 0034 plan names the second).
- [ ] `SECURITY.md`: the D3 text; drop "no fixed response time". The acknowledgement target waits for Christian's ruling on D3.

## Test

| Requirement | Negative Control |
|---|---|
| REQ-0045-01 | `gh api repos/unsigned-gg/uas/branches/main/protection --jq .required_status_checks.contexts` lists `cargo-deny`. The F5 scratch crate under the repo's `deny.toml` fails `cargo deny check advisories`. Goes red if `cargo-deny` leaves the list. |
| REQ-0045-02 | `grep -n -e 'cargo xtask advisory-check' -e 'make audit' .github/workflows/audit.yml` prints both steps. In a scratch worktree, commit `ansi_term = "=0.12.1"` in `xtask`; `make audit` exits non-zero. Add a D2 entry for RUSTSEC-2021-0139 to that worktree's `deny.toml`, expiring within 90 days; `make audit` then exits 0. Goes red if `--deny warnings` is dropped (the first run exits 0) or the ignore list is not passed (the second run exits non-zero), as F7 measured. |
| REQ-0045-03 | Dispatch `audit.yml` on a branch whose lockfile holds `smallvec` 1.6.0; the D7 issue gains a comment. Goes red if `issues: write` is missing: the step gets HTTP 403. |
| REQ-0045-04, 05 | `advisory-check --today 2026-10-10` fails on `expired.toml` (expires 2026-10-09), `too-far.toml` (2027-01-09, 91 days out), `bare-id.toml` and `no-ticket.toml`, and passes on `valid.toml` (2027-01-08). Goes red if the date compare is removed. |
| REQ-0045-06 | In a scratch worktree, commit `smallvec = "=1.6.0"` in `xtask` first; then `make release-notes VERSION=9.9.9` exits non-zero and `git diff --quiet` holds. Goes red if the guard is removed: the target edits `Cargo.toml` and `CHANGELOG.md`. |
| REQ-0045-07 | Dispatch `release.yml` on that scratch branch; the job fails at the advisory step. Goes red if the step is missing: the job passes (release-plz is skipped off `main`). |
| REQ-0045-08, 09 | After the next release, `gh release view v<version> --json assets` lists every `*.cdx.json`, and `jq -r .specVersion` on each prints `1.5`. Red today: `v0.1.0` has no assets and the main artifact prints `1.3` (F8, F9). |
| REQ-0045-10 | `grep -c` for the acknowledgement target D3 names prints 1 on `SECURITY.md`. Red today: F1 says "no fixed response time". |
| REQ-0045-11 | Not exercisable while F2 holds: the PVR endpoint returns 404 on a private repo. Re-run the F2 command at go-public; on `{"enabled":true}`, `SECURITY.md` names PVR. |

```bash
cargo nextest run -p xtask advisory -j 2 && make deny hygiene JOBS=2
```

Passing means exit 0: each failing fixture fails inside its test, `valid.toml` passes, and
`deny.toml` passes `advisory-check` and `cargo deny check`.

## Notes

- Considered and parked: a weekly `cargo deny check advisories` in place of cargo-audit. It
  would use one tool and one config. This spec keeps cargo-audit because the brief names it;
  D1's shared ignore list closes the gap F7 shows.
- Spec 0004 lines 197-198 already reject `syntect` and `yrs` over unmaintained advisories. D2
  is the route if one of them is ever taken with no fix.
- Parked for Christian: D3's acknowledgement target, D8, and the licence mismatch of F10
  (crate `MIT OR Apache-2.0`, workspace `LicenseRef-Proprietary`), which is spec 0004's subject.
- Parked: enabling PVR is an operator step at go-public time; this spec does not track it.
- ceres runs cargo-deny 0.19.0 and CI runs 0.19.9 (F5). The F5 and F6 runs used 0.19.0; F6 is
  confirmed in 0.19.9 source. Aligning the local version is spec 0034's subject.
- Out of scope: CVE assignment, a GitHub security advisory per fix, and signing the SBOM.
