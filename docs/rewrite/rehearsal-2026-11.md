# Release rehearsal (local), 2026-11-02

Phase 3 dry run of the production stack (`rust/compose.yml`: Caddy → app → MariaDB) on the
dev machine. Nothing external was touched: images ran locally under **rootless podman 6.1.4**
(netavark, pasta, podman-compose 1.6.0), with mail going to the dev compose's Mailpit. HEAD
was `0dcd232` (all 7 slices).

## Setup

- Separate compose project `irgendwas-rehearsal`; throwaway env file in the session
  scratchpad (random DB passwords, never committed): `HTTP_BIND=127.0.0.1`, Caddy on
  18080/18443, `CADDY_SITE_ADDRESS=:80`, `CADDY_AUTO_HTTPS=off`, `APP_URL=http://localhost:18080`.
- Mail: the dev Mailpit is published on host loopback only, which rootless containers
  can't reach via `host.containers.internal` (169.254.1.2 → connection refused). The app
  joined the dev network `irgendwas-rust-dev_default` and used `mailpit:1025`
  (overlay, now `rust/deploy/rehearsal/compose.rehearsal.yml`, which also sets
  `RATE_LIMIT_MAIL_PER_IP=1000` because the whole e2e suite comes from one address).
- Client IP observed black-box through the login rate limiter (50 failed logins per client
  IP and 15 min, then 429): one client sends 51 failed logins with distinct unknown emails,
  so the per-email limit (10) never triggers. No debug logging was added. Distinct clients
  were containers with fixed addresses on the edge/outer network, plus the host through
  the published port (and `curl --interface 127.0.0.2/3` for the host-network proxy).
- Reusable now: `rust/scripts/rehearsal.sh up a|b`, `check`, `e2e`, `down` (README
  "Release rehearsal"). The final runs used it.

## Build

`podman build --format docker --build-arg CARGO_BUILD_JOBS=4` (the build arg is new, see
fixes):

| | |
|---|---|
| Build time | 81 s with warm cache mounts (registry + target dir from a build two days earlier; only the workspace crates recompiled). 67 s after a one-line change. A cold build was not measured. |
| Image | 79.1 MB (distroless cc-debian13 + server 45 MB, not stripped + site 3.1 MB). Two days ago, before the slices: 47.8 MB. |
| wasm | 3.14 MB raw, ~0.95 MB gzip / ~0.87 MB zstd (Caddy serves it compressed) |

## Results

### Setup A: Caddy is the edge

| Check | Result |
|---|---|
| `/health` through Caddy | 200 `{"database":"ok","status":"ok"}`. Migrations ran on a fresh DB, healthcheck turns healthy, Caddy waits for it (`service_healthy` works with podman-compose 1.6) |
| Landing page, headers | 200, `<html lang="en" data-timezone="Europe/Berlin">`, zstd encoding, HSTS / nosniff / X-Frame-Options / Referrer-Policy, no `Server` header (a `Via: 1.1 Caddy` stays) |
| e2e (full suite, 55 tests, `BASE_URL=http://localhost:18080`) | **55 passed**, including register → mail → login → create group, no-JS forms, hydration |
| Client IP, direct peers (containers on `edge`) | Client keyed by its own address: 172.31.250.50 got 50×401 then 429; 172.31.250.51 still got 401 |
| Forged `X-Forwarded-For` | Ignored: `203.0.113.x`, a fake chain `203.0.113.8, 172.31.250.10`, and .51 claiming to be .50 all landed on the real peer's key |
| Client IP, host via the published port | **Every client appears as Caddy's own address 172.31.250.10** (Caddy log `remote_ip`). The app keys all of them as 172.31.250.10 (e2e failures + probe shared one bucket). Forged XFF was still ignored (429) |
| HTTPS detection (APP_URL http) | Cookies without `Secure` (e2e i18n-polish AC1); `X-Forwarded-Proto: https` / `Forwarded: proto=https` from the client change nothing |
| Caddy TLS (`CADDY_SITE_ADDRESS=localhost`, `CADDY_AUTO_HTTPS=on`, internal CA, `APP_URL=https://localhost:18443`) | HTTPS + HTTP/2 OK with Caddy's local root; `locale` and `irgendwas_session` cookies `Secure`; an `http://` Origin → 403; HTTP→HTTPS redirect 308 to `https://localhost/` (Caddy doesn't know the published port 18443: rehearsal artefact, fine on 80/443). e2e over HTTPS not run (Playwright would need the local CA, and the config may not change) |

### Setup B: nginx → Caddy → app

nginx 1.31 container on network `outer` (172.31.251.10, published on 127.0.0.1:18081),
Caddy also on `outer`, `TRUSTED_PROXIES_CIDR=172.31.251.10/32`, `APP_URL=http://localhost:18081`.
App trusts `172.31.250.10/32 172.31.251.10/32`.

