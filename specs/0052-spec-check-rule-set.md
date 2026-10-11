---
status: planned
created: 2026-10-11
tags: [spec-check, xtask, ears, hygiene, rules]
priority: medium
depends_on: [0001-cas-architecture-and-crate-layout, 0002-verification-gates-and-evidence, 0049-cpm-migration-to-saga]
---
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# 0052-spec-check-rule-set

> **Status**: planned, **Priority**: medium, **Created**: 2026-10-11

## Overview

`make spec-check` gates every spec pull request (AGENTS.md rule 1). The rules it enforces live
only in the comment block and the code of `xtask/src/spec_check.rs`. Specs 0001 and 0002 name the
checker but state no rule. This spec owns the rule set: what each rule checks, whether it warns
or fails, the line caps, and how a rule changes. the operator ruled on 2026-10-11 about 03:10Z that
this spec is drafted (AskUserQuestion, lane w25:p1, label "Spec-check rule set") [reported:
integrating lane brief]. Content decisions are the integrating lane's unless marked proposed.

The checker is accurate about what it checks. Three things it does not check matter more than any
rule it has: the EARS checks skip the layout 47 of 48 specs use (F5), the keyword check
misfires on the house requirement form (F6), and plans get no check (F7).

### Facts

Measured 2026-10-11 on `origin/development` 3e60a55. Each plant was one violation in a scratch
`git archive` copy, built and run there. The checker bakes its repository root in at compile time
(`xtask/src/main.rs` line 18), so a scratch copy needs its own checkout. The unplanted copy is the
control: it exits 0 with two warnings.

- **F1 [verified]:** the header comment (`spec_check.rs` lines 2 to 19) lists eleven rules. The
  code enforces twelve checks: `depends_on` stems (`check_entries`, line 470) is in no list. Two
  function comments carry other numbers: "Rule 8" on links (line 564) and "Rule 7" on the README
  (line 580), against 11 and 10 in the header. Command: `grep -n 'Rule [0-9]*:' xtask/src/spec_check.rs`.
- **F2 [verified]:** the entry is `Makefile` lines 64 and 65 (`cargo xtask spec-check`), reached
  by `make hygiene` (line 49) and by CI step "make hygiene" (`ci.yml` lines 83 and 84,
  `JOBS=10`). The recipe passes no `-j`, so `JOBS` has no effect on it. `xtask/src/main.rs`
  takes only the command name: `cargo xtask spec-check --strict extra` ran and exited 0.
- **F3 [verified]:** `make spec-check JOBS=2` exits 0 with two warnings: spec 0004 at 318
  lines and spec 0025 at 301. A warning never changes the exit code.
- **F4 [verified]:** a spec is in modern mode if and only if it has an H2 `## Requirements`
  (`spec_check.rs` line 352). One of 48 specs does (0024). A legacy spec with a stray
  `## Requirements` H2 flips mode and fails with three errors (planted copy, 2026-10-11).
- **F5 [verified]:** rules R07, R08 and R09 run only in modern mode. In a legacy spec, a
  `### Requirements (EARS)` block with `will` for `shall` passes, one with `may` passes, and one
  with `Systems shall run.` passes (three planted copies, 2026-10-11, all exit 0).
- **F6 [verified]:** in modern mode, the house bullet form `- **[REQ-0099-01]** (Ubiquitous): The
  system shall run.` draws the keyword warning, because `is_ears_prefix` (line 402) strips a bold
  label and then meets `(Ubiquitous)`. Bullets in the 47 house `### Requirements (EARS)` blocks:
  about 430 with `shall`, of which about 250 would warn, by a throwaway script that mirrors the
  function. The count is approximate; the false positive itself is exact.
- **F7 [verified]:** the checker reads `specs/*.md` only. A broken link in
  `engineering/plans/9999-bad.md` passes. Files in a subdirectory of `specs/` and non-`.md`
  files are ignored. The SPDX line, link anchors, fenced links and fence content are not
  checked. A link in a fence is skipped; so is a link in a code span.
- **F8 [verified]:** the line caps are `WARN_LINES` 300 and `ERROR_LINES` 400 (lines 38 and 39).
  By `wc -l`: 300 lines passes clean, 301 warns, 400 warns, 401 fails.
