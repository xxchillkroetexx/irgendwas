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
compose.yml         production stack: Caddy + app + MariaDB (see "Production")
compose.env.example template for the production env file
caddy/Caddyfile     Caddy config for compose.yml
scripts/            install-cargo-leptos.sh (Dockerfile + CI)
```

CI: `../.forgejo/workflows/rust.yml` (fmt, clippy, `cargo leptos test` with MariaDB and
Mailpit services, release build; e2e in a separate job in the Playwright image).

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

The server loads `./.env` from the current directory only (it never walks up, because the
PHP app's `../.env` is not valid strict dotenv). Real environment variables win.
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
DATABASE_URL=mysql://root:devrootpassword@127.0.0.1:3307/irgendwas_db
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

Image and stack:

```bash
podman build --format docker -t localhost/irgendwas-rust:latest .   # or let compose build it
cp compose.env.example prod.env                                      # edit: secrets, APP_URL, mail, proxy
podman compose --env-file prod.env -f compose.yml up -d --build
curl -s http://127.0.0.1/health                                      # through Caddy
podman compose --env-file prod.env -f compose.yml down               # (-v also drops the DB volume)
```

Always pass `--env-file`: without it compose reads `./.env`, the development config. The
image is distroless (non-root, no shell); its healthcheck is `/app/server --healthcheck`,
which GETs `/health` on `LEPTOS_SITE_ADDR`. Migrations are embedded and run at startup.

Services: `caddy` (published ports, on network `edge` with the fixed address
`CADDY_IP`, default `172.31.250.10` in `EDGE_SUBNET` `172.31.250.0/24`), `app` (port 3000,
not published; networks `edge` and `database`), `mariadb` 11.8 (internal network only).

### Trusted proxies

`TRUSTED_PROXIES_CIDR` (space-separated CIDRs; the app also accepts commas) lists the
proxies **in front of Caddy**. compose passes it to Caddy unchanged and to the app with
Caddy's own `CADDY_IP/32` prepended, so the app always trusts exactly one more hop than
Caddy. Caddy never trusts itself: under rootless podman, connections to published ports
appear to come from Caddy's own address, so trusting it would let clients forge
`X-Forwarded-For`. Caddy's access log (stdout, JSON) shows `remote_ip` (the peer it sees)
and `client_ip`, which helps to find the right value.

### Setup A: Caddy is the edge (terminates TLS)

```
CADDY_SITE_ADDRESS=wichteln.example
CADDY_AUTO_HTTPS=on                 # certificates + HTTP→HTTPS redirect
APP_URL=https://wichteln.example
TRUSTED_PROXIES_CIDR=               # nobody in front of Caddy
HTTP_PORT=80
HTTPS_PORT=443
```

### Setup B: behind Traefik / nginx (TLS terminated outside)

```
CADDY_SITE_ADDRESS=:80
CADDY_AUTO_HTTPS=off
APP_URL=https://wichteln.example    # the public URL (Secure cookies, Origin check)
TRUSTED_PROXIES_CIDR=<outer proxy address as Caddy sees it>
```

- Outer proxy in a container: attach Caddy to the proxy's network (add the external
  network to the `caddy` service in an override file) and set `TRUSTED_PROXIES_CIDR` to
  that network's subnet or the proxy's address, e.g. `172.18.0.0/16`. Route the proxy to
  `caddy:80`.
- Outer proxy on the host: publish Caddy on loopback only (`HTTP_BIND=127.0.0.1`,
  `HTTP_PORT=8080`) and point the proxy at `127.0.0.1:8080`. The source address Caddy sees
  depends on the container engine (often the `edge` gateway `172.31.250.1`); check
  `remote_ip` in Caddy's log and use that `/32`.
- The outer proxy must set `X-Forwarded-For` and forward the `Host` and `Origin` headers
  unchanged.

`CADDY_AUTO_HTTPS` accepts `on`, `off` and `disable_redirects` (anything else makes Caddy
refuse to start). Scheme/HTTPS detection in the app uses `APP_URL` only.

## Login lifetime (#60)

Browser and installed PWA logins stay signed in for **90 days without a request** by
default (`INACTIVITY_LIMIT=7776000`, seconds). Each request renews the persistent
cookie and the MySQL session expiry for another full period, including ordinary
page reads. There is no additional absolute expiry while the app is regularly
used, and no separate Remember Me switch. Closing the app/browser or restarting
the server does not end a stored login. Browser/OS cookie eviction or clearing
site data can still require signing in again.

The prior Rust default was 1200 seconds (20 minutes), with an already persistent,
rolling cookie; the PHP Remember Me checkbox had no implementation and PHP read
a different timeout variable. The Rust rewrite had removed the checkbox rather
than offering a longer session. This policy makes the long lifetime apply to all
logins, including registration completion.

**Existing deployments:** replace any explicit `INACTIVITY_LIMIT=1200` in your
`.env` / production env file with `7776000` (or your chosen seconds), then restart
the app. Explicit configuration overrides the default; changing only the code
does not override an old deployment value. Existing expired sessions cannot be
revived; a current session receives the new duration on its next request.

Logout immediately deletes that device's session on the server, so replaying its
previous cookie cannot authenticate. Other devices remain signed in. Password
reset/change still invalidate previous sessions through the account session epoch;
password change keeps the current device signed in as documented by the account
slice. Cookie flags remain HttpOnly, SameSite=Lax and Secure for HTTPS.