| Check | Result |
|---|---|
| e2e (55 tests via nginx) | **55 passed** (with both nginx XFF variants below, and through the host-network nginx B3) |
| Real client IP (containers on `outer` → nginx) | Correct: .50 got 50×401 then 429, .51 had its own limit. The app resolves the client nginx saw |
| Forged XFF from those clients | Ignored by the app (it walks from the right) |
| **B1: nginx `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for` (the usual config), host client via published port** | **Spoofable.** Rootless podman makes the host client appear as nginx's own address 172.31.251.10, which is trusted; nginx appends it after the client's forged entry, so the app takes the forged entry as the client. 55 failed logins with rotating forged XFF: 55×401, never limited |
| B2: nginx overwrites, `X-Forwarded-For $remote_addr` | Not spoofable (host: 50×401 + 5×429 with rotating forged XFF; containers: real IPs). Host clients all share nginx's address (same as setup A) |
| B3: nginx in the host network (`--network host`, 127.0.0.1:18082) → Caddy's loopback port 18080, `TRUSTED_PROXIES_CIDR=172.31.250.10/32`, appending XFF | **Real client IPs and not spoofable**: 127.0.0.2 and 127.0.0.3 had separate limits, forged XFF ignored. Caddy sees itself (172.31.250.10) as the peer, so it has to trust its own address here, which is safe only because Caddy is published on loopback |
| HTTPS at the outer proxy (B3 + nginx TLS on 127.0.0.1:18444, self-signed; `APP_URL=https://localhost:18444`, Caddy and app plain HTTP) | Cookies `Secure`, `http://` Origin → 403. curl flow register → Mailpit → confirm → complete registration (Secure session cookie) → logout → login → create group → group page: all OK |
| Caddy access log `client_ip` | Before the fix it showed the **forged** leftmost XFF entry (e.g. `203.0.113.7`) while the app used the right one. With `trusted_proxies_strict` it shows the real client |

Traefik as a container with published ports: not tried (setup C below is the chosen topology).

### Setup C: Traefik (host network) → Caddy → app — the production topology

Chosen by the user after A/B (decision log 2026-11-02). `scripts/rehearsal.sh up c`:
Traefik v3.7.17 (`docker.io/library/traefik:v3`) with `--network host`, using
`rust/deploy/rehearsal/traefik/` (the production `deploy/traefik/traefik.yml` settings on
127.0.0.1:18082 → redirect, 127.0.0.1:18443 TLS; a certificate from a throwaway local CA made
by the script instead of ACME; file provider, `watch: true`, JSON access log). Caddy published
on 127.0.0.1:18080 only (no HTTPS port), `CADDY_SITE_ADDRESS=:80`, `CADDY_AUTO_HTTPS=off`,
`TRUSTED_PROXIES_CIDR=172.31.250.10/32`, `APP_URL=https://localhost:18443`.

