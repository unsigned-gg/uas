# Agents for uas

Repo-specific Claude Code subagents. They load at session start from this directory (restart or
`/agents` to pick up a new file). The global set in the user-level `agents/` directory under `.claude` stays available: use
`surface-builder`, `prober`, `ci-watcher`, `verifier`, `prompt-linter` and `linear-filer` as they
are. The global `builder` and `pr-reviewer` assume `origin/main`; here `uas-builder` and
`uas-critic` replace them. Models follow the operator's rule of 2026-10-10: engineer and critic on
opus, foreman on sonnet, builders on sonnet (the operator, 2026-10-10, estate-wide; the haiku rule of the same day is superseded), prober on haiku, every model a Claude 5.x.

| Agent | Model | Writes | Runs when |
|---|---|---|---|
| `uas-engineer` | opus | specs, briefs | a concept or architecture change, or a package needs a brief |
| `uas-builder` | sonnet | one package in a worktree | a brief names stable-Rust files and a done-when |
| `uas-ebpf-builder` | sonnet | `crates/uas-ebpf`, the object and its hash | the package touches the kernel program or the loader |
| `uas-critic` | opus | nothing | once on each merged wave branch before its PR is marked ready, and on a builder SHA that touches unsafe, the sandbox, the pool's kernel calls or crates/uas-ebpf |
| `uas-foreman` | sonnet | nothing | only when a pane gang is applied through saga (`uas-estate`) |
| `uas-architect` | the team slot (max tier) | the design-debt list | in the `uas-estate` gang: a plan, spec or brief needs a design review |
| `uas-prober` | haiku | nothing | in the `uas-estate` gang: the foreman names a CI, PR, repo or inbox probe |
| `uas-planner` | sonnet | `engineering/plans/<spec>.md` | a landed spec is to be built: it packages the Plan into briefs, waves and done-whens |
| `uas-k8s-builder` | sonnet | `crates/uas-operator`, its CRDs and the spec 0039 feature code | a brief names the operator, a CRD or a k3d test; never a live cluster |

## Two tiers

1. **In-lane waves (default).** The lane runs at most five agents at once (builders, critics and
   probers together), at most three of them builders in a wave, through the Agent tool: builders
   in sibling worktrees, then the lane merges them into a wave branch and opens a draft PR, then
   `uas-critic` reads the merged branch, then the lane lands on green. Surfaces are built once with
   `surface-builder` into the research surfaces and every agent reads them there.
2. **Pane gang (on request).** `saga yggdrasil team apply uas-estate --cwd <repo>` splits a
   herdr tab into the `estate-cockpit` grid; the lane starts each pane with
   `herdr agent start <name> --kind claude --pane <id> -- --model <alias>` and briefs it. The
   team is defined in `.saga/teams.toml`; its personas are the files here. The lane stays the
   integrator; the foreman never lands. Measured by the saga lane (2026-10-09): `--model` wins
   over a persona file's `model:` line, so in a team the slot's effort sets the model and the
   persona sets the brief; a flow `[[step]]` may also name a `persona` (saga spec 0027).

## Build handoff

The procedure of record is spec 0050 (`specs/0050-build-handoff-pipeline.md`). In short:

1. `uas-planner` writes `engineering/plans/<spec>.md` from a landed spec: packages with files, a
   done-when, a red-first control, a persona and a wave (at most three builders, disjoint files).
2. The lane resolves anything BLOCKED and dispatches one wave through the Agent tool, each builder
   in its own worktree cut from fresh `origin/development`. The lane runs at most five agents at
   once, counting builders, critics and probers (the operator, 2026-10-10).
3. A builder commits signed, never pushes, never edits `CHANGELOG.md`, and returns the verdict
   block: 15 lines or fewer, bad news first, a tier on each claim.
4. The lane merges the builder branches into one wave branch with merge commits, adds the wave's
   one `docs(changelog)` commit, pushes with `git -C`, and opens the PR as a draft.
5. `uas-critic` makes one pass on the merged wave branch between draft and ready and returns
   numbered defects. A package that touches `unsafe`, the sandbox, the pool's kernel calls or
   `crates/uas-ebpf` also gets a pass on its builder branch before the merge (D8).
6. The lane marks the PR ready and lands it with a merge commit pinned to the head when the six
   required checks (`secrets`, `rust / cargo`, `cargo-deny`, `hygiene`, `ebpf-live`, `fork-gate`) read SUCCESS. Red or pending stops the wave.
7. After `gh pr view <n> --json state` reads MERGED, and not before, the lane removes the wave's
   worktrees, runs `git worktree prune`, and deletes the local builder and wave branches.
8. Conflicts resolve with merge commits. In `CHANGELOG.md` keep every line from both sides; in
   `specs/README.md` keep one row per spec in number order. A scripted resolution drops the whole
   `|||||||` base section (diff3). No conflict marker reaches a pushed commit.
9. The plan's last package moves the spec status (D14).

While a development hold stands, step 2 does not run: planning is reading and is exempt;
dispatching builders is development work and waits for the lift.

## Rules every agent carries

`AGENTS.md` is the rule set. In particular: `-j 2` on shared hosts (rule 2), evidence tiers with a
named command (rule 3), signed conventional commits and merge commits only (rules 5 to 7), one
CHANGELOG line per wave PR, written by the integrator (rule 8), pins in spec 0004 before use (rule 12), `unsafe` only in the syscall
crates with a `SAFETY` comment (rule 14), the crate layering, and `uas-domain` with zero I/O.
Builders never push: the push hooks judge the shared checkout, and the shared checkout belongs to
whoever is in it. Verdicts are 15 lines or fewer and say what a command proved.

## Lint

`prompt-linter` (global) checks each file here against the house manual: allowed frontmatter
fields only, ASCII, 40 to 110 body lines, a tools line, forbidden list present. Run it before a
file is committed.