- **F9 [verified]:** `spec_check.rs` has 22 unit tests (lines 695 to 1029), run by `make test`
  through `--workspace`. `repository_specs_pass_check` (line 1021) runs the real `specs/`. No
  test asserts: the keyword warning, `duplicate key`, `frontmatter is not closed`, the list or
  single-value type error, or `missing H1`.
- **F10 [reported]:** `engineering/plans/README.md` says a plan is deleted or marked done when its
  packages land (read 2026-10-11). Plans are working files.

## Design

Twelve rules, numbered here. The code comment numbers stop being the reference (D11).

| ID | Checks | Mode | Severity | Message stem (exact, after `specs/<file>:<line>: `) |
|---|---|---|---|---|
| R01 | File name `NNNN-kebab-slug.md`, number unique | all | error | `file name must be NNNN-kebab-slug.md`; `number 0099 is already used by 0099-other.md` |
| R02 | Frontmatter: opens on line 1, closes, known keys, no duplicate, enums, date, `status` and `created` present, list keys are lists | all | error | `missing frontmatter (first line must be ---)`; `frontmatter is not closed by a --- line`; `unknown frontmatter key `owner``; `duplicate key `status``; `status `done` must be one of ...`; `priority `urgent` must be one of ...`; `created `2026-13-45` must be YYYY-MM-DD`; `missing required key `created``; `` `tags` must be a list `` |
| R03 | Each `depends_on` entry is a spec file stem | all | error | `depends_on `0777-nope` is not a spec` |
| R04 | Exactly one H1 outside fences, equal to `# <stem>` | all | error | `H1 must be `# 0099-plant`, found `# 0099-wrong``; `more than one H1`; `missing H1 `# 0099-plant`` |
| R05 | The five H2 sections of the mode, in order | all | error | `missing section `## Plan``; `section `## Test` is out of order` |
| R06 | `## Test` (legacy) or `## Verification` (modern) holds a fence marker | all | error | `` `## Test` needs a fenced code block with a runnable command `` |
| R07 | `## Requirements` holds a non-fenced line with the word `shall` | modern | error | `` `## Requirements` needs at least one EARS requirement clause containing 'shall' `` |
| R08 | A line with `shall` holds no word of the modal table | modern | warning | `requirement contains ambiguous modal 'may'; use 'shall' for mandatory requirements` |
| R09 | A bullet line with `shall` starts with an EARS keyword | modern | warning | `requirement clause does not begin with a standard EARS pattern keyword (The, When, While, If, Where)` |
| R10 | Line count: over 300 warns, over 400 fails | all | both | `318 lines is over 300; consider splitting`; `401 lines is over the 400-line limit; split into sub-spec files` |
| R11 | `specs/README.md` exists and links every spec once and no missing file | README | error | `specs/README.md:1: 0099-plant.md is not listed`; `... is listed 2 times`; `lists 0098-ghost.md but the file does not exist`; `file is missing` |
| R12 | Relative links in specs and the README resolve | all | error | `broken link `nope.md`` |

Mode, modern or legacy, is set by an H2 named `Requirements`. Modern sections are Requirements,
Design, Tasks, Verification, Notes. Legacy sections are Overview, Design, Plan, Test, Notes. The
README's own links are checked by R12 and R11. R07 to R09 skip fenced lines and strip code spans
first. R12 skips URLs, `mailto:`, a bare `#anchor`, fenced lines and code spans, and ignores
anchors in `file.md#anchor`. The run prints `warning:` lines to stdout and `error:` lines to
stderr, then `spec-check: ok (N warning(s))` or `spec-check: N error(s)`.

### Ambiguous-modal list

| Word | Reason | Escape |
|---|---|---|
| `should` | States a preference, not a duty | Write it in a code span |
| `could` | States a possibility | Write it in a code span |
| `might` | States a possibility | Write it in a code span |
| `may` | Reads as both permission and possibility | Write it in a code span |

### Decisions

