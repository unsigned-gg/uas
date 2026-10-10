# .claude/ - Claude Code settings and hooks

The rules for this repo live in [AGENTS.md](../AGENTS.md). This directory holds only the Claude Code wiring.

`settings.json` is shared and committed. `settings.local.json` is per user and gitignored.

## Skills

`skills/` holds one symlink per repo skill, pointing into `../../.agents/skills/`. Claude Code only discovers skills under `.claude/skills/`, so a new skill needs its symlink here as well. Run `/skills` to list them.

## Hooks

| Hook | Event | Purpose | Bypass |
|---|---|---|---|
| `guard-main-push.sh` | PreToolUse, Bash | Deny a direct push to `main` or `master`. | Add `# allow-direct-push` to the command, and say why. |
| `ci-gate.sh` | PreToolUse, Bash | On a push command, run `make check`. Deny the push if it fails. The reason holds the last 15 lines of output. | `SKIP_CI_GATE=1` in the environment. |
| `lint-on-edit.sh` | PostToolUse, Edit or Write | Run `rustfmt --edition 2021` on an edited `.rs` file. Never blocks. | `SKIP_LINT=1` in the environment. |

## Tests

One test per hook is in `hooks/tests/`. Each test is plain bash. It sends JSON on stdin, as Claude Code does.

```bash
for t in .claude/hooks/tests/*.sh; do bash "$t" || exit 1; done
```

## Permissions

Allowed without a prompt: `cargo`, `make`, `moon`, `git fetch`, `git rebase`. Denied: reads of `docs/archive/**`.
