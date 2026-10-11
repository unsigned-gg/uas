<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# uas cutover runbook

The order, the owner of each step, and the rollback, from spec 0037. One cutover, not a
drift (D2). History is preserved (D5).

## 1. Preparation, on development, before the transfer

| Step | Owner | State |
|---|---|---|
| `make new-repo` renames every `cas-*` crate; `make template-check` passes on `development` | lane w25:p1 | done, PR #117 |
| Export the REQ-0037-03 settings to `docs/migration/settings-before.*` | lane w25:p1 | done, this PR |
| Publish F1 to F9 as `docs/migration/uas-impact-list.md` | lane w25:p1 | done, this PR |
| sccache step degrades when its secrets are absent | lane w25:p1 | already so: `if: env.SCCACHE_BUCKET != ''` |
| This runbook | lane w25:p1 | done, this PR |
| Linear ticket for the cutover | lane w25:p1 | open |

## 2. Cutover

| Step | Owner |
|---|---|
| D7: organisation secrets and runner access on `unsigned-gg`, or CI that needs neither | Christian or Allen; the `self-hosted` scale set is already visible to `unsigned-gg` (impact list F7) |
| Transfer `cerebral-work/cas-substrate` to `unsigned-gg` and rename it `uas` | lane w25:p1, on the ruling below |
| Decide visibility | Christian |
| Decide the order relative to the `vespertin` move | Christian |

## 3. Post-cutover, one pull request

| Step | Owner |
|---|---|
| `make new-repo NAME=uas ORG=unsigned-gg`, commit the result | lane w25:p1 |
| Spec prose `cas-` to `uas-`; spec numbers, file names and REQ ids unchanged (D4) | workers dispatched by lane w25:p1 |
| Verify `.saga/teams.toml`, `.saga/topologies.toml`, `README.md`, `AGENTS.md` after `new-repo` | lane w25:p1 |
| Compare live settings against `docs/migration/settings-before.*`, record the result | lane w25:p1 |
| Cutover notice to every lane with a checkout (F8): new remote, theirs to update | lane w25:p1 |
| Lift the hold | Christian |

## 4. Ruling 2026-10-10 about 22:45Z

Christian, in lane w25:p1, directed the lane to perform the transfer and rename itself and
to dispatch workers for the rename. This supersedes D1's "no lane performs them" for the
transfer and rename only. Visibility and crates.io publication stay his acts. History is
not rewritten (D5).

## 5. Rollback

Transfer `unsigned-gg/uas` back to `cerebral-work` and rename it `cas-substrate`. GitHub
redirects both ways and PR numbers survive. The post-cutover PR is reverted with a merge
commit, never a force push.