| ID | Decision | Rationale | Source |
|---|---|---|---|
| D1 | This spec owns the rule set: table R01 to R12 and the modal list. The code and this table must agree; a mismatch is a defect in whichever is stale. | The rules live in a comment today, and the comment has a count error (F1). | lane-decided: integrating lane brief, 2026-10-11 |
| D2 | Severities stay as the table shows. Structure, registry, links, the `shall` count and the 400-line cap fail the build. The 300-line cap, the modal check and the keyword check warn. | A warning that blocks every PR on a style point costs more than it saves; the structural rules protect the registry and the gate. | lane-decided: integrating lane brief, uas-engineer, 2026-10-11 |
| D3 | The caps stay 300 and 400, and the 300 warning stays advisory. Specs 0004 (318) and 0025 (301) keep their warning: this spec does not touch either file. Spec 0004 splits before it reaches 400. Spec 0025 may drop under 300 when spec 0049 amends it; nothing here depends on that. | Moving the cap hides the next long spec. Two warnings in 48 specs show the cap does its job (F3). Spec 0004 grows with every pin (assumed ~20 lines a wave, `asserted`). | lane-decided: integrating lane, uas-engineer, 2026-10-11 |
| D4 | The legacy layout is the house layout and stays. Keep both modes. No deprecation date for the legacy. | 47 of 48 specs and the spec template use it (F4, F5). A deprecation date would force 47 rewrites for no new check. Modern mode keeps its one user until spec 0049 removes that user's file from uas. | proposed, for the integrating lane; uas-engineer, 2026-10-11 |
| D5 | Close the enforcement hole instead: run R07 to R09 on the `### Requirements (EARS)` block of legacy specs. Order: (1) fix `is_ears_prefix` to accept `**[REQ-NNNN-NN]** (Pattern):` (F6); (2) turn the three checks on for the block as warnings; (3) promote R07 to an error once the repository is clean. The repository holds one modal word in about 430 bullets (approximate scan, 2026-10-11). Step 3 has no date; the integrating lane sets it. | Today the EARS rules protect one spec (F4, F5). The cost is one parser fix, then warnings. | proposed, for the integrating lane; uas-engineer, 2026-10-11 |
| D6 | Plans (`engineering/plans/*.md`) get no structural check: no section, EARS or length rule. | Plans are working files deleted or marked done when packages land (F10). | lane-decided: uas-engineer, 2026-10-11 |
| D7 | Plans do get link resolution (R12 only), over `engineering/plans/*.md`. | Today a broken link in a plan passes (F7), and plans cite specs by file name. One call to the existing link function. | proposed, for the integrating lane; uas-engineer, 2026-10-11 |
| D8 | R09 stays a warning, now and after D5. | It misfires on the house form (F6), and a keyword style point does not justify failing a PR. | lane-decided: uas-engineer, 2026-10-11 |
| D9 | The modal list stays the four words above. Adding a word follows D10. | Only `shall` lines are scanned; a longer list raises false positives on prose such as "may be empty". | lane-decided: uas-engineer, 2026-10-11 |
| D10 | A rule change ships in one PR with three parts: an amendment of the table above, a unit test in `xtask/src/spec_check.rs` that goes red without the change, and a CHANGELOG line. | Today a rule can change with no spec and no test (F1, F9). | lane-decided: integrating lane brief, 2026-10-11 |
| D11 | The code comment block stops carrying a numbered list. It points at this spec and keeps one line per rule name. That is a code change, left to a builder (Plan). | Two numbering schemes already disagree (F1). | lane-decided: uas-engineer, 2026-10-11 |

### Requirements (EARS)

