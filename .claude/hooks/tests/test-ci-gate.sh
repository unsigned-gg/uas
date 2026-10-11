#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Tests for ci-gate.sh. A stub `make` replaces the real gate.
# The stub drops a marker file (gate reached) and exits with $STUB_MAKE_RC.
# Run: bash .claude/hooks/tests/test-ci-gate.sh
set -uo pipefail
HOOK="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ci-gate.sh"
STUB=$(mktemp -d)
trap 'rm -rf "$STUB"' EXIT
cat > "$STUB/make" <<'EOF'
#!/bin/sh
echo reached > "$CIGATE_MARKER"
echo "stub make output: $*"
exit "${STUB_MAKE_RC:-0}"
EOF
chmod +x "$STUB/make"
pass=0 fail=0

# v <reached|filtered> <make-rc> <skip> <command> <description>
v() {
  local expect="$1" rc="$2" skip="$3" cmd="$4" desc="$5" out got
  export CIGATE_MARKER; CIGATE_MARKER=$(mktemp -u)
  # stdin comes from a file: the hook may exit before it reads, and a pipe would SIGPIPE jq
  jq -nc --arg c "$cmd" '{tool_input:{command:$c}}' > "$STUB/in.json"
  out=$(PATH="$STUB:$PATH" STUB_MAKE_RC="$rc" SKIP_CI_GATE="$skip" CLAUDE_PROJECT_DIR="$STUB" bash "$HOOK" < "$STUB/in.json")
  local hook_rc=$?
  got=filtered; [ -f "$CIGATE_MARKER" ] && got=reached
  rm -f "$CIGATE_MARKER"
  if [ "$got" = "$expect" ] && [ "$hook_rc" = 0 ]; then pass=$((pass+1)); else
    fail=$((fail+1)); printf 'FAIL [%s] expected=%s got=%s rc=%s\n     %s\n' "$desc" "$expect" "$got" "$hook_rc" "$cmd"
  fi
  LAST_OUT="$out"
}

# pushes that must reach the gate
v reached 0 0 'git push'                          'bare push'
v reached 0 0 'git push origin feat/x'            'push a branch'
v reached 0 0 'git commit -am x && git push'      'chained commit and push'
v reached 0 0 'git -C /some/dir push'             'git -C dir push'
v reached 0 0 'true; git push'                    'semicolon then push'
# non-pushes that must not reach the gate
v filtered 0 0 'git status'                       'git status'
v filtered 0 0 'git config alias.push "push --ff"' 'git config alias.push'
v filtered 0 0 'echo deploy and push later'       'prose with the word push'
v filtered 0 0 'git pull'                         'git pull'
# bypass
v filtered 0 1 'git push'                         'SKIP_CI_GATE=1 skips the gate'

# a failing gate must deny, and the reason must carry the make output
v reached 1 0 'git push' 'failing gate'
if printf '%s' "$LAST_OUT" | grep -q '"permissionDecision":"deny"' && printf '%s' "$LAST_OUT" | grep -q 'stub make output: check'; then
  pass=$((pass+1))
else
  fail=$((fail+1)); printf 'FAIL [failing gate denies with output tail] got: %s\n' "$LAST_OUT"
fi
# a passing gate must print nothing
v reached 0 0 'git push' 'passing gate'
if [ -z "$LAST_OUT" ]; then pass=$((pass+1)); else
  fail=$((fail+1)); printf 'FAIL [passing gate prints nothing] got: %s\n' "$LAST_OUT"
fi

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
