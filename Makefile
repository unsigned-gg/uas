# uas task runner. Run `make` to list targets.
# Shared workstations keep JOBS=2. CI passes JOBS=10 (specs/0002-verification-gates-and-evidence.md).
SHELL := bash
.SHELLFLAGS := -euo pipefail -c
.ONESHELL:
.DEFAULT_GOAL := help

JOBS ?= 2
NEXTEST_PROFILE ?= default
ORG ?= cerebral-work
# Dated nightly for the eBPF object (spec 0005 D1). Pinned by commit hash in specs/0004.
EBPF_TOOLCHAIN ?= nightly-2026-10-06
SUDO ?= sudo
SBOM_DIR ?= sbom

.PHONY: help build fmt fmt-check clippy test doc-test deny check ci hygiene typos machete \
	spec-check toolchain-check pin-check text-check oss-check spdx-check hook-test smoke audit sbom sbom-check doc release-notes custody-check pgt-install new-repo template-check \
	ebpf-build ebpf-test fetch-contracts pool-live-test features feature-leg

help: ## List targets
	awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

build: ## Build the workspace
	cargo build --workspace --locked -j $(JOBS)

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting
	cargo fmt --all -- --check

clippy: ## Lint with clippy, warnings are errors
	cargo clippy --workspace --all-targets --locked -j $(JOBS) -- -D warnings

test: ## Run tests with nextest (NEXTEST_PROFILE=default)
	cargo nextest run --workspace --locked --build-jobs $(JOBS) --profile $(NEXTEST_PROFILE)

doc-test: ## Run doc tests
	cargo test --doc --workspace --locked -j $(JOBS)

deny: ## Check licences, advisories, bans and sources
	cargo deny --locked check
	cargo deny --locked --manifest-path crates/uas-ebpf/ebpf/Cargo.toml check --config deny.toml licenses

check: fmt-check clippy test doc-test deny ## Full gate: fmt, clippy, test, doc-test, deny

ci: ## What the CI rust job runs
	$(MAKE) NEXTEST_PROFILE=ci fmt-check clippy test doc-test

hygiene: typos machete spec-check toolchain-check pin-check oss-check spdx-check hook-test ## Typos, unused deps, specs, toolchain, pins, tree hygiene, hook tests

# Spec 0051 D6, REQ-0051-13. The three named scripts must exist; any extra test-*.sh runs too.
hook-test: ## Run the Claude Code hook tests
	@for n in guard-main-push ci-gate lint-on-edit; do \
	  test -f .claude/hooks/tests/test-$$n.sh || { echo "hook-test: missing .claude/hooks/tests/test-$$n.sh" >&2; exit 1; }; \
	done
	@for t in .claude/hooks/tests/test-*.sh; do echo "hook-test: $$t"; bash "$$t" || exit 1; done

# Spec 0048 REQ-0048-08: the pattern leg, then the leg that bars the three removed vendor directories (plan P11).
# The vendor LICENSE leg is spec 0048 D16 (plan P16).
# Each alternative is written so that this text does not match its own pattern.
OSS_PATTERN := \.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|<surfaces>(s)

