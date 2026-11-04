# irgendwas: Secret Santa

Groups, invitations, wishlists and a Secret Santa draw with exclusion rules and mail
notifications (en + de). Rust: Leptos 0.8 (SSR + hydration) on Axum, built with
cargo-leptos, MariaDB 11.8 via SQLx. Work happens on `dev`; `main` is what runs in
production (images `:dev` / `:latest`).

- `docs/decisions.md`: why things are the way they are. Read the entries for the area you
  touch, and add one line for every non-trivial decision you make.
- `docs/specs/<feature>.md`: one spec per feature, with numbered acceptance criteria (ACs).
  New features and larger changes start with a spec; implement only from `status: approved`.
  If the spec is `draft` or missing, stop and say so.
- `README.md`: layout, setup, tests, production.

The app replaced a PHP version in November 2026 (kept on the branch `dev_php`, read it with
`git show origin/dev_php:<path>`; never restore it). Production holds the migrated data:
accounts that haven't reset their password since still carry a bcrypt hash, which never
verifies.

## Rules

- **Data:** production holds real data. Schema changes are new SQLx migrations in
  `migrations/` (next free number); never edit a committed one (dev and production DBs
  would break). Every migration must apply to the populated database and preserve its rows.
- **SQL:** runtime queries only (`sqlx::query_as::<_, Row>`), no compile-time macros. Name
  columns explicitly, no `SELECT *`. `groups` is a reserved word: always backtick it.
- **Identity columns** (emails, tokens, invite codes) use `COLLATE utf8mb4_bin`, with values
  normalised in Rust before storing/looking up; the table default `utf8mb4_unicode_ci` is
  case- and accent-insensitive. Mails go to the address stored in the DB, never the one the
  caller typed.
- **Time:** columns are `TIMESTAMP` (never `DATETIME`), mapped to `DateTime<Utc>`; the pool
  pins `time_zone='+00:00'`. Convert to and from `APP_TIMEZONE` (default Europe/Berlin) only
  for display and form input (`app::tz`).
- **Feature gating:** server-only dependencies (sqlx, lettre, argon2, tower-sessions) live
  behind the `ssr` feature. `--all-features` compiles `app` with ssr and hydrate together:
  gate server-only code with `#[cfg(feature = "ssr")]` / `#[cfg(not(feature = "ssr"))]`
  pairs, not ssr/hydrate pairs.
- **Server fn context:** bodies read only `Parts`, `ResponseOptions` and `AppState` from the
  Leptos context (via `state()`, `session()`, `request_locale`, `client_ip`, `respond`).
  Never `use_i18n`, `SessionResource`, router hooks, resources or views inside a server fn:
  leptos leaves another request's page owner as the thread's current owner, so other
  contexts silently resolve to *another visitor's* data. Build mail i18n with
  `mail_i18n`/`account_locale`.
- **Mutations** are Leptos server functions (POST; the Origin check rejects cross-site
  requests). Every server fn that touches a group checks membership/admin through the
  shared authorization helper (`app::server::authz`); never trust IDs from the client.
- **Secret-keeping:** never expose another user's gift assignment. Not to admins either.
- **Transactions** that rely on REPEATABLE READ (gap locks, a repeatable snapshot) set
  their isolation level explicitly: wishlist writes use READ COMMITTED, and a pooled
  connection can carry a pending level if a request is dropped between `SET TRANSACTION`
  and `BEGIN`.
- **`<textarea>`:** never put user text as its children directly: tachys renders textarea
  children unescaped during SSR (like `<script>`), so `</textarea><script>` breaks out. Use
  the shared `TextArea` component.
- **Routes:** new routes use `ssr=SsrMode::Async` (no-JS pages otherwise lack their
  Suspense content).
- **Hydration:** nothing time-dependent or random during render. Locale and time zone come
  from `<html lang>` / `data-timezone`.
- **Strings:** user-facing text goes through i18n: `t!(i18n, "key", name = v)`, flat
  catalogs in `locales/{en,de}.json`; add every key to both. No hard-coded UI text.
- **Client IP / HTTPS:** the client IP comes from `X-Forwarded-For`, walked from the right
  past the hops in `TRUSTED_PROXIES_CIDR` (in production: Traefik's address). Scheme/HTTPS
  detection uses `APP_URL`, never request headers.

## Codebase conventions

- `app/src/server/` (ssr only) holds config, db, auth (`load_user` is the single
  session → user choke point), authz, password, flash, i18n, client_ip, mail and the
  `*_flow.rs` logic. `server/` holds only the session store, middleware, router and `main`.
- Errors are `AppError` (`Unauthorized`, `Forbidden`, `NotFound`, `Validation(key)`,
  `Internal`).
- Mail: `state()?.mailer.send(Message { to, subject, html, text })`; escape every
  interpolated value with `escape_html`; await `handle.outcome()` only when delivery status
  matters.
- **All ssr tests go in `server/tests/it/`, one integration binary** (`it`, entry
  `tests/it/main.rs`; a binary per file linked the whole app 25 times). A feature's file is
  `tests/it/<feature>.rs`, registered with `mod <feature>;` in `main.rs`; shared helpers are
  `crate::common` (`use crate::common::auth::*;`, no `mod common;` in test files). Don't add
  `tests/*.rs` files. Tests share one process: no `set_var`, no per-test global state
  (`capture_logs()` routes by thread). `cargo leptos test` does not run unit tests inside
  `app`. Fixtures are in `app/src/test_support.rs` (feature `test-support`). Use
  `#[sqlx::test(migrations = "../migrations")]`, and `server::router_with(state, extra)` for
  HTTP-level tests.
- Tests: POST through `crate::common::post(...)` (it sends the Origin; a POST without one
  gets 403). Mail tests send to `Mailpit::unique_email()` (one Mailpit is shared by parallel
  tests). In e2e, call `waitForHydration(page)` before interacting and
  `collectPageErrors(page)` to catch hydration panics.
- `.env` must exist (copy `.env.example`, strict dotenv). The dev host uses podman
  (`podman compose`); production uses Docker (Portainer).

## Commands

```bash
podman compose -f compose.dev.yml up -d      # mariadb (dev, :3307), mariadb-test (RAM, :3308, for #[sqlx::test]), mailpit
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo leptos test                             # unit + #[sqlx::test] integration tests
cargo leptos end-to-end                       # Playwright acceptance tests
cargo leptos watch                            # dev server
```

The gate for any change: fmt, clippy, test and end-to-end all pass.
