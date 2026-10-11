---
status: planned
created: 2026-10-11
tags: [ci, github-actions, nextest, labeler, branch-protection, runners]
priority: medium
depends_on: [0003-branch-model-and-promotion, 0005-ci-cache-and-runners, 0051-repository-guards, 0053-changelog-and-commit-text-policy]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0054-ci-pipeline-policy

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

Eight workflow files, one labeler map and one nextest file make up the CI pipeline. Spec 0005
owns the runner lanes, the cache and the release flow, 0051 owns `fork-gate`, 0053 owns
`text-check`. No spec owns the pipeline as one contract: which events start it, what cancels a
run, which jobs a fork PR skips, what gates a merge, and what a job may assume of its runner.
This spec owns that contract and adds a test that turns red when a workflow drifts from it. the operator ruled on 2026-10-11 about 05:50Z that this spec is drafted [reported:
AskUserQuestion in lane w25:p1, label "CI pipeline policy"]. Content decisions are the
integrating lane's unless marked proposed.

The pipeline is healthy today. The gaps are in the docs around it: three documents list the
required checks wrongly (F7), one glob matches no file (F5), one run line interpolates a ref (F11).

### Facts

Read 2026-10-11 at `origin/development` ebc7be8. Line numbers are for that commit.

- **F1 [verified]:** triggers. `ci.yml` lines 7 to 11: `pull_request` (no branch filter, so
  default types opened, synchronize, reopened), `push` to `development` and `main`, and
  `workflow_dispatch`. `semver.yml` 6 to 8: `pull_request`, dispatch. `labeler.yml` 4 to 6:
  `pull_request` types opened, synchronize, reopened. `codeql.yml` 13 to 19: pull_request, push
  to both branches, weekly cron `17 6 * * 1`, dispatch. `audit.yml` 6 to 9: cron `41 5 * * 1`,
  dispatch. `sbom.yml` 5 to 8: push to `main`, dispatch. `release.yml` 13 to 16: push to `main`,
  dispatch. `_rust.yml` 16 to 27: `workflow_call` only. Command: `grep -n -A8 '^on:' .github/workflows/*.yml`.
- **F2 [verified]:** concurrency. `ci.yml` 16 to 18 and `semver.yml` 13 to 15 set group
  `<name>-${{ github.ref }}` and cancel only when the event is `pull_request`. `release.yml`
  21 to 23 sets group `release` with no cancel. `labeler`, `codeql`, `audit`, `sbom` and
  `_rust.yml` set none.
- **F3 [verified]:** fork guard. Seven `ci.yml` jobs carry `head.repo.full_name == github.repository`
  (lines 29, 49, 58, 71, 91, 136, 158); `text-check` also requires `pull_request`. `semver.yml`
  21, `labeler.yml` 14 and `codeql.yml` 29 carry it. `_rust.yml` has none: its caller's `if`
  (`ci.yml` 49) skips the called job. `fork-gate` (`ci.yml` 225 to 266) has none by design and
  runs on `ubuntu-24.04`.
- **F4 [verified]:** no `paths` or `paths-ignore` key in any workflow
  (`grep -rn 'paths' .github/workflows` prints nothing). A docs-only PR runs all eight `ci.yml`
  jobs. `moon-affected` filters itself: it exits 0 when no task is affected (`ci.yml` 121 to 126).
- **F5 [verified]:** `.github/labeler.yml` defines five labels: `crates`, `specs`, `ci`, `docs`,
  `agents`. All five exist in the repository (`gh label list`). `sync-labels: true`
  (`labeler.yml` 23). Glob `.lean-spec` matches no tracked file. These tracked areas match no
  glob: `.config/`, `.cargo/`, `scripts/`, `.saga/`, `tasks/`, six files under `.github/` outside
  `workflows/`, `deny.toml`, `release-plz.toml`, `rust-toolchain.toml`, `_typos.toml`,
  `.gitleaks.toml`. No workflow reads a PR label except `fork-gate`, which reads
  `fork-ci-approved` (`ci.yml` 250 to 260).
- **F6 [verified]:** `.config/nextest.toml` lines 1 to 12: `default` has `fail-fast = true`; `ci`
  has `retries = 1`, `fail-fast = false` and `[profile.ci.junit] path = "junit.xml"`; `pre-push`
  has `retries = 2`. `Makefile` 9 sets the default profile to `default`; line 47 runs `ci` for
  CI. `_rust.yml` 87 to 94 uploads `target/nextest/<profile>/junit.xml` with
  `if-no-files-found: ignore`, 14 days. Nothing calls `pre-push`: `grep -rn 'pre-push'` outside
  specs prints the definition only. The latest `ci` junit has 706 testcases and no `flaky` element.
