# rust-monorepo-template task runner. Run `make` to list targets.
# Shared workstations keep JOBS=2. CI passes JOBS=10 (specs/0002-verification-gates-and-evidence.md).
SHELL := bash
.SHELLFLAGS := -euo pipefail -c
.ONESHELL:
.DEFAULT_GOAL := help

JOBS ?= 2
NEXTEST_PROFILE ?= default
ORG ?= cerebral-work

.PHONY: help build fmt fmt-check clippy test doc-test deny check ci hygiene typos machete \
	spec-check toolchain-check smoke audit sbom doc release-notes pgt-install new-repo template-check

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
	cargo deny check

check: fmt-check clippy test doc-test deny ## Full gate: fmt, clippy, test, doc-test, deny

ci: ## What the CI rust job runs
	$(MAKE) NEXTEST_PROFILE=ci fmt-check clippy test doc-test

hygiene: typos machete spec-check toolchain-check ## Typos, unused deps, specs, toolchain

typos: ## Spell check
	typos

machete: ## Find unused dependencies
	cargo machete

spec-check: ## Validate specs/ (lean-spec rules)
	cargo xtask spec-check

toolchain-check: ## Verify rustc matches rust-toolchain.toml
	cargo xtask toolchain-check

smoke: ## Build the CLI, run it, check the domain crate has no I/O
	cargo build -p skel-cli --locked -j $(JOBS)
	out="$$(cargo run -q -p skel-cli --locked -j $(JOBS) -- --version)"
	echo "$$out"
	[[ "$$out" =~ ^skel\ [0-9]+\.[0-9]+\.[0-9]+\ \(.+\)$$ ]] || { echo "smoke: unexpected --version output" >&2; exit 1; }
	cargo run -q -p skel-cli --locked -j $(JOBS) -- demo | grep -q '"done"' || { echo "smoke: demo did not reach done" >&2; exit 1; }
	if grep -rnE 'std::net|std::fs|tokio|std::process' crates/skel-domain/src; then
		echo "smoke: skel-domain must not use I/O" >&2
		exit 1
	fi
	echo "smoke: ok"

audit: ## Check dependencies for known advisories
	cargo audit

sbom: ## Write a CycloneDX SBOM (*.cdx.json)
	cargo cyclonedx --format json

doc: ## Build API docs
	cargo doc --workspace --no-deps -j $(JOBS)

release-notes: ## Roll CHANGELOG and bump version (VERSION=x.y.z)
	[[ "$(VERSION)" =~ ^[0-9]+\.[0-9]+\.[0-9]+$$ ]] || { echo "usage: make release-notes VERSION=x.y.z" >&2; exit 2; }
	[ -f CHANGELOG.md ] || { echo "CHANGELOG.md not found" >&2; exit 1; }
	grep -q '^## \[Unreleased\]' CHANGELOG.md || { echo "CHANGELOG.md has no [Unreleased] heading" >&2; exit 1; }
	sed -i '0,/^version = ".*"/s//version = "$(VERSION)"/' Cargo.toml
	cargo update -w --offline 2>/dev/null || cargo update -w
	awk -v v="$(VERSION)" -v d="$$(date -u +%F)" '
		/^## \[Unreleased\]/ && !done { print; print ""; print "## [" v "] - " d; done = 1; next }
		{ print }' CHANGELOG.md > CHANGELOG.md.tmp
	mv CHANGELOG.md.tmp CHANGELOG.md
	echo "release-notes: set version $(VERSION). Review the diff, then open a PR to development."

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
	git mv crates/skel-domain "crates/$$name-domain"
	git mv crates/skel-cli "crates/$$name-cli"
	git ls-files -z | while IFS= read -r -d '' f; do
		# vendor/ is upstream text; the new-repo skill stays verbatim as the
		# record of how this repo was made from the template.
		case "$$f" in vendor/*|Cargo.lock|.agents/skills/new-repo/*) continue ;; esac
		[ -f "$$f" ] || continue
		grep -Iq . "$$f" || continue
		# The bare token is matched on word boundaries so that a word which
		# merely contains it is never rewritten.
		sed -i \
			-e "s#cerebral-work/rust-monorepo-template#$$org/$$name#g" \
			-e "s#rust-monorepo-template#$$name#g" \
			-e "s/SKEL_/$${upper}_/g" \
			-e "s/skel_/$${snake}_/g" \
			-e "s/\bskel\b/$$name/g" \
			"$$f"
	done
	git rm -q -f --ignore-unmatch .github/workflows/template-check.yml
	cargo update -w --offline 2>/dev/null || cargo update -w
	# The rename changes crate names, and rustfmt sorts imports by name, so a
	# name that sorts before `clap` (for example `cas`) leaves misordered imports.
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
	# new-repo reformats it (measured 2026-10-08: NAME=cas failed fmt-check).
	$(MAKE) -f Makefile new-repo NAME=aaa-probe
	cargo fmt --all --check
	cargo build --workspace -j $(JOBS)
	$(MAKE) -f Makefile spec-check
	if grep -rIn -E '\bskel\b|skel_|SKEL_' --exclude-dir={target,.git,vendor} . | grep -v '^\./\.agents/skills/new-repo/'; then
		echo "template-check: leftover 'skel' after rename" >&2
		exit 1
	fi
	echo "template-check: ok"
