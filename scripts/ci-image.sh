#!/bin/sh
# Release image steps for CI (#103), run in quay.io/buildah/stable with chroot isolation.
#   sh scripts/ci-image.sh build     build the image from the checkout (tag $LOCAL)
#   sh scripts/ci-image.sh smoke     start it against MariaDB and check what only the image
#                                    can get wrong (see below)
#   sh scripts/ci-image.sh promote   main only: re-tag the image dev built and tested from
#                                    the same tree instead of rebuilding; sets PROMOTED=1
#   sh scripts/ci-image.sh push      push :sha-<commit>, :tree-<tree> and the channel tag
#
# Environment: IMAGE (registry path), GITHUB_SHA, GITHUB_REF, GITHUB_ENV; for promote and
# push REGISTRY_USER and REGISTRY_TOKEN; for smoke the MariaDB service `mariadb` (database
# irgendwas_db, user irgendjemand, password ci-password).
#
# `:tree-<git tree hash>` names the exact sources of an image. main's merge commit of dev
# has dev's tree, so main promotes the bytes that ran as :dev (#103).
set -eu

: "${IMAGE:?}" "${GITHUB_SHA:?}"
SHORT=$(printf '%s' "$GITHUB_SHA" | cut -c1-12)
TREE=$(git rev-parse 'HEAD^{tree}' | cut -c1-12)
LOCAL="localhost/irgendwas-ci:${SHORT}"
REGISTRY=${IMAGE%%/*}

channel() {
    case "${GITHUB_REF:-}" in
        refs/heads/main) echo latest ;;
        refs/heads/dev) echo dev ;;
        *) echo "" ;;
    esac
}

login() {
    : "${REGISTRY_USER:?}" "${REGISTRY_TOKEN:?}"
    printf '%s' "$REGISTRY_TOKEN" | buildah login -u "$REGISTRY_USER" --password-stdin "$REGISTRY"
}

build() {
    # --format docker keeps the HEALTHCHECK; the build runs `cargo leptos build --release`.
    buildah build \
        --format docker \
        --layers=false \
        --build-arg CARGO_BUILD_JOBS=4 \
        --label org.opencontainers.image.source=https://git.scheissndreck.de/irgendwas/irgendwas \
        --label org.opencontainers.image.revision="${GITHUB_SHA}" \
        -t "$LOCAL" \
        -f Dockerfile .
}

# Runs the image like compose.yml does: its own user, ENV and entry binary, with a minimal
# environment, then checks
#   - the server starts and applies every migration to an empty MariaDB 11.8 (it exits on
#     a failed migration, so /health answering means they all ran),
#   - GET /health is 200 and `/app/server --healthcheck` exits 0,
#   - GET / is 200 HTML with `<html lang`,
#   - every local asset the page references (wasm, JS, CSS, icons, manifest) is 200, and
#     the page references at least one .wasm, .js and .css.
smoke() {
    ctr=$(buildah from --pull=never "$LOCAL")
    log=/tmp/smoke-server.log
    # chroot isolation shares the job container's network: the server listens on its
    # localhost and reaches the `mariadb` service by name.
    set -- \
        --env APP_URL=http://localhost:3000 \
        --env DB_HOST=mariadb --env DB_PORT=3306 \
        --env DB_DATABASE=irgendwas_db --env DB_USERNAME=irgendjemand \
        --env DB_PASSWORD=ci-password \
        --env MAIL_HOST=localhost --env MAIL_PORT=1025 --env MAIL_ENCRYPTION=none \
        --env MAIL_FROM_ADDRESS=ci@example.test
    buildah run "$@" "$ctr" -- /app/server >"$log" 2>&1 &
    pid=$!
    ok=""
    for _ in $(seq 1 90); do
        if curl -fs -o /dev/null http://localhost:3000/health; then
            ok=1
            break
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            break
        fi
        sleep 2
    done
    status=0
    if [ -z "$ok" ]; then
        echo "smoke: the server did not become healthy" >&2
        status=1
    else
        check_pages || status=1
        # A second container: the running one is busy with the server.
        probe=$(buildah from --pull=never "$LOCAL")
        if ! buildah run "$probe" -- /app/server --healthcheck; then
            echo "smoke: /app/server --healthcheck failed" >&2
            status=1
        fi
        buildah rm "$probe" >/dev/null
    fi
    echo "--- server log"
    cat "$log"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    buildah rm "$ctr" >/dev/null
    return "$status"
}

check_pages() {
    page=/tmp/smoke-index.html
    code=$(curl -sS -o "$page" -w '%{http_code}' http://localhost:3000/)
    if [ "$code" != 200 ] || ! grep -q '<html lang' "$page"; then
        echo "smoke: GET / answered $code without <html lang" >&2
        return 1
    fi
    # Local paths only: href/src values starting with a single "/" (not CDN URLs or
    # protocol-relative ones), plus the /pkg/ files the hydration script imports.
    assets=$( {
        grep -oE '(href|src)="/[^/"][^"]*"' "$page" | sed -E 's/^(href|src)="//; s/"$//'
        grep -oE '/pkg/[A-Za-z0-9_.-]+' "$page"
    } | sed 's/[?#].*//' | grep -E '\.(wasm|js|css|png|svg|ico|webmanifest|woff2?)$' | sort -u)
    failed=0
    for ext in wasm js css; do
        if ! printf '%s\n' "$assets" | grep -q "\.$ext\$"; then
            echo "smoke: the page references no .$ext file" >&2
            failed=1
        fi
    done
    for path in $assets; do
        code=$(curl -sS -o /dev/null -w '%{http_code}' "http://localhost:3000$path")
        echo "smoke: $code $path"
        [ "$code" = 200 ] || failed=1
    done
    return "$failed"
}

promote() {
    [ "$(channel)" = latest ] || { echo "promote: only for main"; return 0; }
    login
    tested="${IMAGE}:tree-${TREE}"
    if ! buildah pull --quiet "$tested" >/dev/null 2>&1; then
        echo "promote: no image for tree ${TREE} yet (main differs from dev): building"
        return 0
    fi
    echo "promote: reusing ${tested}"
    buildah tag "$tested" "$LOCAL"
    echo "PROMOTED=1" >>"$GITHUB_ENV"
}

push() {
    chan=$(channel)
    [ -n "$chan" ] || { echo "push: no channel for ${GITHUB_REF:-?}" >&2; return 1; }
    login
    for tag in "sha-${SHORT}" "tree-${TREE}" "$chan"; do
        buildah push "$LOCAL" "docker://${IMAGE}:${tag}"
    done
    buildah logout "$REGISTRY"
}

case "${1:-}" in
    build | smoke | promote | push) "$1" ;;
    *)
        echo "usage: $0 build|smoke|promote|push" >&2
        exit 2
        ;;
esac
