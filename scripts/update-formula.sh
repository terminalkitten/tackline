#!/usr/bin/env bash
# After a release is published: copy its Homebrew formula into Formula/ (this
# repo is the tap), commit and push it, then start the install check.
#
#   scripts/update-formula.sh          # the newest v* tag
#   scripts/update-formula.sh v0.1.3
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

git fetch --tags --quiet
tag="${1:-$(git tag -l 'v*' --sort=-v:refname | head -n1)}"
[ -n "$tag" ] || { echo "No v* tag found." >&2; exit 1; }

# Never go back: an older tag would downgrade everyone's brew install.
current=$(sed -n 's/^ *version "\(.*\)"/\1/p' Formula/tackline.rb 2>/dev/null || true)
if [ -n "$current" ] && [ "$current" != "${tag#v}" ] &&
  [ "$(printf '%s\n%s\n' "$current" "${tag#v}" | sort -V | tail -n1)" = "$current" ]; then
  echo "Formula is at $current, newer than $tag: not going back." >&2
  exit 1
fi

if ! gh release download "$tag" -p tackline.rb -O Formula/tackline.rb --clobber; then
  echo "No published release $tag with tackline.rb yet: wait for the Release run to finish." >&2
  exit 1
fi

if git diff --quiet Formula/tackline.rb; then
  echo "Formula already at $tag."
  exit 0
fi

git commit -m "chore: homebrew formula ${tag#v}" Formula/tackline.rb
git push
gh workflow run install-check.yml
echo "Formula updated to $tag. Install check started: gh run watch (or the Actions tab)."
