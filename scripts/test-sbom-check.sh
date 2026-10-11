#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# Fixture tests for scripts/sbom-check.sh (spec 0055 D10). Each case asserts the
# exit code and the first output line, so a validator that accepts everything
# turns the bad-input cases red.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

r=0
# expect NAME WANT_RC WANT_FIRST_LINE DIR COUNT
expect() {
  local name="$1" want_rc="$2" want_line="$3" dir="$4" count="$5" out rc=0
  out="$("$here/sbom-check.sh" "$dir" "$count" 2>&1)" || rc=$?
  local first="${out%%$'\n'*}"
  if [ "$rc" = "$want_rc" ] && [ "$first" = "$want_line" ]; then
    echo "ok   $name"
  else
    echo "RED  $name: got rc=$rc '$first', want rc=$want_rc '$want_line'"
    r=1
  fi
}

doc() { printf '{"bomFormat":"CycloneDX","specVersion":"%s","components":%s}\n' "$1" "$2"; }

mkdir "$tmp/ok" "$tmp/empty" "$tmp/zero" "$tmp/obj" "$tmp/missing" "$tmp/old" "$tmp/nocomp" "$tmp/notjson"
doc 1.5 '[{"name":"a"}]' > "$tmp/ok/a.cdx.json"
doc 1.5 '[]' > "$tmp/ok/b.cdx.json"
: > "$tmp/zero/a.cdx.json"
echo '{}' > "$tmp/obj/a.cdx.json"
doc 1.5 '[{"name":"a"}]' > "$tmp/missing/a.cdx.json"
doc 1.3 '[{"name":"a"}]' > "$tmp/old/a.cdx.json"
doc 1.5 '[]' > "$tmp/nocomp/a.cdx.json"
echo 'not json' > "$tmp/notjson/a.cdx.json"

expect pass         0 "sbom-check: ok"                       "$tmp/ok"      2
expect empty        1 "sbom-check: count 0 want 1: $tmp/empty" "$tmp/empty"  1
expect zero-byte    1 "sbom-check: invalid: $tmp/zero/a.cdx.json" "$tmp/zero" 1
expect empty-object 1 "sbom-check: invalid: $tmp/obj/a.cdx.json"  "$tmp/obj"  1
expect not-json     1 "sbom-check: invalid: $tmp/notjson/a.cdx.json" "$tmp/notjson" 1
expect missing-file 1 "sbom-check: count 1 want 2: $tmp/missing"  "$tmp/missing" 2
expect spec-1.3     1 "sbom-check: invalid: $tmp/old/a.cdx.json"  "$tmp/old"  1
expect no-component 1 "sbom-check: no components: $tmp/nocomp"    "$tmp/nocomp" 1
expect no-such-dir  1 "sbom-check: count 0 want 1: $tmp/none"     "$tmp/none" 1
expect bad-count    2 "usage: sbom-check.sh <dir> [expected-count]"  "$tmp/none" abc

exit "$r"
