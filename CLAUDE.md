# irgendwas: Secret Santa

The PHP app (`src/`, `public/`, `docker/`) is being rewritten in Rust/Leptos under `rust/` on branch `rust_rewrite`.
Read `docs/rewrite/PLAN.md` first: it holds the decisions, phases, slice tracker, risks and decision log.

## Rules for the rewrite

- PHP code is a **reference only**. Do not modify it; it is deleted at cutover.
- Implement from approved specs in `docs/rewrite/specs/`. If a spec is `status: draft` or missing, stop and say so.
- Fresh start: no production data is migrated, and the PHP schema is only a reference. Schema changes are new SQLx migrations in `rust/migrations/` (don't edit one that's already committed; dev DBs would break). Migration numbers in specs are placeholders: use the next free number at implementation time.
- Name columns explicitly in queries; no `SELECT *`.
- Server-only dependencies (sqlx, lettre, argon2, tower-sessions) live behind the `ssr` feature.
- All mutations are Leptos server functions (POST). Every server fn that touches a group checks membership/admin through the shared authorization helper; never trust IDs from the client.
- Never expose another user's gift assignment. Not to admins either.
- User-facing strings go through i18n (en + de); no hard-coded UI text.
- Record any non-trivial design decision in the decision log in `docs/rewrite/PLAN.md`.

## Codebase conventions (from Phase 1A)

- `app/src/server/` (ssr only) holds config, db, auth (`load_user` is the single session → user choke point), authz, password, flash, i18n, client_ip. `server/` holds only the session store, middleware, router and `main`.
- **All ssr tests go in `server/tests/`.** `cargo leptos test` does not run unit tests inside `app`. Fixtures are in `app/src/test_support.rs` (feature `test-support`). Use `#[sqlx::test(migrations = "../migrations")]`, and `server::router_with(state, extra)` for HTTP-level tests.
- `--all-features` compiles `app` with ssr and hydrate together: gate server-only code with `#[cfg(feature = "ssr")]` / `#[cfg(not(feature = "ssr"))]` pairs, not ssr/hydrate pairs.
- i18n is custom and flat: `t!(i18n, "key", name = v)`, catalogs in `locales/{en,de}.json`; add every key to both.
- Runtime SQL only (`sqlx::query_as::<_, Row>`), no compile-time macros. Errors are `AppError` (`Unauthorized`, `Forbidden`, `NotFound`, `Validation(key)`, `Internal`).
- Nothing time-dependent or random during render (hydration). Locale and time zone come from `<html lang>` / `data-timezone`.
- Tests: POST through `common::post(...)` (it sends the Origin; a POST without one gets 403). Mail tests send to `Mailpit::unique_email()` (one Mailpit is shared by parallel tests). In e2e, call `waitForHydration(page)` before interacting and `collectPageErrors(page)` to catch hydration panics.
- Mail: `state()?.mailer.send(Message { to, subject, html, text })`; escape every interpolated value with `escape_html`; await `handle.outcome()` only when delivery status matters.
- `rust/.env` must exist (copy `.env.example`); otherwise cargo-leptos picks up the PHP `.env` at the repo root.

## Commands (run from `rust/`)

```bash
podman compose -f compose.dev.yml up -d      # mariadb + mailpit
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo leptos test                             # unit + #[sqlx::test] integration tests
cargo leptos end-to-end                       # Playwright acceptance tests
cargo leptos watch                            # dev server
```

The gate for any change: fmt, clippy, test and end-to-end all pass.
