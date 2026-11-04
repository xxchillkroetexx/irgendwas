#!/bin/sh
# Acceptance test with a fresh, isolated database. Needs Docker Compose v2 and curl.
# COMPOSE_BIN=/absolute/path/to/compose selects a standalone Compose binary.
# APP_IMAGE=<already-built-image> skips the build. HTTP_PORT defaults to 18089;
# SMOKE_PROXY_PORT defaults to 18090 (a throwaway nginx stands in for the TLS proxy).
set -eu
cd "$(dirname "$0")/.."
project="irgendwas-smoke-$$"
tmp=$(mktemp -d)
compose() {
    if [ -n "${COMPOSE_BIN:-}" ]; then
        "$COMPOSE_BIN" --project-name "$project" --env-file "$tmp/test.env" -f compose.proxy.yml -f "$tmp/proxy.yml" "$@"
    else
        docker compose --project-name "$project" --env-file "$tmp/test.env" -f compose.proxy.yml -f "$tmp/proxy.yml" "$@"
    fi
}
cleanup() {
    result=$?
    trap - EXIT HUP INT TERM
    if [ "$result" -ne 0 ]; then compose logs --tail=100 >&2 || true; fi
    # Only this script's uniquely named throwaway project is removed.
    compose down -v --remove-orphans || true
    rm -rf "$tmp"
    exit "$result"
}
trap cleanup EXIT HUP INT TERM
cat >"$tmp/test.env" <<'ENV'
APP_URL=https://deployment.invalid
DB_PASSWORD=smoke-only-app
DB_ROOT_PASSWORD=smoke-only-root
MAIL_HOST=localhost
MAIL_PORT=1025
MAIL_ENCRYPTION=none
TRUSTED_PROXIES_CIDR=
ENV
export HTTP_PORT=${HTTP_PORT:-18089}
export SMOKE_PROXY_PORT=${SMOKE_PROXY_PORT:-18090}
export PROXY_SUBNET=${SMOKE_PROXY_SUBNET:-172.31.252.0/24}
export PROXY_GATEWAY=${SMOKE_PROXY_GATEWAY:-172.31.252.1}
cat >"$tmp/nginx.conf" <<'NGINX'
events {}
http {
    server {
        listen 80;
        location / {
            proxy_pass http://app:3000;
            proxy_set_header Host $http_host;
            proxy_set_header X-Forwarded-For $remote_addr;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }
}
NGINX
cat >"$tmp/proxy.yml" <<YAML
services:
  test-proxy:
    image: docker.io/library/nginx:1.28-alpine
    depends_on:
      app:
        condition: service_healthy
    ports:
      - "127.0.0.1:\${SMOKE_PROXY_PORT}:80"
    volumes:
      - "$tmp/nginx.conf:/etc/nginx/nginx.conf:ro,Z"
    networks:
      - proxy
YAML
base="http://127.0.0.1:$SMOKE_PROXY_PORT"
if [ -n "${APP_IMAGE:-}" ]; then
    export APP_IMAGE
    compose up -d --no-build --wait --wait-timeout 180
else
    compose up -d --build --wait --wait-timeout 180
fi
curl -fsS --retry 10 --retry-connrefused --retry-delay 1 "$base/health" >"$tmp/health"
grep -q '"database":"ok"' "$tmp/health"
curl -fsS "$base/" >"$tmp/page"
grep -q '<html' "$tmp/page"
curl -fsS "$base/assets/css/styles.css" >/dev/null
curl -fsS "$base/pkg/irgendwas.js" >/dev/null
curl -fsS "$base/pkg/irgendwas.wasm" >/dev/null

# Public HTTPS origin controls cookies even though the upstream uses plain HTTP.
curl -fsS -D "$tmp/headers" -o /dev/null -H 'Origin: https://deployment.invalid' \
    -H 'X-Forwarded-Proto: http' --data 'locale=de' "$base/api/set_locale"
grep -i '^set-cookie: irgendwas_session=' "$tmp/headers" | grep -qi '; Secure'
status=$(curl -sS -o /dev/null -w '%{http_code}' -H 'Origin: https://wrong.invalid' \
    --data 'locale=de' "$base/api/set_locale")
[ "$status" = 403 ]

# Verify the documented backup/restore commands and persistence after recreation.
compose exec -T mariadb sh -c 'mariadb -uroot -p"$MARIADB_ROOT_PASSWORD" "$MARIADB_DATABASE"' <<'SQL'
CREATE TABLE deployment_probe (id INT PRIMARY KEY);
INSERT INTO deployment_probe VALUES (42);
SQL
compose exec -T mariadb sh -c 'exec mariadb-dump -uroot -p"$MARIADB_ROOT_PASSWORD" --single-transaction --routines --triggers --databases "$MARIADB_DATABASE"' >"$tmp/backup.sql"
compose stop app
compose exec -T mariadb sh -c 'mariadb -uroot -p"$MARIADB_ROOT_PASSWORD" "$MARIADB_DATABASE" -e "DROP TABLE deployment_probe"'
compose exec -T mariadb sh -c 'exec mariadb -uroot -p"$MARIADB_ROOT_PASSWORD"' <"$tmp/backup.sql"
compose down
compose up -d --no-build --wait --wait-timeout 180
value=$(compose exec -T mariadb sh -c 'mariadb -N -uroot -p"$MARIADB_ROOT_PASSWORD" "$MARIADB_DATABASE" -e "SELECT id FROM deployment_probe"')
[ "$value" = 42 ]
curl -fsS --retry 10 --retry-connrefused --retry-delay 1 "$base/health" >/dev/null
echo 'Deployment smoke test passed (reverse proxy, assets, secure cookies, CSRF, backup/restore, persistence)'
