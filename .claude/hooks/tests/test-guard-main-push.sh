#!/usr/bin/env bash
# Tests for guard-main-push.sh. Feeds PreToolUse JSON on stdin, as Claude Code does.
# A deny prints a JSON decision. An allow prints nothing. The hook always exits 0.
# Run: bash .claude/hooks/tests/test-guard-main-push.sh
set -uo pipefail
HOOK="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/guard-main-push.sh"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
pass=0 fail=0

# run_hook <branch> <command> : prints hook stdout, records exit code in $WORK/rc
run_hook() {
  local branch="$1" cmd="$2"
  ( cd "$WORK" && rm -rf repo && git init -q repo && cd repo \
      && git symbolic-ref HEAD "refs/heads/$branch" \
      && jq -nc --arg c "$cmd" '{tool_input:{command:$c}}' | bash "$HOOK"; echo $? > "$WORK/rc" )
}

# t <deny|allow> <branch> <command> <description>
t() {
  local expect="$1" branch="$2" cmd="$3" desc="$4" out got
  out=$(run_hook "$branch" "$cmd")
  got=allow; printf '%s' "$out" | grep -q '"permissionDecision":"deny"' && got=deny
  if [ "$got" = "$expect" ] && [ "$(cat "$WORK/rc")" = "0" ]; then pass=$((pass+1)); else
    fail=$((fail+1)); printf 'FAIL [%s] expected=%s got=%s rc=%s\n     %s\n' "$desc" "$expect" "$got" "$(cat "$WORK/rc")" "$cmd"
  fi
}

t deny  feat 'git push origin main'                       'push to main'
t deny  feat 'git push origin master'                     'push to master'
t deny  feat 'git push -u origin main'                    'push -u to main'
t deny  main 'git push'                                   'bare push while on main'
t allow feat 'git push origin feat/x'                     'push a feature branch'
t allow feat 'git push'                                   'bare push on a feature branch'
t allow feat 'git push origin development'                'push development'
t allow feat 'git status'                                 'not a push'
t allow main 'git pull'                                   'pull while on main'
t allow feat 'git push origin main # allow-direct-push'   'override marker'

rm -rf "$WORK/repo"
printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
