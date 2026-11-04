---
slice: auth
status: approved
---
# Auth

Register, login, logout, forgot/reset password. Covers the session contract (`user_id`, intended-URL redirect, fixation, invalidation) that every later slice relies on.

PHP reference: `src/Controllers/AuthController.php`, `src/Controllers/BaseController.php` (`requireAuth`), `src/Core/Auth.php`, `src/Core/Session.php`, `src/Repositories/UserRepository.php`, `src/Models/User.php`, `src/Services/EmailService.php` (`sendPasswordReset`, `sendWelcomeEmail`, `sendExistingAccountNotification`, `sendEmail`), `src/Views/auth/*`, `src/Views/layouts/main.php`, `src/Core/Application.php` (routes).

Fresh start: no production data is migrated and users register again. The Rust app owns its schema; the PHP schema is only a reference for shapes and names.

Revised 2026-10-31: aligned with i18n-foundation (built before this slice). `safe_next` builds on `safe_return_path`; login adopts the account language (`users.locale`); registration completion stores it; mails to existing accounts use it; dates and plurals go through `app::format` / `t_plural!`; `generic_error_message` → `error.generic.message`. AC2, AC33, AC43 and AC45 were revised; AC47–AC51 were added.

## Current PHP behaviour

### Routes

| Method | Path | Handler | Auth | What it does |
|---|---|---|---|---|
| GET | `/auth/login` | `showLogin` | none (logged-in → 302 `/user/dashboard`) | Login form: email, password, "remember me" checkbox, links to forgot-password and register. |
| POST | `/auth/login` | `login` | none | Empty email or password → flash `flash.error.email_password_required`, 302 `/auth/login`. Bad credentials → flash `flash.error.invalid_credentials`, 302 `/auth/login`. Success → flash `flash.success.logged_in`, 302 to session `intended_url` (then removed) or `/user/dashboard`. |
| GET | `/auth/register` | `showRegister` | none (logged-in → 302 dashboard) | Form: name, email, password, password_confirm. Prefills name/email from flash `old_input`. |
| POST | `/auth/register` | `register` | none | Validates (below). Errors → flash `errors` (field → hard-coded English string) + `old_input`, 302 `/auth/register`. Valid: if email exists → `sendExistingAccountNotification`; else `createUser` (bcrypt) + `sendWelcomeEmail`. Both: flash `flash.success.registration_instructions`, sleep 100–200 ms, 302 `/auth/login`. The user is **not** logged in. |
| GET | `/auth/logout` | `logout` | none | Removes `user_id`, regenerates session id (other session data kept), flash `flash.success.logged_out`, 302 `/`. |
| GET | `/auth/forgot-password` | `showForgotPassword` | none (also shown when logged in) | Form: email. |
| POST | `/auth/forgot-password` | `forgotPassword` | none | Empty → flash `flash.error.enter_email`, 302 back. Otherwise if user exists: generate token, send mail. Always flash `flash.success.reset_instructions`, 302 `/auth/login`. |
| GET | `/auth/reset-password/:token` | `showResetPassword` | none | Token not found / expired → flash `flash.error.reset_token_expired`, 302 `/auth/forgot-password`. Else form (hidden token, password, password_confirm). |
| POST | `/auth/reset-password` | `resetPassword` | none | Any field empty → `flash.error.all_fields_required`; mismatch → `flash.error.passwords_dont_match`; `strlen < 8` → `flash.error.password_too_short`; each 302 `/auth/reset-password/<token>`. Then `UserRepository::resetPassword`: success → bcrypt new password, clear token, flash `flash.success.password_reset`, 302 `/auth/login`; failure → `flash.error.invalid_reset_token`, 302 `/auth/forgot-password`. |

`BaseController::requireAuth()` (used by all protected controllers): if `user_id` not in session, stores raw `REQUEST_URI` in session `intended_url`, flash `auth.login.required`, 302 `/auth/login`. Applies to GET and POST routes alike.

`BaseController::__construct` calls `Session::checkInactivity()` on every request: if `now - last_activity > SESSION_INACTIVITY_LIMIT` (env, default 1200 s) the session is destroyed.

### Validation (register)
- email: required; `FILTER_VALIDATE_EMAIL`. Not trimmed, not lowercased.
- name: required (`empty()`), no length limit (column is `VARCHAR(255)`), not trimmed (whitespace-only accepted).
- password: required; `strlen >= 8` (bytes, not characters); no maximum.
- password_confirm: must equal password.

### Login internals
- `UserRepository::authenticateUser`: `findBy(['email' => $email])` (exact `=`, email not normalised); unknown email → `password_verify` against a dummy hash, return null; else `password_verify`; on success sets `last_login = date('Y-m-d H:i:s')` and `save()`s the whole row (which also sets `updated_at`).
- `Auth::login`: puts the *previous* `last_login` into flash `last_login`, sets `user_id`, `session_regenerate_id(true)`.
- `Auth::check()` = "session has `user_id`" (touches `last_activity`). It does not check that the user row still exists.

### Password reset internals
- Token `bin2hex(random_bytes(32))` (64 hex chars), stored **plaintext** in `users.reset_token`, `reset_token_expires = now + 24h`. A new request overwrites the previous token.
- Reset mail link: `APP_URL/auth/reset-password/<token>`.

### Emails (all hard-coded English HTML, sent synchronously via PHPMailer)
- **Password reset** ("Password Reset Request"): greeting with name, button + raw link, "expires in 24 hours", placeholder `support@secretsanta.example.com`.
- **Welcome** ("Welcome to Secret Santa!"): sent to new accounts; called without the name, so it always says "Hello,"; link to `/user/dashboard`.
- **Existing account** ("Account Information"): sent when someone registers an already-registered email; link to `/auth/forgot-password`.
- `APP_DEBUG=true` (the `.env.template` default) skips SMTP and writes mails to `storage/emails/`. Send failures are swallowed (`return true`).

