# Task library

`blackwall start --task <name>` resolves `tasks/<name>/`.

| file | required | what it holds |
|---|---|---|
| `prompt.md` | yes | The work order for the agent. `{{key}}` is replaced from `[vars]` in `task.toml`. |
| `task.toml` | no | `provider`, `model`, `budget_tokens`, `conventional_subject` (the PR title; it must parse as `type(scope): text`), and `[vars]`. |

`read_paths` and `write_paths` are parsed but not applied to the jail in blackwall 1.2.0. Do not rely on them.

Write one task per change. State the acceptance test in the prompt, so the run proves it under the `[verify]` gate.
See `.agents/skills/custody-run/SKILL.md` for the full chain.
