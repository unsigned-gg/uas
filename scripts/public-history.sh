#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
# public-history: build the public form of this repository's history (spec 0048 D17 to D20).
# usage: public-history.sh <source-repo> <target-dir> <personas.toml> [ref]
# Clones <ref> (default main) of <source-repo> into <target-dir>, then rewrites every commit:
#   - drops vendor/cortex, vendor/soma and vendor/saga from every tree (spec 0048 F12);
#   - replaces tailnet hostnames, operator paths (D7) and operator names (D20) in blobs and
#     messages with neutral text;
#   - re-attributes author and committer to the bot persona for the commit's role (D19):
#     merges, changelog and release commits to `integrator`; `docs(...)` commits to `engineer`;
#     a change under crates/uas-ebpf/ to `ebpf-builder`; every other change to `builder`.
#     Identities listed under [keep] in the manifest (for example dependabot) stay as they are;
#     any other author or committer that survives the rewrite is a red leg;
#   - restores the four files D20 keeps with their names (MAINTAINERS.md, .github/CODEOWNERS,
#     SECURITY.md, CODE_OF_CONDUCT.md) at the head, in one unsigned commit by the integrator persona;
#   - imports no tag: a tag carries its tagger's identity and message, so the public repository
#     starts with none and gets its own at its first release (D19).
# Signatures do not survive a rewrite: the public history is unsigned (spec 0048 D19).
# Addresses and unix identifiers are not scrubbed from blobs (spec 0048 D19); a ruling may add rules.
# Then it verifies the result and exits non-zero when any leg is red. Needs git-filter-repo at
# the version pinned in spec 0004 (CI tools), python3 with tomllib (3.11+), and gitleaks.
# Every name below is spelt so that this file does not match its own patterns and survives
# the rewrite unchanged; the Makefile's NAMES_PATTERN uses the same spelling.
set -euo pipefail

