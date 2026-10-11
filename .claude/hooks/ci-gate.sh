#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Before a `git push`, run `make check` under `timeout` (CI_GATE_TIMEOUT, default 840 s).
# Deny the push when it fails or times out (fail closed).
# Bypass: SKIP_CI_GATE=1. Reads the Claude Code PreToolUse JSON on stdin. Always exits 0.
[ "${SKIP_CI_GATE:-}" = "1" ] && exit 0
input=$(cat); cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
# A push is "git", optional "-C dir" style options, then "push" as a whole word.
printf '%s' "$cmd" | grep -qE '(^|&&|;|\||[[:space:]])git( +-[^ ]+( +[^ -][^ ]*)?)* +push([[:space:]]|$)' || exit 0
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
limit="${CI_GATE_TIMEOUT:-840}"   # under the 900 s harness timeout, so this fires first
out=$(timeout -k 10 "$limit" make check 2>&1); rc=$?
if [ "$rc" -ne 0 ]; then
  if [ "$rc" -eq 124 ] || [ "$rc" -eq 137 ]; then
    reason="ci-gate: make check hit the ${limit}s timeout (exit $rc); push denied"
  else
    reason=$(printf '%s' "$out" | tail -15)
  fi
  echo "ci-gate: push denied, make check exited $rc" >&2
  reason=$(printf '%s' "$reason" | jq -Rsa . 2>/dev/null || printf '"make check failed"')
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":%s}}' "$reason"
fi
exit 0
