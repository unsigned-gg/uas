---
name: custody-run
description: Use when you run agent work under blackwall custody in this repo. Covers the task file, the jailed run, the delta trace, settle, reconcile with the right base, the PR into development, and the manual merge-commit rule.
---

# Custody run

The chain is start, settle, reconcile, pr, then a manual merge. Config: `blackwall.toml`. SOP: `~/.agents/sops/blackwall-custody-process.md`.

## Before you start

1. Run `make custody-check` (`blackwall doctor`). The jail needs Landlock; the provider needs `claude` on PATH.
2. Work in a clean worktree cut from `origin/development`. `blackwall init` copies the tree and does not honour `.gitignore`.
3. Run blackwall as `env -u GITHUB_TOKEN blackwall ...` if your shell sets a token for `gh`.

## The chain

```bash
blackwall init .                                    # once per worktree
blackwall start --task <name> --provider claude --model sonnet
blackwall show <hash>; blackwall changeset <hash>   # read what the run did
blackwall run trace <hash>                          # the delta trace: one entry per tool call
blackwall settle <hash> select                      # the settle gate: read the changeset first
blackwall reconcile <hash> --base <base_commit>     # runs [verify]; branch blackwall/<hash12>
blackwall pr <hash>                                 # pushes the branch, opens a PR into development
blackwall merge-gate <PR> --watch                   # reports `ready`; it never merges here
gh pr merge <PR> --merge --match-head-commit <sha>  # the merge, by a person or the lane
```

## Rules

- **Base commit.** A changeset holds whole files. Pass the run's base commit to `reconcile` (`blackwall show <hash>` prints it). The default HEAD can silently revert newer work.
- **Settle is a gate, but nothing enforces it.** Do not run `blackwall watch`, which skips it. Read the changeset and the trace before `settle`.
- **Merge commits only.** `merge-gate` squash-merges when armed, so `[merge] automerge` stays `false`.
- **Signed commits.** reconcile commits with the host git config, so set `commit.gpgsign=true`.
- **Never push to main.** `pr` pushes only `blackwall/*` branches.
