---
name: ci-triage
description: Use when a CI run or required check is red on a PR or on main. Shows how to find the failing job, read the last 30 lines of its log, and reproduce it with the matching make target.
---

# CI triage

A red check is a signal, not a verdict. Find the cause before you change code.

## Step 1: Find the failing job

```bash
gh pr checks <PR-number>
gh run list --branch <branch> --limit 5
```

Note the job name. The required jobs are `secrets`, `rust`, `cargo-deny` and `hygiene`. `moon-affected` and `template-check` also run.

## Step 2: Read the last 30 lines

```bash
gh run view <run-id> --log-failed | tail -30
```

Read the error text. Do not guess from the job name.

## Step 3: Map the job to a local command

| Job | Reproduce with |
|---|---|
| `rust` | `make ci` (use `JOBS=2` on a shared host) |
| `cargo-deny` | `make deny` |
| `hygiene` | `make hygiene` (runs `typos`, `machete`, `spec-check`, `toolchain-check`) |
| `secrets` | `gitleaks detect --config .gitleaks.toml` |
| `moon-affected` | `moon ci --base origin/main --downstream deep` |
| `template-check` | `make template-check` |

Inside `make ci`, the failing step is one of `fmt-check`, `clippy`, `test`, `doc-test`. Run that target alone.

## Step 4: Classify the failure

- **Code or spec fault.** The local run fails too. Fix it on your branch.
- **Toolchain drift.** `make toolchain-check` fails. `rust-toolchain.toml` must match the CI image Rust exactly. Spec 0005 names the image.
- **Cache or runner fault.** The local run passes and CI fails on a download, a timeout or a missing tool. Re-run once. If it repeats, report it to the operator with the log tail. Do not edit the workflow without the main session.
- **Flaky test.** Run the test five times locally. Report it. Do not add retries to hide it.

## Step 5: Report

State the job, the 30-line tail, the cause, and the fix or the next step. Use the evidence tiers: a local reproduction is `verified`. A guess is `heuristic`.

## Do not

- Do not merge with red CI.
- Do not skip a hook or a check to get green.
- Do not edit `.github/workflows/` unless your brief names it.