[ $# -ge 3 ] && [ $# -le 4 ] || { echo "usage: public-history.sh <source-repo> <target-dir> <personas.toml> [ref]" >&2; exit 2; }
src="$1"; dst="$2"; manifest="$3"; ref="${4:-main}"
[ -f "$manifest" ] || { echo "public-history: no manifest at $manifest" >&2; exit 2; }
[ -e "$dst" ] && { echo "public-history: $dst exists; refusing to overwrite" >&2; exit 2; }
command -v git-filter-repo >/dev/null || { echo "public-history: git-filter-repo not on PATH" >&2; exit 2; }
command -v gitleaks >/dev/null || { echo "public-history: gitleaks not on PATH" >&2; exit 2; }
filter_repo_pin="a40bce548d2c"  # what git-filter-repo 2.47.0 prints; the spec 0004 CI tools row
have=$(git filter-repo --version)
[ "$have" = "$filter_repo_pin" ] || { echo "public-history: git-filter-repo prints $have; spec 0004 pins $filter_repo_pin" >&2; exit 2; }

# The same patterns as OSS_PATTERN and NAMES_PATTERN in the Makefile; make oss-check keeps them in step.
oss_pattern='\.ts\.net|/(home|Users)/[a-z]+/|/roo(t)/|~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)|/tmp/research-surface(s)'
names_pattern='[Cc]hris[t]ian|Tod[i]e|Al[l]en|Hack[l]ey|Patr[i]ck'
kept_files=(MAINTAINERS.md .github/CODEOWNERS SECURITY.md CODE_OF_CONDUCT.md)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Blob and message replacements. filter-repo applies each line to every blob (replace-text)
# and every commit message (replace-message). Order matters: longer forms first.
cat > "$work/replacements.txt" <<'EOF'
regex:[a-z0-9-]+\.tail[0-9a-f]+\.ts\.net==>a-host.example
regex:/(home|Users)/[a-z]+/==><home>/
regex:/roo(t)/==><home>/
regex:~/(projects|\.agents|\.claude|vault|\.local|\.config|keys)==><home>/\1
regex:/tmp/research-surface(s)==><surfaces>
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Chris[t]ian M\\?\.? Tod[i]e's==>\1The operator's
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Chris[t]ian M\\?\.? Tod[i]e==>\1The operator
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Chris[t]ian( Tod[i]e)?'s==>\1The operator's
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Chris[t]ian( Tod[i]e)?\b==>\1The operator
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Al[l]en( Hack[l]ey)?'s==>\1The Director of Engineering's
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Al[l]en( Hack[l]ey)?\b==>\1The Director of Engineering
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Patr[i]ck( Gr[a]y)?'s==>\1The CTO's
regex:(\A|\n\n|\n- |\n\* |\n#+ |[.!?]\s|\| )Patr[i]ck( Gr[a]y)?\b==>\1The CTO
regex:Chris[t]ian M\\?\.? Tod[i]e's==>the operator's
regex:Chris[t]ian M\\?\.? Tod[i]e==>the operator
regex:Chris[t]ian Tod[i]e's==>the operator's
regex:Chris[t]ian Tod[i]e==>the operator
regex:\bChris[t]ian's==>the operator's
regex:\bChris[t]ian\b==>the operator
regex:\bTod[i]e's==>the operator's
regex:\bTod[i]e\b==>the operator
regex:Al[l]en Hack[l]ey's==>the Director of Engineering's
regex:Al[l]en Hack[l]ey==>the Director of Engineering
regex:al[l]en-hack[l]ey==>the-director-of-engineering
regex:\bAl[l]en's==>the Director of Engineering's
regex:\bAl[l]en\b==>the Director of Engineering
regex:\bHack[l]ey's==>the Director of Engineering's
regex:\bHack[l]ey\b==>the Director of Engineering
regex:Patr[i]ck Gr[a]y's==>the CTO's
regex:Patr[i]ck Gr[a]y==>the CTO
regex:\bPatr[i]ck's==>the CTO's
regex:\bPatr[i]ck\b==>the CTO
regex:\bchris[t]ian\b==>operator
EOF

# Personas, the rewrite set and the keep set, as JSON for the callback.
personas_json=$(python3 - "$manifest" <<'EOF'
import json, sys, tomllib
with open(sys.argv[1], "rb") as f:
    m = tomllib.load(f)
roles = ("integrator", "engineer", "builder", "ebpf-builder")
missing = [r for r in roles if r not in m.get("personas", {})]
if missing:
    sys.exit("public-history: manifest lacks personas: " + ", ".join(missing))
for r in roles:
    p = m["personas"][r]
    if not p.get("name") or not p.get("email"):
        sys.exit(f"public-history: persona {r} needs name and email")
rewrite = m.get("rewrite", {}).get("emails", [])
if not rewrite:
    sys.exit("public-history: [rewrite] emails is empty; name the author emails to re-attribute")
keep = m.get("keep", {}).get("emails", [])
print(json.dumps({"personas": m["personas"], "rewrite": rewrite, "keep": keep}))
EOF
)
export PUBLIC_HISTORY_PERSONAS="$personas_json"
persona_field() { printf '%s' "$personas_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["personas"][sys.argv[1]][sys.argv[2]])' "$1" "$2"; }

read -r -d '' callback <<'EOF' || true
import json, os
cfg = json.loads(os.environ["PUBLIC_HISTORY_PERSONAS"])
personas = {k: (v["name"].encode(), v["email"].encode()) for k, v in cfg["personas"].items()}
rewrite = {e.encode() for e in cfg["rewrite"]}
subject = commit.message.split(b"\n", 1)[0]
if len(commit.parents) > 1 or subject.startswith((b"Merge ", b"docs(changelog)", b"chore(release)", b"Revert ", b"Initial commit")):
    role = "integrator"
elif subject.startswith(b"docs"):
    role = "engineer"
elif any(fc.filename.startswith(b"crates/uas-ebpf/") for fc in commit.file_changes):
    role = "ebpf-builder"
else:
    role = "builder"
if commit.author_email in rewrite:
    commit.author_name, commit.author_email = personas[role]
if commit.committer_email in rewrite:
    commit.committer_name, commit.committer_email = personas["integrator"]
EOF

echo "public-history: cloning $ref of $src"
git clone --quiet --no-local --no-tags --single-branch --branch "$ref" "$src" "$dst"
src_files=$(git -C "$src" ls-tree -r --name-only "$ref" | grep -v -E '^vendor/(cortex|soma|saga)/' | sort)

echo "public-history: rewriting"
git -C "$dst" filter-repo --quiet --force \
  --invert-paths --path vendor/cortex --path vendor/soma --path vendor/saga \
  --replace-text "$work/replacements.txt" \
  --replace-message "$work/replacements.txt" \
  --commit-callback "$callback" \
  --prune-empty always

echo "public-history: restoring the kept files at the head (spec 0048 D20)"
for f in "${kept_files[@]}"; do
  if git -C "$src" cat-file -e "$ref:$f" 2>/dev/null; then
    git -C "$src" show "$ref:$f" > "$dst/$f"
    git -C "$dst" add -- "$f"
  fi
done
if ! git -C "$dst" diff --cached --quiet; then
  GIT_AUTHOR_NAME=$(persona_field integrator name) GIT_AUTHOR_EMAIL=$(persona_field integrator email) \
  GIT_COMMITTER_NAME=$(persona_field integrator name) GIT_COMMITTER_EMAIL=$(persona_field integrator email) \
  git -C "$dst" -c commit.gpgsign=false commit --quiet --no-verify \
    -m "docs: restore the maintainer and contact files after the public rewrite (spec 0048 D20)"
fi

echo "public-history: verifying"
bad=0
count=$(git -C "$dst" rev-list --count HEAD); echo "commits: $count"

# Every git read below lands in a file and its exit code is checked, so a failed git command is red,
# never an empty pass. grep exits 1 on no match, which is the pass; anything above 1 is an error.
gitout() { local out=$1; shift; if ! git -C "$dst" "$@" > "$out"; then echo "public-history: git $1 failed" >&2; bad=1; fi; }
grephits() { local what=$1 pattern=$2 file=$3 hits=$4 rc=0; grep -n -E "$pattern" "$file" > "$hits" || rc=$?
  if [ "$rc" -gt 1 ]; then echo "public-history: grep over $what failed with $rc" >&2; bad=1; fi
  if [ -s "$hits" ]; then echo "public-history: $(wc -l < "$hits") $what hits, first:" >&2; head -5 "$hits" >&2; bad=1; fi; }

echo "== revisions to scan"
gitout "$work/revs" rev-list --all
mapfile -t revs < "$work/revs"
if [ "${#revs[@]}" -eq 0 ]; then echo "public-history: no revision to scan; every history leg would read the work tree" >&2; bad=1; fi
echo "revisions: ${#revs[@]}"

echo "== authors"
gitout "$work/identities" log --format='%an <%ae>'
sort "$work/identities" | uniq -c | sort -rn
# Every surviving identity must be a persona or an address the manifest keeps on purpose.
allowed=$(printf '%s' "$personas_json" | python3 -c 'import json,sys; c=json.load(sys.stdin); print("\n".join(sorted(set(p["email"] for p in c["personas"].values()) | set(c["keep"]))))')
gitout "$work/emails" log --format='%ae%n%ce'
survivors=$(sort -u "$work/emails" | grep -v -x -F -f <(printf '%s\n' "$allowed") || true)
if [ -n "$survivors" ]; then
  printf 'public-history: identity survived the rewrite and is not in [keep]: %s\n' $survivors >&2; bad=1
fi

echo "== no tag was imported"
gitout "$work/tags" tag --list
if [ -s "$work/tags" ]; then echo "public-history: tags survived the clone: $(tr '\n' ' ' < "$work/tags")" >&2; bad=1; fi

echo "== files that must be gone from every commit"
gitout "$work/vendor-paths" log --all --format= --name-only -- vendor/cortex vendor/soma vendor/saga
if [ -s "$work/vendor-paths" ]; then echo "public-history: a removed vendor directory is still in history" >&2; bad=1; fi

echo "== hostnames and operator paths in every blob of every commit; names in every blob outside the kept files"
excl=(); for f in "${kept_files[@]}"; do excl+=(":!$f"); done
rc=0; git -C "$dst" grep -l -E "$oss_pattern" "${revs[@]}" > "$work/path-hits" || rc=$?
if [ "$rc" -gt 1 ]; then echo "public-history: git grep (paths) failed with $rc" >&2; bad=1; fi
if [ -s "$work/path-hits" ]; then echo "public-history: $(wc -l < "$work/path-hits") blob hits on the path pattern, first:" >&2; head -5 "$work/path-hits" >&2; bad=1; fi
rc=0; git -C "$dst" grep -l -E "$names_pattern" "${revs[@]}" -- "${excl[@]}" > "$work/name-hits" || rc=$?
if [ "$rc" -gt 1 ]; then echo "public-history: git grep (names) failed with $rc" >&2; bad=1; fi
if [ -s "$work/name-hits" ]; then echo "public-history: $(wc -l < "$work/name-hits") blob hits on the names pattern, first:" >&2; head -5 "$work/name-hits" >&2; bad=1; fi

echo "== hostnames, operator paths and names in every commit message"
gitout "$work/messages" log --all --format=%B
grephits "message" "$oss_pattern|$names_pattern" "$work/messages" "$work/message-hits"

echo "== the head tree holds the files of $ref"
dst_files=$(git -C "$dst" ls-tree -r --name-only HEAD | sort)
if [ "$src_files" != "$dst_files" ]; then
  echo "public-history: file list differs from $ref" >&2; diff <(printf '%s\n' "$src_files") <(printf '%s\n' "$dst_files") | head -20 >&2; bad=1
fi

echo "== gitleaks over the rewritten history"
if ! gitleaks git "$dst" --redact --no-banner >"$work/gitleaks.log" 2>&1; then
  echo "public-history: gitleaks found a leak in the rewritten history" >&2; tail -20 "$work/gitleaks.log" >&2; bad=1
fi

if [ "$bad" -ne 0 ]; then echo "public-history: FAILED; $dst is not fit to push" >&2; exit 1; fi
echo "public-history: ok; $count commits in $dst, head $(git -C "$dst" rev-parse --short HEAD)"
