# Rust Rewrite Plan

Rewrite the PHP Secret Santa app as a Leptos full-stack (SSR + hydration) Rust application.

## Decisions (agreed 2026-10-31)

| Topic | Decision |
|---|---|
| Fidelity | **Free redesign.** PHP is a feature reference, not a spec. URLs, UI and behaviour may change. |
| Stack | **Leptos** (SSR + hydrate) on **Axum** via `leptos_axum`, built with `cargo-leptos`. SQLx for MariaDB. |
| Data | **Fresh start** (revised 2026-10-31). No production data is migrated; users register again. The Rust app owns its schema; the PHP schema is only a reference. |
| Deploy | Rust binary in a slim container **behind the existing Caddy** (TLS, security headers). Caddy may itself sit behind another reverse proxy (Traefik, nginx). Same env var names where they still make sense. |

## Layout

```
rust/                    # cargo-leptos workspace (PHP keeps src/ and public/ until cutover)
  Cargo.toml             # workspace
  app/                   # shared: components, pages, server fns; app/src/server/ = ssr-only services
  frontend/              # hydrate entry (wasm)
  server/                # axum main, router, session store, middleware; server/tests/ = all ssr tests
  migrations/            # sqlx migrations; 0001 = current PHP schema verbatim
  locales/               # en.json, de.json (seeded from src/Localization)
  end2end/               # Playwright acceptance tests
  compose.dev.yml        # mariadb + mailpit for local dev and tests
docs/rewrite/
  PLAN.md                # this file: phases, slice tracker, decision log
  specs/<slice>.md       # one spec per slice, written by spec-writer, approved by the user
```

## Phases

### Phase 0: Specs (gate: user approves each spec)
`spec-writer` produces `docs/rewrite/specs/<slice>.md` for each slice in the tracker below. Each spec records the PHP behaviour, the redesign decisions taken, and numbered acceptance criteria. Nothing is implemented from a spec with `status: draft`.

### Phase 1: Foundation (sequential; no feature work in parallel). **Done 2026-10-31** (Forgejo CI not yet run on a runner)
1. Toolchain: `rustup`, `wasm32-unknown-unknown` target, `cargo-leptos`, `sqlx-cli`, Playwright. Pin the Leptos version current at setup and record it in the decision log.
2. Workspace skeleton from the cargo-leptos Axum workspace template; `ssr`/`hydrate` feature split.
3. Config from env (`APP_URL`, `APP_DEBUG`, `APP_LOCALE`, `DB_*`, `MAIL_*`, `INACTIVITY_LIMIT`).
4. DB: SQLx MySQL pool with connection `time_zone='+00:00'`; `APP_TIMEZONE` config (default `Europe/Berlin`) for display/input (see risks). `0001_baseline.sql` is the current `src/Config/schema.sql`, the starting schema (shaped like the PHP schema, which the specs reference).
5. Sessions: `tower-sessions` with a MySQL store; `HttpOnly`, `SameSite=Lax`, `Secure` when `APP_URL` is https; inactivity timeout.
6. Auth primitives: `CurrentUser` extractor usable from server fns, password hashing (argon2id only).
7. i18n: `leptos_i18n` (or an equivalent) loading the existing en/de message JSON; locale from session/cookie, falling back to `APP_LOCALE`.
8. Layout shell, nav, flash/toast mechanism, 404/500 pages. Reuse the existing CSS/Bootstrap at first.
9. Mail: `lettre` SMTP (465 = implicit TLS, 587 = STARTTLS). In debug, send to Mailpit.
10. Containers: multi-stage Dockerfile (cargo-leptos build → slim runtime), compose with Caddy `reverse_proxy app:3000`, healthcheck endpoint, migrations run on startup.
11. CI (Forgejo workflow): `fmt --check`, `clippy -D warnings`, unit and integration tests with a MariaDB service, e2e smoke.
12. Test harness: `#[sqlx::test]` integration tests, a Mailpit client helper for reading tokens from mails, Playwright config, and a seed fixture.

