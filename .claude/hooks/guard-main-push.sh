#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Deny direct push to main, master or development, including `git -C dir push` and a bare push
# while main or development is checked out. Override: "# allow-direct-push".
# Reads the Claude Code PreToolUse JSON on stdin. Prints a deny decision or nothing. Always exits 0.
input=$(cat); cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
case "$cmd" in *"# allow-direct-push"*) exit 0 ;; esac

protected='(refs/heads/)?(main|master|development)'
deny=0
# One shell segment per line; a segment is a push when it is git, option words, then push.
while IFS= read -r seg; do
  seg=${seg#"${seg%%[![:space:]]*}"}
  read -ra w <<<"$seg"
  [ "${w[0]:-}" = git ] || continue
  i=1 dir=.
  while [ "$i" -lt "${#w[@]}" ] && [ "${w[$i]}" != push ]; do
    case "${w[$i]}" in
      -C) dir=${w[$((i+1))]:-.}; i=$((i+2)) ;;
      -c) i=$((i+2)) ;;
      -*) i=$((i+1)) ;;
      *) i=${#w[@]} ;;
    esac
  done
  [ "${w[$i]:-}" = push ] || continue
  # Positional args after push: the first is the remote, the rest are refspecs.
  args=()
  for a in "${w[@]:$((i+1))}"; do
    case "$a" in -*) ;; *) args+=("$a") ;; esac
  done
  for a in "${args[@]:1}"; do
    a=${a#+}; a=${a##*:}
    if [[ $a =~ ^${protected}$ ]]; then deny=1; fi
  done
  if [ "${#args[@]}" -le 1 ]; then
    cur=$(git -C "$dir" branch --show-current 2>/dev/null)
    case "$cur" in main|development) deny=1 ;; esac
  fi
done < <(printf '%s\n' "$cmd" | sed -E 's/(\|\||&&|[;&|(])/\n/g')

if [ "$deny" = 1 ]; then
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Direct push to main, master or development is blocked. Open a PR. Override: # allow-direct-push and surface why."}}'
fi
exit 0