- **[REQ-0052-01]** (Ubiquitous): `make spec-check` shall run `cargo xtask spec-check` over every `specs/*.md` file, exit 0 when it reports no error whatever the warning count, and exit non-zero when it reports one.
- **[REQ-0052-02]** (Event-driven): WHEN a file under `specs/` other than `README.md` is not named `NNNN-kebab-slug.md`, or two files share a number, the checker shall report an error naming the file (R01).
- **[REQ-0052-03]** (Unwanted behavior): IF a spec's frontmatter is missing, unclosed, holds an unknown or duplicate key, holds a `status`, `priority` or `created` value outside the allowed set, lacks `status` or `created`, or gives a scalar for a list key, THEN the checker shall report an error with the file, line and key (R02).
- **[REQ-0052-04]** (Unwanted behavior): IF a `depends_on` entry is not the stem of a file in `specs/`, THEN the checker shall report an error naming the entry (R03).
- **[REQ-0052-05]** (Unwanted behavior): IF the first H1 outside a fence is not `# <stem>`, or no H1 or a second H1 exists, THEN the checker shall report an error (R04).
- **[REQ-0052-06]** (State-driven): WHILE a spec has no H2 `Requirements`, the checker shall require the five legacy H2 sections in order; WHILE it has one, the checker shall require the five modern sections in order (R05).
- **[REQ-0052-07]** (Unwanted behavior): IF the `Test` or `Verification` section holds no fence marker, THEN the checker shall report an error (R06).
- **[REQ-0052-08]** (Unwanted behavior): IF a modern spec's `Requirements` section holds no non-fenced line containing `shall`, THEN the checker shall report an error (R07).
- **[REQ-0052-09]** (Event-driven): WHEN a modern requirement line holds `shall` and a word of the modal list outside a code span, the checker shall print a warning naming the word (R08); WHEN a bullet with `shall` does not begin with The, When, While, If or Where, it shall print a warning (R09).
- **[REQ-0052-10]** (Event-driven): WHEN a spec has more than 300 lines the checker shall warn, and WHEN it has more than 400 lines the checker shall report an error (R10).
- **[REQ-0052-11]** (Unwanted behavior): IF `specs/README.md` is missing, omits a spec, lists one twice, or lists a file that does not exist, THEN the checker shall report an error (R11).
- **[REQ-0052-12]** (Unwanted behavior): IF a relative link in a spec or in `specs/README.md` does not resolve to an existing path, THEN the checker shall report an error naming the target (R12).
- **[REQ-0052-13]** (Ubiquitous): A change to a rule, a severity, a cap or the modal list shall ship with an amendment of this spec's tables and a unit test in `xtask/src/spec_check.rs` that fails without the change (D10).
- **[REQ-0052-14]** (Ubiquitous): The comment block of `spec_check.rs` shall name this spec as the owner of the rule list and shall carry no second numbering of the rules (D11).

## Plan

Package 1, one builder, one PR, no behaviour change (D10, D11):

- [ ] Replace the numbered list in the `spec_check.rs` header with a pointer to this spec (REQ-0052-14).
- [ ] Add the five missing tests: keyword warning, duplicate key, unclosed frontmatter, list or scalar type error, missing H1 (REQ-0052-03, 05, 09). Each goes red when its check is removed.
- [ ] Point spec 0001 line 65 and spec 0002 line 40 at this spec.

Package 2, only after the integrating lane rules D5:

- [ ] Fix `is_ears_prefix` for `**[REQ-NNNN-NN]** (Pattern):`, with a test on the house form (F6).
- [ ] Run R07 to R09 on the legacy `### Requirements (EARS)` block as warnings, and clear the repository's warnings.

Package 3, only after the integrating lane rules D7:

- [ ] Run R12 over `engineering/plans/*.md`, with a test that plants a broken link.

## Test

Each row plants one violation in a scratch `git archive` copy of 3e60a55 and runs `cargo xtask
spec-check` there. The unplanted copy is the control: exit 0, two warnings (F3). A row goes red
when the check named in it is removed from the code.

