# irgendwas: Secret Santa

A web app for Secret Santa: create groups, invite participants, keep wishlists and let the
app draw who gives to whom, with exclusion rules and mail notifications (English and
German).

Leptos 0.8 (SSR + hydration) on Axum, built with `cargo-leptos`, MariaDB 11.8 via SQLx.
Decisions and their reasons: `docs/decisions.md`; feature specs: `docs/specs/`; rules for
coding agents: `AGENTS.md`. The previous PHP version is on the branch `dev_php`.

## Layout

```
Cargo.toml          workspace + [[workspace.metadata.leptos]] (bin = server, lib = frontend)
app/                shared crate: App, shell, components, pages, server fns (*_api.rs)
  src/server/       ssr-only: config, db, auth (current user), authz, password, mail, flows
  src/test_support  fixtures for integration tests (feature `test-support`)
frontend/           wasm entry: hydrate()
server/             axum wiring: session store, middleware, /health, router, main
  tests/it/         ALL tests that need the server side (one binary, see "Tests")
migrations/         sqlx migrations, embedded in the binary and applied at startup
locales/            en.json, de.json (flat keys, `{name}` placeholders)
public/             static assets, served at / (e.g. /assets/css/styles.css)
end2end/            Playwright tests (run in the Playwright container image, see "E2E")
scripts/            install-cargo-leptos.sh (Dockerfile + CI), ci-target-dir.sh (CI)
compose.dev.yml     MariaDB (dev + RAM test DB) and Mailpit for development and tests
Dockerfile          production image (cargo-leptos release build → distroless, non-root)
compose.yml         production stack: app + MariaDB behind Traefik (see "Production")
compose.env.example its variables
docs/               decisions.md (decision log), specs/ (one spec per feature)
.agents/skills/     skills for coding agents (/feature); .claude/skills links here
.claude/agents/     Claude Code subagents of the /feature pipeline
```

CI: `.forgejo/workflows/ci.yml` (fmt, clippy, `cargo leptos test` with MariaDB and Mailpit
services; e2e in a separate job in the Playwright image; on pushes to `dev` and `main` the
image is built and pushed as `:dev` or `:latest`, plus `:sha-<12>`). Documentation-only
changes skip it. check and e2e keep `target/` in a named volume of the runner's engine; the
runner setup (Docker-in-Docker, `valid_volumes`, seccomp for buildah) is in the workflow's
header comment.

## Setup

```bash
cp .env.example .env                       # strict dotenv
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

The server loads `./.env` from the current directory only (it never walks up). Real
environment variables win. Migrations run on startup.

## Tests

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo leptos test            # needs the dev compose (MariaDB + Mailpit)
cargo leptos end-to-end      # Playwright, see "E2E"
```

`cargo leptos test` runs `cargo test -p server` and `cargo test -p frontend --lib`. It does
**not** run tests inside the `app` crate, so every server-side test (unit or integration)
goes into `server/tests/it/` (one test binary) and uses `app`'s public API plus `app::test_support`.

Database tests use `#[sqlx::test(migrations = "../migrations")]`: each test gets a fresh
database `_sqlx_test_database_N`, created from `DATABASE_URL`. The test binary finds
`DATABASE_URL` in the environment, or in `.env` (sqlx loads the nearest `.env` walking
up from `server/`). The URL needs a user that may `CREATE DATABASE`; in dev that is root:

```
DATABASE_URL=mysql://root:devrootpassword@127.0.0.1:3308/irgendwas_db
```

Test-only HTTP routes can be mounted inside the session/locale layers with
`server::router_with(state, extra_routes)`; see `server/tests/it/session.rs`.

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

## Conventions

The rules for people and coding agents are in `AGENTS.md`; the most used APIs:

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

```
client ──https──▶ Traefik (existing container, Docker network proxynet: TLS, headers, compression)
                    └─http──▶ app:3000 ─▶ MariaDB 11.8 (internal network)
```

`compose.yml` is the production stack (a Portainer stack named `irgendwas`; its variables
are listed in `compose.env.example`). It pulls the CI image (`APP_TAG`: `latest` = main,
`dev`, or `sha-<12>`); to build locally, `podman build --format docker -t <image> .`
(`--build-arg CARGO_BUILD_JOBS=4` limits the build's CPU use; `--format docker` keeps the
HEALTHCHECK). The image is distroless (non-root, no shell); its healthcheck is
`/app/server --healthcheck`, which GETs `/health`. Migrations are embedded and run at
startup.

```bash
docker compose --env-file prod.env up -d     # outside Portainer; never without --env-file
                                             # (./.env is the development config)
curl -s https://<domain>/health              # {"database":"ok","status":"ok"}
curl -sI https://<domain>/ | grep -iE 'strict-transport|x-frame|nosniff'
```

What the setup relies on:

- **Traefik** routes `Host(APP_HOST)` on `TRAEFIK_ENTRYPOINT` with `TRAEFIK_CERTRESOLVER`
  to `app:3000` (labels in `compose.yml`, Docker provider; with a file provider use the
  same router and middlewares and the URL `http://irgendwas:3000`). HSTS, nosniff,
  X-Frame-Options, Referrer-Policy and compression are Traefik middlewares: the app sets
  none of them. Traefik must forward `Host` and `Origin` unchanged.
- **Client IP:** the app takes the client from `X-Forwarded-For`, read from the right past
  the hops in `TRUSTED_PROXIES_CIDR`. That is Traefik's own address on `proxynet` as a
  `/32` (give Traefik a fixed IP), never the whole network: any container on it could
  forge client addresses otherwise. Traefik's entry point must not trust incoming
  forwarded headers (no `forwardedHeaders.insecure`, no `trustedIPs` unless a CDN sits in
  front): then it replaces a client's `X-Forwarded-For` with the address it sees. The rate
  limits (50 failed logins per 15 minutes, `RATE_LIMIT_MAIL_PER_IP` registrations/resets
  per hour) are keyed by that address.
- **HTTPS** detection uses `APP_URL` only (Secure cookies, Origin check), never
  `X-Forwarded-Proto`.
- **MariaDB** stays on 11.8 (LTS): never `latest`, a datadir cannot be downgraded.


## Install as an app

Open `/install` for English/German installation instructions (also linked in the
footer). Use HTTPS in production. iPhone/iPad: Safari → Share → Add to Home Screen
(keep Open as Web App enabled). Android: Chrome → Install app/Add to Home screen.
Desktop: Chrome/Edge's install command, or Safari on Mac → File → Add to Dock.
Launch from the installed icon to open in a standalone window. Browser support
and menu wording vary; regular browser use remains available.

The manifest is `/manifest.webmanifest`; its dashboard launch URL uses the normal
login flow. An internet connection is required: no offline cache is provided.
Icons use the app's existing red/green colors and a gift symbol; regenerate with
`python3 scripts/generate-app-icons.py` (standard library only).
See `docs/specs/pwa.md` for the contract and platform references.
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