### Data
`users` (from `src/Config/schema.sql`): `id, email UNIQUE, password, name, created_at, updated_at ON UPDATE, last_login, reset_token, reset_token_expires, pending_email, email_verification_token, email_verification_expires`.

### History
Session-based login/registration rate limiting (with a "welcome back, last login {date}, {attempts} failed attempts" flash) existed and was removed in `dde3cc5` ("removing friction"). It was stored in the PHP session, so a client could bypass it by dropping its cookie.

## Problems in the PHP version

Security:
- **Login timing oracle.** The dummy hash `$2y$10$nevergonnagiveyouuppppppp$` is not a valid bcrypt hash, so `password_verify` returns immediately for unknown emails. Response time reveals whether an account exists.
- **Register enumeration is only cosmetic.** The response is identical, but an attacker can register `victim@x` with password P and then try logging in with P: success means the account was new. Registering also needs no proof of inbox ownership, so an attacker can pre-create an account under someone else's address ("pre-hijacking") and join groups as them.
- **Forgot-password timing oracle.** Existing email → DB write + synchronous SMTP (30 s timeout); unknown email → instant.
- **Open redirect after login.** `intended_url` is the raw `REQUEST_URI`. A request to `https://app//evil.example/groups` routes as `/groups` (PHP `parse_url` treats `//evil.example` as a host), stores `//evil.example/groups`, and after login sends `Location: //evil.example/groups`. Likely exploitable; not verified against Caddy's path handling.
- **No CSRF protection** anywhere: login CSRF (victim gets logged into the attacker's account), logout via a `GET` link.
- **No rate limiting / brute-force protection** on login, register or forgot-password (mail bombing).
- **Reset tokens stored in plaintext**, valid 24 h, and **logged**: `error_log` writes the full reset link and token prefixes; `SMTPDebug = DEBUG_SERVER` writes the full SMTP transcript (AUTH credentials and message bodies, so the reset links too) to the PHP error log.
- **Password reset does not end other sessions.** A thief with a live session stays logged in after the victim resets.
- **Session cookie flags never set**: PHP defaults (no `HttpOnly`, no `Secure`, no `SameSite`).
- **Error details leak**: `Router` echoes exception messages ("500 Internal Server Error: SQLSTATE..."), `public/index.php` always echoes message + stack trace for uncaught `Throwable`s. A duplicate-email race on register shows the SQL error.
- User names are interpolated into mail HTML without escaping.
- `AuthController::getClientIp()` (dead code) trusts spoofable `X-Forwarded-For` / `Client-IP` headers.

Bugs / odd behaviour:
- `last_login` is put into flash on login but never rendered (dead since `dde3cc5`).
- The register view calls `getFlash('old_input')` twice; the first call consumes it, so after a validation error the email field is never repopulated (only name is).
- Register validation messages are hard-coded English.
- "Remember me" checkbox does nothing.
- `Auth::check()` does not verify the user still exists: a deleted user's session counts as logged in, and the layout then fatals on `$auth->user()->getName()`.
- The inactivity check destroys the session but the cookie's id is reused on the next `session_start()`, so the id is not rotated after expiry.
- `.env` defines `INACTIVITY_LIMIT`, PHP reads `SESSION_INACTIVITY_LIMIT`, and compose passes neither, so the timeout is always 1200 s.
- `intended_url` is also stored for POST routes (redirecting to them later as GET), and never expires if the user doesn't log in.
- Reset POST with an empty token redirects to `/auth/reset-password/` → 404.
- Login rewrites the whole user row via `save()` (lost-update risk) and bumps `updated_at`.
- Welcome mail is always "Hello," (name not passed); mails are English-only; send failures are silently reported as success.
- Password length is counted in bytes (`strlen`); bcrypt silently ignores bytes beyond 72.

Out of scope here but noticed: `HomeController::setLanguage` redirects to the raw `Referer` (open redirect), for the i18n slice.

## Redesign

Summary of decisions:
1. **Two-step registration.** The register form asks only for the email. The server always answers "check your inbox". The mail contains either a completion link (new address) or an "you already have an account" notice. The account is created when the link is used, and only then are name and password chosen. This closes enumeration and pre-hijacking without a new column on `users`. A `next` path given at registration start is stored with the pending registration and applied after completion.
2. Forgot-password: identical response and timing for existing and unknown emails; mails are sent off the request path.
3. Login: constant-work verification (real argon2id dummy hash), server-side rate limit keyed on email and client IP, same error for unknown and wrong-password cases.
4. Intended URL travels as a validated `next` query parameter (relative path only), not in the session.
5. Session id is cycled on every login. All session data is deleted on logout. A per-user `session_epoch` invalidates every session of a user when their password is reset.
6. Passwords are hashed and verified with argon2id only.
7. Reset tokens are stored as SHA-256, valid 1 hour, single use.
8. URL paths stay under `/auth/*`. Logout is POST-only.
9. Emails are normalised (trim, lowercase) on every write and compared in normalised form.
10. No separate "remember me" control: all browser and PWA logins persist for 90 days of inactivity by default, renewed on every request (revised by #60 on 2026-11-03).

### Pages / routes

All pages are public. "Logged in" means `current_user()` returns a user (see the session contract below).

| URL | Page | Notes |
|---|---|---|
| `/auth/login?next=<path>` | Login | Logged-in visitor → redirect to `safe_next(next)` or the dashboard. If `next` is present and valid, shows info alert `auth.login.required`. |
| `/auth/register?next=<path>` | Register (step 1: email) | Logged-in visitor → redirect to dashboard. `next` is passed to the `register` server fn. |
| `/auth/register/confirm/:token` | Complete registration (step 2: name, password) | Invalid/expired token → redirect `/auth/register` with flash `flash.error.registration_link_invalid`. Accessible when logged in (completing logs in as the new user). |
| `/auth/forgot-password` | Forgot password | Accessible when logged in. |
| `/auth/reset-password/:token` | Reset password | Invalid/expired token → redirect `/auth/forgot-password` with flash `flash.error.reset_token_expired`. Accessible when logged in. |
| `GET /auth/logout` | not routed (404) | Logout is the `logout` server fn only. |

Token pages (`/auth/register/confirm/*`, `/auth/reset-password/*`) send `Referrer-Policy: same-origin` and `Cache-Control: no-store` (revised 2026-11-01: `no-referrer` makes browsers send `Origin: null` on a no-JS form POST, which the Origin check rejects; `same-origin` still never sends the token URL to another origin). Rendering them does not consume the token.

"Dashboard" means the shared constant `DASHBOARD_PATH` (currently `/user/dashboard`; the account slice owns the page).

#### Protected-page guard (contract for all slices)
- An anonymous **GET** of a protected page responds with a redirect to `/auth/login?next=<percent-encoded path+query of the request>`.
- A protected **server fn** called anonymously returns `AuthError::Unauthorized` (HTTP 401). The client then navigates to `/auth/login?next=<current location path+query>`.

#### `safe_next(raw: &str) -> Option<String>`
`safe_next(raw)` = i18n-foundation's `safe_return_path(raw)` (length 1..=512 bytes; starts with `/` and the second character is neither `/` nor `\`; no `\`, no ASCII control characters (incl. CR, LF, TAB), no spaces; not `/api` and not under `/api/`) **plus**: not `/auth` and not under `/auth/`, so no redirect loops into auth pages or server fn endpoints. `None` → the caller falls back to the dashboard. It lives next to `safe_return_path` in `app/src/paths.rs`.

The redirect target is exactly the validated string. Never prefix it with a host.

### Server fns

All are POST (Leptos server fns), protected by the foundation's Origin check. None requires a logged-in user. Inputs are never trusted for identity: the user is always resolved from the session or a token.

Common input normalisation:
- `email`: trim, then lowercase. Every email written to the DB (`users.email`, `pending_registrations.email`, and the account slice's `users.pending_email`) is in this normalised form, so lookups use `WHERE email = ?` with the normalised value and do not depend on the column collation. Valid iff ≤ 255 chars, exactly one `@`, non-empty local part and domain, domain contains a `.` not at either end, no whitespace or control characters. This is the shared `validate_email` used by the account slice.
- `name`: trim; 1..=100 characters (Unicode scalar values); no control characters (otherwise `NameInvalid` → `validation.name_invalid`). Shared validator for the account slice.
- `password`: not trimmed; 8..=256 characters (Unicode scalar values). `password_confirm` must be byte-equal. Shared policy for the account slice.

Error type (serialisable enum, each variant maps to one i18n key in the UI):

| Variant | i18n key |
|---|---|
| `Validation(Vec<(Field, FieldError)>)` | per field: `validation.email_required`, `validation.email_invalid`, `validation.name_required`, `validation.name_too_long` (`{max}`), `validation.name_invalid`, `flash.error.password_too_short`, `validation.password_too_long` (`{max}`), `flash.error.passwords_dont_match` |
| `MissingCredentials` | `flash.error.email_password_required` |
| `InvalidCredentials` | `flash.error.invalid_credentials` |
| `TooManyAttempts { minutes: u32 }` | `flash.error.too_many_attempts` (plural: `.one` / `.other`, rendered with `t_plural!(…, minutes, minutes = minutes)`) |
| `TooManyRequests` | `flash.error.too_many_requests` |
| `InvalidRegistrationToken` | `flash.error.registration_link_invalid` |
| `InvalidResetToken` | `flash.error.reset_token_expired` |
| `Unauthorized` | `auth.login.required` |
| `Internal` | `error.generic.message` (no internal detail ever reaches the client) |

#### `login(email, password, next: Option<String>) -> Result<(), AuthError>`
1. Empty email or password → `MissingCredentials` (does not count as a failure).
2. Normalise email. If the rate limiter blocks (`login:email:<email>` or `login:ip:<ip>`, see Rate limiting) → `TooManyAttempts { minutes }`. The password is not checked, even if correct.
3. Look up the user. Unknown email → verify the password against a process-wide argon2id dummy hash (computed once at startup with the production parameters), record a failure, return `InvalidCredentials`.
4. Verify the password against the stored argon2id hash. Mismatch → record a failure, return `InvalidCredentials`.
5. Success:
   - Read the previous `last_login`, then `UPDATE users SET last_login = CURRENT_TIMESTAMP WHERE id = :id`.
   - Clear the failure counter for `login:email:<email>` (the IP counter is left as is).
   - **Cycle the session id**, then store `user_id` and `session_epoch` (the user's current value).
   - **Adopt the account language**: `adopt_account_locale(&session, users.locale, config.secure_cookies())` (i18n-foundation); if it returns a cookie, `append_set_cookie` it. A NULL or unknown `users.locale` changes nothing (the page keeps its current locale and no `locale` cookie is set). The `locale` column is read with the user row in step 3.
   - Flash success: if previous `last_login` was non-null → `flash.success.logged_in_last_login` built with `with_datetime("date", previous_last_login)`, so it renders with `app::format` in the locale and `APP_TIMEZONE` of the page that shows it (en `1 Dec 2026, 11:00`, de `01.12.2026 11:00`); else `flash.success.logged_in`.
   - Redirect to `safe_next(next)` or the dashboard.

#### `logout() -> Result<(), AuthError>`
Idempotent, works anonymously. Deletes the session record and all its data (`flush`). Then, in a fresh session, sets flash `flash.success.logged_out`, and redirects to `/`. A replay of the old session cookie is anonymous.

#### `register(email, next: Option<String>) -> Result<(), AuthError>`
1. Validate email → `Validation`.
2. Per-IP limiter `mail:ip:<ip>` blocked → `TooManyRequests` (this depends only on the caller's IP, never on whether the account exists).
3. `DELETE FROM pending_registrations WHERE expires_at < CURRENT_TIMESTAMP`.
4. If `users` has the email: queue the **existing-account mail**. Else: generate a token (below), upsert `pending_registrations` by email (`token_hash`, `expires_at = now + 24h`, `next_path = safe_next(next)`, `created_at = now`), and queue the **registration mail** with link `APP_URL/auth/register/confirm/<token>`. An upsert replaces an earlier pending token (and its `next_path`) for the same email, so the earlier link stops working.
5. Mail queueing is subject to `mail:email:<email>`. When that limit is exceeded, the mail is silently not sent. No DB row is written in that case either (so the last delivered link stays valid).
6. Return `Ok(())`. The UI shows `flash.success.registration_instructions` and stays on a "check your inbox" state. Same response in every branch of step 4/5.

`next_path` is stored server-side, so it survives the mail link even when the link is opened in another browser or on another device. The link itself carries only the token.

#### `get_pending_registration(token) -> Result<PendingRegistration { email }, AuthError>`
Read-only. Looks up `token_hash = sha256(token)` with `expires_at > CURRENT_TIMESTAMP`, returns the email for display. Otherwise `InvalidRegistrationToken`.

#### `complete_registration(token, name, password, password_confirm) -> Result<(), AuthError>`
1. Validate name/password/confirm → `Validation` (token not consumed).
2. In one transaction: `SELECT ... FROM pending_registrations WHERE token_hash = ? AND expires_at > CURRENT_TIMESTAMP FOR UPDATE`. If there is no row → `InvalidRegistrationToken`. `INSERT INTO users (email, name, password, locale)` with an argon2id hash and the canonical code of the request locale (`request_locale()`), which becomes the account language. A duplicate-key error (the email was taken meanwhile, e.g. by another user's email change) → delete the pending row, commit, return `InvalidRegistrationToken`. Otherwise delete the pending row and commit.
3. Cycle the session id, store `user_id` and `session_epoch = 0`. Set `last_login = CURRENT_TIMESTAMP`.
4. Flash `flash.success.registration_complete` (`{name}`). Redirect to `safe_next(next_path)` (the value stored by `register`) or the dashboard.
No welcome mail (the registration mail replaces it).

#### `request_password_reset(email) -> Result<(), AuthError>`
1. Empty → `Validation(email_required)`. Malformed → `Validation(email_invalid)`. These are format checks only, the same for any address.
2. Per-IP limiter `mail:ip:<ip>` blocked → `TooManyRequests`.
3. If the user exists and `mail:email:<email>` allows: generate token, `UPDATE users SET reset_token = sha256hex(token), reset_token_expires = CURRENT_TIMESTAMP + INTERVAL 1 HOUR WHERE id = ?` (a newer request replaces an older token), queue the **password-reset mail** with link `APP_URL/auth/reset-password/<token>`.
4. Always `Ok(())`. The UI shows `flash.success.reset_instructions` and redirects to `/auth/login`.

#### `check_reset_token(token) -> Result<(), AuthError>`
Read-only validity check used by the reset page. Invalid/expired → `InvalidResetToken`.

#### `reset_password(token, password, password_confirm) -> Result<(), AuthError>`
1. Validate password/confirm → `Validation` (token not consumed).
2. Single statement: `UPDATE users SET password = :argon2, reset_token = NULL, reset_token_expires = NULL, session_epoch = session_epoch + 1 WHERE reset_token = :sha256hex AND reset_token_expires > CURRENT_TIMESTAMP`. If 0 rows are affected → `InvalidResetToken`. This makes the token single-use, also under concurrent submits.
3. Clear the limiter counter `login:email:<that user's email>`.
4. Does not log in. Flash `flash.success.password_reset`, redirect `/auth/login`. Because the epoch changed, every existing session of that user (including the caller's, if it is the same user) is now anonymous.

#### Tokens
32 bytes from the OS CSPRNG, encoded base64url without padding (43 chars) in URLs. At rest: lowercase hex SHA-256 of the token string (64 chars). Tokens never appear in logs. The account slice uses the same token helper for email verification.

#### Mail sending
- Mails are queued to a background task (`tokio::spawn`) *after* the DB work. The server fn returns without awaiting SMTP. SMTP failures are logged at `warn` with the mail kind and, where known, the user id. The email address, token and link are never logged. Responses never reveal a failure.
- Language (i18n-foundation F4): mails to an existing account (existing-account notice, password reset) use that account's `users.locale` via `mail_i18n` (NULL or unknown → `APP_LOCALE`), whatever the requester's page shows. The registration mail goes to an address without an account and uses the request locale (`I18n::new(request_locale(), config.app_timezone)`). The mail's HTML root has `lang="<mail locale>"`. HTML with a plain-text alternative. All interpolated values are HTML-escaped. Links are built from `APP_URL`. No placeholder support address.
- Dev/test: delivered to Mailpit (foundation).
- Mails:
  - **Registration**: subject `email.registration.subject`; explains the link, button + plain link, expires in 24 h, "ignore if you didn't request this".
  - **Existing account**: subject `email.existingAccount.subject`; "someone tried to register with this address; you already have an account", links to `/auth/login` and `/auth/forgot-password`, "ignore if it wasn't you".
  - **Password reset**: subject `email.passwordReset.subject`; greeting with the user's name, button + plain link, expires in 1 hour, "ignore if you didn't request this".

#### Rate limiting (proportionate for a small self-hosted instance)
In-memory, per process: a map of key → (count, window start) behind a mutex, with fixed windows and stale entries purged opportunistically. Counters are lost on restart, which is acceptable for a single container. The clock is injectable for tests. Limits are code constants:

| Key | Counts | Limit | Effect when exceeded |
|---|---|---|---|
| `login:email:<normalised email>` | failed logins (unknown email counts too) | 10 / 15 min | `TooManyAttempts { minutes until window end, rounded up }`; cleared on successful login or password reset |
| `login:ip:<ip>` | failed logins | 50 / 15 min (generous: families share one NAT IP) | `TooManyAttempts` |
| `mail:email:<normalised email>` | mails triggered by register + forgot-password for that address | 3 / 60 min | mail silently skipped, response unchanged |
| `mail:ip:<ip>` | register + forgot-password calls | 20 / 60 min | `TooManyRequests` |

The token submissions (`complete_registration`, `reset_password`) are not limited: 256-bit tokens can't be guessed.

Client IP. Deployment: Caddy is the edge proxy in front of the app; Caddy may itself sit behind another reverse proxy (Traefik, nginx). `TRUSTED_PROXIES_CIDR` (comma-separated CIDRs, default empty) must list every proxy hop in front of the app: Caddy's address and, if present, the outer proxy's address. Rule: start from the TCP peer address. If the peer is not inside `TRUSTED_PROXIES_CIDR`, it is the client IP and forwarding headers are ignored. Otherwise walk `X-Forwarded-For` from right to left, skip every address inside `TRUSTED_PROXIES_CIDR`, and take the first address that is not. If there is no such address (header missing, unparseable, or every entry trusted), use the peer address. Other headers (`X-Real-IP`, `Client-IP`, `Forwarded`) are ignored.

#### Session contract (with foundation's `current_user()` / `require_user()`)
- Session keys: `user_id: i32`, `session_epoch: u32`, plus flash.
- `current_user()` loads the user row by `user_id`. If the row is missing or `users.session_epoch != session.session_epoch`, it flushes the session and returns anonymous.
- Cookie: `HttpOnly`, `SameSite=Lax`, `Secure` when `APP_URL` is https; persistent cookie with rolling inactivity timeout `INACTIVITY_LIMIT` seconds (default 7,776,000 = 90 days). Every request renews both cookie Max-Age and server expiry, including reads without session mutations; normal browser and installed PWA follow the same policy. No absolute maximum is imposed while regularly used. Explicit logout deletes that session immediately; password reset/change still invalidate sessions through `session_epoch`. Existing deployments that explicitly set 1200 must update their configuration. Revised by #60 on 2026-11-03.
- Cross-slice: the account slice's "change password" must increment `session_epoch` and write the new value into the *current* session (others die, the current one stays).

### Data & migrations

Tables touched: `users` (read/write: `email`, `name`, `password`, `last_login`, `reset_token`, `reset_token_expires`, new `session_epoch`; `locale` read at login and for mails, written at registration completion; the column comes from the i18n-foundation migration), new `pending_registrations`.

New migration `rust/migrations/NNNN_auth.sql` (placeholder number: the next free one after `0001_baseline.sql`):

1. `ALTER TABLE users ADD COLUMN session_epoch INT UNSIGNED NOT NULL DEFAULT 0;`
2. ```sql
   CREATE TABLE pending_registrations (
       id INT AUTO_INCREMENT PRIMARY KEY,
       email VARCHAR(255) NOT NULL UNIQUE,
       token_hash CHAR(64) NOT NULL UNIQUE,
       next_path VARCHAR(512) NULL,
       created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
       expires_at TIMESTAMP NULL DEFAULT NULL
   );
   ```
   `expires_at` is declared `NULL DEFAULT NULL` on purpose. With `explicit_defaults_for_timestamp=OFF` (MariaDB < 10.10 default), a `TIMESTAMP NOT NULL` column without a default would silently get `ON UPDATE CURRENT_TIMESTAMP`. Code always writes `expires_at`; a NULL row is treated as expired.

From the baseline, unchanged: `users.reset_token` (`VARCHAR(255)`) holds a SHA-256 hex. `password` (`VARCHAR(255)`) fits argon2id PHC strings. Always name columns in queries. `pending_email` / `email_verification_*` belong to the account slice. `users.email` and `pending_registrations.email` only ever hold normalised (trimmed, lowercase) addresses.

### UI notes

Reuse the existing Bootstrap card layout. Errors render inline: a page-level alert for non-field errors, `invalid-feedback` under fields for `Validation`. Entered email/name are preserved after an error; password fields are cleared.

- **Login**: email, password, submit; links "forgot password" and "register" (both carry `next` through). No "remember me". Reuses `auth.login.title|email|password|submit|forgotPassword|noAccount`, `auth.register`, `auth.login.required`.
- **Register (step 1)**: email + submit. After success, replace the form with the `flash.success.registration_instructions` message. Link to login (carries `next` through). Reuses `auth.register.title|email|submit|haveAccount`, `auth.login`. New `auth.register.description`.
- **Complete registration (step 2)**: shows `auth.registerConfirm.intro` with `{email}`; fields name, password, confirm. Reuses `auth.register.name|password|passwordRequirements|confirmPassword`. New `auth.registerConfirm.title`, `auth.registerConfirm.submit`.
- **Forgot password**: unchanged content; reuses `auth.forgotPassword.*`.
- **Reset password**: password + confirm; reuses `auth.resetPassword.*`.
- **Nav**: logged in → user name + logout button (a form posting to `logout`); anonymous → login/register links. Reuses `auth.logout`, `auth.login`, `auth.register`.

i18n keys (en + de for each):
- Reused unchanged (copied from the PHP catalogs when the code that uses them is added; i18n-foundation rule): listed above, plus `flash.error.email_password_required`, `flash.success.logged_in`, `flash.error.invalid_credentials`, `flash.success.logged_out`, `flash.success.reset_instructions`, `flash.error.reset_token_expired`, `flash.error.passwords_dont_match`, `flash.error.password_too_short`, `flash.success.password_reset`. Already in the catalogs (foundation): `error.generic.message`.
- Reused, text changed: `flash.success.registration_instructions` → en "Check your inbox: we have sent you an email to continue." (de accordingly); `auth.register.passwordRequirements` / `auth.resetPassword.passwordRequirements` → "8 to 256 characters."
- New: `auth.register.description`, `auth.registerConfirm.title`, `auth.registerConfirm.intro` (`{email}`), `auth.registerConfirm.submit`, `flash.success.registration_complete` (`{name}`), `flash.error.registration_link_invalid`, `flash.success.logged_in_last_login` (`{date}`; en "You have successfully logged in. Your last login was on {date}."), `flash.error.too_many_attempts.one` / `.other` (`{minutes}`; en "Too many failed login attempts. Please try again in {minutes} minute." / "… in {minutes} minutes.", de "Zu viele fehlgeschlagene Anmeldeversuche. Bitte versuchen Sie es in {minutes} Minute erneut." / "… in {minutes} Minuten erneut."), `flash.error.too_many_requests`, `validation.email_required`, `validation.email_invalid`, `validation.name_required`, `validation.name_too_long` (`{max}`), `validation.password_too_long` (`{max}`), and mail keys `email.common.linkFallback`, `email.common.ignore`, `email.common.signature`, `email.registration.{subject,heading,body,button,expires}`, `email.existingAccount.{subject,heading,body,loginButton,resetButton}`, `email.passwordReset.{subject,greeting,body,button,expires}`.
- Not ported (never added to the catalogs): `auth.login.remember`, `flash.error.all_fields_required`, `flash.error.invalid_reset_token`, `flash.error.enter_email` (replaced by `validation.email_required`).
- Heads-up for the i18n foundation: the PHP JSON is flat and has both `auth.login` and `auth.login.title`, so a nested key loader would collide.

### Decision log entries (to add to PLAN.md on approval)
- `auth | two-step registration (email → link → name+password) | closes enumeration and account pre-hijacking without touching users columns`
- `auth | intended URL as validated ?next= relative path; stored in pending_registrations across the registration mail | no session state, open-redirect safe, invite links survive registration`
- `auth | users.session_epoch invalidates all sessions on password reset | tower-sessions records can't be queried by user`
- `auth | in-memory rate limiter keyed on email + client IP | single instance; PHP's cookie-based limiter was bypassable and was removed`
- `auth | reset token SHA-256 at rest, 1 h, single use; mails sent off the request path | enumeration via timing, token leakage`
- `auth | emails lowercased on write, compared normalised | no dependency on column collation`

## Acceptance criteria

Fixtures: user **Alice** (`alice@example.com`, name "Alice", password `correct horse 1`, argon2id hash, `last_login` NULL); user **Bob** (`bob@example.com`, password `bobpassword1`, argon2id hash, `last_login = 2026-12-01 10:00:00 UTC`). Mail assertions read Mailpit. Rate-limit ACs use the injectable clock.

Login
- AC1: Given Alice, when she logs in with `ALICE@Example.com ` (case and trailing space) and the right password, then she lands on the dashboard and sees `flash.success.logged_in`.
- AC2: Given Bob (`users.locale` NULL) on an English page, when he logs in with `bobpassword1`, then login succeeds and the flash is `flash.success.logged_in_last_login` with date `1 Dec 2026, 11:00` (Europe/Berlin). Given Bob's `users.locale = 'de'`, then the dashboard is German and the flash shows `01.12.2026 11:00`.
- AC3: When logging in with an unknown email, or with Alice's email and a wrong password, then both return `InvalidCredentials` with an identical serialized response, and no session cookie with `user_id` is issued.
- AC4: When `login` is called with empty email or empty password, then `MissingCredentials` is returned and no failure is counted.
- AC5: Given a successful login, then `users.last_login` is set to the current time (UTC) and the session id cookie value differs from the one the client had before login (session fixation).
- AC6: Given an anonymous client that requests `GET /groups`, then it is redirected to `/auth/login?next=%2Fgroups`; the login page shows `auth.login.required`; after logging in the browser is at `/groups`.
- AC7: For each `next` in `//evil.example/x`, `/\evil.example`, `https://evil.example`, `javascript:alert(1)`, `/%0d%0aSet-Cookie:x` decoded to contain CR/LF, `/auth/login`, `/api/login`, and a 600-byte path: when logging in with that `next`, then the redirect goes to the dashboard. With `next=/groups/3?tab=wishlist` it goes exactly there.
- AC8: Given Alice is logged in, when she opens `/auth/login` or `/auth/register`, then she is redirected to the dashboard.
- AC9: When the `login` server fn is POSTed with an `Origin` header of a foreign site, then it is rejected (403) and no session is created (login CSRF).
- AC10: Given a successful login, then the session cookie has `HttpOnly` and `SameSite=Lax` (and `Secure` when `APP_URL` is https). The login page has no "remember me" control.

Rate limiting
- AC11: Given 10 failed logins for `alice@example.com` within 15 minutes, when the 11th attempt uses the correct password, then `TooManyAttempts { minutes }` is returned with `1 <= minutes <= 15`, and no session is created.
- AC12: Given 10 failed logins for an unknown email, then the 11th attempt returns the same `TooManyAttempts` response shape as AC11 (no enumeration via lockout).
- AC13: Given AC11's lockout, when the clock advances past the window, then Alice can log in with the correct password.
- AC14: Given 9 failed logins for Alice followed by one successful login, then the counter is reset and 10 further failures are needed to lock again.
- AC15: Given 50 failed logins from one client IP spread over different emails within 15 minutes, then the 51st attempt from that IP returns `TooManyAttempts`, while the same attempt from another IP proceeds normally.
- AC16: Given `TRUSTED_PROXIES_CIDR` contains the peer address and the request has `X-Forwarded-For: 203.0.113.9, <peer>`, then the limiter keys on `203.0.113.9`. Given `TRUSTED_PROXIES_CIDR` contains both the peer (Caddy) and an outer proxy address `10.0.0.5`, and the request has `X-Forwarded-For: 198.51.100.7, 203.0.113.9, 10.0.0.5`, then the limiter keys on `203.0.113.9` (the spoofable left part is not used). Given the peer is not trusted, a spoofed `X-Forwarded-For` is ignored and the peer address is used.

Logout
- AC17: Given Alice is logged in, when she clicks logout, then she is on `/` with `flash.success.logged_out`, and replaying her previous session cookie against a protected page redirects to login.
- AC18: When `GET /auth/logout` is requested, then the response is 404 and an existing session stays logged in.

Session lifecycle
- AC19: Given Alice is logged in and idle for longer than `INACTIVITY_LIMIT` (test config: small value), when she opens a protected page, then she is redirected to `/auth/login?next=...`.
- AC19a (#60): With default configuration, login and registration completion issue persistent 90-day cookies; opening the app in a fresh browser context with stored cookies or after a server restart keeps the login.
- AC19b (#60): A request before expiry renews the persisted session expiry and cookie Max-Age to the full configured period, even without a session mutation. An expired session remains anonymous.
- AC19c (#60): Logging out one device immediately invalidates its old cookie while another device remains signed in.
- AC20: Given Alice is logged in and her `users` row is deleted, when she opens any page, then she is treated as anonymous (no error page).

Registration
- AC21: When registering ` Carol@Example.COM `, then the UI shows `flash.success.registration_instructions`, a `pending_registrations` row exists for `carol@example.com` with a 64-char `token_hash` that differs from the token in the mail, and Mailpit has one registration mail to `carol@example.com` with a link `APP_URL/auth/register/confirm/<43-char token>`. No `users` row is created.
- AC22: When registering `Alice@example.com` (existing), then the UI response is byte-identical to AC21's, no `pending_registrations` row is created, and Mailpit has one existing-account mail to Alice containing links to `/auth/login` and `/auth/forgot-password`.
- AC23: For each invalid email (`""`, `"carol"`, `"carol@"`, `"@example.com"`, `"carol@example"`, `"ca rol@example.com"`, 256+ chars), register returns `Validation` with the matching `validation.email_*` key and no mail is sent.
- AC24: Given AC21's mail, when Carol opens the link, then the page shows her email (`auth.registerConfirm.intro`). Opening it twice does not consume the token.
- AC25: Given the confirmation page, when Carol submits name `  Carol  `, password `carolpass1` twice, then a `users` row with email `carol@example.com`, name `Carol` and an `$argon2id$` hash exists, the pending row is gone, she is logged in on the dashboard with `flash.success.registration_complete`, and `last_login` is set.
- AC26: Given the confirmation page, when Carol submits an empty/whitespace name, a 101-char name, a 7-char password, a 257-char password, or mismatching confirmation, then `Validation` with the corresponding key is returned, no user is created, and the link still works afterwards.
- AC27: Given a registration link older than 24 hours, or one already used, or one superseded by a later register call for the same email, when it is opened, then the browser is redirected to `/auth/register` with `flash.error.registration_link_invalid`; `complete_registration` with that token returns `InvalidRegistrationToken`.
- AC28: Given a pending registration for `carol@example.com`, when a `users` row with that email appears before Carol completes, then `complete_registration` returns `InvalidRegistrationToken`, the pending row is deleted, and no duplicate user exists.
- AC29: Given an anonymous visit to `/join/ABCDEFGH` that was redirected to login, when the user follows "register" (carrying `next`), registers and completes via the mail link, then they end on `/join/ABCDEFGH`.
- AC30: Given `register("dave@example.com", next = Some("/join/ABCDEFGH"))`, then the pending row has `next_path = "/join/ABCDEFGH"`; when the mail link is opened in a fresh browser context (no cookies from the register request) and `complete_registration` succeeds there, then the redirect goes to `/join/ABCDEFGH`. Given `register("erin@example.com", next = Some("//evil.example/x"))`, then `next_path` is NULL and completion redirects to the dashboard.
- AC31: Given 3 register calls for `carol@example.com` within an hour, when a 4th is made, then the UI response is unchanged, no 4th mail is sent, and the link from the 3rd mail still works.
- AC32: Given 20 register/forgot-password calls from one IP within an hour, then the 21st returns `TooManyRequests`, regardless of whether the email exists.

Password reset
- AC33: When requesting a reset for Alice, then the UI shows `flash.success.reset_instructions` and goes to `/auth/login`; Alice's `reset_token` is a 64-char hex that is not the token in the mail; `reset_token_expires` is about 1 hour ahead; the mail (in Alice's account locale; `users.locale` NULL → `APP_LOCALE`) contains `APP_URL/auth/reset-password/<token>` and Alice's name.
- AC34: When requesting a reset for an unknown email, then the UI response is byte-identical to AC33's and no mail is sent.
- AC35: Given SMTP is unreachable, when requesting a reset for Alice, then the response is still the success response from AC33 and returns without waiting for the SMTP timeout (e.g. within 1 s).
- AC36: Given a valid reset link, when Alice sets `newpassword1` twice, then she is redirected to `/auth/login` with `flash.success.password_reset`; logging in with `newpassword1` works and `correct horse 1` returns `InvalidCredentials`; `reset_token` and `reset_token_expires` are NULL; the stored hash is argon2id.
- AC37: Given Alice is logged in on browser A, when she resets her password via browser B, then browser A's next request to a protected page redirects to login (`session_epoch` incremented).
- AC38: Given a used reset link, an expired one (> 1 hour), a superseded one (a newer reset was requested), or a random token, when it is opened, then the browser is redirected to `/auth/forgot-password` with `flash.error.reset_token_expired`; `reset_password` with it returns `InvalidResetToken` and the password is unchanged.
- AC39: Given a valid reset link, when the form is submitted with a 7-char password or mismatching confirmation, then `Validation` is returned and the link still works.
- AC40: Given Alice is locked out by AC11, when she completes a password reset, then she can log in immediately with the new password.
- AC41: Given two concurrent `reset_password` calls with the same valid token, then exactly one succeeds and the other returns `InvalidResetToken`.

Mail and logging
- AC42: Given a user named `<b>Eve</b>`, when her reset mail is sent, then the HTML part contains `&lt;b&gt;Eve&lt;/b&gt;` and no `<b>` tag from the name.
- AC43: Given the German UI locale, when registering a new address, then the registration mail's subject and body come from the `de` messages and its HTML root has `lang="de"`. (Reset and existing-account mails follow the account locale instead: AC49, AC50.)
- AC44: During AC21, AC33 and AC36 (with `RUST_LOG=debug`), the application log contains neither the token, the link, nor the plaintext password.
- AC45: Given a DB error during any auth server fn, then the client receives `Internal` and the UI shows `error.generic.message` without SQL or stack details.
- AC46: When `complete_registration` is called with a name containing a control character (e.g. `"Ca\nrol"`), then `Validation` contains `(name, validation.name_invalid)` and no user is created.

Account language (added 2026-10-31, i18n-foundation)
- AC47: Given Alice's `users.locale = 'de'` and a browser with cookie `locale=en`, when she logs in, then the dashboard is German, the login response sets `locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly` (plus `Secure` with an https `APP_URL`), and after logout the page is still German. Given Bob's `users.locale` is NULL and his browser shows English, when he logs in, then the dashboard is English and the login response sets no `locale` cookie.
- AC48: Given a visitor whose page is German (cookie `locale=de`), when they complete registration, then the new `users` row has `locale = 'de'` and the dashboard is German. Given a visitor with no cookie and `Accept-Language: en-US` on an instance with `APP_LOCALE=de`, then the new row has `locale = 'en'`.
- AC49: Given Alice's `users.locale = 'de'`, when someone registers `alice@example.com` from an English page, then the existing-account mail is German.
- AC50: Given Alice's `users.locale = 'de'`, when a reset for Alice is requested from an English page, then the reset mail is German. Given Bob's `users.locale` is NULL and `APP_LOCALE=en`, when a reset for Bob is requested from a German page, then the reset mail is English. Given a stored `locale = 'fr'` (written directly), the mail uses `APP_LOCALE`.
- AC51: Given the lockout of AC11 with 1 minute left, then the English message reads "… Please try again in 1 minute." and with 2 or more minutes "… in {n} minutes."; in German "… in 1 Minute erneut." / "… in {n} Minuten erneut."

## Open questions

None.

## Resolved (2026-10-31, user)

1. **Registration flow** (two-step vs. single-step with immediate login): two-step. A `next=` path supplied at registration start survives the registration mail link and is applied after completion (stored in `pending_registrations.next_path`; AC29, AC30). The groups invite flow depends on it.
2. **"Remember me"**: separate control dropped. Revised by #60 (2026-11-03): every login now has a persistent, rolling 90-day default lifetime for everyday and PWA use; no short-lived mode.
3. **Link lifetimes**: reset link 1 hour, registration link 24 hours.
4. **Rate limiter**: in-memory, with the proposed limits (10 failed logins per email / 50 per IP per 15 min; 3 mails per address and 20 register/forgot calls per IP per hour).
5. **Production `users` table shape** (collation, `failed_login_attempts`, case-duplicate emails): moot, there is no production DB to migrate. Emails are normalised to lowercase on write and compared lowercased.
6. **Proxy topology for client IP**: Caddy is the edge proxy in front of the app and may itself sit behind Traefik or nginx. The app reads the client IP by walking `X-Forwarded-For` from the right and skipping hops in `TRUSTED_PROXIES_CIDR` (AC16). The PHP env-name mismatch (`TRUSTED_PROXY_CIDR` in the Caddyfile vs. `TRUSTED_PROXIES_CIDR` in `.env.template`) is fixed in the deploy config (PLAN.md risks).
