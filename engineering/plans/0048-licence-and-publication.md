<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Build plan: 0048-licence-and-publication

Spec: `specs/0048-licence-and-publication.md` (status planned). Base: `origin/development` `8711ceb`.
Planner: `uas-planner`, 2026-10-11. Every path below was read in a worktree cut from that base.
Tiers: `verified` = I ran or read it at the path named; `reported` = the spec says it; `asserted` = my judgement.

## Findings the builders must know

1. **Red legs at `8711ceb` [verified, each leg of the spec's fenced command run alone]:** red are the workspace licence line, the BPF licence line, the five licence files, the old-file removal, the holder grep, the DCO grep, the code SPDX leg (129 files), the vendor leg and the hostname/path leg (45 lines in 25 files). Green are the inherit leg (`license.workspace = true` in all 11 manifests) and the `strings` leg. The Markdown SPDX leg is red by count (65 files, none tagged). The BPF `cargo deny --offline --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses` exits 0 today (`licenses ok`). I did not run `make deny hygiene` as a whole [reported: the spec says it passes].
2. **The code count is 129, not 127 [verified]:** `git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier' | wc -l` prints 129. One is `crates/uas-ebpf/ebpf/src/main.rs` (the BPF expression); 128 take `Apache-2.0 OR MIT`. Seven are `.sh` files, each starting with `#!/usr/bin/env bash`, so the SPDX line is line 2. Per directory: build-support 1, `.claude/hooks` 6, scripts 1, xtask 4, uas-acp 5, uas-cli 6, uas-deck 9, uas-domain 12, uas-ebpf 19, uas-engine 28, uas-pool 17, uas-router 4, uas-sandbox 16, uas-storage 1.
3. **The Markdown count is 65 now, 64 after `docs/IP_NOTICE.md` goes [verified]:** docs 9, specs 48, engineering 8 (four plans, `plans/README.md` and three others). 47 files (every spec but the registry README) start with `---`; the tag goes on the line after the closing `---`, which is line 8 at most (the longest frontmatter closes on line 7, so `head -n 10` holds). The rest take the tag on line 1.
4. **The SPDX line does not change the BPF object [verified in part]:** `objdump -h` on the committed object lists no debug section, and `strings` finds no `main.rs`. So a comment line at the top of `main.rs` should leave `uas-ebpf.o` and its `.sha256` byte-identical. The CI step "drift" (`ci.yml` lines 172 to 178) rebuilds and runs `git diff --exit-code -- crates/uas-ebpf/ebpf/`. The package that edits `main.rs` rebuilds locally (`bpf-linker` 0.11.1 is under the mise shim; the dated nightly is installed) and shows an empty diff. Whether the committed object is reproducible on this host is `asserted`.
5. **`uas-ebpf-builder` may not do the BPF package [verified: `.claude/agents/uas-ebpf-builder.md` Boundaries]:** it forbids "any crate licence". P5 goes to `uas-builder` and its brief must say that the `license` field and the `authors` field of `crates/uas-ebpf/ebpf/Cargo.toml` change, and that the `Dual MIT/GPL` string in `main.rs` and the object must not change.
6. **`oss-check` must be added in two steps [verified by reading REQ-0048-08 and the tree]:** the spec's `oss-check` also fails when `vendor/cortex`, `vendor/soma` or `vendor/saga` exists. Adding that leg in P2 makes `make hygiene` red until P11. So P2 adds the pattern leg only; P11 adds the directory leg in the same commit that removes the directories.
7. **`typos` will read the licence texts [asserted]:** `_typos.toml` excludes only `vendor/` and `Cargo.lock`. Verbatim Apache, GPL and CC-BY texts may trip `make typos` (part of `make hygiene`). P4 adds `LICENSE-*` and `NOTICE` to `extend-exclude` if, and only if, the run shows a hit.
8. **Stale "proprietary" text outside the spec's Plan [verified]:** `README.md` lines 7 and 8 ("Proprietary licence (see `LICENSE`)") and `crates/uas-ebpf/README.md` line 37 ("The userspace crate stays proprietary"). Removing `LICENSE` leaves the first as a dead link. P4 fixes both; they close no REQ and are not Plan items (OPEN for the integrator to confirm).
9. **`publish = false` stays [verified: `Cargo.toml` line 11; spec D6 and Notes]:** no decision flips a `publish` flag. Publishing is the operator's act, `uas-ebpf` cannot publish as packaged (F7) and which crates publish is spec 0039's subject. No package touches `publish`.
10. **`SECURITY.md` has no 0048 package [verified: `SECURITY.md` still says "no fixed response time"; spec 0045 owns the text]:** the spec 0048 Plan lists it as a pointer. Before the feature PR opens the integrator runs `grep -c 'business days' SECURITY.md`; 0 means the spec 0045 plan has not landed and `SECURITY.md` stays as it is. No REQ-0048 needs it.
11. **In-flight work will turn red on landing [asserted]:** `hygiene` is a required check. After P2 a new tracked path or hostname fails it; after P9 a new `.rs`, `.sh`, spec, plan or doc without its tag fails it. Open spec and plan PRs from other lanes must rebase and add the tag. Recommended, not a Plan item: add the CC-BY tag line to `.agents/skills/spec-driven-dev/spec-template.md` so new specs start tagged (OPEN).
12. **The fetch tool needs no new pin [verified]:** `sha2` 0.11.0 is a workspace dependency (`Cargo.toml` line 54) and a spec 0004 row (line 188, "used by uas-ebpf, uas-router"). `xtask` uses `anyhow` only today. P10 adds `sha2 = { workspace = true }` to `xtask`; P1 amends the row's "used by" cell first. The lock file is a plain line format parsed by hand, so no `toml` or `serde` enters `xtask`. Git is called through `std::process::Command` (`.cargo/config.toml` already sets `git-fetch-with-cli`).
13. **Fetch pins may not resolve [asserted]:** the READMEs record cortex `a63d858` (`cerebral-work/cortex`), soma `1558012` (`unsigned-gg/soma-os`) and saga `1e4be86` (`unsigned-gg/saga`). Soma has moved repositories (estate brief); P10 must prove each commit resolves with `gh api repos/<repo>/commits/<sha>`, with the cortex one as the positive control, and report any that does not.
14. **`blackwall.toml` is a reviewed custody file [verified: `crates/uas-cli/src/wave_flow.rs` line 494 reads `[merge] automerge` from it]:** line 32 holds the OTLP endpoint with the tailnet host. Whether `blackwall` reads an untracked override is not checked (spec Notes). P2 reads `blackwall --help` and its config documentation first. If no override exists, P2 leaves that one line and reports; the integrator rules. That is the only P2 line that can stay red.

## Plan items to packages

| Spec Plan item | Package |
|---|---|
| Pkg 1, spec 0004: line 55 requirement, 2026-10-09 position (lines 158 to 163), `aya-ebpf` row (line 180), lines 15 to 18 | P1 |
| Pkg 1, spec 0045 D8 points at 0048 D6, "only once the operator confirms D6" | P12 (done 2026-10-11) |
| Pkg 1, `LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE`, remove `LICENSE` and `docs/IP_NOTICE.md` | P4 |
| Pkg 1, `LICENSE-DOCS` | P6 |
| Pkg 1, `crates/uas-ebpf/ebpf/LICENSE` | P5 |
| Pkg 1, holder `unsigned.gg` (NOTICE, MIT copyright lines, `authors` x2) | P4 (NOTICE, MIT line), P3 (root `authors`), P5 (BPF `authors`, BPF LICENSE) |
| Pkg 1, workspace and BPF `license` fields | P3, P5 |
| Pkg 1, SPDX line on the code files, tag on the Markdown files, `spdx-check` target | P5 (the BPF `main.rs`), P7 (128 code files), P8 (64 Markdown files), P9 (the target) |
| Pkg 1, `deny.toml` and the BPF leg of `make deny` | P3 |
| Pkg 1, `CONTRIBUTING.md` DCO section | P4 |
| Pkg 1, one CHANGELOG line | the integrator, once, on the feature PR (not a builder) |
| Pkg 2, hostnames and 42 path lines, `blackwall.toml`, service file, `oss-check` | P2 |
| Pkg 2, `SECURITY.md` per spec 0045 | no package (finding 10) |
| Pkg 3, spec 0004 vendored table and the `vendor/` row of `AGENTS.md` | P1 |
| Pkg 3, `fetch-contracts`, `contracts.lock`, `.contracts/` in `.gitignore`, `build.rs` | P10 |
| Pkg 3, remove the three vendor directories, `oss-check` guards them | P11 |
| Pkg 3, CI job for `cortex-grpc` | none (operator-gated credential; CI stays on default features) |
| Public form: fresh repository, DCO required check, private reporting, publish `uas` | none (the operator's acts; REQ-0048-10 and 12) |

## Requirement coverage

| Requirement | Package |
|---|---|
| REQ-0048-01 | P3 |
| REQ-0048-02 | P5 |
| REQ-0048-03 | P4 (four files, DCO, removals), P5 (BPF LICENSE), P6 (LICENSE-DOCS) |
| REQ-0048-04 | P5 (BPF `main.rs`), P7, P8, P9 (the check) |
| REQ-0048-05 | P3 (root and BPF legs of `make deny`), P5 (the BPF value they read) |
| REQ-0048-06 | holds today; P3 re-runs the libbpf grep with its control as a guard |
| REQ-0048-07 | not exercisable (spec Test); no package |
| REQ-0048-08 | P2 (pattern), P11 (directories) |
| REQ-0048-09 | holds; the integrator runs `gh api repos/unsigned-gg/uas --jq .private` before the PR (no package) |
| REQ-0048-10 | BLOCKED (P13): needs the fresh repository |
| REQ-0048-11 | P10 |
| REQ-0048-12 | BLOCKED (P13): needs the public form |

8 of 12 requirements are closed by a package (01 to 06, 08, 11). 09 holds by a command, 07 is not exercisable, 10 and 12 wait on the operator.

## Waves and the spec's command

| Wave | Packages | Disjoint because | Legs of the fenced command green after the wave |
|---|---|---|---|
| 1 | P1, P2, P6 | P1: `specs/0004`, `AGENTS.md`. P2: 25 tracked files, `Makefile`. P6: one new file | hostname/path; `LICENSE-DOCS` exists. The chain still stops at the manifest leg |
| 2 | P3, P4, P5 | P3: `Cargo.toml`, `deny.toml`, `Makefile`. P4: root licence files, READMEs, `CONTRIBUTING.md`, `_typos.toml`. P5: `crates/uas-ebpf/ebpf/` | `make deny hygiene`, both manifest legs, inherit, five files, old files gone, `unsigned.gg` in NOTICE, holder, DCO. First red: code SPDX |
| 3 | P7, P8 | P7: 128 code files. P8: 64 Markdown files | code SPDX, Markdown SPDX. First red: vendor |
| 4 | P9, P10 | P9: `Makefile`. P10: `xtask/`, `build.rs`, `contracts.lock`, `.gitignore` | no new leg; `spdx-check` joins `hygiene`; REQ-0048-11 closes |
| 5 | P11 | alone: `vendor/` and `Makefile` | vendor leg. The whole command exits 0 |
| later | P13 | BLOCKED | none |

P7 and P8 start from a `development` that already holds P4 (the Markdown set shrinks by one) and P1 to P3. P10 follows P7 because P7 tags `xtask/src/*.rs` and `crates/uas-engine/build.rs`; P10 edits both. P11 follows P10 and P9 because both edit `Makefile` or its inputs. The integrator owns the single `CHANGELOG.md` line.

## Packages

### P1: amend spec 0004 and `AGENTS.md` (wave 1)
- **Persona:** `uas-engineer` (spec amendment).
- **Files:** `specs/0004-dependency-and-licence-policy.md` (exists, 307 lines); `AGENTS.md` (exists; the `vendor/` row, line 27).
- **Change, commit 1 (licence, D1 and D2):** replace the requirement at line 55 with D1 (the root workspace `Apache-2.0 OR MIT`, the BPF program package `GPL-2.0-only OR MIT`, copyleft dependencies still denied); rewrite lines 15 to 18 ("The template is proprietary", with the holder `unsigned.gg`); amend the 2026-10-09 position and line 158 ("This repo is proprietary") to D1; amend the `aya-ebpf` row (line 180) so the package is `GPL-2.0-only OR MIT` and the object still declares `Dual MIT/GPL`. Line 56 ("WHILE building eBPF kernel probe objects ... dual MIT/GPL") stays true. Line numbers are at `8711ceb`; re-read before editing.
- **Change, commit 2 (contracts, D9):** the vendored table at lines 123 to 128 describes `.contracts/` fetched by `cargo xtask fetch-contracts` and pinned in `contracts.lock`, with `vendor/leanspec/` (MIT) the only row left in `vendor/`; the `AGENTS.md` `vendor/` row says the same. The `sha2` row (line 188) gains `xtask` in "used by".
- **Done-when:** `make spec-check pin-check JOBS=2 && ! grep -n 'LicenseRef-Proprietary' specs/0004-dependency-and-licence-policy.md && ! grep -n -i -E 'is proprietary|stay proprietary|template is proprietary' specs/0004-dependency-and-licence-policy.md && grep -q 'contracts.lock' specs/0004-dependency-and-licence-policy.md && grep -q 'fetch-contracts' AGENTS.md` exits 0. The greps are `asserted`: the spec's Plan names the edit and its Test table has no row for it.
- **Red-first:** run the done-when before the edit and watch it exit 1 at the first `grep -n` (it prints lines 16, 55). After the edit, reintroduce one "proprietary" sentence and watch the second grep fail.

### P2: tree hygiene and `oss-check` (wave 1)
- **Persona:** `uas-builder`.
- **Files, all exist [verified: `git grep -c` with the F16 pattern]:** `.agents/skills/custody-run/SKILL.md` (1), `.claude/agents/README.md` (2), `uas-architect.md` (3), `uas-builder.md` (2), `uas-ebpf-builder.md` (1), `uas-engineer.md` (1), `uas-foreman.md` (1), `uas-k8s-builder.md` (2), `uas-planner.md` (2), `uas-prober.md` (1), `.github/CODEOWNERS` (1), `blackwall.toml` (2), `crates/uas-ebpf/uas-ebpfd.service` (1), `engineering/cluster-survey-atlas.md` (1), `specs/0008` (1), `0013` (2), `0017` (1), `0022` (3), `0025` (1), `0029` (1), `0037` (4), `0038` (3), `0039` (3), `0046` (3), `0047` (2); and `Makefile`. Counts are lines per file, 45 in all.
- **Change:** each match becomes a description of the thing: "the research surfaces", "the estate SOP on custody", "the shared checkout", "the tailnet" without a name. Do not reword a decision or a number. `uas-ebpfd.service` `Documentation=` points at `specs/0014-ebpf-priority-clamping-and-audit-probes.md` by repository path (no `file://` home path). `blackwall.toml` line 32: see finding 14. In `Makefile` add a target `oss-check` that runs `git grep -n -E -e "$$PATTERN"` with the F16 pattern held in a make variable, and fails (exit 1, files named by `git grep`) on any match; add it to the `hygiene` dependency list and `.PHONY`. Do not add the vendor-directory leg (finding 6). Write the pattern so the Makefile does not match itself (F16 says it is written so).
- **Done-when:** `! git grep -n -E '\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|<surfaces>(s)' && make oss-check hygiene JOBS=2` exits 0. This is the hostname/path leg of the spec's command, red today with 45 lines in 25 files. If finding 14 leaves the `blackwall.toml` line, the leg stays red and the verdict says so in its first line.
- **Red-first:** before editing, `make oss-check` must exit non-zero and list the 25 files (positive control: the same `git grep` on `vendor/leanspec` prints nothing). After green, put a home path in a scratch tracked file and watch `make hygiene` fail naming it (spec Test, REQ-0048-08 guard); remove it.

### P6: `LICENSE-DOCS` (wave 1)
- **Persona:** `uas-builder`.
- **Files:** `LICENSE-DOCS` (new, repository root).
- **Change:** the CC-BY-4.0 legal code, verbatim, plain text. Fetch it to a file, do not pipe it anywhere: `https://raw.githubusercontent.com/spdx/license-list-data/main/text/CC-BY-4.0.txt` or the Creative Commons canonical page. Put the source URL and the `sha256sum` of what you saved in the verdict. No holder line is needed in the legal text.
- **Done-when:** `test -f LICENSE-DOCS && grep -q 'Attribution 4.0 International' LICENSE-DOCS` exits 0. This turns the `LICENSE-DOCS` part of the five-file leg green (REQ-0048-03). The greps are `asserted`.
- **Red-first:** run the command before saving the file and watch `test -f` exit 1. The text is boilerplate, so this package adds about 400 lines by design.

### P3: workspace manifests and the licence gate (wave 2, needs P1)
- **Persona:** `uas-builder`.
- **Files, all exist:** `Cargo.toml` (line 12 `license`, line 13 `authors`); `deny.toml` (lines 21 to 23, the `[licenses.private]` table); `Makefile` (the `deny` target, line 41).
- **Change:** `license = "Apache-2.0 OR MIT"` and `authors = ["unsigned.gg"]` in `[workspace.package]`. In `deny.toml`, `ignore = false` and delete the comment "This repo is proprietary and unpublished. Do not check its own licence." The allow list does not change (F10, F17). `make deny` keeps `cargo deny check` and adds `cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses`. No `publish` change (finding 9).
- **Done-when:** `make deny JOBS=2 && grep -qx 'license = "Apache-2.0 OR MIT"' Cargo.toml && test -z "$(grep -L 'license.workspace = true' crates/*/Cargo.toml xtask/Cargo.toml)" && ! grep -rn libbpf Cargo.lock crates/uas-ebpf/ebpf/Cargo.lock && test "$(grep -c '^name = "aya"' Cargo.lock)" = 1` exits 0. REQ-0048-01, 05, 06. If `cargo deny` cannot reach the advisory database on the host, also run `cargo deny --offline check licenses` and say so.
- **Red-first (spec Test rows REQ-0048-01 and 05, measured F10):** on a scratch copy, set only `ignore = false` and watch `cargo deny check licenses` exit 4 with 11 rejections, the first naming `uas-domain v0.1.0`. With the D1 licence it exits 0. Set one crate to `license = "GPL-2.0-only"` and watch exit 4; set `ignore = true` again and watch that GPL crate pass unseen (the reason for D8). Remove the BPF leg from `make deny`, set the BPF package to `GPL-2.0-only` in a scratch copy and watch `make deny` pass wrongly. Restore each.

### P4: root licence files, notice, contributing (wave 2, needs P1)
- **Persona:** `uas-builder`.
- **Files:** `LICENSE-APACHE` (new, about 200 lines), `LICENSE-MIT` (new), `NOTICE` (new); `LICENSE` (exists, 16 lines, delete); `docs/IP_NOTICE.md` (exists, 31 lines, delete); `CONTRIBUTING.md` (exists, 45 lines); `README.md` (lines 7 and 8); `crates/uas-ebpf/README.md` (line 37); `_typos.toml` (only if finding 7 shows a hit).
- **Change:** `LICENSE-APACHE` is the Apache-2.0 text verbatim. `LICENSE-MIT` is the MIT text with `Copyright (c) 2026 unsigned.gg`. `NOTICE` names `unsigned.gg` as the copyright holder and maps each licence to its files: code and non-code files outside `vendor/` to `Apache-2.0 OR MIT` (`LICENSE-APACHE`, `LICENSE-MIT`); `crates/uas-ebpf/ebpf/` to `GPL-2.0-only OR MIT` (`crates/uas-ebpf/ebpf/LICENSE`); `*.md` under `docs/`, `specs/` and `engineering/` to `CC-BY-4.0` (`LICENSE-DOCS`); `vendor/` to its own READMEs (D2, F12). `CONTRIBUTING.md` gains a "Developer Certificate of Origin" section: the DCO 1.1 text, the rule that every commit carries a `Signed-off-by:` trailer matching its author, `git commit -s`, no CLA, and that commits stay GPG-signed (D5). The README sentences name the new licences and point at `LICENSE-APACHE`, `LICENSE-MIT` and `NOTICE`.
- **Done-when:** `test -f LICENSE-APACHE -a -f LICENSE-MIT -a -f NOTICE && test ! -e LICENSE -a ! -e docs/IP_NOTICE.md && grep -q unsigned.gg NOTICE && ! git grep -q -E 'the operator M\. Todie / cerebral\.work|All rights reserved' -- ':!specs' ':!vendor' ':!engineering' ':!CHANGELOG.md' && grep -q 'Signed-off-by' CONTRIBUTING.md && make typos` exits 0. These are the REQ-0048-03 legs of the spec's command; they are red today (F11, F18, F19).
- **Red-first:** run the command first and watch it fail at `test -f`. After green, restore `LICENSE` and watch the removal leg fail; delete the DCO section and watch the `Signed-off-by` grep fail (the spec's control: `grep -c -i commit CONTRIBUTING.md` printing 2 shows the file is readable). Licence text sources: SPDX `license-list-data`; save to a file, record URL and `sha256sum`, and compare the Apache and MIT bodies with a registry crate's copy ignoring the copyright line.

### P5: the BPF program package (wave 2, needs P1)
- **Persona:** `uas-builder` (finding 5: not `uas-ebpf-builder`).
- **Files:** `crates/uas-ebpf/ebpf/LICENSE` (new, GPL-2.0 text about 340 lines plus the MIT text); `crates/uas-ebpf/ebpf/Cargo.toml` (exists; line 8 `license`, new `authors`); `crates/uas-ebpf/ebpf/src/main.rs` (exists; one line added at the top).
- **Change:** `license = "GPL-2.0-only OR MIT"` and `authors = ["unsigned.gg"]`. `LICENSE` offers either licence, with a short header stating the choice and `Copyright (c) 2026 unsigned.gg` on the MIT part. `main.rs` line 1 becomes `// SPDX-License-Identifier: GPL-2.0-only OR MIT`. The `LICENSE` static at `main.rs` lines 328 to 330 and the committed object stay exactly as they are.
- **Done-when:** `grep -qx 'license = "GPL-2.0-only OR MIT"' crates/uas-ebpf/ebpf/Cargo.toml && test -f crates/uas-ebpf/ebpf/LICENSE && head -n 2 crates/uas-ebpf/ebpf/src/main.rs | grep -qx '// SPDX-License-Identifier: GPL-2.0-only OR MIT' && cargo deny --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses && strings crates/uas-ebpf/ebpf/uas-ebpf.o | grep -cx 'Dual MIT/GPL' | grep -qx 1 && make ebpf-build JOBS=2 && git diff --exit-code -- crates/uas-ebpf/ebpf/` exits 0. That covers REQ-0048-02, the BPF part of 03 and 05, and finding 4. Run the toolchain check from `uas-ebpf-builder.md` first; if `make ebpf-build` cannot run, report it and leave the drift proof to CI.
- **Red-first (spec Test row REQ-0048-02):** set the package licence to `GPL-2.0-only` and watch the `cargo deny` command exit 4 naming `uas-ebpf-ebpf v0.1.0`; restore. Change the `LICENSE` static in `main.rs` without rebuilding and watch the `strings` count print 0 after a rebuild (the control that the check reads the object); restore.

### P7: SPDX line on the code files (wave 3, needs P5)
- **Persona:** `uas-builder`.
- **Files:** the 128 tracked `.rs` and `.sh` files outside `vendor/` and outside `crates/uas-ebpf/ebpf/` that lack the tag; list them with the command in finding 2 at the start. Nothing else is edited.
- **Change:** `// SPDX-License-Identifier: Apache-2.0 OR MIT` as line 1 of each `.rs` file; `# SPDX-License-Identifier: Apache-2.0 OR MIT` as line 2 of each `.sh` file, under its shebang. A script does the insert and is not committed. `//!` module docs stay valid after a plain comment.
- **Done-when:** `test -z "$(git ls-files -z -- '*.rs' '*.sh' ':!vendor' | xargs -0 grep -L 'SPDX-License-Identifier')" && make fmt-check clippy JOBS=2` exits 0. The first half is the code SPDX leg; `make test JOBS=2` also exits 0 (a comment changes no behaviour, and `build-support/build_rev.rs` is included by other files).
- **Red-first:** the first half prints 128 names before the change (129 less the BPF file P5 tagged). After green, delete the tag from one file and watch it listed.

### P8: tag on the Markdown files (wave 3, needs P4)
- **Persona:** `uas-builder`.
- **Files:** every tracked `*.md` under `docs/`, `specs/` and `engineering/` without the tag, found by `git ls-files 'docs/*.md' 'specs/*.md' 'engineering/*.md' | while read -r f; do head -n 10 "$f" | grep -q 'SPDX-License-Identifier: CC-BY-4.0' || echo "$f"; done`. 64 files: docs 8, specs 48 (the registry README and spec 0048 included), engineering 8 (this plan's own file already carries it and is not counted).
- **Change:** `<!-- SPDX-License-Identifier: CC-BY-4.0 -->` on line 1; in a file that starts with `---`, on the line after the closing `---` (D10, F20).
- **Done-when:** the spec's Markdown leg prints nothing and `make spec-check JOBS=2` exits 0: `test -z "$(git ls-files 'docs/*.md' 'specs/*.md' 'engineering/*.md' | while read -r f; do head -n 10 "$f" | grep -q 'SPDX-License-Identifier: CC-BY-4.0' || echo "$f"; done)" && make spec-check`. Red today: 65 files. Spec 0025 reaches 301 lines and spec-check warns; the warning is expected (F20), an error is not.
- **Red-first:** the loop prints the file list before the change. After green, remove the tag from one spec and watch it listed and `spec-check` still pass.

### P9: `spdx-check` in `make hygiene` (wave 4, needs P7 and P8)
- **Persona:** `uas-builder`.
- **Files:** `Makefile` (exists; new target `spdx-check`, the `hygiene` dependency list, `.PHONY`).
- **Change:** `spdx-check` fails, naming the file, when a tracked `.rs` or `.sh` outside `vendor/` lacks an `SPDX-License-Identifier` line in its first two lines; when the expression is not `GPL-2.0-only OR MIT` under `crates/uas-ebpf/ebpf/` or not `Apache-2.0 OR MIT` elsewhere; or when a tracked `*.md` under `docs/`, `specs/` or `engineering/` lacks `SPDX-License-Identifier: CC-BY-4.0` in its first ten lines. Shell in the recipe, no new dependency.
- **Done-when:** `make spdx-check hygiene JOBS=2` exits 0.
- **Red-first (spec Test row REQ-0048-04 guard):** remove the tag from one `.rs` file and one spec and watch both listed; put `Apache-2.0 OR MIT` in `crates/uas-ebpf/ebpf/src/main.rs` and watch it fail; put `GPL-2.0-only OR MIT` in an `xtask` file and watch it fail. Restore each.

### P10: contract fetch (wave 4, needs P1 and P7)
- **Persona:** `uas-builder`.
- **Files:** `xtask/src/fetch_contracts.rs` (new); `xtask/src/main.rs` (exists; `mod`, dispatch arm, usage line); `xtask/Cargo.toml` (exists; `sha2 = { workspace = true }`); `contracts.lock` (new, one line per fetched file: sha256, upstream commit, repository, upstream path, destination); `.gitignore` (exists; add `.contracts/`); `crates/uas-engine/build.rs` (exists; line 9 reads `vendor/cortex/proto`).
- **Change:** `cargo xtask fetch-contracts [--lock <path>]` reads the lock, fetches each commit with `git` (a throwaway repository, `git fetch --depth 1 origin <commit>`, `git show <commit>:<path>`), hashes the bytes with `sha2`, and writes the file into `.contracts/` only when the hash matches; on a mismatch it exits non-zero, names the file and leaves it out (REQ-0048-11). The lock starts from the README pins (finding 13) and lists the 52 non-README files now under `vendor/cortex` (7), `vendor/soma` (23) and `vendor/saga` (22); each hash is computed from upstream, then compared with the vendored copy, and a difference is reported. `build.rs` reads `.contracts/cortex/proto` and, when `pool.proto`, `lane.proto` or `peer.proto` is absent, fails with the text `run cargo xtask fetch-contracts`. The default build does not read the directory. Pure functions (lock parse, hash compare) carry unit tests in `xtask`.
- **Done-when:** `cargo nextest run -p xtask fetch_contracts -j 2 && cargo xtask fetch-contracts && cargo build -p uas-engine --features cortex-grpc --locked -j 2 && make check JOBS=2` exits 0, then the builder removes `.contracts/`, runs the same `cargo build` and pastes its output: the build must fail and the output must hold `cargo xtask fetch-contracts` (`asserted`; the spec has no row for it). The fetch needs read access to the private upstream repositories as the invoking user.
- **Red-first (spec Test row REQ-0048-11):** change one sha256 character in a scratch lock and run `cargo xtask fetch-contracts --lock <scratch>`; it exits non-zero and names the file, and `.contracts/` lacks that file; with the true value it exits 0. Remove the compare from the code and watch the unit test go red. Positive control for finding 13: the cortex commit resolves before the others are judged.

### P11: remove the vendored contracts and guard them (wave 5, needs P9 and P10)
- **Persona:** `uas-builder`.
- **Files:** `vendor/cortex/`, `vendor/soma/`, `vendor/saga/` (exist, 8, 24 and 23 tracked files; delete); `Makefile` (the `oss-check` target).
- **Change:** `git rm -r` the three directories. `vendor/leanspec/` stays. `oss-check` also fails when any of the three paths exists (REQ-0048-08).
- **Done-when:** `test ! -e vendor/cortex -a ! -e vendor/soma -a ! -e vendor/saga && make oss-check hygiene JOBS=2 && cargo xtask fetch-contracts && cargo build -p uas-engine --features cortex-grpc --locked -j 2` exits 0. This is the vendor leg, red today. After it the spec's whole fenced command should exit 0; the builder runs it end to end and pastes the result.
- **Red-first:** before the removal `make oss-check` already passes the pattern leg; after adding the directory leg and before `git rm`, it must exit non-zero naming the directory. After green, `git checkout` one removed directory and watch it fail again; `git rm` it.

### P12: spec 0045 D8 points at 0048 D6 (DONE 2026-10-11)
- **Persona:** `uas-engineer`.
- **DONE 2026-10-11:** the operator confirmed D6 ("Confirm, the crate is the vehicle", lane w25:p1, AskUserQuestion); spec 0045 D8 now points at it. Was blocked because spec 0048 line 84 (D6) was `proposed, the operator to rule`, and the Plan says "Only once the operator confirms D6, spec 0045 D8 points at it". Spec 0045 line 58 (D8) is also his row. Unblocks when the operator rules D6. Files then: `specs/0045-security-disclosure-and-advisory-policy.md` only. Done-when: `grep -q 'spec 0048' specs/0045-security-disclosure-and-advisory-policy.md && make spec-check` exits 0 (`asserted`).

### P13: the public form (BLOCKED, the operator's acts)
- **BLOCKED:** the cut date and the public repository name are not ruled (spec Notes, "Rulings needed"); creating the repository is the operator's act (spec 0037 D1). Whether `uas-operator` (spec 0038) publishes separately is not ruled. Covers REQ-0048-10 (one commit holding the tree of `main`, `make check` green there, a required DCO check) and REQ-0048-12 (the crates.io `uas` crate's `repository` URL returns 200; today 404). Also the CI credential for `cortex-grpc`, an operator-gated secret. No builder acts on any of it. Until then `gh api repos/unsigned-gg/uas --jq .private` must print `true` (REQ-0048-09).

## Open

- Whether P2 can move the `blackwall.toml` endpoint (finding 14).
- Whether the committed BPF object rebuilds identically on this host (finding 4); CI's drift step is the arbiter.
- Whether the three upstream commits are reachable (finding 13).
- Findings 8 and 11 add two small edits the spec's Plan does not list (the README sentences, the spec template tag); the integrator confirms.
- I did not run `make deny hygiene` whole; the spec reports it green at an earlier commit.
