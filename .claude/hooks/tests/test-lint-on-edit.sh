#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Tests for lint-on-edit.sh. A stub `rustfmt` records its arguments.
# Run: bash .claude/hooks/tests/test-lint-on-edit.sh
set -uo pipefail
HOOK="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/lint-on-edit.sh"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir "$WORK/bin"
cat > "$WORK/bin/rustfmt" <<'EOF'
#!/bin/sh
echo "$*" > "$RUSTFMT_LOG"
exit "${STUB_RUSTFMT_RC:-0}"
EOF
chmod +x "$WORK/bin/rustfmt"
echo 'fn main(){}' > "$WORK/a.rs"
echo 'x' > "$WORK/b.md"
pass=0 fail=0

# t <called|skipped> <json> <env assignments...> -- description handled by caller
t() {
  local expect="$1" json="$2" desc="$3" rc_stub="${4:-0}" skip="${5:-0}" got hook_rc
  export RUSTFMT_LOG="$WORK/log"; rm -f "$RUSTFMT_LOG"
  printf '%s' "$json" | PATH="$WORK/bin:$PATH" STUB_RUSTFMT_RC="$rc_stub" SKIP_LINT="$skip" bash "$HOOK"
  hook_rc=$?
  got=skipped; [ -f "$RUSTFMT_LOG" ] && got=called
  if [ "$got" = "$expect" ] && [ "$hook_rc" = 0 ]; then pass=$((pass+1)); else
    fail=$((fail+1)); printf 'FAIL [%s] expected=%s got=%s rc=%s\n' "$desc" "$expect" "$got" "$hook_rc"
  fi
}

t called  "{\"tool_input\":{\"file_path\":\"$WORK/a.rs\"}}"      'rs file from tool_input'
if grep -q -- "--edition 2021 $WORK/a.rs" "$WORK/log"; then pass=$((pass+1)); else fail=$((fail+1)); echo 'FAIL [rustfmt arguments]'; fi
t called  "{\"tool_response\":{\"filePath\":\"$WORK/a.rs\"}}"    'rs file from tool_response'
t skipped "{\"tool_input\":{\"file_path\":\"$WORK/b.md\"}}"      'markdown file'
t skipped "{\"tool_input\":{\"file_path\":\"$WORK/none.rs\"}}"   'missing file'
t skipped '{}'                                                    'no path in input'
t called  "{\"tool_input\":{\"file_path\":\"$WORK/a.rs\"}}"      'rustfmt failure never blocks' 1
t skipped "{\"tool_input\":{\"file_path\":\"$WORK/a.rs\"}}"      'SKIP_LINT=1' 0 1

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
