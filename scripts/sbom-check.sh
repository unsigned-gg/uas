#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# sbom-check: validate a directory of CycloneDX SBOMs (spec 0055 D10).
# usage: sbom-check.sh <dir> [expected-count]
# The count defaults to the workspace package count. Exits 1 with
# "sbom-check: <reason>: <path>" on the first failure, else prints "sbom-check: ok".
# No per-file component floor: uas-storage has no components (spec 0055 F7).
set -euo pipefail

[ $# -ge 1 ] && [ $# -le 2 ] || { echo "usage: sbom-check.sh <dir> [expected-count]" >&2; exit 2; }
dir="$1"
if [ $# -eq 2 ]; then
  want="$2"
else
  want="$(cargo metadata --no-deps --format-version 1 | jq '.packages | length')"
fi

files=()
if [ -d "$dir" ]; then
  for f in "$dir"/*.cdx.json; do
    [ -e "$f" ] && files+=("$f")
  done
fi

if [ "${#files[@]}" -ne "$want" ]; then
  echo "sbom-check: count ${#files[@]} want $want: $dir"
  exit 1
fi

for f in "${files[@]}"; do
  if ! { [ -s "$f" ] && jq -e '.bomFormat == "CycloneDX" and .specVersion == "1.5"' "$f" >/dev/null 2>&1; }; then
    echo "sbom-check: invalid: $f"
    exit 1
  fi
done

total="$(jq -s '[.[].components // [] | length] | add // 0' "${files[@]}")"
if [ "$total" -le 0 ]; then
  echo "sbom-check: no components: $dir"
  exit 1
fi

echo "sbom-check: ok"
