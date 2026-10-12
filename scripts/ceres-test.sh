#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# ceres-test: run the repeatable local substrate test suite on host ceres.
# Validates spec conformity, zero-I/O domain boundary, crate integration suites,
# and delegated cgroup v2 freezer/limits without requiring sudo.
set -euo pipefail

JOBS="${JOBS:-2}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "=== [1/6] Toolchain and LeanSpec Verification ==="
cargo xtask toolchain-check
cargo xtask spec-check
echo "toolchain and specs: ok"

echo "=== [2/6] Domain Zero-I/O Boundary Check ==="
if grep -rnE 'std::net|std::fs|tokio|std::process' crates/uas-domain/src; then
    echo "ceres-test: error: uas-domain contains forbidden I/O calls" >&2
    exit 1
fi
echo "domain zero-i/o: ok"

echo "=== [3/6] Monorepo Crate Integration Suites (jobs: ${JOBS}) ==="
cargo test -p uas-domain --locked -j "${JOBS}"
cargo test -p uas-sandbox --locked -j "${JOBS}"
cargo test -p uas-pool --locked -j "${JOBS}"
cargo test -p uas-engine --locked -j "${JOBS}"
cargo test -p uas-acp --locked -j "${JOBS}"
cargo test -p uas-router --locked -j "${JOBS}"
cargo test -p uas-deck --locked -j "${JOBS}"
cargo test -p uas-cli --locked -j "${JOBS}"
echo "crate integration suites: ok"

echo "=== [4/6] Live Cgroup v2 Subtree Delegation Test ==="
if command -v systemd-run >/dev/null 2>&1; then
    make pool-live-test JOBS="${JOBS}"
    echo "cgroup v2 subtree delegation: ok"
else
    echo "systemd-run not found; skipping delegated cgroup v2 live test"
fi

echo "=== [5/6] CLI Smoke & Version Protocol ==="
make smoke JOBS="${JOBS}"
echo "cli smoke: ok"

echo "=== [6/6] Repository Hygiene ==="
cargo xtask pin-check
cargo xtask text-check --base "${BASE:-origin/development}" --head "${HEAD:-HEAD}"
echo "hygiene: ok"

echo "================================================="
echo "ceres-test: ALL SUBSTRATE TESTS VERIFIED ON CERES"
echo "================================================="
