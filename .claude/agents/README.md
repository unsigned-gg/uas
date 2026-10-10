# Agents for cas-substrate

Repo-specific Claude Code subagents. They load at session start from this directory (restart or
`/agents` to pick up a new file). The global set in `~/.claude/agents/` stays available: use
`surface-builder`, `prober`, `ci-watcher`, `verifier`, `prompt-linter` and `linear-filer` as they
are. The global `builder` and `pr-reviewer` assume `origin/main`; here `cas-builder` and
`cas-critic` replace them. Models follow Christian's rule of 2026-10-10: engineer and critic on
opus, foreman on sonnet, builders on sonnet (Christian, 2026-10-10, estate-wide; the haiku rule of the same day is superseded), prober on haiku, every model a Claude 5.x.

| Agent | Model | Writes | Runs when |
|---|---|---|---|
| `cas-engineer` | opus | specs, briefs | a concept or architecture change, or a package needs a brief |
| `cas-builder` | sonnet | one package in a worktree | a brief names stable-Rust files and a done-when |
| `cas-ebpf-builder` | sonnet | `crates/cas-ebpf`, the object and its hash | the package touches the kernel program or the loader |
| `cas-critic` | opus | nothing | before any PR is marked ready, and on every builder SHA |
| `cas-foreman` | sonnet | nothing | only when a pane gang is applied through saga (`cas-estate`) |
| `cas-architect` | the team slot (max tier) | the design-debt list | in the `cas-estate` gang: a plan, spec or brief needs a design review |
| `cas-prober` | haiku | nothing | in the `cas-estate` gang: the foreman names a CI, PR, repo or inbox probe |

## Two tiers

1. **In-lane waves (default).** The lane runs up to three subagents at once through the Agent
   tool: builders in sibling worktrees, then `cas-critic`, then the lane pushes with
   `git -C <worktree>`, opens the PR and lands on green. Surfaces are built once with
   `surface-builder` into `/tmp/research-surfaces/cas/` and every agent reads them there.
2. **Pane gang (on request).** `saga yggdrasil team apply cas-estate --cwd <repo>` splits a
   herdr tab into the `estate-cockpit` grid; the lane starts each pane with
   `herdr agent start <name> --kind claude --pane <id> -- --model <alias>` and briefs it. The
   team is defined in `.saga/teams.toml`; its personas are the files here. The lane stays the
   integrator; the foreman never lands. Measured by the saga lane (2026-10-09): `--model` wins
   over a persona file's `model:` line, so in a team the slot's effort sets the model and the
   persona sets the brief; a flow `[[step]]` may also name a `persona` (saga spec 0027).

## Rules every agent carries

`AGENTS.md` is the rule set. In particular: `-j 2` on shared hosts (rule 2), evidence tiers with a
named command (rule 3), signed conventional commits and merge commits only (rules 5 to 7), a
CHANGELOG line (rule 8), pins in spec 0004 before use (rule 12), `unsafe` only in the syscall
crates with a `SAFETY` comment (rule 14), the crate layering, and `cas-domain` with zero I/O.
Builders never push: the push hooks judge the shared checkout, and the shared checkout belongs to
whoever is in it. Verdicts are 15 lines or fewer and say what a command proved.

## Lint

`prompt-linter` (global) checks each file here against the house manual: allowed frontmatter
fields only, ASCII, 40 to 110 body lines, a tools line, forbidden list present. Run it before a
file is committed.