- **F7 [verified]:** required checks. `gh api repos/unsigned-gg/uas/branches/development/protection
  --jq .required_status_checks.contexts` prints six: `secrets`, `rust / cargo`, `cargo-deny`,
  `hygiene`, `ebpf-live`, `fork-gate`. The same call on `main` prints four, without `ebpf-live`
  and `fork-gate`. `strict` is false, approvals 0, admins enforced. Stale copies: spec 0003's
  table says `fork-gate` is not required (PR #194 corrects it); spec 0005 says four required checks and lists
  `template-check.yml`, which does not exist here; `ci.yml` lines 2 to 3 list five and defer
  `fork-gate`. The context `rust / cargo` is the caller job `rust` plus the called job `cargo`.
- **F8 [verified]:** runners. Every job but one uses group `ent-default`; labels are
  `self-hosted` (`secrets`, `labeler`), `ci-rust` (10 jobs) and `ci-kernel` (`ebpf-live`).
  `fork-gate` uses `ubuntu-24.04`. `.github/actionlint.yaml` lists `ci-rust`, `ci-kernel` and
  `reverie`; no job uses `reverie`. Lane contents are spec 0005's table. Facts jobs may rely
  on: baked toolchain and read-only rustup (`_rust.yml` 6 to 13), ephemeral pods without
  volumes (`_rust.yml` 10 to 11), no `gh` in the `ci-rust` image (`release.yml` 39 to 42).
- **F9 [verified]:** timeouts, minutes: `secrets` 10, `rust / cargo` 45, `cargo-deny` 15,
  `hygiene` 20, `moon-affected` 30, `text-check` 10, `ebpf-live` 45, `fork-gate` 5, `labeler` 5,
  `semver-checks` 30, `audit` 15, `codeql` 60, `sbom` 20, `release` 20.
- **F10 [verified]:** manual and scheduled runs. `release.yml` 71 to 74 tags only on
  `refs/heads/main`, so a dispatch elsewhere tags nothing. `fork-gate` exits 0 when the event is
  not `pull_request` (`ci.yml` 246 to 249). `text-check` is skipped on dispatch. The 1000
  latest runs span 23 hours: one `workflow_dispatch`, no `schedule` run, so no cron fired.
- **F11 [verified]:** two run lines hold `${{`: `_rust.yml` 85 (`inputs.jobs`,
  `inputs.nextest-profile`, set by an in-repo caller) and `ci.yml` 118 (`github.base_ref`).
  A branch name needs write access to create, so the risk is low.
- **F12 [verified]:** `actionlint -config-file .github/actionlint.yaml` exits 0 on the tree, but
  no workflow, Makefile target or lefthook stage runs it (`grep -rn actionlint .github Makefile lefthook.yml`).

#### Baseline (measured 2026-10-11)

Command: `gh run list -R unsigned-gg/uas --limit 1000 --json name,event,conclusion,createdAt`.
The window is 2026-10-10 06:29Z to 2026-10-11 05:48Z, so it covers one day of a busy lane and
is not a long-run rate. Per-job seconds come from `gh run view <id> --json jobs` on the 20
latest `CI` runs (1 cancelled run dropped per job as shown by n).

| Workflow, event | Runs | Success | Failure | Cancelled |
|---|---|---|---|---|
| CI, pull_request | 234 | 209 | 10 | 15 |
| CI, push | 131 | 129 | 0 | 1 (1 has no conclusion) |
| semver, pull_request | 26 | 25 | 0 | 1 |
| labeler, pull_request | 233 | 233 | 0 | 0 |
| codeql, both events | 364 | 0 (364 skipped) | 0 | 0 |

| Job | n | Median s | Max s |
|---|---|---|---|
| secrets | 19 | 5 | 5 |
| fork-gate | 19 | 3 | 4 |
| cargo-deny | 19 | 8 | 10 |
| hygiene | 19 | 10 | 11 |
| text-check | 13 | 7 | 8 |
| moon-affected | 19 | 9 | 47 |
| rust / cargo | 18 | 28 | 29 |
| ebpf-live | 18 | 43 | 49 |

The longest job is 49 s against a 45 minute timeout. The causes of the 10 PR failures are not
classified here, so the flake rate is unmeasured: `retries = 1` can hide a flake behind a
green job.

## Design

### Decisions

Classes: `ruled` (the operator decided it), `lane-decided` (the integrating lane decided it on
2026-10-11 and the operator may override), `proposed: <who>` (not decided; the named party rules).

| # | Decision | Why | Class |
|---|---|---|---|
| D1 | The trigger set of F1 stays. `ci.yml` runs on every PR whatever its base, on push to the two long-lived branches, and on dispatch. A new workflow that gates a merge triggers on `pull_request`. | A PR into a stacked branch needs the same gates. Push runs verify the merge result. | lane-decided |
| D2 | `ci.yml` and `semver.yml` group by ref and cancel only on `pull_request`, so a newer push to a PR branch replaces the older run and a push to `development` or `main` is never cancelled by a later one. `release.yml` serializes and never cancels. Other workflows set no group. | A push to a PR branch makes the older run stale. A release must finish. GitHub keeps one pending run per group, so a third push can drop a queued second push run [asserted: GitHub docs]; the PR run is the gate, so that loss is accepted. | lane-decided |
| D3 | Every job of a `pull_request` workflow carries the same-repo condition of F3, except `fork-gate`. A fork PR therefore skips every self-hosted job. A skipped job counts as passing for a required check [asserted: GitHub docs], so `fork-gate` is what turns a fork PR red; 0051 D8 owns that rule and this spec does not restate it. A called workflow relies on its caller's `if`. | The condition keeps fork code off the self-hosted pods. | lane-decided (0051 D8 ruled) |
| D4 | No workflow uses `paths` or `paths-ignore`. A docs-only PR runs the full pipeline. Cost: the 113 median job-seconds of F9's table, 43 s of them on the single `ci-kernel` runner, and 49 s of wall time. The labeler map is not a filter source: its job runs in a separate workflow, so labels land after the checks start, and F5 leaves many files unlabeled. | A workflow skipped by a path filter leaves its required check unreported and blocks the merge [asserted: GitHub docs]. The cost is small. `moon-affected` already skips its own work. | lane-decided |
| D5 | `labeler.yml` is a contract. Each label exists in the repository. Each glob matches a tracked file. `fork-ci-approved` is never defined there, because a person applies it (0051 D8). The `ci` label also covers the files that change pipeline behaviour: `.config/**`, `.cargo/**`, `scripts/**`, `.github/**`, `deny.toml`, `release-plz.toml`, `rust-toolchain.toml`, `_typos.toml`, `.gitleaks.toml`. The dead glob `.lean-spec` goes (Plan P1). Labels inform; no gate reads them. | F5. A label that misses `.config/nextest.toml` hides a pipeline change from a reader scanning labels. | lane-decided |
| D6 | The profiles stay as F6 shows. CI runs `ci`: `retries = 1`, `fail-fast = false`, junit at `target/nextest/ci/junit.xml`. Retry is the flake policy: a test that passes only on retry is a defect. The integrating lane files it. `pre-push` has no caller (lefthook runs `cargo test`, not nextest, and no Make target names it), so the profile is dropped (Plan P5). | A retry keeps one flake from blocking a PR; a filed defect keeps it from staying. | lane-decided; the `pre-push` drop: lane-decided: integrating lane w25:p1, 2026-10-11 |
| D7 | The required checks are the contexts of F7: six on `development`, four on `main`. A job becomes required by the 0053 D10 form: it lands non-required, runs seven days on pull requests with no false failure, then the lane adds it to the protection, to the spec 0003 table and to the `ci.yml` header in one landing. A job name or caller job id that is a required context is part of the contract. Renaming or removing one needs the protection edit in the same landing. `moon-affected`, `text-check` until D10 of 0053 fires, `semver-checks`, `labeler` and `codeql` stay non-required. | A renamed context leaves the old one pending forever. F7 shows three copies already stale. | ruled: the operator, 0053 D10, lane w25:p1, 2026-10-11 about 03:40Z, for the date form; the rename rule is lane-decided |
| D8 | A job names one runner from F8. A job on `ci-rust` may assume the baked toolchain, 10 CPU and no `gh`. It may not assume a toolchain install, a persistent disk or `sudo`. `ebpf-live` runs as uid 0 with no `sudo` (0005 lane table). `fork-gate` is the only hosted job, holds no secret and checks out nothing. A new job takes one of the existing labels, so `actionlint.yaml` stays true. | The lane facts are 0005's. This row fixes which jobs may rely on them. | lane-decided |
| D9 | Every job sets `timeout-minutes`. Raising one needs a measurement in the PR body. | A hung job holds a pod; `ebpf-live` runs one pod at a time (0005). | lane-decided; `ebpf-live` goes from 45 to 15 minutes: lane-decided: integrating lane w25:p1, 2026-10-11 (maximum measured 49 s) |
| D10 | `workflow_dispatch` is allowed on any workflow and runs the same jobs as a push. A workflow with a `schedule` also has dispatch, and a scheduled workflow is never a required check. `audit` and `codeql` run weekly on Monday. Nothing in the repository alerts on a red scheduled run. | Dispatch is the manual re-run path for a flaky scheduled job. A required scheduled check would block merges on a calendar event. | lane-decided; who watches a red `audit` is open (see Notes) |
| D11 | A `run:` line interpolates `${{` only for `inputs.*` of a called workflow. `github.event.*`, `head_ref`, `base_ref` and `ref_name` reach the shell through `env:`. One exception stands until Plan P2 removes it: `ci.yml` 118. | F11. 0053 REQ-14 set the pattern. | lane-decided; the `ci.yml` 118 fix: lane-decided: integrating lane w25:p1, 2026-10-11 |
| D12 | Every workflow sets top-level `permissions: contents: read`. A job raises a scope only when it writes: `labeler` (`pull-requests: write`), `release` (`contents: write`), `codeql` (`security-events: write`). | A default token with write scope turns any injected step into a write. | lane-decided |
| D13 | `actionlint` joins `make hygiene` so CI runs it, after a spec 0004 CI-tools row pins it by version and sha256, as for `cargo-semver-checks`. | F12: the tool passes today and runs nowhere. | lane-decided: integrating lane w25:p1, 2026-10-11 |
| D14 | The `junit` step in `_rust.yml` also writes the count of `flakyFailure` elements to the step summary. It does not fail the job. | D6 needs a count to file a defect from, and F6 shows none is recorded. | lane-decided: integrating lane w25:p1, 2026-10-11 |

### Requirements (EARS)

- **[REQ-0054-01]** (Ubiquitous): The `ci.yml` workflow shall trigger on `pull_request`, on
  `push` to `development` and `main`, and on `workflow_dispatch` (0054 D1).
- **[REQ-0054-02]** (Ubiquitous): The `ci.yml` and `semver.yml` workflows shall each declare a
  concurrency group that contains `github.ref` and cancel in progress only for `pull_request`
  (0054 D2).
- **[REQ-0054-03]** (Ubiquitous): The `release.yml` workflow shall serialize on group `release`
  and shall not cancel in progress (0054 D2).
- **[REQ-0054-04]** (Unwanted behavior): IF a job of a `pull_request` workflow other than
  `fork-gate` lacks the same-repo condition, THEN the test shall fail (0054 D3).
- **[REQ-0054-05]** (Event-driven): WHEN a fork PR opens, every self-hosted job shall be skipped
  and only `fork-gate` shall decide the PR (0054 D3, 0051 D8).
- **[REQ-0054-06]** (Unwanted behavior): IF any workflow declares `paths` or `paths-ignore`,
  THEN the policy is broken and the reviewer shall reject the change (0054 D4).
- **[REQ-0054-07]** (Ubiquitous): The labels that `labeler.yml` defines shall each exist in the
  repository, every glob shall match a tracked file, and `fork-ci-approved` shall not be defined
  there (0054 D5).
- **[REQ-0054-08]** (Event-driven): WHEN a PR changes a file under `.config/`, `.cargo/` or
  `scripts/`, the labeler shall apply `ci` (0054 D5).
- **[REQ-0054-09]** (Ubiquitous): The nextest profile `ci` shall set `retries = 1`,
  `fail-fast = false` and a junit path of `junit.xml`, and `_rust.yml` shall upload that file
  (0054 D6).
- **[REQ-0054-10]** (Event-driven): WHEN a CI run reports a test that passed only on retry, the
  integrating lane shall file a defect for that test (0054 D6).
- **[REQ-0054-11]** (Ubiquitous): The required checks on `development` shall be exactly
  `secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live` and `fork-gate`, and each
  shall resolve to a job in `ci.yml` (0054 D7).
- **[REQ-0054-12]** (Event-driven): WHEN a job is made a required check, the lane shall have run
  it seven days on pull requests with no false failure, and shall update the protection, the
  spec 0003 table and the `ci.yml` header in one landing (0054 D7, 0053 D10).
- **[REQ-0054-13]** (Ubiquitous): The jobs shall each run on `ubuntu-24.04` (`fork-gate` only) or on
  group `ent-default` with exactly one of the labels `self-hosted`, `ci-rust` and `ci-kernel`,
  and shall set `timeout-minutes` (0054 D8, D9).
- **[REQ-0054-14]** (Unwanted behavior): IF a `run:` line interpolates `${{` for anything other
  than `inputs.*` of a called workflow, THEN the test shall fail, apart from the one named
  exception (0054 D11).
- **[REQ-0054-15]** (Ubiquitous): The workflows shall each set top-level `permissions` to
  `contents: read` (0054 D12).
- **[REQ-0054-16]** (Optional feature): WHERE a workflow has a `schedule` trigger, it shall also
  have `workflow_dispatch` and shall not be a required check (0054 D10).
- **[REQ-0054-17]** (Event-driven): WHEN `release.yml` runs by dispatch on a ref other than
  `main`, it shall create no tag (0054 D10, `release.yml` 71 to 74).

## Plan

- [ ] P1, `.github/labeler.yml`: drop `.lean-spec`; add the D5 paths to `ci`. Then delete the
      `.lean-spec` exception from Test. Red first: Test row 4 with the glob left in.
- [ ] P2, `ci.yml`: move `github.base_ref` at line 118 into an `env:` entry; rewrite the header
      lines 2 to 3 to the F7 list and the non-required jobs (PR #194 does not touch the header). Delete the `moon-affected`
      exception from the Test script. One PR with P1.
- [ ] P3, amend spec 0003 (`fork-gate` row, and `main` stays at four) and spec 0005 (the
      required-check sentence, and the workflow table: drop `template-check.yml`, add `semver`
      and `text-check`). PR #194 already corrects the 0003 `fork-gate` row (development yes, main no); 0005 remains.
- [ ] P4, D9: `ebpf-live` `timeout-minutes` 45 to 15.
- [ ] P5, D6: remove `[profile.pre-push]` from `.config/nextest.toml` and the 0002 mention.
- [ ] P6, D13 and D14: pin `actionlint` in spec 0004, then add it to `make hygiene`; add the
      flaky count to the `junit` step.

## Test

The block runs today from the repo root with `gh`, `git` and Python 3.11 with PyYAML. It
changes no file and clears every `GIT_*` variable before `git` runs. Row 0 is the pipeline as it
stands. Rows 1 to 7 plant a fault in memory and print `ok` only when the check catches it, so a
check that passes everything turns them red and one that fails everything turns row 0 red.

```bash
ctx=$(gh api repos/unsigned-gg/uas/branches/development/protection --jq '.required_status_checks.contexts|join("|")')
lbl=$(gh label list -R unsigned-gg/uas --limit 100 --json name --jq 'map(.name)|join("|")')
python3 -I - "$ctx" "$lbl" <<'PY'
import copy, glob, os, re, subprocess, sys, tomllib, yaml
W = {f: yaml.safe_load(open(f)) for f in sorted(glob.glob(".github/workflows/*.yml"))}
S = dict(wf=W, live=sys.argv[1].split("|"), labels=sys.argv[2].split("|"),
         lab=yaml.safe_load(open(".github/labeler.yml")), nx=tomllib.load(open(".config/nextest.toml", "rb")))
SIX = ["cargo-deny", "ebpf-live", "fork-gate", "hygiene", "rust / cargo", "secrets"]
ENV = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
def tracked(g):
    return subprocess.run(["git", "ls-files", "--", ":(glob)" + g], env=ENV, capture_output=True, text=True).stdout.split()
def check(s):
    bad, wf = [], s["wf"]
    for f, d in wf.items():
        on = d.get("on", d.get(True)) or {}
        if d.get("permissions") != {"contents": "read"}: bad.append(f"permissions: {f}")
        for jn, j in d["jobs"].items():
            if "runs-on" in j:
                if "timeout-minutes" not in j: bad.append(f"timeout: {f} {jn}")
                ro = j["runs-on"]
                if isinstance(ro, str):
                    if jn != "fork-gate" or ro != "ubuntu-24.04": bad.append(f"runner: {f} {jn}")
                elif ro.get("group") != "ent-default" or ro.get("labels") not in (["self-hosted"], ["ci-rust"], ["ci-kernel"]):
                    bad.append(f"runner: {f} {jn}")
            if "pull_request" in on and jn != "fork-gate" and "head.repo.full_name" not in str(j.get("if", "")):
                bad.append(f"fork guard: {f} {jn}")
            for st in j.get("steps", []):
                r = re.sub(r"\$\{\{\s*inputs\.[\w-]+\s*\}\}", "", st.get("run", ""))
                if jn == "moon-affected": r = r.replace("${{ github.base_ref || 'development' }}", "")
                if "${{" in r: bad.append(f"run interpolation: {f} {jn}")
    for f in (".github/workflows/ci.yml", ".github/workflows/semver.yml"):
        c = wf[f].get("concurrency") or {}
        if "github.ref" not in str(c.get("group")) or "pull_request" not in str(c.get("cancel-in-progress")): bad.append(f"concurrency: {f}")
    names = set()
    for jn, j in wf[".github/workflows/ci.yml"]["jobs"].items():
        n = j.get("name", jn)
        if "uses" in j:
            for cn, cj in wf[j["uses"].removeprefix("./")]["jobs"].items(): names.add(f"{n} / {cj.get('name', cn)}")
        else: names.add(n)
    if sorted(s["live"]) != SIX or not set(s["live"]) <= names: bad.append("required contexts")
    for lab, rules in s["lab"].items():
        if lab not in s["labels"] or lab == "fork-ci-approved": bad.append(f"label: {lab}")
        for g in rules[0]["changed-files"][0]["any-glob-to-any-file"]:
            if not tracked(g) and g != ".lean-spec": bad.append(f"dead glob: {g}")
    p = s["nx"]["profile"]["ci"]
    if p.get("retries") != 1 or p.get("fail-fast") is not False or p["junit"]["path"] != "junit.xml": bad.append("nextest ci")
    return bad
RC = 0
def row(n, name, mutate):
    global RC
    s = copy.deepcopy(S); mutate(s); got = check(s)
    ok = (not got) if n == 0 else any(name in b for b in got)
    RC |= not ok
    print(f"row {n}: " + ("ok" if ok else f"RED {got}"))
J = lambda s, f, j: s["wf"][".github/workflows/" + f]["jobs"][j]
row(0, "", lambda s: None)
row(1, "concurrency", lambda s: s["wf"][".github/workflows/ci.yml"].pop("concurrency"))
row(2, "fork guard", lambda s: J(s, "ci.yml", "cargo-deny").pop("if"))
row(3, "run interpolation", lambda s: J(s, "ci.yml", "hygiene")["steps"][-1].update(run="echo ${{ github.head_ref }}"))
row(4, "dead glob", lambda s: s["lab"]["ci"][0]["changed-files"][0]["any-glob-to-any-file"].append("nope/**"))
row(5, "required contexts", lambda s: s["live"].append("text-check"))
row(6, "timeout", lambda s: J(s, "ci.yml", "secrets").pop("timeout-minutes"))
row(7, "nextest ci", lambda s: s["nx"]["profile"]["ci"].update(retries=0))
sys.exit(RC)
PY
```

| REQ | Row or check |
|---|---|
| 02 | 1 (`release.yml` group: by hand) |
| 04, 05 | 2 |
| 06 | `! grep -rnE '^\s*paths(-ignore)?:' .github/workflows` |
| 07, 08 | 4, and row 0 for the label and `fork-ci-approved` clauses |
| 09 | 7 |
| 11, 13 | 5, 6 and row 0 (live protection, runners, labels) |
| 14, 15 | 3 and row 0 |
| 01, 03, 10, 12, 16, 17 | manual, reviewer or lane: read the `on:` blocks, `release.yml` 71 to 74, the junit summary of D14 and the seven-day date |

## Notes

- Ruling that this spec is drafted: the operator, lane w25:p1, 2026-10-11 about 05:50Z
  [reported: lane brief]. No other ruling is pending.
- Open: does the label `self-hosted` also match `ci-rust` and `ci-kernel` pods? `secrets` and
  `labeler` use it (`ci.yml` 30). Spec 0005 says it is for jobs without Rust. This spec cannot
  read the runner scale sets; the paas lane can.
- Open: who reads a red `audit` run? The repository has no alert (D10).
- Open: the claims marked `[asserted: GitHub docs]` (skipped counts as passing, path filters
  leave a check pending, one pending run per group) were not run here.
- Parked: a `make ci-contract` target for Test (needs `gh`, so not in hygiene); a change filter for
  `ebpf-live` if its single runner queues; a rerun of the baseline after 14 days to classify the 10 failures.
