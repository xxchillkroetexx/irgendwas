#!/bin/sh
# Local release rehearsal of the production stack (compose.yml), see README "Release rehearsal".
#
#   scripts/rehearsal.sh up [a|b|c]  build the image and start the stack (project irgendwas-rehearsal)
#                                    a: Caddy is the edge, http://localhost:18080 (default)
#                                    b: nginx container in front of Caddy, http://localhost:18081
#                                    c: Traefik in the host network in front of Caddy (the
#                                       production setup), https://localhost:18443 (local CA)
#   scripts/rehearsal.sh check       /health, headers, client-IP / X-Forwarded-For spoofing
#                                    probe, register -> mail -> login -> create group
#   scripts/rehearsal.sh e2e [args]  Playwright suite against the stack (args go to playwright)
#   scripts/rehearsal.sh down        remove the project's containers, networks and volumes
#
# Needs the dev compose running (mail goes to its Mailpit). The env file (random DB
# passwords) and the test CA live in $REHEARSAL_DIR (default $XDG_RUNTIME_DIR/irgendwas-rehearsal).
# CARGO_BUILD_JOBS (default 4) limits the image build; REHEARSAL_SKIP_BUILD=1 skips it.
set -eu
cd "$(dirname "$0")/.."

PROJECT=irgendwas-rehearsal
IMAGE=localhost/irgendwas-rust:rehearsal
DIR="${REHEARSAL_DIR:-${XDG_RUNTIME_DIR:-/tmp}/irgendwas-rehearsal}"
CLIENT_IMAGE=docker.io/library/nginx:1 # any image with sh + curl
TRAEFIK_IMAGE=docker.io/library/traefik:v3
PLAYWRIGHT_BASE=mcr.microsoft.com/playwright:v1.64.1-noble # as in end2end/run.sh
PLAYWRIGHT_CA_IMAGE=localhost/irgendwas-playwright:rehearsal
MAILPIT="${MAILPIT_URL:-http://127.0.0.1:8026}"

usage() {
    sed -n '2,16p' "$0" >&2
    exit 2
}

mode() { cat "$DIR/mode" 2>/dev/null || echo a; }
base_url() {
    case "$(mode)" in
    b) echo http://localhost:18081 ;;
    c) echo https://localhost:18443 ;;
    *) echo http://localhost:18080 ;;
    esac
}
# curl that trusts the rehearsal CA (mode c).
ccurl() {
    if [ "$(mode)" = c ]; then curl --cacert "$DIR/tls/ca.crt" "$@"; else curl "$@"; fi
}

# compose with the rehearsal overlays; the nginx overlay in mode b (and for `down`).
compose() {
    if [ "$(mode)" = b ] || [ "${WITH_NGINX:-0}" = 1 ]; then
        podman compose -p "$PROJECT" --env-file "$DIR/rehearsal.env" -f compose.yml \
            -f deploy/rehearsal/compose.rehearsal.yml -f deploy/rehearsal/compose.nginx.yml "$@"
    else
        podman compose -p "$PROJECT" --env-file "$DIR/rehearsal.env" -f compose.yml \
            -f deploy/rehearsal/compose.rehearsal.yml "$@"
    fi
}

wait_healthy() {
    url="$(base_url)/health"
    i=0
    until ccurl -fsS "$url" >/dev/null 2>&1; do
        i=$((i + 1))
        [ "$i" -lt 90 ] || { echo "not healthy: $url" >&2; exit 1; }
        sleep 2
    done
    echo "healthy: $url"
}

# Test CA + certificate for localhost (mode c), once per rehearsal directory.
make_certs() {
    [ -f "$DIR/tls/localhost.crt" ] && return 0
    mkdir -p "$DIR/tls"
    openssl req -x509 -newkey rsa:2048 -nodes -days 7 -subj "/CN=irgendwas rehearsal CA" \
        -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign" \
        -keyout "$DIR/tls/ca.key" -out "$DIR/tls/ca.crt" 2>/dev/null
    openssl req -newkey rsa:2048 -nodes -subj "/CN=localhost" \
        -keyout "$DIR/tls/localhost.key" -out "$DIR/tls/localhost.csr" 2>/dev/null
    printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' >"$DIR/tls/ext"
    openssl x509 -req -in "$DIR/tls/localhost.csr" -CA "$DIR/tls/ca.crt" -CAkey "$DIR/tls/ca.key" \
        -CAcreateserial -days 7 -extfile "$DIR/tls/ext" -out "$DIR/tls/localhost.crt" 2>/dev/null
}

