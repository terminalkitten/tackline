#!/usr/bin/env bash
# Retake the web guide's screenshots (docs/site/img/web-*.png) on the demo project.
# Needs Python with Playwright (pip install playwright && playwright install chromium).
# Usage: docs/demo/web-shots.sh   (from the repo root; PYTHON and TACKLINE_BIN override)
set -euo pipefail
repo="$PWD"
bin="${TACKLINE_BIN:-$repo/target/release}"
log="$(mktemp)"
demo="$(TACKLINE_BIN="$bin" bash -c "source '$repo/docs/demo/setup.sh' >/dev/null 2>&1; pwd")"
(cd "$demo" && exec "$bin/tack" --theme TwoDark serve --no-open --port 0 >"$log" 2>&1) &
server=$!
trap 'kill $server' EXIT
until url="$(grep -o 'http[^ ]*' "$log")"; do sleep 0.2; done
"${PYTHON:-python3}" "$repo/docs/demo/web_shots.py" "$url" "$repo/docs/site/img"
