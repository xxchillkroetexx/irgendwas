#!/bin/sh
# Runs the Playwright suite. `cargo leptos end-to-end` calls this (end2end-cmd) after it
# has started the server; extra arguments go to `playwright test`.
#
# - Inside the Playwright image (CI job container): runs Playwright directly.
# - Elsewhere (dev host without npm): runs it in mcr.microsoft.com/playwright via podman,
#   with host networking so the browser reaches the server on localhost. The whole repository
#   is mounted (as in CI), so specs can read e.g. `../../locales/en.json`.
#
# Environment: BASE_URL (default http://localhost:3000, must match APP_URL's origin),
# MAILPIT_URL, PLAYWRIGHT_IMAGE, CONTAINER_ENGINE (default podman).
set -eu
cd "$(dirname "$0")"

IMAGE="${PLAYWRIGHT_IMAGE:-mcr.microsoft.com/playwright:v1.64.1-noble}"
ENGINE="${CONTAINER_ENGINE:-podman}"

# Install node_modules only when missing or older than the lockfile.
INSTALL='if [ ! -f node_modules/.package-lock.json ] || [ package-lock.json -nt node_modules/.package-lock.json ]; then npm ci --no-audit --no-fund; fi'

if [ -n "${PLAYWRIGHT_BROWSERS_PATH:-}" ] && [ -d "${PLAYWRIGHT_BROWSERS_PATH}" ] && command -v npx >/dev/null 2>&1; then
    sh -c "$INSTALL"
    exec npx playwright test "$@"
fi

exec "$ENGINE" run --rm --init --network host --ipc host \
    -v "$PWD/..:/work" -w /work/end2end \
    -e CI -e BASE_URL -e MAILPIT_URL \
    "$IMAGE" sh -c "$INSTALL && exec npx playwright test \"\$@\"" playwright "$@"
