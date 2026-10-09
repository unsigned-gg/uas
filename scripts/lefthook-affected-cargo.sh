#!/usr/bin/env bash
# lefthook affected-cargo: run cargo only for the workspace crates a change set
# touches. Workspace-level changes (lockfile, root manifest, CI) fall back to the
# full workspace. Non-crate paths (docs, specs) skip cargo.
#
# Usage: lefthook-affected-cargo.sh [--staged|--push] [test|clippy]
#   --staged  diff --cached                 (pre-commit; default)
#   --push    diff against origin/development, then development, then HEAD~1
#   test      cargo test -p <affected> -j 2 (default)
#   clippy    cargo clippy -p <affected> --all-targets -j 2 -- -D warnings
#
# All cargo calls use -j 2 so a shared workstation stays responsive.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

mode="${1:---staged}"
cmd="${2:-test}"
jobs=2

case "$mode" in
  --staged) changed=$(git diff --cached --name-only --diff-filter=ACMR) ;;
  --push)
    base=""
    for ref in origin/development development; do
      if git rev-parse --verify --quiet "$ref^{commit}" >/dev/null; then
        base=$(git merge-base "$ref" HEAD 2>/dev/null || echo "")
        [ -n "$base" ] && break
      fi
    done
    if [ -z "$base" ] && git rev-parse --verify --quiet "HEAD~1^{commit}" >/dev/null; then
      base=$(git rev-parse HEAD~1)
    fi
    if [ -n "$base" ]; then
      changed=$(git diff --name-only --diff-filter=ACMR "$base"..HEAD)
    else
      # No usable base: fail closed with a full workspace run rather than an
      # empty diff that would silently skip every pre-push check.
      echo "lefthook: no base ref found - full workspace $cmd" >&2
      changed="Cargo.toml"
    fi
    ;;
  *) echo "unknown mode: $mode" >&2; exit 2 ;;
esac

[ -z "${changed//[[:space:]]/}" ] && exit 0

# Workspace-level triggers run the full workspace. This includes the hook's own tooling.
if grep -qE '^(Cargo\.(toml|lock)|rust-toolchain.*|\.github/|\.cargo/|Makefile|lefthook\.yml|scripts/|xtask/)' <<<"$changed"; then
  echo "lefthook: workspace-level change - full workspace $cmd"
  if [ "$cmd" = clippy ]; then
    exec cargo clippy --workspace --all-targets -j "$jobs" -- -D warnings
  fi
  exec cargo test --workspace --quiet -j "$jobs"
fi

pkgs=()
while IFS= read -r f; do
  case "$f" in
    crates/*/*)
      # The workspace crate is always crates/<name>. A nested package below it
      # (crates/cas-ebpf/ebpf, the kernel program, built outside the workspace)
      # maps to its parent crate, whose build.rs checks the committed object.
      d=$(cut -d/ -f1-2 <<<"$f")
      [ -f "$d/Cargo.toml" ] || continue
      name=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$d/Cargo.toml" | head -1)
      [ -n "$name" ] && pkgs+=("$name")
      ;;
  esac
done <<<"$changed"

if [ ${#pkgs[@]} -eq 0 ]; then
  echo "lefthook: no crate changes - skipping cargo $cmd"
  exit 0
fi

mapfile -t pkgs < <(printf '%s\n' "${pkgs[@]}" | sort -u)
args=(); for p in "${pkgs[@]}"; do args+=(-p "$p"); done
echo "lefthook: affected crates: ${pkgs[*]}"
if [ "$cmd" = clippy ]; then
  exec cargo clippy "${args[@]}" --all-targets -j "$jobs" -- -D warnings
fi
exec cargo test "${args[@]}" --quiet -j "$jobs"
