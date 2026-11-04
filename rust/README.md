# irgendwas (Rust/Leptos rewrite)

Leptos 0.8 (SSR + hydration) on Axum, built with `cargo-leptos`, MariaDB via SQLx.
See `../docs/rewrite/PLAN.md` for decisions and the slice tracker.

## Layout

```
Cargo.toml          workspace + [[workspace.metadata.leptos]] (bin = server, lib = frontend)
app/                shared crate: App, shell, components, pages, server fns
  src/server/       ssr-only: config, db, password, auth (current user), authz, flash, i18n, client_ip
  src/test_support  fixtures for integration tests (feature `test-support`)
frontend/           wasm entry: hydrate()
server/             axum wiring: session store + layer, locale middleware, /health, router, main
  tests/            ALL tests that need the server side live here (see "Tests")
migrations/         sqlx migrations, embedded in the binary and applied at startup
locales/            en.json, de.json (flat keys, `{name}` placeholders)
public/             static assets, served at / (e.g. /assets/css/styles.css)
end2end/            Playwright tests (run in the Playwright container image, see "E2E")
compose.dev.yml     MariaDB 11.8 (LTS) + Mailpit for development and tests
Dockerfile          production image (cargo-leptos release build → distroless, non-root)
compose.yml         production stack: Caddy + app + MariaDB, behind Traefik (see "Production")
compose.env.example template for the production env file
caddy/Caddyfile     Caddy config for compose.yml
deploy/traefik/     production edge: Traefik (host network, file provider) in front of Caddy
deploy/caddy-edge/  overlay for the alternative "Caddy is the TLS edge" (publishes 443)
deploy/rehearsal/   overlays, nginx and Traefik configs for the local release rehearsal
scripts/            install-cargo-leptos.sh (Dockerfile + CI), ci-target-dir.sh (CI), rehearsal.sh
                    (see "Release rehearsal"), migrate-php-data.sh (see "Migrating the PHP data")
```

CI: `../.forgejo/workflows/rust.yml` (fmt, clippy, `cargo leptos test` with MariaDB and
Mailpit services; e2e in a separate job in the Playwright image; on pushes to `dev` and
`main` the image is built and pushed as `:dev` or `:latest`, plus `:sha-<12>`). check and
e2e keep `target/` in a named volume of the runner's engine; the runner setup
(Docker-in-Docker, `valid_volumes`, seccomp for buildah) is in the workflow's header comment.

## Setup

```bash
cd rust
cp .env.example .env                       # strict dotenv; never point tools at ../.env
podman compose -f compose.dev.yml up -d    # MariaDB on 127.0.0.1:3307, Mailpit SMTP 1026 / UI http://127.0.0.1:8026
```

Toolchain is pinned by `rust-toolchain.toml` (1.99.1 + `wasm32-unknown-unknown`).
Needs `cargo-leptos` 0.3.11; `sqlx-cli` is optional (`sqlx migrate add <name>`).

## Run

```bash
cargo leptos watch      # dev server with reload on http://127.0.0.1:3000
cargo leptos serve      # build once and serve
curl http://127.0.0.1:3000/health   # {"database":"ok","status":"ok"}
```

The server loads `./.env` from the current directory only (it never walks up: a `../.env` left
over from the PHP app is not valid strict dotenv). Real environment variables win.
Migrations run on startup.

