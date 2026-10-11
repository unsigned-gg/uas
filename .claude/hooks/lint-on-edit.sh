#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# After an Edit or Write of a .rs file, run rustfmt on that file. Advisory: never blocks.
# Bypass: SKIP_LINT=1. Reads the Claude Code PostToolUse JSON on stdin. Always exits 0.
[ "${SKIP_LINT:-}" = "1" ] && exit 0
input=$(cat); f=$(printf '%s' "$input" | jq -r '.tool_input.file_path // .tool_response.filePath // ""')
[ -f "$f" ] || exit 0
case "$f" in
  *.rs) command -v rustfmt >/dev/null 2>&1 && rustfmt --edition 2021 "$f" >/dev/null 2>&1 ;;
esac
exit 0