| Check | Result |
|---|---|
| `/health`, headers through Traefik | 200 over HTTPS; HSTS / nosniff / X-Frame-Options from Caddy |
| Real client IP | 127.0.0.2 (via `curl --interface`): 50×401 then 429; 127.0.0.3 had its own limit. Traefik access log `ClientHost` and Caddy `client_ip` show the real source address |
| Client-forged `X-Forwarded-For` | **Dropped by Traefik** (`forwardedHeaders` without `trustedIPs`): 127.0.0.3 sent `X-Forwarded-For: 127.0.0.2`, Caddy received only `127.0.0.3`; rotating forged values from 127.0.0.2 did not escape the limit |
| Client-forged `X-Forwarded-Proto: http` | Overwritten by Traefik (`https` at Caddy); the app ignores it anyway: session cookie still `Secure` |
| Cookies / Origin check | `irgendwas_session` and `locale` `Secure`; Origin `https://localhost:18443` accepted, `http://localhost:18443` → 403 |
| HTTP entry point | `http://localhost:18082/` → 301 `https://localhost:18443/` (Traefik answers GET with 301, other methods with 308) |
| curl flow | register → Mailpit → confirm → complete registration → logout → login → create group → group page: OK (`check`) |
| e2e (55 tests over HTTPS) | **53 passed, 2 failed, both test assumptions of an http base URL, not app bugs**: auth AC7 expects `^http://…/user/dashboard` (the app correctly landed on `https://localhost:18443/user/dashboard`); i18n-polish AC1 asserts the `locale` cookie is not `Secure` (correct only for an http APP_URL). Playwright ran unchanged in a derived image that trusts the test CA (NSS db + `NODE_EXTRA_CA_CERTS`) via run.sh's `PLAYWRIGHT_IMAGE` |
| Trust boundary | A host-local process that bypasses Traefik and talks to 127.0.0.1:18080 directly can forge `X-Forwarded-For` (51×401, never limited): accepted, it needs local access. Neither 18080 nor Traefik's ports answer on the LAN address |
| Production config `deploy/traefik/` | Started offline (`--network none`): parses, file provider and ACME resolver start (ACME then fails without network, as intended); no warnings after setting `aliasHeadersStrategy: delete` and rejecting encoded `/`, `\`, NUL |

## Issues found and fixed

1. **Caddy logged forged client IPs** (`rust/caddy/Caddyfile`): added `trusted_proxies_strict`
   so Caddy reads `X-Forwarded-For` from the right like the app. Validated with Caddy
   v2.11.7 for empty and set `TRUSTED_PROXIES_CIDR`, auto_https on/off.
2. **Outer-proxy spoofing under rootless podman** (docs; no code change, the app logic is
   right): README "Trusted proxies" and setup B now explain the rootless forwarding effect.
   An edge proxy behind published ports must overwrite XFF; the host-network proxy is the
   way to get real client IPs. Corrected the "often the edge gateway" claim (rootless:
   Caddy's own address). Template: `rust/deploy/rehearsal/compose.nginx.yml` + `nginx.conf`.
3. **Container logs full of ANSI escape codes** (`rust/server/src/lib.rs` `init_tracing`):
   colours only when stdout is a terminal.
4. **`RATE_LIMIT_MAIL_PER_IP` not passable in production** (`rust/compose.yml`,
   `compose.env.example`): passed through, empty = app default 20. Needed for the rehearsal
   and useful under rootless setup A, where all clients share one key.
5. **Dockerfile ignored `CARGO_BUILD_JOBS`**: optional `--build-arg CARGO_BUILD_JOBS=N`
   (unset/empty = all CPUs).
6. **Production topology = Traefik (setup C)**: `rust/deploy/traefik/` (static
   `traefik.yml`, `dynamic/irgendwas.yml`, `compose.yml` with `network_mode: host`);
   `compose.yml` now publishes only plain HTTP, default `127.0.0.1:8080`, and HTTPS moved to
   the overlay `rust/deploy/caddy-edge/compose.yml` for the Caddy-as-edge alternative;
   `compose.env.example` holds the Traefik values (`TRUSTED_PROXIES_CIDR=172.31.250.10/32`);
   README "Production" rewritten (Traefik recommended, A/B as alternatives with caveats,
   options for binding 80/443 rootless); `scripts/rehearsal.sh up c`.
7. README: podman-compose doesn't recreate containers when only the env file changed
   (`up -d --force-recreate`); overrides of `caddy.networks` must use mapping form
   (podman-compose can't merge list and mapping); `CARGO_BUILD_JOBS`; rehearsal helper.

Gate after the changes (`CARGO_BUILD_JOBS=4`): `cargo fmt --all --check` ok, clippy
(`--all-targets --all-features -D warnings`) ok, `cargo leptos test` 692 passed,
`cargo leptos end-to-end` 55 passed.

## Still needs a real staging host

- Topology decided (setup C). On the host: how Traefik binds 80/443 (lower
  `net.ipv4.ip_unprivileged_port_start`, rootful Traefik, or a firewall redirect; README
  "Ports 80/443"), and Caddy's `remote_ip` for Traefik's connections (rootless:
  172.31.250.10, rootful: likely the gateway) → `TRUSTED_PROXIES_CIDR`.
- Real certificates: Let's Encrypt HTTP-01 through Traefik (port 80 reachable), renewal,
  `acme.json` on its volume; HTTP/3 only if UDP 443 is opened and `http3` enabled.
- Probe from two real client machines (rate-limit keying, forged XFF), as `check` does
  locally.
- The two e2e tests that assume an http base URL (auth AC7, i18n-polish AC1) fail against
  any https deployment; make them scheme-agnostic if e2e should run against staging.
- Real SMTP (465/587 with credentials), SPF/DKIM of the sender, mail locale.
- Restart/upgrade behaviour: `podman compose up -d` with a new image, migration on an
  existing volume, `restart: unless-stopped` after a host reboot (rootless needs
  lingering / a systemd unit or quadlet), MariaDB backup/restore of `mariadb_data`.
- A cold image build (time, peak memory) on the target or CI hardware.
- Optional: `strip = "symbols"` / `debug = "line-tables-only"` trade-off for the 45 MB
  server binary (smaller image vs. symbol names in panic backtraces).

## For Forgejo CI

- CI builds with `cargo leptos build --release`, not the Dockerfile. Add an image build
  job (`podman build --format docker --build-arg CARGO_BUILD_JOBS=…`) if the runner can
  build containers, and push to a registry for `APP_IMAGE`.
- `scripts/rehearsal.sh` needs podman with compose and the dev Mailpit network, so it
  isn't a CI job as it stands; a CI variant would need Mailpit as a service and a
  different mail overlay.
- The Dockerfile's cache mounts only help on a runner with persistent buildah storage.
