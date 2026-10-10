# Agents for uas

Repo-specific Claude Code subagents. They load at session start from this directory (restart or
`/agents` to pick up a new file). The global set in `~/.claude/agents/` stays available: use
`surface-builder`, `prober`, `ci-watcher`, `verifier`, `prompt-linter` and `linear-filer` as they
are. The global `builder` and `pr-reviewer` assume `origin/main`; here `uas-builder` and
`uas-critic` replace them. Models follow Christian's rule of 2026-10-10: engineer and critic on
opus, foreman on sonnet, builders on sonnet (Christian, 2026-10-10, estate-wide; the haiku rule of the same day is superseded), prober on haiku, every model a Claude 5.x.

| Agent | Model | Writes | Runs when |
|---|---|---|---|
| `uas-engineer` | opus | specs, briefs | a concept or architecture change, or a package needs a brief |
| `uas-builder` | sonnet | one package in a worktree | a brief names stable-Rust files and a done-when |
| `uas-ebpf-builder` | sonnet | `crates/uas-ebpf`, the object and its hash | the package touches the kernel program or the loader |
| `uas-critic` | opus | nothing | before any PR is marked ready, and on every builder SHA |
| `uas-foreman` | sonnet | nothing | only when a pane gang is applied through saga (`uas-estate`) |
| `uas-architect` | the team slot (max tier) | the design-debt list | in the `uas-estate` gang: a plan, spec or brief needs a design review |
| `uas-prober` | haiku | nothing | in the `uas-estate` gang: the foreman names a CI, PR, repo or inbox probe |
| `uas-planner` | sonnet | `engineering/plans/<spec>.md` | a landed spec is to be built: it packages the Plan into briefs, waves and done-whens |
| `uas-k8s-builder` | sonnet | `crates/uas-operator`, its CRDs and the spec 0039 feature code | a brief names the operator, a CRD or a k3d test; never a live cluster |

## Two tiers

1. **In-lane waves (default).** The lane runs up to three subagents at once through the Agent
   tool: builders in sibling worktrees, then `uas-critic`, then the lane pushes with
   `git -C <worktree>`, opens the PR and lands on green. Surfaces are built once with
   `surface-builder` into `/tmp/research-surfaces/uas/` and every agent reads them there.
2. **Pane gang (on request).** `saga yggdrasil team apply uas-estate --cwd <repo>` splits a
   herdr tab into the `estate-cockpit` grid; the lane starts each pane with
   `herdr agent start <name> --kind claude --pane <id> -- --model <alias>` and briefs it. The
   team is defined in `.saga/teams.toml`; its personas are the files here. The lane stays the
   integrator; the foreman never lands. Measured by the saga lane (2026-10-09): `--model` wins
   over a persona file's `model:` line, so in a team the slot's effort sets the model and the
   persona sets the brief; a flow `[[step]]` may also name a `persona` (saga spec 0027).

## Build handoff

Christian's rule of 2026-10-10 in lane w25:p1: build tasks go to orchestrated agents, and the
lane keeps decisions, integration and every gated action. The pipeline, per spec:

1. `uas-planner` reads the landed spec and writes `engineering/plans/<spec>.md`: packages with
   files, a done-when from the spec's Test section, the red-first control, the persona, and a
   wave order of at most three concurrent builders with disjoint files.
2. The lane reads the plan, resolves anything BLOCKED (a ruling, a spec 0004 pin), and
   dispatches wave 1 through the Agent tool: `uas-builder`, `uas-ebpf-builder` or
   `uas-k8s-builder`, each in its own worktree cut from fresh `origin/development`, each with
   the package brief verbatim. Up to the lane's cap at once: three by the global rule, raised to five by Christian on
   2026-10-10 for this lane (the account's token ceiling is the reason for a cap at all).
3. Each builder returns a SHA and the verdict block. `uas-critic` reads every SHA against its
   brief and the spec; a defect goes back to the same builder with the critic's line.
4. The lane merges the wave's branches into one feature branch, pushes with `git -C`, opens the
   PR with the receipts in the body, and `ci-watcher` (global) or `uas-prober` watches the
   checks. Green lands with a merge commit; red stops the wave (no merge around it).
5. The next wave starts from the new `origin/development`. The plan file is marked done when
   every package landed, and the spec's status moves to `in-progress` or `complete`.

While a development hold stands, step 2 does not run: planning is reading and is exempt;
dispatching builders is development work and waits for the lift.

## Rules every agent carries

`AGENTS.md` is the rule set. In particular: `-j 2` on shared hosts (rule 2), evidence tiers with a
named command (rule 3), signed conventional commits and merge commits only (rules 5 to 7), a
CHANGELOG line (rule 8), pins in spec 0004 before use (rule 12), `unsafe` only in the syscall
crates with a `SAFETY` comment (rule 14), the crate layering, and `uas-domain` with zero I/O.
Builders never push: the push hooks judge the shared checkout, and the shared checkout belongs to
whoever is in it. Verdicts are 15 lines or fewer and say what a command proved.

## Lint

`prompt-linter` (global) checks each file here against the house manual: allowed frontmatter
fields only, ASCII, 40 to 110 body lines, a tools line, forbidden list present. Run it before a
file is committed.