oss-check: ## No tailnet hostname or operator path, no removed vendor directory, a LICENSE in each vendor directory (spec 0048)
	@bad=0
	if git grep -n -E -e '$(OSS_PATTERN)'; then echo "oss-check: tracked files above hold a hostname or operator path" >&2; bad=1; fi
	for d in vendor/cortex vendor/soma vendor/saga; do if [ -e "$$d" ]; then echo "oss-check: $$d must not exist; contracts live in .contracts/ (make fetch-contracts)" >&2; bad=1; fi; done
	for d in vendor/*/; do d=$${d%/}; [ -d "$$d" ] || continue; if [ ! -f "$$d/LICENSE" ]; then echo "oss-check: $$d has no LICENSE file" >&2; bad=1; fi; done
	exit $$bad

# Spec 0048 D9, REQ-0048-11. Not a prerequisite of check: the default build reads no contract. Run it before
# 'cargo build -p uas-engine --features cortex-grpc' and before 'make features', which builds the same feature.
# It needs SSH read access to the private upstream repositories.
fetch-contracts: ## Fetch the pinned estate contracts into .contracts/ (contracts.lock)
	cargo xtask fetch-contracts

# Spec 0033. One crate:feature pair per declared cargo feature. 'cargo xtask feature-check' reads this line.
FEATURE_LEGS := uas-engine:cortex-grpc uas-operator:k8s

# Not a prerequisite of check or ci (the contracts are fetched on demand). Needs 'make fetch-contracts' first.
feature-leg: ## Clippy and tests for one cargo feature (CRATE=<crate> FEATURE=<feature>)
	crate="$${CRATE-}"; feature="$${FEATURE-}"
	[[ "$$crate" =~ ^[a-z][a-z0-9-]*$$ && "$$feature" =~ ^[a-z][a-z0-9-]*$$ ]] || { echo "usage: make feature-leg CRATE=<crate> FEATURE=<feature>" >&2; exit 2; }
	cargo clippy -p "$$crate" --features "$$feature" --all-targets --locked -j $(JOBS) -- -D warnings
	cargo nextest run -p "$$crate" --features "$$feature" --locked --build-jobs $(JOBS) --profile $(NEXTEST_PROFILE)

features: ## Clippy and tests once per declared cargo feature; needs make fetch-contracts first
	failed=0
	for leg in $(FEATURE_LEGS); do
		if ! $(MAKE) --no-print-directory feature-leg CRATE="$${leg%%:*}" FEATURE="$${leg#*:}"; then
			echo "features: FAILED $$leg" >&2
			failed=1
		fi
	done
	exit $$failed

# Spec 0048 REQ-0048-04, D2, D10. Code: SPDX line in the first two lines with the right expression.
# Markdown under docs, specs and engineering: CC-BY-4.0 tag in the first ten lines.
spdx-check: ## Every code file and doc carries its SPDX tag (spec 0048 REQ-04)
	@bad=0
	code="$$(git ls-files -- '*.rs' '*.sh' ':!:vendor/*')" || { echo "spdx-check: git ls-files failed (not a git tree?)" >&2; exit 1; }
	docs="$$(git ls-files -- 'docs/*.md' 'specs/*.md' 'engineering/*.md')" || { echo "spdx-check: git ls-files failed (not a git tree?)" >&2; exit 1; }
	[ -n "$$code" ] || { echo "spdx-check: git ls-files lists no .rs or .sh files" >&2; exit 1; }
	[ -n "$$docs" ] || { echo "spdx-check: git ls-files lists no Markdown under docs, specs or engineering" >&2; exit 1; }
	while IFS= read -r f; do
	  expr="$$(head -n 2 "$$f" | sed -n 's/.*SPDX-License-Identifier:[[:space:]]*\(.*[^[:space:]]\)[[:space:]]*$$/\1/p' | head -n 1)"
	  case "$$f" in crates/uas-ebpf/ebpf/*) want='GPL-2.0-only OR MIT';; *) want='Apache-2.0 OR MIT';; esac
	  if [ -z "$$expr" ]; then echo "spdx-check: $$f: no SPDX-License-Identifier line in the first two lines" >&2; bad=1
	  elif [ "$$expr" != "$$want" ]; then echo "spdx-check: $$f: expression '$$expr', want '$$want'" >&2; bad=1; fi
	done <<< "$$code"
	while IFS= read -r f; do
	  head -n 10 "$$f" | grep -q 'SPDX-License-Identifier: CC-BY-4.0' || { echo "spdx-check: $$f: no CC-BY-4.0 tag in the first ten lines" >&2; bad=1; }
	done <<< "$$docs"
	[ "$$bad" -eq 0 ] || exit 1
	echo "spdx-check: ok"

typos: ## Spell check
	typos

machete: ## Find unused dependencies
	cargo machete

spec-check: ## Validate specs/ (EARS rules)
	cargo xtask spec-check

toolchain-check: ## Verify rustc matches rust-toolchain.toml
	cargo xtask toolchain-check

pin-check: ## Verify the spec 0004 pin tables against the lockfiles
	cargo xtask pin-check

# Spec 0053 D5. Not part of `hygiene`: that job also runs on push, which has no base. Values come
# from the environment (BASE, HEAD, PR_BODY_FILE, HEAD_REF), never through make expansion, so a
# hostile branch name or PR body cannot reach a shell word (REQ-0053-14).
text-check: ## CHANGELOG line, commit subjects, PR body (BASE=origin/development; PR_BODY_FILE, HEAD_REF optional)
	cargo xtask text-check --base "$${BASE:-origin/development}" --head "$${HEAD:-HEAD}" $${PR_BODY_FILE:+--body-file "$$PR_BODY_FILE"} $${HEAD_REF:+--head-ref "$$HEAD_REF"}

smoke: ## Build the CLI, run it, check the domain crate has no I/O
	cargo build -p uas-cli --locked -j $(JOBS)
	out="$$(cargo run -q -p uas-cli --locked -j $(JOBS) -- --version)"
	echo "$$out"
	[[ "$$out" =~ ^uas\ [0-9]+\.[0-9]+\.[0-9]+\ \(.+\)$$ ]] || { echo "smoke: unexpected --version output" >&2; exit 1; }
	# grep reads all of the demo output on purpose: `grep -q` exits at the first match,
	# the demo then dies of SIGPIPE, and pipefail reports that as a failed smoke (1 in 4 runs).
	cargo run -q -p uas-cli --locked -j $(JOBS) -- demo | grep '"done"' >/dev/null || { echo "smoke: demo did not reach done" >&2; exit 1; }
	if grep -rnE 'std::net|std::fs|tokio|std::process' crates/uas-domain/src; then
		echo "smoke: uas-domain must not use I/O" >&2
		exit 1
	fi
	echo "smoke: ok"

ebpf-build: ## Build the kernel eBPF artifact into crates/uas-ebpf/ebpf/uas-ebpf.o
	PATH="$$HOME/.local/share/mise/shims:$$PATH" cargo +$(EBPF_TOOLCHAIN) build \
		--manifest-path crates/uas-ebpf/ebpf/Cargo.toml \
		--target bpfel-unknown-none -Z build-std=core --release --locked -j $(JOBS)
	cp crates/uas-ebpf/ebpf/target/bpfel-unknown-none/release/uas-ebpf-ebpf crates/uas-ebpf/ebpf/uas-ebpf.o
	sha256sum crates/uas-ebpf/ebpf/uas-ebpf.o | cut -d' ' -f1 > crates/uas-ebpf/ebpf/uas-ebpf.o.sha256

# Live eBPF probes (spec 0014 D8). Build unprivileged, then run only the test binary. SUDO defaults to sudo on a workstation; CI passes SUDO= (uid 0 in the ci-kernel pod).
ebpf-test: ## Run the live eBPF probe tests (SUDO on a workstation, SUDO= in CI)
	BIN=$$(cargo test -p uas-ebpf --test live_probes --no-run -j $(JOBS) --message-format=json 2>/dev/null | jq -r 'select(.reason=="compiler-artifact" and .target.name=="live_probes") | .executable' | grep -v null | tail -n1); \
	test -x "$$BIN" || { echo "ebpf-test: live_probes binary not found"; exit 1; }; \
	$(SUDO) "$$BIN" --nocapture --test-threads=1

# Live uas-pool cgroup cases (plans 0040 P8, 0041 P8, 0015 P6). Needs a delegated cgroup v2 subtree: a uid-owned
# user slice, run as that uid, no sudo. CI does not run it (the CI pod cannot create cgroups [asserted, plan 0041 P8; not probed]). A live case that
# cannot run prints "skipped:" and passes, so the target reads the log and fails on any such line.
# freezer runs under nextest. limits_live must enable controllers on its own cgroup, which fails while a runner
# process shares it, so its binary runs directly inside a delegated systemd scope (as its header says).
POOL_LIVE_LOG ?= target/pool-live-test.log

pool-live-test: ## Run the uas-pool live cgroup tests, fail on any skip (delegated cgroup v2 subtree, no sudo)
	mkdir -p "$$(dirname "$(POOL_LIVE_LOG)")"
	: > "$(POOL_LIVE_LOG)"
	rc=0
	cargo nextest run -p uas-pool --test freezer --build-jobs $(JOBS) --locked --no-fail-fast --nocapture --color never >> "$(POOL_LIVE_LOG)" 2>&1 || rc=1
	BIN=$$(cargo test -p uas-pool --test limits_live --no-run -j $(JOBS) --message-format=json 2>/dev/null | jq -r 'select(.reason=="compiler-artifact" and .target.name=="limits_live") | .executable' | grep -v null | tail -n1)
	test -x "$$BIN" || { echo "pool-live-test: limits_live binary not found" >&2; exit 1; }
	systemd-run --user --scope -q -p Delegate=yes "$$BIN" --nocapture --test-threads=1 >> "$(POOL_LIVE_LOG)" 2>&1 || rc=1
	cat "$(POOL_LIVE_LOG)"
	grep -Eq 'tests run: [1-9][0-9]* passed' "$(POOL_LIVE_LOG)" || { echo "pool-live-test: no freezer case ran" >&2; exit 1; }
	grep -Eq 'test result: ok\. [1-9][0-9]* passed' "$(POOL_LIVE_LOG)" || { echo "pool-live-test: no limits_live case ran" >&2; exit 1; }
	if grep -En 'skipped:|[1-9][0-9]* (ignored|skipped)' "$(POOL_LIVE_LOG)"; then
		echo "pool-live-test: a live case was skipped, not run (lines above)" >&2
		exit 1
	fi
	[ "$$rc" -eq 0 ] || { echo "pool-live-test: a test binary failed" >&2; exit 1; }
	echo "pool-live-test: ok"

audit: ## Check dependencies for known advisories
	cargo audit

sbom: ## Write a CycloneDX SBOM (*.cdx.json)
	cargo cyclonedx --format json --spec-version 1.5

sbom-check: ## Validate the SBOMs in SBOM_DIR (spec 0055 D10)
	scripts/sbom-check.sh "$(SBOM_DIR)"

doc: ## Build API docs
	cargo doc --workspace --no-deps -j $(JOBS)

release-notes: ## Roll CHANGELOG and bump version (VERSION=x.y.z)
	[[ "$(VERSION)" =~ ^[0-9]+\.[0-9]+\.[0-9]+$$ ]] || { echo "usage: make release-notes VERSION=x.y.z" >&2; exit 2; }
	[ -f CHANGELOG.md ] || { echo "CHANGELOG.md not found" >&2; exit 1; }
	grep -q '^## \[Unreleased\]' CHANGELOG.md || { echo "CHANGELOG.md has no [Unreleased] heading" >&2; exit 1; }
	rev=$$(git describe --always --dirty --abbrev=7); case "$$rev" in *-dirty) echo "release-notes: refusing a dirty tree ($$rev)" >&2; exit 2;; esac
	sed -i '0,/^version = ".*"/s//version = "$(VERSION)"/' Cargo.toml
	cargo update -w --offline 2>/dev/null || cargo update -w
	awk -v v="$(VERSION)" -v d="$$(date -u +%F)" '
		/^## \[Unreleased\]/ && !done { print; print ""; print "## [" v "] - " d; done = 1; next }
		{ print }' CHANGELOG.md > CHANGELOG.md.tmp
	mv CHANGELOG.md.tmp CHANGELOG.md
	echo "release-notes: set version $(VERSION). Review the diff, then open a PR to development."

custody-check: ## Check the blackwall custody setup (blackwall doctor)
	blackwall doctor

pgt-install: ## Install the pgt CLI into .tools/bin
	cargo install --git https://github.com/cerebral-work/pgt --rev 54d90f5 pgt --root .tools --locked

new-repo: ## Rename the template (NAME=<kebab> [ORG=cerebral-work])
	name="$(NAME)"
	org="$(ORG)"
	[[ "$$name" =~ ^[a-z][a-z0-9-]*$$ ]] || { echo "usage: make new-repo NAME=<kebab-name> [ORG=cerebral-work]" >&2; exit 2; }
	[[ "$$org" =~ ^[A-Za-z0-9][A-Za-z0-9-]*$$ ]] || { echo "new-repo: bad ORG" >&2; exit 2; }
	if [ -n "$$(git status --porcelain)" ]; then
		echo "new-repo: the working tree is not clean" >&2
		exit 1
	fi
	snake="$${name//-/_}"
	upper="$${snake^^}"
	# Every crate that carries the prefix moves, not a hard-coded pair (CER-2813).
	for d in crates/uas-*; do
		[ -d "$$d" ] || continue
		git mv "$$d" "crates/$$name-$${d#crates/uas-}"
	done
	# Files whose own name carries the prefix move too, or a manifest path
	# rewritten below points at a file that no longer exists.
	git ls-files -z | grep -zE '(^|/)uas[^/]*$$' | grep -zvE '^(vendor/|specs/|\.agents/skills/new-repo/)' | while IFS= read -r -d '' f; do
		git mv "$$f" "$$(dirname "$$f")/$$name$${f##*/uas}"
	done
	git ls-files -z | while IFS= read -r -d '' f; do
		# vendor/ is upstream text; the new-repo skill stays verbatim as the
		# record of how this repo was made from the template; specs/ keeps its
		# file names and slug references, so its prose is renamed by hand in
		# the post-cutover change (spec 0037 D4), never by this loop.
		case "$$f" in vendor/*|Cargo.lock|.agents/skills/new-repo/*|specs/*) continue ;; esac
		[ -f "$$f" ] || continue
		grep -Iq . "$$f" || continue
		# The repo name first, then the bare token on word boundaries, so that
		# `case`, `broadcast` and `replicas` are never rewritten. `\b` sits
		# between `s` and `-`, so `uas-pool` still becomes `<name>-pool`.
		# Hyphenated forms are names, never identifiers, so they take the kebab
		# name everywhere. A bare `uas` inside Rust source is an identifier and
		# takes the snake form, except inside a string literal or the Cargo
		# binary env var, where it names the binary and takes the kebab form.
		# Spec file names do not change (spec 0037 D4), so a reference such as
		# specs/0001-cas-architecture-and-crate-layout.md is shielded first and
		# restored last; without this it would dangle after the rename.
		sed -i \
			-e "s/\([0-9][0-9][0-9][0-9]\)-uas-/\1-cas-/g" \
			-e "s#unsigned-gg/uas#$$org/$$name#g" \
			-e "s/\bcas-substrate\b/$$name/g" \
			-e "s/\bcas-/$$name-/g" \
			-e "s/\bCAS_/$${upper}_/g" \
			-e "s/\bcas_/$${snake}_/g" \
			-e "s/\bskel\b/$$name/g" \
			"$$f"
		case "$$f" in
			*.rs)
				# Inside a string literal or a comment the token is text and takes
				# the kebab name; outside both it is an identifier and takes the
				# snake name. The loops rewrite every occurrence in a span.
				sed -i \
					-e ':s; s/\("[^"]*\)\bcas\b\([^"]*"\)/\1@KEBAB@\2/; ts' \
					-e ':c; s#\(//.*\)\bcas\b#\1@KEBAB@#; tc' \
					-e "s/CARGO_BIN_EXE_uas\b/CARGO_BIN_EXE_$$name/g" \
					-e "s/\bcas\b/$$snake/g" \
					-e "s/@KEBAB@/$$name/g" \
					"$$f" ;;
			*)    sed -i -e "s/CARGO_BIN_EXE_uas\b/CARGO_BIN_EXE_$$name/g" -e "s/\bcas\b/$$name/g" "$$f" ;;
		esac
		sed -i -e "s/cas/uas/g" "$$f"
	done
	git rm -q -f --ignore-unmatch .github/workflows/template-check.yml
	cargo update -w --offline 2>/dev/null || cargo update -w
	# The rename changes crate names, and rustfmt sorts imports by name, so a
	# name that sorts before `clap` (for example `uas`) leaves misordered imports.
	cargo fmt --all
	git status --short

template-check: ## Run check, spec-check, smoke, then prove the rename in a temp copy
	$(MAKE) check spec-check smoke
	tmp="$$(mktemp -d)"
	trap 'rm -rf "$$tmp"' EXIT
	git ls-files -z | tar --null -T - -cf - | tar -xf - -C "$$tmp"
	export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
	export GIT_AUTHOR_NAME=probe GIT_AUTHOR_EMAIL=probe@example.invalid
	export GIT_COMMITTER_NAME=probe GIT_COMMITTER_EMAIL=probe@example.invalid
	export CARGO_TARGET_DIR="$(CURDIR)/target/probe"
	cd "$$tmp"
	git init -q -b development
	git add -A
	git commit -q -m probe
	# `aaa-probe` sorts before every dependency name, so the rename moves the
	# domain import to the top of the CLI's import block; fmt-check proves that
	# new-repo reformats it (measured 2026-10-08: NAME=uas failed fmt-check).
	$(MAKE) -f Makefile new-repo NAME=aaa-probe
	cargo fmt --all --check
	cargo build --workspace -j $(JOBS)
	$(MAKE) -f Makefile spec-check
	# Spec file names keep their uas- (spec 0037 D4), and the Makefile's own
	# rename rules carry the token by necessity; neither is a leftover.
	if grep -rIn -E '\bskel\b|\bcas_|\bCAS_|\bcas\b' --exclude-dir={target,.git,vendor,specs} --exclude=Makefile . | grep -vE '^\./\.agents/skills/new-repo/|[0-9]{4}-uas-'; then
		echo "template-check: leftover 'uas' after rename" >&2
		exit 1
	fi
	echo "template-check: ok"