| Requirement | Plant, exact output, negative control |
|---|---|
| REQ-0052-01 | Control: `make spec-check JOBS=2` prints `spec-check: ok (2 warning(s))` and exits 0 with warnings present. Any planted error below makes it print `spec-check: 1 error(s)` and exit 1. |
| REQ-0052-02 | File renamed `0099_Plant.md`: `error: specs/0099_Plant.md:1: file name must be NNNN-kebab-slug.md`. Two files numbered 0099: `number 0099 is already used by 0099-other.md`. |
| REQ-0052-03 | First line deleted: `missing frontmatter (first line must be ---)`. Closing `---` deleted: `frontmatter is not closed by a --- line`. `owner:` added: `unknown frontmatter key `owner``. `status: done`: `status `done` must be one of planned, in-progress, complete, archived`. `priority: urgent`: `priority `urgent` must be one of low, medium, high, critical`. `created: 2026-13-45`: `created `2026-13-45` must be YYYY-MM-DD`. `created` deleted: `missing required key `created``. Second `status:`: `duplicate key `status``. `tags: a`: `` `tags` must be a list ``. |
| REQ-0052-04 | `depends_on: [0777-nope]`: `depends_on `0777-nope` is not a spec`. Control: `0001-cas-architecture-and-crate-layout` passes. |
| REQ-0052-05 | `# 0099-wrong`: ``H1 must be `# 0099-plant`, found `# 0099-wrong` ``. A second `# 0099-plant`: `more than one H1`. H1 deleted: ``missing H1 `# 0099-plant` ``. A trailing space on a correct H1 passes. |
| REQ-0052-06 | `## Plan` removed: ``missing section `## Plan` ``. `Plan` and `Notes` swapped: ``section `## Test` is out of order`` and ``section `## Notes` is out of order``. Legacy `Overview` renamed `Requirements`: three errors (missing `Tasks`, missing `Verification`, no `shall`). |
| REQ-0052-07 | Fence lines deleted from a legacy spec: ``error: ... `## Test` needs a fenced code block with a runnable command``. Same text for `## Verification` in a modern spec. |
| REQ-0052-08 | Modern spec with `will` for `shall`: ``... `## Requirements` needs at least one EARS requirement clause containing 'shall' ``. Same edit on a legacy spec exits 0 (F5). |
| REQ-0052-09 | Modern line `The system shall run and may stop.`: `warning: ...:13: requirement contains ambiguous modal 'may'; use 'shall' for mandatory requirements`, exit 0. With `may` in backticks: no warning. Bullet `- Systems shall run.`: `warning: ...:13: requirement clause does not begin with a standard EARS pattern keyword (The, When, While, If, Where)`, exit 0. The house `(Ubiquitous):` form draws the same warning (F6). |
| REQ-0052-10 | Padded to 300 lines: no warning. 301: `warning: ...:301: 301 lines is over 300; consider splitting`. 400: still a warning. 401: `error: ...:401: 401 lines is over the 400-line limit; split into sub-spec files`, exit 1. |
| REQ-0052-11 | Row removed: `error: specs/README.md:1: 0099-plant.md is not listed`. Row added twice: `0099-plant.md is listed 2 times`. Row for `0098-ghost.md`: `lists 0098-ghost.md but the file does not exist` (plus the R12 error). README deleted: `specs/README.md:1: file is missing`. |
| REQ-0052-12 | `[x](nope.md)` in a spec or in the README: ``error: ...: broken link `nope.md` ``. Controls that pass: an https URL, a bare `#frag`, `0001-...md#nothing`, a link inside a fence. `../NOPE.md` fails and `../AGENTS.md` passes. |
| REQ-0052-13 | Not exercisable by a plant. Review check: a diff that touches a table row above or a rule constant must also touch the other two parts. `git diff --stat origin/development -- specs/0052-spec-check-rule-set.md xtask/src/spec_check.rs` lists both files. |
| REQ-0052-14 | `grep -c 'Rule [0-9]*:' xtask/src/spec_check.rs` prints 0 and `grep -c 'spec 0052' xtask/src/spec_check.rs` prints at least 1. Red today: the first prints 3 (F1) and the second prints 0. |

```bash
make spec-check JOBS=2 && s="$(mktemp -d)" && git archive HEAD | tar -x -C "$s" \
  && sed -i 's/^# 0049-cpm-migration-to-saga$/# 0049-wrong/' "$s/specs/0049-cpm-migration-to-saga.md" \
  && out="$(cd "$s" && cargo xtask spec-check 2>&1)"; rc=$?; rm -rf "$s"; \
  test "$rc" -eq 1 && printf '%s\n' "$out" | grep -q 'H1 must be `# 0049-cpm-migration-to-saga`, found `# 0049-wrong`'
```

Run it from the repository root. It exits 0 when the real specs pass and the planted copy goes
red with the exact H1 message; the scratch copy lives outside the repository.

## Notes

- Mode choice rests on one H2 name. A spec cannot be half modern.
- The README step 6 says to split a long spec into sub-spec files (`vendor/leanspec/
  sub-spec-files.md`). The checker ignores subdirectories of `specs/` (F7), so a split into a
  subdirectory goes unchecked, and a flat split needs a number each. Open: which form the
  integrating lane wants before spec 0004 splits (D3).
- Spec 0002 line 40 lists `make hygiene` as `typos machete spec-check toolchain-check`; the
  Makefile (line 49) also runs `pin-check` and `oss-check`. Not fixed here; a spec 0002 amendment.
- Spec 0048 REQ-0048-04 checks the SPDX line by shell, not by the checker (F7). Moving it into the
  checker is a rule change under D10 and has no ruling.
- Specs 0050 and 0051 are drafted in parallel lanes and are not on `development`. The README row
  for this spec will conflict with theirs on integration; keep all three.
- Caveat: F6's count comes from a throwaway script, not from the checker, and is approximate.