### Phase 2: Feature slices (pipeline: `/rewrite-slice <slice>`)
Run one slice at a time, in tracker order. Each slice runs spec (approved) → acceptance tests → implementation → gate → review → fix loop.

### Phase 3: Release rehearsal
Deploy the production compose (Caddy + app + fresh MariaDB) on a staging host, with and without an outer reverse proxy (Traefik or nginx) in front of Caddy. Run the e2e smoke suite against it, and check that the client IP and HTTPS detection work in both setups.

### Phase 4: Cutover
Stop PHP → deploy Rust with a new, empty database → smoke test. Users register again. Keep the PHP image and its DB volume untouched for a while as a fallback.

## Slice tracker

| # | Slice | Scope (PHP reference) | Spec | Tests | Impl | Review |
|---|---|---|---|---|---|---|
| 0 | i18n-foundation | format helpers, plurals, locale detection order, `users.locale` + mail locale, `safe_return_path`, key-coverage test (split from i18n-polish; runs before auth) | approved | – | – | – |
| 1 | auth | register, login, logout, forgot/reset password (`AuthController`, `Auth`, `UserRepository`) | approved | – | – | – |
| 2 | account | dashboard, profile, change password, email change + verify (`UserController`) | approved | – | – | – |
| 3 | groups | create/edit/delete, list, show, join via code, leave, regenerate invite (`GroupController`) | approved | – | – | – |
| 4 | wishlists | edit own list, add/update/delete/reorder items, view others' lists (`WishlistController`) | approved | – | – | – |
| 5 | exclusions | manage exclusion rules (`ExclusionController`) | approved | – | – | – |
| 6 | draw | draw algorithm, assignment reveal, notifications (`GroupRepository::performDraw`, `EmailService`) | approved | – | – | – |
| 7 | i18n-polish | no-JS language switch, landing page, final key scan | approved | – | – | – |

Status values: `–` not started, `draft`, `approved`, `wip`, `done`.

## Known issues to address in the redesign

- No CSRF protection; delete/leave/draw/regenerate/remove are `GET` links. → All mutations go through server fns (POST) + Origin check + SameSite cookies.
- Draw: reciprocal pairs are forbidden, so a 2-person group can never be drawn; it makes 10 random attempts and then reports failure. → A complete, uniform draw algorithm (see draws spec); the admin gets only a generic "draw impossible" message (exclusion rules stay private); reciprocal avoidance becomes a soft preference.
- Reset/verification tokens stored in plaintext. → Store a SHA-256 hash of a random base64url token.
- Invitation codes come from `str_shuffle` (not a CSPRNG, no repeated characters). → CSPRNG.
- Group delete deletes children by hand despite `ON DELETE CASCADE`.
- `email_templates` table is never used by the code. → Decide in a spec whether to drop it.
- Authorization checks are repeated in every controller. → One "load group as member/admin" helper.

## Risks / hiccup checklist

- **Time zones (resolved):** store UTC (`TIMESTAMP` columns). The Rust connection pins `time_zone='+00:00'`, maps columns to `DateTime<Utc>`, and converts to and from `APP_TIMEZONE` (default `Europe/Berlin`, via `chrono-tz`) only for display and form input. Never introduce `DATETIME` columns.
- **`groups` is a reserved word:** always backtick-quote it in SQL, or rename it in a migration.
- **Leptos feature gating:** server-only crates (sqlx, lettre, argon2) must sit behind `ssr` or the wasm build breaks.
- **Hydration mismatches:** no `Utc::now()`, random values or locale lookups that differ between server and client during render.
- **Proxy chain:** the client IP comes from `X-Forwarded-For` through Caddy and, optionally, an outer proxy. Caddy's `trusted_proxies` and the app's `TRUSTED_PROXIES_CIDR` must list the hops in front of each. The app walks `X-Forwarded-For` from the right and skips trusted hops. Fix the PHP env-name mismatch (`TRUSTED_PROXY_CIDR` vs `TRUSTED_PROXIES_CIDR`). Scheme/HTTPS detection uses `APP_URL`, never request headers.
- **Podman:** `docker` here is podman; compose files must work with `podman compose`.
- **Secret-keeping:** a user's own assignment must be the only one they can fetch. Nothing (admin included) lists the full pairing.