start_traefik() {
    podman rm -f "${PROJECT}-traefik" >/dev/null 2>&1 || true
    podman run -d --name "${PROJECT}-traefik" --network host \
        -v "$PWD/deploy/rehearsal/traefik/traefik.yml:/etc/traefik/traefik.yml:ro" \
        -v "$PWD/deploy/rehearsal/traefik/dynamic:/etc/traefik/dynamic:ro" \
        -v "$DIR/tls:/certs:ro" "$TRAEFIK_IMAGE" >/dev/null
}

up() {
    m="${1:-a}"
    case "$m" in a | b | c) ;; *) usage ;; esac
    mkdir -p "$DIR" && chmod 700 "$DIR"
    umask 077
    # Secrets once per project: MariaDB keeps the first password in its volume.
    if [ ! -f "$DIR/secrets.env" ]; then
        rand() { head -c 18 /dev/urandom | base64 | tr -d '/+='; }
        printf 'DB_PASSWORD=%s\nDB_ROOT_PASSWORD=%s\n' "$(rand)" "$(rand)" >"$DIR/secrets.env"
    fi
    echo "$m" >"$DIR/mode"
    case "$m" in
    b) trusted=172.31.251.10/32 ;; # nginx on the outer network
    # Traefik reaches Caddy's loopback port; rootless podman delivers that as Caddy's own
    # address. Safe because the port is bound to 127.0.0.1 (host-local processes only).
    c) trusted=172.31.250.10/32 ;;
    *) trusted= ;;
    esac
    {
        echo "APP_URL=$(base_url)"
        echo "APP_DEBUG=false"
        echo "CADDY_SITE_ADDRESS=:80"
        echo "CADDY_AUTO_HTTPS=off"
        echo "TRUSTED_PROXIES_CIDR=$trusted"
        echo "HTTP_BIND=127.0.0.1"
        echo "HTTP_PORT=18080"
        echo "MAIL_HOST=mailpit"
        echo "MAIL_PORT=1025"
        echo "MAIL_ENCRYPTION=none"
        echo "APP_IMAGE=$IMAGE"
        cat "$DIR/secrets.env"
    } >"$DIR/rehearsal.env"
    if [ "${REHEARSAL_SKIP_BUILD:-0}" != 1 ]; then
        podman build --format docker --build-arg CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" -t "$IMAGE" .
    fi
    # Switching modes: drop the other modes' proxies (caddy is recreated without them).
    [ "$m" = b ] || podman rm -f "${PROJECT}_nginx_1" >/dev/null 2>&1 || true
    [ "$m" = c ] || podman rm -f "${PROJECT}-traefik" >/dev/null 2>&1 || true
    compose up -d --force-recreate
    if [ "$m" = c ]; then
        make_certs
        start_traefik
    fi
    wait_healthy
}

# Failed logins (unknown emails) from one client; prints one HTTP status per line. The app
# allows 50 failures per client IP and 15 minutes, then answers 429: that shows which
# address it keys the client by. XFF may contain %i (replaced by the attempt number).
# CURL_OPTS: extra curl options (word-split), e.g. --interface / --cacert.
PROBE='i=1
while [ "$i" -le "$N" ]; do
  xff=$(echo "$XFF" | sed "s/%i/$i/g")
  curl -s $CURL_OPTS -o /dev/null -w "%{http_code}\n" -H "Origin: $ORIGIN" -H "X-Forwarded-For: $xff" \
    -H "X-Forwarded-Proto: http" \
    --data "email=rehearsal-$TAG-$i@example.invalid&password=wrong" "$TARGET/api/login"
  i=$((i + 1))
done'

probe_container() { # ip n tag xff
    podman run --rm --network "$NET" --ip "$1" -e N="$2" -e TAG="$3" -e XFF="$4" -e CURL_OPTS= \
        -e ORIGIN="$(base_url)" -e TARGET="$INNER" "$CLIENT_IMAGE" sh -c "$PROBE"
}
probe_host() { # n tag xff [source address]
    opts=""
    [ -z "${4:-}" ] || opts="--interface $4"
    [ "$(mode)" != c ] || opts="$opts --cacert $DIR/tls/ca.crt --resolve localhost:18443:127.0.0.1"
    N="$1" TAG="$2" XFF="$3" CURL_OPTS="$opts" ORIGIN="$(base_url)" TARGET="$(base_url)" sh -c "$PROBE"
}
count() { grep -c "^$1\$" || true; }
summary() { echo "$(echo "$1" | count 401)x401 $(echo "$1" | count 429)x429"; }

