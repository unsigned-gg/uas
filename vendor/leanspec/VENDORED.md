# Vendored LeanSpec

Do not edit these files. Re-vendor by running the fetch again at a new commit.

- Source: `codervisor/leanspec` (MIT)
- Commit: `ee122d6fe4bdf3c22f2507e2944f5a213ddcf7a6`
- Date vendored: 2026-10-08
- Licence: MIT. See `LICENSE` in this directory.

## File map

| File here | Source path in `codervisor/leanspec` |
|---|---|
| `SKILL.md` | `skills/leanspec/SKILL.md` |
| `spec-template.md` | `packages/cli/templates/standard/files/README.md` |
| `first-principles.md` | `specs/049-leanspec-first-principles/README.md`, then `FIRST-PRINCIPLES.md` from the same directory, appended after a `---` separator |
| `frontmatter.md` | `specs/005-structured-frontmatter/README.md` |
| `sub-spec-files.md` | `specs/012-sub-spec-files/README.md` |
| `LICENSE` | `LICENSE` |

## Re-vendor

Run this for each row of the table. Replace `<ref>` with the new commit.

```bash
gh api "repos/codervisor/leanspec/contents/<path>?ref=<ref>" --jq .content | base64 -d > <dest>
```

Check that each file is not empty. Update the commit and date in this file.
The `make new-repo` target skips `vendor/`, so these files stay as they are.