## Decision log

Append one line per decision: `YYYY-MM-DD | slice | decision | why`.

- 2026-10-31 | all | Free redesign, Leptos, keep data, behind Caddy | user choice
- 2026-10-31 | all | Rust lives in `rust/` until cutover | PHP's `src/`/`public/` collide with Leptos conventions
- 2026-10-31 | foundation | DB connection in UTC, `DateTime<Utc>` everywhere, Europe/Berlin only at the UI edge | prod TZ is Europe/Berlin and all columns are TIMESTAMP, so stored values are already UTC
- 2026-10-31 | foundation | Rust toolchain 1.99.1 (stable) installed via rustup | baseline for pinning `rust-toolchain.toml`
- 2026-10-31 | groups | Spec approved: join/leave/remove only before the draw; leaving deletes the member's wishlist and exclusions; admin transfer; `/join/{CODE}` confirm page; member emails visible to admin only; no invite-by-email; `draw_date` informational | user
- 2026-10-31 | draw | Must provide an admin "reset draw" and take the same group row lock (`FOR UPDATE`) as join/leave | groups spec Q2 + concurrency
- 2026-10-31 | account | Proposals accepted (password needed for email change, non-enumerating, 60 s throttle, keep URLs); session-invalidation mechanism waits for the auth spec | user
- 2026-10-31 | all | Email comparisons are explicitly case-insensitive, independent of collation | collation of prod not yet verified
- 2026-10-31 | all | **Fresh start: no production data migration.** Users register again; drop bcrypt compatibility, legacy-token clearing and data-cleanup migrations; Phase 3 becomes a staging release rehearsal | user: prod DB not available, re-registration acceptable
- 2026-10-31 | deploy | Caddy is the edge proxy, optionally behind Traefik/nginx; the app trusts `TRUSTED_PROXIES_CIDR` hops when reading `X-Forwarded-For` | user
- 2026-10-31 | auth | Recommendations accepted: two-step registration (the invite `next=` survives the registration mail), drop "remember me", reset link 1 h, registration link 24 h, in-memory rate limits as proposed | user
- 2026-10-31 | auth | Spec approved. Intended URL as validated `?next=` relative path, stored in `pending_registrations.next_path` across the registration mail | open-redirect safe; invite links survive registration
- 2026-10-31 | auth | `users.session_epoch` invalidates all of a user's sessions on password reset/change (account increments it too) | tower-sessions records can't be queried by user
- 2026-10-31 | auth | In-memory rate limiter keyed on email + client IP; groups reuses it per user for invalid invitation codes | single instance
- 2026-10-31 | auth | Tokens: random base64url, SHA-256 at rest, single use; mails sent off the request path. Account uses the same token helper | timing-based enumeration, token leakage
- 2026-10-31 | auth | Emails lowercased on write, compared normalised | no dependency on column collation
- 2026-10-31 | account | Spec approved; validators (name 1..=100, password 8..=256) shared with auth | consistency
- 2026-10-31 | foundation | Phase 1A done: Leptos 0.8.23 / leptos_axum 0.8.10, axum 0.8, sqlx 0.9 (runtime queries), tower-sessions 0.15 with our own MySQL store (migration 0002; the sqlx-store crate pins sqlx 0.8), argon2 0.6, MariaDB 11.8 in dev | latest stable, version-compatible
- 2026-10-31 | foundation | Custom flat i18n instead of leptos_i18n; locale and time zone fixed per page load via `<html lang>` / `data-timezone`; language switch = full reload | keys are both leaves and prefixes; avoids hydration mismatch
- 2026-10-31 | foundation | Flash messages live in the session and are taken by a blocking `get_session_info` resource on each navigation; PHP `app.js` dropped | survives server-fn redirects; app.js mutated Leptos-owned DOM
- 2026-10-31 | foundation | Config, auth, authz, password, flash live in `app::server` (not `server/`) | server fns in `app` need them
- 2026-10-31 | foundation | Playwright runs in its official container image via podman (also in CI) | no npm on the dev host; Playwright browsers aren't supported on CachyOS
- 2026-10-31 | foundation | Phase 1B done: Origin/Sec-Fetch-Site check as the outermost middleware on all non-GET requests; authz gives non-members NotFound; lettre 0.11 (rustls) mailer with async send + `MailHandle::outcome()`; distroless image with `--healthcheck`; Playwright in a container | per plan
- 2026-10-31 | deploy | Caddy never trusts its own compose address; the app trusts Caddy's fixed IP (172.31.250.10) plus `TRUSTED_PROXIES_CIDR` = only the proxies in front of Caddy (space- or comma-separated) | rootless podman makes every client appear as Caddy's address, which would otherwise allow X-Forwarded-For spoofing
- 2026-10-31 | deploy | Prod compose always runs with `--env-file prod.env`; `CADDY_AUTO_HTTPS` = on/off/disable_redirects via Caddyfile snippets | avoid picking up the dev `.env`; `auto_https on` isn't valid Caddy syntax
- 2026-10-31 | foundation | Navbar toggle is a Leptos signal; `<html data-hydrated>` marks hydration for e2e | proves hydration in tests without a mismatch risk
- 2026-10-31 | wishlists | Spec approved: every current member sees every member's list, before and after the draw; no wishlist fn reads gift_assignments; Move up/down reordering (position column); http/https links only, rel="noopener noreferrer nofollow"; max 50 items | user; PHP had stored XSS via links
- 2026-10-31 | exclusions | Spec approved: self-service one-way rules only, no admin view; reject rules that make the draw impossible; cap 5 per member per group; composite FKs to group_members (cascade on leave); `check_feasibility` + `load_rules` are the draw's only interface | user; admin control of rules would let them steer the draw
- 2026-10-31 | draw | Spec approved: receiver name in the mail; APP_LOCALE for mails (pending i18n-polish); cap 200 members; generic DrawImpossible only; exact uniform draw for n <= 9, rejection sampling + matching fallback above; in-process mail worker (3 attempts, admin resend); reset keeps rules and wishlists; drop email_templates; CHECK giver <> receiver (drop and log if MariaDB rejects it) | user + spec
- 2026-10-31 | all | Lock order: the group row first (FOR UPDATE; shared mode for wishlist writes), then child rows, in every slice | no deadlocks, no writes interleaving with the draw
- 2026-10-31 | i18n | i18n-foundation split from i18n-polish and built before auth; auth, account, groups, wishlists and draw specs revised in place to use it | earlier slices are built on the final rules instead of being reworked
- 2026-10-31 | i18n | Fixed formats per locale (en `10 Dec 2026, 14:30`, de `10.12.2026 14:30`); plurals as `.one`/`.other` keys | user
- 2026-10-31 | i18n | Locale order session → cookie → Accept-Language → APP_LOCALE; `users.locale` is copied into the session at login (other devices follow at their next login) | user
- 2026-10-31 | i18n | Every mail to an account uses `users.locale` (fallback APP_LOCALE); only the registration mail uses the request locale. Supersedes the draw entry's APP_LOCALE for mails | consistent per-recipient language
- 2026-10-31 | i18n | Unused keys fail the coverage test; catalogs cut to the keys in use, each slice adds keys with the code that uses them (reused texts are copied from the PHP catalogs, which stay until slice 7 is done) | user
- 2026-10-31 | i18n-polish | Language switch is a POST server fn with a validated `next`; a logged-in switch also updates `users.locale` | replaces PHP's Referer open redirect
- 2026-10-31 | draw | No "only one possible draw" admin warning; the small-group hint covers n = 2/3 | user