FAILED=0
expect() { # description condition
    if [ "$2" = 0 ]; then echo "PASS  $1"; else echo "FAIL  $1"; FAILED=1; fi
}

probe_ip() {
    tag="$(date +%s)"
    if [ "$(mode)" = c ]; then
        # Host clients with different source addresses straight into Traefik.
        out=$(probe_host 51 "x$tag" "203.0.113.%i" 127.0.0.2)
        expect "client 127.0.0.2: 50 failures then 429, forged X-Forwarded-For ignored ($(summary "$out"))" \
            "$([ "$(echo "$out" | count 401)" = 50 ] && [ "$(echo "$out" | count 429)" = 1 ]; echo $?)"
        out=$(probe_host 1 "y$tag" 127.0.0.2 127.0.0.3)
        expect "client 127.0.0.3 claiming to be 127.0.0.2 has its own limit ($out)" "$([ "$out" = 401 ]; echo $?)"
        return 0
    fi
    if [ "$(mode)" = b ]; then NET="${PROJECT}_outer" INNER=http://nginx P=172.31.251; else NET="${PROJECT}_edge" INNER=http://caddy P=172.31.250; fi
    out=$(probe_container "$P.50" 51 "x$tag" "203.0.113.%i")
    expect "client $P.50: 50 failures then 429, its forged X-Forwarded-For ignored ($(summary "$out"))" \
        "$([ "$(echo "$out" | count 401)" = 50 ] && [ "$(echo "$out" | count 429)" = 1 ]; echo $?)"
    out=$(probe_container "$P.51" 1 "y$tag" "$P.50")
    expect "client $P.51 claiming to be $P.50 has its own limit ($out)" "$([ "$out" = 401 ]; echo $?)"
    out=$(probe_host 51 "h$tag" "198.51.100.%i")
    expect "host client via the published port cannot escape the limit with forged X-Forwarded-For ($(summary "$out"))" \
        "$([ "$(echo "$out" | count 429)" -ge 1 ]; echo $?)"
}

# Cookies and Origin check against APP_URL's scheme.
probe_https() {
    url="$(base_url)"
    hdrs=$(ccurl -s -D - -o /dev/null -H "Origin: $url" -H "X-Forwarded-Proto: http" \
        --data 'locale=de' "$url/api/set_locale")
    secure=1
    echo "$hdrs" | grep -i '^set-cookie: irgendwas_session=' | grep -q '; Secure' && secure=0
    case "$url" in
    https:*) expect "session cookie Secure with an https APP_URL (forged X-Forwarded-Proto: http)" "$secure" ;;
    *) expect "session cookie not Secure with an http APP_URL" "$([ "$secure" = 1 ]; echo $?)" ;;
    esac
    other=$(echo "$url" | sed 's/^https:/x:/; s/^http:/https:/; s/^x:/http:/')
    code=$(ccurl -s -o /dev/null -w '%{http_code}' -H "Origin: $other" --data 'locale=de' "$url/api/set_locale")
    expect "Origin with the other scheme ($other) is rejected ($code)" "$([ "$code" = 403 ]; echo $?)"
    if [ "$(mode)" = c ]; then
        loc=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' http://localhost:18082/)
        expect "http://localhost:18082/ redirects to https ($loc)" \
            "$(case "$loc" in "301 https://localhost:18443/" | "308 https://localhost:18443/") echo 0 ;; *) echo 1 ;; esac)"
    fi
}

