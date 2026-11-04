#!/bin/sh
# Validate the production Compose contract without starting containers.
# Usage: sh scripts/check-deployment.sh [compose-binary]
# Default: docker compose. Requires Python 3, no Docker daemon needed.
set -eu
cd "$(dirname "$0")/.."
compose() {
    if [ -n "${compose_binary:-}" ]; then "$compose_binary" "$@"; else docker compose "$@"; fi
}
compose_binary=${1:-}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

# The shipped template must refuse to deploy with missing passwords.
if DB_PASSWORD= DB_ROOT_PASSWORD=test-only-root compose --env-file deploy.proxy.env.example -f compose.proxy.yml config --quiet >"$tmp/error" 2>&1; then
    echo 'Deployment accepted empty database passwords' >&2
    exit 1
fi
grep -q 'DB_PASSWORD' "$tmp/error"
if DB_PASSWORD=test-only-app DB_ROOT_PASSWORD= compose --env-file deploy.proxy.env.example -f compose.proxy.yml config --quiet >"$tmp/error" 2>&1; then
    echo 'Deployment accepted an empty root password' >&2
    exit 1
fi
grep -q 'DB_ROOT_PASSWORD' "$tmp/error"

DB_PASSWORD=test-only-app DB_ROOT_PASSWORD=test-only-root \
    compose --env-file deploy.proxy.env.example -f compose.proxy.yml config --format json >"$tmp/config.json"
python3 - "$tmp/config.json" <<'PY'
import json
import sys

with open(sys.argv[1]) as config:
    c = json.load(config)
assert set(c['services']) == {'app', 'mariadb'}
app, db = c['services']['app'], c['services']['mariadb']
assert app['ports'] == [dict(mode='ingress', host_ip='127.0.0.1', target=3000, published='8080', protocol='tcp')]
assert not db.get('ports')
assert app['depends_on']['mariadb']['condition'] == 'service_healthy'
assert app['healthcheck']['test'] == ['CMD', '/app/server', '--healthcheck']
assert app['environment']['APP_URL'] == 'https://wichteln.example'
assert app['environment']['TRUSTED_PROXIES_CIDR'] == '172.31.251.1/32'
assert app['environment']['DB_PASSWORD'] == db['environment']['MARIADB_PASSWORD']
assert c['networks']['database']['internal']
assert set(db['networks']) == {'database'}
assert not c['networks']['proxy'].get('internal', False)
assert any(v['type'] == 'volume' and v['target'] == '/var/lib/mysql' for v in db['volumes'])
assert app['restart'] == db['restart'] == 'unless-stopped'
print('Deployment configuration checks passed')
PY

# Test the documented external-network override as well.
cat >"$tmp/proxy.yml" <<'YAML'
services:
  app:
    networks:
      proxy: {}
      database: {}
      existing_proxy: {}
networks:
  existing_proxy:
    external: true
    name: mein-proxy-netz
YAML
DB_PASSWORD=test-only-app DB_ROOT_PASSWORD=test-only-root \
    compose --env-file deploy.proxy.env.example -f compose.proxy.yml -f "$tmp/proxy.yml" config --quiet