## Tests

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo leptos test            # needs the dev compose (MariaDB + Mailpit)
cargo leptos end-to-end      # Playwright, see "E2E"
```

`cargo leptos test` runs `cargo test -p server` and `cargo test -p frontend --lib`. It does
**not** run tests inside the `app` crate, so every server-side test (unit or integration)
goes into `server/tests/*.rs` and uses `app`'s public API plus `app::test_support`.

Database tests use `#[sqlx::test(migrations = "../migrations")]`: each test gets a fresh
database `_sqlx_test_database_N`, created from `DATABASE_URL`. The test binary finds
`DATABASE_URL` in the environment, or in `rust/.env` (sqlx loads the nearest `.env` walking
up from `server/`). The URL needs a user that may `CREATE DATABASE`; in dev that is root:

```
DATABASE_URL=mysql://root:devrootpassword@127.0.0.1:3308/irgendwas_db
```

Test-only HTTP routes can be mounted inside the session/locale layers with
`server::router_with(state, extra_routes)`; see `server/tests/session.rs`.

**POSTs in tests need the right `Origin`** (CSRF check, see below). `common::post(uri,
cookie, form_body)` sends `Origin: http://localhost:3000` (= `common::TEST_ORIGIN`, the
default `APP_URL`); for another `APP_URL` use `common::post_from(Some(&config.app_origin()), ..)`.

Mail tests talk to the dev compose's Mailpit through `app::test_support::mailpit`
(`Mailpit::from_env()`, `smtp_vars()` for the `MAIL_*` config, `unique_email()`,
`wait_for_latest(addr)`, `Mail::first_url_with_path("/auth/reset/")`, `delete_for`,
`delete_all`). Always send to a `unique_email`: tests run in parallel against one Mailpit.
Override with `MAILPIT_URL`, `MAILPIT_SMTP_HOST`, `MAILPIT_SMTP_PORT` (CI does).

### E2E

```bash
cargo leptos end-to-end      # builds, starts the server on :3000, runs end2end/run.sh
```

There is no npm on the dev host: `end2end/run.sh` runs Playwright in
`mcr.microsoft.com/playwright:v1.64.1-noble` via podman with `--network host` (inside that
image, e.g. in CI, it runs Playwright directly). `node_modules` is installed into
`end2end/` by the container on first run. Extra arguments go to `playwright test`, and
`BASE_URL` selects another server, e.g. `BASE_URL=http://localhost:8088 sh end2end/run.sh`.
`BASE_URL` must have the same origin as the server's `APP_URL`, or POSTs get 403.

Helpers in `end2end/tests/support/`: `waitForHydration(page)` (the frontend sets
`<html data-hydrated>` after hydration; wait for it before clicking Leptos-driven
elements), `collectPageErrors(page)` (console errors, catches hydration panics),
`appOrigin(baseURL)`, and `mailpit.ts` (same API as the Rust client: `uniqueEmail`,
`waitForLatest`, `firstUrlWithPath`, `deleteFor`, `deleteAll`; `MAILPIT_URL`).
To change the Playwright version, bump it in `package.json`, `run.sh` and the CI image
together and regenerate the lockfile in the container:
`podman run --rm -v "$PWD/end2end:/work" -w /work mcr.microsoft.com/playwright:vX-noble npm install --package-lock-only`.

## Conventions for slices

- Server fns return `Result<T, app::AppError>`. Get the user with
  `app::server::auth::require_user().await?`, the pool with `app::server::pool()?`, and
  check group access with `app::server::authz::require_member/require_admin(&pool, user.id, group_id)`.
- Authz semantics (groups spec D2): `require_member` gives `NotFound` for a missing group
  **and** for a non-member; `require_admin` gives `NotFound` for those and `Forbidden` for a
  member who is not the admin.
- CSRF: every non-GET/HEAD/OPTIONS request passes `server::middleware::origin_check`:
  `Origin` must equal `Config::app_origin()` (origin of `APP_URL`), or, without `Origin`,
  `Sec-Fetch-Site` must be `same-origin`/`none`; otherwise 403. Server fns stay POST.
- Mail: `app::server::state()?.mailer.send(Message { to, subject, html, text })` returns a
  `MailHandle` immediately (the send runs on a spawned task, at most 4 SMTP sessions at a
  time). Drop it for fire-and-forget, or `handle.outcome().await` → `Result<(), MailError>`
  when delivery status matters (draw: `notification_sent`). Escape every value interpolated
  into `html` with `app::server::mail::escape_html`. Failures are logged with the recipient
  domain only (no address, subject, body or token).
- Flash after a redirect: `app::server::flash::flash(FlashMessage::success("flash.success.x")).await?`
  then `leptos_axum::redirect("/somewhere")`.
- Strings: `let i18n = use_i18n(); i18n.t("key")` or `t!(i18n, "key", name = value)`. Add new
  keys to both `locales/en.json` and `locales/de.json`.
- Dates: columns are `TIMESTAMP` ↔ `DateTime<Utc>`; convert with `app::tz` and
  `app::tz::use_timezone()` only for display and `datetime-local` inputs.
- SQL: runtime queries (`sqlx::query_as::<_, Row>`), explicit column lists, backtick `groups`.
- New schema: next free number in `migrations/`, never edit a committed migration.

## Production

Recommended topology (decision log 2026-11-02):

```
client ──https──▶ Traefik (host network, :80/:443, Let's Encrypt, file provider)
                    └─http──▶ 127.0.0.1:8080 ─▶ Caddy (headers, compression) ─▶ app:3000 ─▶ MariaDB
```

Traefik runs in the host network, so it sees the real client addresses; Caddy publishes
plain HTTP on loopback only. Files: `compose.yml` + `compose.env.example` (the app stack),
`deploy/traefik/` (`traefik.yml` static config, `dynamic/irgendwas.yml` routes,
`compose.yml` to run Traefik).

### Steps

```bash
podman build --format docker -t localhost/irgendwas-rust:latest .   # or let compose build it
                                     # (--build-arg CARGO_BUILD_JOBS=4 limits the build's CPU use)
cp compose.env.example prod.env      # secrets, APP_URL=https://<domain>, mail; proxy values as below
podman compose --env-file prod.env -f compose.yml up -d --build
curl -s http://127.0.0.1:8080/health                          # Caddy on loopback

# Traefik: set the domain in deploy/traefik/dynamic/irgendwas.yml (Host rule) and the ACME
# email in deploy/traefik/traefik.yml, then (see "Ports 80/443" below):
cd deploy/traefik && podman compose up -d
curl -s https://<domain>/health
```

The app stack settings for this topology (already the defaults in `compose.env.example`):

```
APP_URL=https://wichteln.example     # public URL: Secure cookies, Origin check
CADDY_SITE_ADDRESS=:80
CADDY_AUTO_HTTPS=off
HTTP_BIND=127.0.0.1                  # never 0.0.0.0 in this setup (see below)
HTTP_PORT=8080                       # = the service URL port in dynamic/irgendwas.yml
TRUSTED_PROXIES_CIDR=172.31.250.10/32
```

No HTTPS port is published (`compose.yml` publishes only `HTTP_BIND:HTTP_PORT`; Caddy as
the TLS edge adds `deploy/caddy-edge/compose.yml`).

Always pass `--env-file`: without it compose reads `./.env`, the development config.
podman-compose does not recreate running containers when only the env file changed: use
`up -d --force-recreate` after editing it. The image is distroless (non-root, no shell); its
healthcheck is `/app/server --healthcheck`, which GETs `/health` on `LEPTOS_SITE_ADDR`.
Migrations are embedded and run at startup.

Services: `caddy` (published HTTP port, on network `edge` with the fixed address
`CADDY_IP`, default `172.31.250.10` in `EDGE_SUBNET` `172.31.250.0/24`), `app` (port 3000,
not published; networks `edge` and `database`), `mariadb` 11.8 (internal network only).

### Migrating the PHP data

One-off, at cutover. From the PHP host's data volume (`mariadb_data`, archived as a tarball
or as a directory) to an SQL file with the data in the Rust schema:

```bash
scripts/migrate-php-data.sh ~/wichteln_mariadb_data.tar.gz php-data-migrated.sql
```

It needs podman only; the old data is opened by a throwaway MariaDB of its own version
without network, the result is checked against every migration in a second throwaway
MariaDB. The output is mode 600 (personal data, password hashes). Import it after the app's
first start (migrations applied) and before anyone registers:

```bash
podman compose --env-file prod.env exec -T mariadb sh -c \
  'mariadb -u"$MARIADB_USER" -p"$MARIADB_PASSWORD" "$MARIADB_DATABASE"' < php-data-migrated.sql
```

The file refuses to load into a database with users or groups, or at a different migration
version (then regenerate it with the deployed `migrations/`). Migrated users log in after
"forgot password" (PHP's bcrypt hashes are not accepted). What gets transformed is listed in
the script's header.

### Traefik

- Static config `deploy/traefik/traefik.yml`: entry points `web` (:80, permanent redirect
  to https) and `websecure` (:443, TLS via the `letsencrypt` resolver, HTTP-01 challenge
  on `web`; `tlsChallenge` is the alternative without port 80), file provider on
  `/etc/traefik/dynamic` with `watch: true`, access log on stdout, no Docker provider, no
  API/dashboard, `aliasHeadersStrategy: delete`, encoded `/`, `\` and NUL rejected in paths.
  Certificates and the ACME account key are in `acme.json` on the volume `traefik_acme`.
- Dynamic config `deploy/traefik/dynamic/irgendwas.yml`: router ``Host(`wichteln.example`)``
  → service `http://127.0.0.1:8080`, `passHostHeader: true`. No header middlewares: HSTS,
  nosniff, X-Frame-Options, Referrer-Policy and compression come from Caddy.
- Forwarded headers: `forwardedHeaders` has no `trustedIPs` and `insecure: false`.
  Traefik therefore drops the `X-Forwarded-*` headers a client sends and sets
  `X-Forwarded-For` to the address it sees and `X-Forwarded-Proto` to `https` (verified in
  the rehearsal). Add `trustedIPs` only if another proxy or a CDN sits in front of Traefik.
- **Ports 80/443** with rootless podman: an unprivileged user can't bind ports below 1024
  by default. Options (the host admin chooses; nothing here changes system settings):
  1. lower the limit: `sysctl net.ipv4.ip_unprivileged_port_start=80` (persist in
     `/etc/sysctl.d/`); then rootless `podman compose up -d` in `deploy/traefik/` works;
  2. run only Traefik rootful (`sudo podman compose up -d` in `deploy/traefik/`): it still
     reaches the rootless stack on `127.0.0.1:8080`;
  3. let Traefik listen on high ports (e.g. 8081/8443 in `traefik.yml`) and redirect
     80/443 to them in the firewall (nftables `redirect`, keeps the client address).
     Then the HTTP→HTTPS redirect and ACME HTTP-01 need care (use `tlsChallenge`).
  Rootless containers also need lingering (`loginctl enable-linger <user>`) and
  `podman-restart.service` (or quadlets) to come back after a reboot.

### Trusted proxies

`TRUSTED_PROXIES_CIDR` (space-separated CIDRs; the app also accepts commas) lists the
proxies **in front of Caddy, as Caddy sees them**. compose passes it to Caddy unchanged and
to the app with Caddy's own `CADDY_IP/32` prepended, so the app always trusts exactly one
more hop than Caddy. Both read `X-Forwarded-For` from the right and take the first
untrusted address (Caddy: `trusted_proxies_strict`). Caddy's access log (stdout, JSON)
shows `remote_ip` (the peer it sees) and `client_ip`.

**Rootless podman** (measured, `../docs/rewrite/rehearsal-2026-11.md`): its port forwarder
(rootlessport, used for bridge networks) makes every connection to a published port
appear to come from the **receiving container's own address**. So:

- In the recommended topology Caddy sees Traefik as `172.31.250.10`, its own address, and
  has to trust it (`TRUSTED_PROXIES_CIDR=172.31.250.10/32`). That is safe only because the
  port is bound to `127.0.0.1`: nobody but host-local processes (Traefik) can connect, and
  Traefik sends only the address it saw. A local process that bypasses Traefik could forge
  `X-Forwarded-For` (measured); this is accepted on a host that runs only this service. With
  `HTTP_BIND=0.0.0.0` the same trust would let every client forge its address.
- Rootful podman/Docker deliver loopback connections differently (often from the `edge`
  gateway `172.31.250.1`): check `remote_ip` in Caddy's log and trust that `/32` instead.

### Alternative: Caddy is the edge (terminates TLS)

```
CADDY_SITE_ADDRESS=wichteln.example
CADDY_AUTO_HTTPS=on                 # certificates + HTTP→HTTPS redirect
APP_URL=https://wichteln.example
TRUSTED_PROXIES_CIDR=               # nobody in front of Caddy
HTTP_BIND=0.0.0.0
HTTP_PORT=80
HTTPS_PORT=443
```

```bash
podman compose --env-file prod.env -f compose.yml -f deploy/caddy-edge/compose.yml up -d
```

Caveat: under rootless podman every client appears as Caddy's own address. Forged
`X-Forwarded-For` is ignored, but all clients share one rate-limit key (50 failed logins
per 15 minutes, `RATE_LIMIT_MAIL_PER_IP` registrations/resets per hour, for everybody
together). Fine with rootful podman/Docker if the source address survives (check `remote_ip`).

### Alternative: outer proxy in a container (nginx, Traefik with published ports)

```
CADDY_SITE_ADDRESS=:80
CADDY_AUTO_HTTPS=off
APP_URL=https://wichteln.example
TRUSTED_PROXIES_CIDR=<the proxy's address on the shared network>/32
```

Attach Caddy to the proxy's network in an override file, in mapping form (`networks:
{proxy_net: {}}`; podman-compose cannot merge a list into compose.yml's mapping) and route
the proxy to `caddy:80`. Template: `deploy/rehearsal/compose.nginx.yml` +
`deploy/rehearsal/nginx.conf`. Caveat under rootless podman: the proxy sees every client as
its own address (shared rate-limit key), and it **must overwrite** `X-Forwarded-For` with
the address it sees (nginx `proxy_set_header X-Forwarded-For $remote_addr;`). With nginx's
usual `$proxy_add_x_forwarded_for` a client's forged entry became the client IP in the
rehearsal (rate limits bypassed).

Any outer proxy must forward the `Host` and `Origin` headers unchanged.
`CADDY_AUTO_HTTPS` accepts `on`, `off` and `disable_redirects` (anything else makes Caddy
refuse to start). Scheme/HTTPS detection in the app uses `APP_URL` only, never
`X-Forwarded-Proto`.

### Release rehearsal

`scripts/rehearsal.sh` runs the production stack locally as the separate compose project
`irgendwas-rehearsal` (Caddy on 127.0.0.1:18080; mail goes to the dev Mailpit, so the dev
compose must be running):

```bash
scripts/rehearsal.sh up c      # build the image; Traefik (host network) -> Caddy, https://localhost:18443
scripts/rehearsal.sh check     # /health, headers, client-IP / XFF spoofing probe, cookies,
                               # Origin check, register -> mail -> login -> create group
scripts/rehearsal.sh e2e       # the Playwright suite against the stack
scripts/rehearsal.sh up a      # alternatives: Caddy as the edge (http://localhost:18080),
scripts/rehearsal.sh up b      #   nginx container -> Caddy (http://localhost:18081)
scripts/rehearsal.sh down      # remove containers, networks, volumes, env file and test CA
```

`check` sends failed logins (the app allows 50 per client IP and 15 minutes) from
distinct client addresses (`127.0.0.2/3` in mode c, fixed container addresses in a/b) with
forged `X-Forwarded-For`, and expects the 429 to follow the real address; it restarts the
app afterwards to clear the in-memory limiter. Mode c uses `deploy/rehearsal/traefik/`
(the production Traefik config on 127.0.0.1:18082/18443 with a test CA made by the script
instead of ACME); `e2e` then runs Playwright in a derived image that trusts that CA
(`PLAYWRIGHT_IMAGE`). Two e2e tests assume an http base URL and fail in mode c (auth AC7
URL regex, i18n-polish AC1 "cookie not Secure"). The overlay
`deploy/rehearsal/compose.rehearsal.yml` (dev Mailpit, raised mail limit) is for the
rehearsal only.