# Register -> mail (Mailpit) -> confirm -> logout -> login -> create group -> group page.
flow() {
    url="$(base_url)" jar="$DIR/jar" email="flow-$(date +%s)@example.test" pw='Rehearsal-Passw0rd!'
    rm -f "$jar"
    c() { ccurl -s -b "$jar" -c "$jar" -H "Origin: $url" "$@"; }
    ok=0
    [ "$(c -o /dev/null -w '%{http_code}' --data-urlencode "email=$email" "$url/api/register")" = 200 ] || ok=1
    id="" i=0
    while [ -z "$id" ] && [ "$i" -lt 20 ]; do
        sleep 0.5
        id=$(curl -s "$MAILPIT/api/v1/search?query=to:$email" | grep -o '"ID":"[^"]*"' | head -1 | cut -d'"' -f4)
        i=$((i + 1))
    done
    token=$(curl -s "$MAILPIT/api/v1/message/$id" | grep -o '/auth/register/confirm/[A-Za-z0-9_-]*' | head -1 | sed 's|.*/||')
    [ -n "$token" ] || ok=1
    [ "$(c -o /dev/null -w '%{http_code}' --data-urlencode "token=$token" --data-urlencode name=Rehearsal \
        --data-urlencode "password=$pw" --data-urlencode "password_confirm=$pw" "$url/api/complete_registration")" = 200 ] || ok=1
    [ "$(c -o /dev/null -w '%{http_code}' -X POST "$url/api/logout")" = 200 ] || ok=1
    [ "$(c -o /dev/null -w '%{http_code}' --data-urlencode "email=$email" --data-urlencode "password=$pw" "$url/api/login")" = 200 ] || ok=1
    gid=$(c --data-urlencode 'name=Rehearsal group' --data-urlencode description= \
        --data-urlencode registration_deadline= --data-urlencode draw_date= "$url/api/create_group")
    c "$url/groups/$gid" | grep -q 'Rehearsal group' || ok=1
    [ -z "$id" ] || curl -s -X DELETE -H 'Content-Type: application/json' -d "{\"IDs\":[\"$id\"]}" "$MAILPIT/api/v1/messages" >/dev/null
    rm -f "$jar"
    expect "register -> mail -> confirm -> logout -> login -> create group (group $gid)" "$ok"
}

check() {
    url="$(base_url)"
    ccurl -fsS "$url/health" && echo
    ccurl -fsS -D - -o /dev/null "$url/" |
        grep -i -E '^(strict-transport-security|x-content-type-options|x-frame-options|server):' || true
    probe_ip
    probe_https
    flow
    # The limiter is in memory: a restart clears the probe's failures (e.g. before e2e).
    podman restart "${PROJECT}_app_1" >/dev/null
    wait_healthy
    return "$FAILED"
}

# Playwright image that trusts the rehearsal CA (Chromium: NSS db; Node: NODE_EXTRA_CA_CERTS),
# so the suite runs unchanged against https://localhost:18443.
playwright_ca_image() {
    mkdir -p "$DIR/pw" && cp "$DIR/tls/ca.crt" "$DIR/pw/ca.crt"
    podman build -q -t "$PLAYWRIGHT_CA_IMAGE" -f - "$DIR/pw" >/dev/null <<EOF
FROM $PLAYWRIGHT_BASE
RUN apt-get update && apt-get install -y --no-install-recommends libnss3-tools && rm -rf /var/lib/apt/lists/*
COPY ca.crt /usr/local/share/irgendwas-rehearsal-ca.crt
RUN mkdir -p /root/.pki/nssdb && certutil -d sql:/root/.pki/nssdb -N --empty-password \\
    && certutil -d sql:/root/.pki/nssdb -A -t "C,," -n irgendwas-rehearsal -i /usr/local/share/irgendwas-rehearsal-ca.crt
ENV NODE_EXTRA_CA_CERTS=/usr/local/share/irgendwas-rehearsal-ca.crt
EOF
}

e2e() {
    if [ "$(mode)" = c ]; then
        playwright_ca_image
        export PLAYWRIGHT_IMAGE="$PLAYWRIGHT_CA_IMAGE"
    fi
    BASE_URL="$(base_url)" MAILPIT_URL="$MAILPIT" sh end2end/run.sh "$@"
}

down() {
    podman rm -f "${PROJECT}-traefik" >/dev/null 2>&1 || true
    if [ -f "$DIR/rehearsal.env" ]; then
        WITH_NGINX=1 compose down -v
    fi
    podman rmi "$PLAYWRIGHT_CA_IMAGE" >/dev/null 2>&1 || true
    rm -rf "$DIR"
    echo "removed project $PROJECT (image $IMAGE kept)"
}

cmd="${1:-}"
[ $# -gt 0 ] && shift
case "$cmd" in
up) up "$@" ;;
check) check ;;
e2e) e2e "$@" ;;
down) down ;;
*) usage ;;
esac
