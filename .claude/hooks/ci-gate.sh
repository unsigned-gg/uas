#!/usr/bin/env bash
# Before a `git push`, run `make check`. Deny the push when it fails (fail closed).
# Bypass: SKIP_CI_GATE=1. Reads the Claude Code PreToolUse JSON on stdin. Always exits 0.
[ "${SKIP_CI_GATE:-}" = "1" ] && exit 0
input=$(cat); cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
# A push is "git", optional "-C dir" style options, then "push" as a whole word.
printf '%s' "$cmd" | grep -qE '(^|&&|;|\||[[:space:]])git( +-[^ ]+( +[^ -][^ ]*)?)* +push([[:space:]]|$)' || exit 0
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
if ! out=$(make check 2>&1); then
  reason=$(printf '%s' "$out" | tail -15 | jq -Rsa . 2>/dev/null || printf '"make check failed"')
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":%s}}' "$reason"
fi
exit 0
