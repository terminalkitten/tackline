#!/usr/bin/env bash
# Release notes for a tag: the commits since the previous v* tag, grouped by
# their conventional-commit type.
#
#   scripts/release-notes.sh v0.1.1 > notes.md
set -euo pipefail

tag="${1:?usage: scripts/release-notes.sh <tag>}"
repo="https://github.com/terminalkitten/tackline"
prev=$(git describe --tags --abbrev=0 --match 'v*' "$tag^" 2>/dev/null || true)
range="${prev:+$prev..}$tag"

# One bullet per commit whose subject starts with one of the types in $2.
section() {
  local lines
  lines=$(git log --no-merges --reverse --format='%s' "$range" \
    | { grep -E "^($2)(\([^)]*\))?!?: " || true; } \
    | { grep -vE '^chore: release ' || true; } \
    | perl -pe 's/^[a-z]+(\([^)]*\))?!?: (.)/- \u$2/')
  if [ -n "$lines" ]; then
    printf '## %s\n\n%s\n\n' "$1" "$lines"
  fi
}

section "Features" "feat"
section "Fixes" "fix"
section "Performance" "perf"
section "Documentation" "docs"
section "Other" "chore|ci|refactor|test|build|style"

printf 'Install or update with `cargo install tackline`.\n'
if [ -n "$prev" ]; then
  printf '\n**Full changes**: %s/compare/%s...%s\n' "$repo" "$prev" "$tag"
fi
