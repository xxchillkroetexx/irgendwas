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
| 0 | i18n-foundation | format helpers, plurals, locale detection order, `users.locale` + mail locale, `safe_return_path`, key-coverage test (split from i18n-polish; runs before auth) | approved | done | done | approved |
| 1 | auth | register, login, logout, forgot/reset password (`AuthController`, `Auth`, `UserRepository`) | approved | done | done | approved |
| 2 | account | dashboard, profile, change password, email change + verify (`UserController`) | approved | done | done | approved |
| 3 | groups | create/edit/delete, list, show, join via code, leave, regenerate invite (`GroupController`) | approved | done | done | approved |
| 4 | wishlists | edit own list, add/update/delete/reorder items, view others' lists (`WishlistController`) | approved | done | done | approved |
| 5 | exclusions | manage exclusion rules (`ExclusionController`) | approved | done | done | approved |
| 6 | draw | draw algorithm, assignment reveal, notifications (`GroupRepository::performDraw`, `EmailService`) | approved | done | done | approved |
| 7 | i18n-polish | no-JS language switch, landing page, final key scan | approved | done | done | approved |

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
- 2026-11-01 | i18n-foundation | `generate_route_list(App)` is cached in a `OnceLock` (`server/src/lib.rs` `route_list()`) | Leptos sets a process-wide SuppressResourceLoad flag while building the route list; parallel tests that each built a router hung on concurrent renders (likely the Phase 1 one-off hang). Don't render resource-holding components via bare `to_html` concurrently with the first router build
- 2026-11-01 | i18n-foundation | `disable-erase-components = true` in the leptos metadata | debug builds otherwise emit `<!>` markers after dynamic text, so `cargo leptos test`/e2e saw different markup than release; release builds unaffected. Cost: slower debug compiles; revisit if later slices hit compile-time/type-length problems
- 2026-11-01 | i18n-foundation | Catalogs cut to the 23 keys in use; migration 0003 adds `users.locale VARCHAR(8) NULL`; locale cookie `Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly` (+ Secure on https) | spec
- 2026-11-01 | i18n-foundation | Slice done (review round 2 APPROVE). Deferred nits: coverage scanner C9 only checks the first `{}` child block (`<p>{"a"}{"b"}</p>` passes), C9 ignores `value=`/`content=`/`attr:*`, C10 misses `.format(CONST)`, C5 misses typo namespaces; `I18n` PartialEq could use `ptr::eq`; `append_set_cookie` could `debug_assert!` | non-blocking; tighten when a slice touches the scanner
- 2026-11-01 | auth | Login and logout force a full page load (Location without leptos' client-redirect header; 302 only for plain HTML form posts); other fns use `leptos_axum::redirect` | the page language can change and the locale is fixed per page load
- 2026-11-01 | auth | Page guards (`RequireUser`, `RedirectIfLoggedIn`) are blocking resources, so SSR answers with a real 302; on the client they navigate via the router | no flash of protected content, works without JS
- 2026-11-01 | auth | Token pages push the invalid-link flash only after the layout's session resource resolved | otherwise the 302 response itself consumes it
- 2026-11-01 | auth | `auth::login(session, id)` means epoch 0; a missing `session_epoch` in the session counts as 0. Logout keeps only the session locale under a new session id (spec says flush all; locale is not sensitive) | foundation API kept stable
- 2026-11-01 | auth | Rate limits: validation failures and blocked calls are not counted; register uses one of 3 mails per address whether or not the account exists, forgot-password only when it exists | responses stay identical either way
- 2026-11-01 | auth | reset_password checks the token with a SELECT before hashing, then a conditional UPDATE (single use); logs out the caller's own session first if it is that user. Rehash on login when `needs_rehash`; process-wide dummy hash for unknown emails | no argon2 work for unknown tokens; timing equalisation
- 2026-11-01 | auth | Dev profile: `opt-level = 3` for argon2 and blake2 | debug argon2 made the rate-limit tests take ~20 s each; production parameters unchanged
- 2026-11-01 | auth | Email (and other identity) columns use `utf8mb4_bin`; emails are normalised (trimmed, lowercase) before storage and lookup; mails go to the stored address | review blocker: utf8mb4_unicode_ci is accent-insensitive, `alice@gmäil.com` matched `alice@gmail.com` (reset-link and pending-registration takeover)
- 2026-11-01 | auth | Slice done (review round 2 APPROVE). Login limiter reserves both keys atomically (`try_acquire_all`), limiter keys are SHA-256 hashes; `db_error` logs SQLSTATE/errno/kind only. Deferred: `From<sqlx::Error> for AppError` (error.rs) still logs raw sqlx messages (fix in account slice); `db_error` call-site labels; `release()` across a window rollover allows one extra attempt; NFC/NFD spellings are distinct accounts (mails go to the stored address, so no takeover) | non-blocking
- 2026-11-01 | account | Error statuses: Unauthorized 401; validation, CurrentPasswordIncorrect, EmailUnchanged 422; EmailChangeTooSoon/TooManyAttempts 429; InvalidOrExpiredToken 400; EmailTaken 409 (only visible at confirm time, to whoever controls the new mailbox, so no enumeration); MailSendFailed 503. Shared `HttpStatus` trait + generic `respond::with_error_status` | review-checked
- 2026-11-01 | account | change_password: one conditional UPDATE (`WHERE id=? AND password=<old hash>`) that bumps session_epoch; new epoch read in the same tx, then `login_with_epoch` cycles the id. request_email_change: `FOR UPDATE`, 60 s throttle in DB time, recipient re-read from the row, old values restored on mail failure (guarded by the new token hash) | concurrency-safe, mails go to the stored address
- 2026-11-01 | all | Resources are created synchronously at page level, never inside a guard's async children (`RequireUser` Suspend); fetchers call `get_current_user` first so an anonymous SSR render doesn't run protected fns | creating one inside the Suspend caused a hydration panic (root cause unconfirmed: likely creation-order-dependent serialized ids)
- 2026-11-01 | account | Current-password attempts are rate limited (`account:pw:<user_id>`, 10/15 min, shared by change_password and request_email_change, independent of login); per-recipient mail limit applies to the verification mail; `no-store` on profile/verify (spec AC40–43) | review: a stolen session could otherwise guess the password without limit and take the account over via email change
- 2026-11-01 | auth+account | Token pages send `Referrer-Policy: same-origin` (was `no-referrer`); no-JS form posts on them must work and end with an explicit redirect + flash | with `no-referrer`, browsers send `Origin: null` on a form POST and the Origin check rejects it (pre-hydration click → 403); `same-origin` still never sends the token URL to another origin
- 2026-11-01 | e2e | `end2end/run.sh` mounts all of `rust/` at /work (-w /work/end2end), as CI does | specs read `../../locales/en.json`
- 2026-11-01 | build | `[profile.dev] debug = "line-tables-only"`; `cargo clean` after each slice commit. Planned after the account slice: merge the ~25 `server/tests/*.rs` integration binaries into one (`tests/it/main.rs` with modules), updating CLAUDE.md's test conventions | target/debug reached 77 GB (each test binary links the whole app with debuginfo); one link instead of ~25 also cuts gate time and CPU load
- 2026-11-01 | account | AC40: `account:pw:<user_id>` reserved via `try_acquire_all` right before argon2 in `check_current_password`, kept on a wrong password, cleared on success. AC43: one `mail:email:<new email>` slot after the 60 s throttle passes; the slot stays used after `MailSendFailed`. `TooManyAttempts` uses its own key `flash.error.too_many_password_attempts` (auth's text talks about login attempts) | spec AC40/AC43
- 2026-11-01 | account | No-JS form posts (detected like auth's plain HTML posts) store the result as a session flash and answer 302 to `/user/profile`; a no-JS confirm without a session, or an `Unauthorized` profile post, goes straight to `/auth/login?next=%2Fuser%2Fprofile` | going via /user/profile would redirect again and that render would consume the flash
- 2026-11-01 | all | Every route uses `SsrMode::Async` | with streaming, pending Suspense/Transition content (navbar, flash, guarded pages) arrives as `<template>` chunks that only JS inserts, so no-JS pages were empty; cost: first byte waits for the whole page
- 2026-11-01 | all | Static files are served by `tower_http::services::ServeDir` (precompressed gzip/br); leptos' `file_and_error_handler` only renders the 404 page | leptos leaves a rendering request's root owner as the thread's current owner while the response streams; `file_and_error_handler` creates a child `Owner` and copied that request's `ResponseOptions`, so `/pkg/irgendwas.wasm` came back as another page's 302 and the page never hydrated (flaky e2e)
- 2026-11-01 | account | Slice done (review round 2 APPROVE; no cross-request leak today). Rule added to CLAUDE.md: server fns use only Parts/ResponseOptions/AppState contexts (leptos' leaked thread owner makes other contexts resolve to another request's page). Deferred: optional layer that unsets the thread-local owner around server-fn polls + upstream bug report; a test that every route renders without out-of-order `<template>` chunks; check whether the `<Routes fallback>` 404 render streams (no-JS navbar) | non-blocking
- 2026-11-02 | groups | Migration 0005: `groups.invitation_code` → `utf8mb4_bin` (MODIFY keeps the UNIQUE index); codes from `getrandom`, 32-char alphabet (uniform: 256 % 32 = 0), 5 retries on collision, regenerate never reuses the old code | CLAUDE.md identity-column rule
- 2026-11-02 | groups | Every group mutation: check rights via authz → `SELECT … FOR UPDATE` on the group row → re-check rights, `is_drawn` and (for remove/transfer) that the target is a member of this group under the lock. Join looks the code up unlocked, then locks by id and re-compares the code bytewise (deleted/regenerated → InvalidCode). `remove_member_data` deletes gift_assignments → exclusion_rules → wishlists → group_members | lock order "group row first"; later slices (wishlists, exclusions, draw) reuse this pattern
- 2026-11-02 | groups | Status map: InvalidCode 404; CodeRequired, CannotRemoveSelf, TargetNotMember, TargetIsSelf 422; AlreadyMember, RegistrationClosed, AlreadyDrawn, AdminCannotLeave 409; non-members always get NotFound before any state-specific error | no membership/existence leak
- 2026-11-02 | groups | Invalid-code limiter: 10 per user per 10 min, reserved atomically before the lookup and given back when the code exists; codes > 32 chars count as invalid without a query; no IP limit (preview requires login) | giving back is no extra oracle: the response already says whether the code is valid
- 2026-11-02 | groups | Extra server fn `open_invitation(code)` for the no-JS /join form: 302 to `/join/{percent-encoded code}` without lookup or counting; with JS the browser normalises and navigates | invalid codes aren't counted twice
- 2026-11-02 | groups | Outcomes: success flash stored in the session; create/update/join/delete/leave navigate via the server redirect, regenerate/remove/transfer reload group + flash; no-JS errors = flash + redirect back | one flow for JS and no-JS, no duplicate messages
- 2026-11-02 | all | `components/layout.rs` strips the user's email from the session data embedded in every page | nothing on the client used it; groups test forbids even the viewer's own email on /join/{code}
- 2026-11-02 | groups | Slice done (review round 2 APPROVE). Codes without a letter or digit are InvalidCode before any redirect (a `..` segment would resolve to `/`); copy toast only after `clipboard.writeText` resolves; `group_path` lives in paths.rs. Note: the `it` test binary now takes ~5.5 min (380 tests) | non-blocking
- 2026-11-02 | all | `<textarea>` content only through `components::TextArea` (HTML-escaped during SSR, raw on the client; DOM text equals the raw value, so hydration matches and no-JS forms stay prefilled); rule in CLAUDE.md | tachys 0.2.19 renders textarea children unescaped (like script/style), so a stored `</textarea><script>` broke out of the wishlist and group edit forms (self-XSS: only the author sees their own edit form)
- 2026-11-02 | auth | `RATE_LIMIT_MAIL_PER_IP` (default 20) sets the per-IP register/reset mail limit; 1000 only in the dev/e2e env (`rust/.env.example`, CI e2e job); compose.env.example (production) leaves it unset | the whole e2e suite registers from one IP (~21 mails per run, growing with each slice)
- 2026-11-02 | wishlists | Write lock order: group row LOCK IN SHARE MODE → caller's `group_members` row FOR UPDATE → authz check → `wishlists` row FOR UPDATE → items FOR UPDATE; deadlock (1213) and snapshot-isolation (1020) errors retried up to 3 times | MariaDB 11.8 rejected a second tab's first add with 1020 when the membership check ran before the locks; the member-row lock orders the owner's writes before the list row exists
- 2026-11-02 | wishlists | Each user's wishlist writes queue in-process (per-user async mutex) before taking a DB connection | otherwise one user's burst used up the pool; caps one user at one connection while a draw holds the group lock. Assumes a single app process (true for this deployment)
- 2026-11-02 | wishlists | Undeserialisable args (non-integer ids, unknown direction, missing field) → NotFound 404, never 500; `confirm` accepts `true`/`on`; add_item renumbers positions 1..n if not strictly increasing, deletes leave gaps, a no-op move doesn't bump updated_at; failed plain posts: NotFound → /groups + flash, Unauthorized → login?next=edit page, others → edit page + flash; panel "View list" links use rel="external" | spec gaps, consistent with groups
- 2026-11-02 | wishlists | Write transactions run under READ COMMITTED (`SET TRANSACTION ISOLATION LEVEL READ COMMITTED` right before BEGIN on the same connection, one transaction only; supersedes the REPEATABLE READ assumption in the earlier lock-order entry); 1213/1020 retry kept as a safety net. Residual: if BEGIN itself fails, the pending level could carry to that connection's next transaction (only on a dead connection, which the pool's ping drops) | review: REPEATABLE READ gap-locked the next index record after a new empty list, so different members' first adds deadlocked (reproduced 5/5 by the burst tests)
- 2026-11-02 | all | Textarea input is normalised CRLF/CR → LF on the server (`validation::normalize_newlines`) before length checks and storage (wishlist and group descriptions); scheme-less links are prefixed with `https://` only when parsing fails with RelativeUrlWithoutBase | browsers count a line break as 1 toward maxlength but submit CRLF; `www.shop.de/x?ret=https://y` was rejected
- 2026-11-02 | wishlists | Slice done (review round 2 APPROVE). A request dropped between `SET TRANSACTION` and `BEGIN` can also leave the pending level on a pooled connection; harmless today (all other locking reads are point FOR UPDATE on PK/unique keys); CLAUDE.md rule: code relying on REPEATABLE READ sets its level explicitly (relevant for the draw) | non-blocking
- 2026-11-02 | tests | `#[sqlx::test]` runs against a separate RAM-backed `mariadb-test` service (compose.dev.yml, port 3308, tmpfs, durability off; DATABASE_URL points there); the app and e2e keep the persistent dev DB on 3307; CI's check job uses a tmpfs MariaDB service too | measured: the suite took ~500 s because every test creates a database and runs all migrations (DDL file creation on the btrfs/LUKS disk, serialised by MariaDB); argon2 cost was not the cause (minimum cost saved ~5 s, change discarded); with tmpfs the 465 tests take ~10 s
- 2026-11-02 | exclusions | `check_feasibility` (app::domain::feasibility) is an exact Kuhn bipartite matching (iterative, ascending ids); `load_rules(&mut Transaction<'_, MySql>, group_id) -> Result<Vec<(UserId, UserId)>, AppError>` in app::server::exclusions is the draw's only interface (signature noted in draw.md) | exactness tested against brute-force and matching oracles up to n=200
- 2026-11-02 | exclusions | add/remove: authz → group row FOR UPDATE → re-check everything, under explicit READ COMMITTED; get_exclusions reads in one explicit REPEATABLE READ snapshot (rolled back) so the feasibility notice is from one moment | CLAUDE.md isolation rule; concurrent adds can't jointly make the draw impossible
- 2026-11-02 | exclusions | Statuses: AlreadyDrawn, WouldMakeDrawImpossible 409; CannotExcludeSelf, TargetNotMember, TooManyExclusions 422; undeserialisable args 404. No-JS: success/errors → back to the page + flash, NotFound → /groups, no session → login?next; idempotent re-add flashes exclusion_added. The notice's private field serialises as `me`; submit disabled only after hydration (no-JS form stays usable) | consistent with groups/wishlists
- 2026-11-02 | exclusions | Migration 0007 adds the composite FKs to group_members (ON DELETE CASCADE) and first deletes rules whose giver/excluded user isn't a member (no-op on a fresh DB; spec said no cleanup needed) | makes the FK creation safe on any existing data
- 2026-11-02 | exclusions | Slice done (review APPROVE). Nits fixed: select value seeded on hydration, aria-labels on Remove buttons, one `server::db` begin helper taking the isolation level. Note for the draw: exclusions and groups writes have no per-user queue; a burst of >10 writes while a long group-lock holder runs can exhaust the pool (10 connections, 5 s acquire timeout) — keep the draw's group-lock hold short | review
- 2026-11-02 | draw | Algorithm (app::server::draw): n ≤ 9 exact two-pass bitmask enumeration, uniform over the valid assignments with the fewest reciprocal pairs (D4 soft preference); larger n: rejection sampling that aborts an attempt as soon as a fixed position breaks a rule (still uniform over accepted results), then the matching-guided fallback (Kuhn); runs on spawn_blocking with `StdRng::from_os_rng()` | spec D4/§algorithm
- 2026-11-02 | draw | perform/reset: explicit READ COMMITTED, group row FOR UPDATE, re-check admin + is_drawn under the lock; assignments in one INSERT; mails enqueued only after commit. A group counts as drawn for reveal only when `is_drawn` AND `drawn_at` are set | short group-lock hold (pool of 10)
- 2026-11-02 | draw | Mail worker: one task, channel queue, one job at a time; each row re-read with all conditions right before sending; no DB connection held during SMTP; 3 attempts with per-row retry scheduling (60 s / 300 s), admin resend, startup recovery; logs ids + error kind only; mails in each recipient's users.locale | spec; AC43/45 in-flight reading
- 2026-11-02 | draw | During SSR the group-page panel and the dashboard call the flow fns directly (a failing panel is omitted instead of turning the page into 403/500); only the admin fetches the admin status; small-group hint for 2–3 members; reset mail links to the group | page robustness, no extra keys
- 2026-11-02 | draw | Migration 0008: is_drawn NOT NULL (NULL → FALSE), drawn_at, CHECK giver_id <> receiver_id (MariaDB 11.8 accepts it; self rows deleted first), drop email_templates | spec; guards are no-ops on a fresh DB
- 2026-11-02 | draw | Accepted residuals (review): the draw-mail job queue is unbounded and reset has no cooldown (an admin looping draw/reset mails every member ~2n times per cycle; spec asks for no cap); the G4 fallback's 100k-attempt rejection loop runs only when no reciprocal-free assignment was found, so a slow draw can hint at reciprocal pairs or dense rules (inherent to the spec's design; one noisy sample per draw) | revisit if abuse shows up
- 2026-11-02 | draw | Exact tier: pass 2 walks the whole pruned search space (no early exit at the picked leaf) | run time must not reveal the result (S2/D10; review)
- 2026-11-02 | all | `MailError::Smtp` carries only class + SMTP reply code (e.g. "permanent, code 550"); logs add `MailError::kind()` | lettre's error text quotes the server reply, which can name the recipient address (S5; review; tested with an in-process SMTP stub)
- 2026-11-02 | draw | Each mail-worker job runs in its own awaited task; a panic is logged with job kind and id only, `outstanding` is decremented and the worker continues | one bad job must not stop all draw mails
- 2026-11-02 | draw | Slice done (review round 2 APPROVE; no secrecy leak found). Residual: if a worker job panics between `retrying.remove` and the delivery UPDATE, that row stays pending until the next restart's recovery (resend only touches rows with ≥3 attempts) | panic-only, not a secrecy issue
- 2026-11-02 | i18n-polish | `set_locale(locale, next)` at /api/set_locale (no tracing instrumentation, `next` can hold a token): success via `respond::redirect_document` (plain form post → 302, server-fn client → Location without client redirect, page reloads since the locale is fixed per page load); an unsupported locale from a plain form post → 302 to the validated `next` (or `/`) with nothing changed (otherwise leptos would redirect to the Referer), JS client → 422 Validation; `load_user` runs before `remember_locale` (clearing a stale session would drop the new locale) | spec AC1–AC12
- 2026-11-02 | i18n-polish | Every page renders native POST switch forms (before `.navbar-collapse`, visible at 375 px without JS) with hidden `next` = current path + query as a `value` attribute (`prop:` isn't rendered during SSR). Earlier byte-comparison tests (404 indistinguishability, /join?code prefill) normalise the `next` value | the echoed value is the requester's own URL, so no existence or code leak
- 2026-11-02 | i18n-polish | set_locale's error type wraps AppError (same JSON); undeserialisable args → NotFound 404; every plain-form error is a 302 to the validated next (or "/"), never leptos' Referer fallback; users.locale is written before session + cookie | review: no Referer redirect, no 500, consistent state
- 2026-11-02 | i18n-polish | Slice done (review APPROVE; nits fixed). Watch: `account_confirm::ac31_a_link_works_only_once` failed once in a full run, not reproducible (0/20 isolated, 0/5 full runs). Resolved (user): `lang` only on a `<span>` around the native name, aria-label stays on the button. Open: route paths partly still string literals instead of `paths.rs` constants | non-blocking
