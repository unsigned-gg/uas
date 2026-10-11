#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Deny direct push to main/master. Override: "# allow-direct-push".
# Reads the Claude Code PreToolUse JSON on stdin. Prints a deny decision or nothing. Always exits 0.
input=$(cat); cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
case "$cmd" in *"# allow-direct-push"*) exit 0 ;; esac
if printf '%s' "$cmd" | grep -qE 'git +push.*(origin +)?(main|master)\b' \
   || { printf '%s' "$cmd" | grep -qE 'git +push' && [ "$(git branch --show-current 2>/dev/null)" = "main" ]; }; then
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Direct push to main is blocked. Open a PR. Override: # allow-direct-push and surface why."}}'
fi
exit 0
