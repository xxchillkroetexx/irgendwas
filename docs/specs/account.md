---
slice: account
status: approved
---
# Account

Dashboard shell, profile (name), change password, email change with verification. The `/api/user/:id` endpoint is removed.
Login, logout, registration and password reset are covered by the `auth` slice. This spec only relies on its `require_user()` guard, its shared validators and its token helper.

Fresh start: no production data is migrated and users register again. The Rust app owns its schema; the PHP schema is only a reference for shapes and names.

Revised 2026-10-31: aligned with i18n-foundation (built before auth). Both mails use the account language (`users.locale` via `mail_i18n`) instead of the request locale; the pending-change expiry is formatted with `app::format`. AC27 was revised; AC38 and AC39 were added.
Revised 2026-11-01 (review): current-password attempts are rate limited (AC40); no-JS confirm works and token pages use `Referrer-Policy: same-origin` (AC41); `no-store` on profile/verify (AC42); per-recipient mail limit (AC43); no-JS profile forms show their result as a flash (AC44).
Amended 2026-11-09 (#71, credential races): a password change (and auth's password reset) also clears a pending email change, so a change started with the old credentials can no longer be confirmed; `request_email_change` re-checks the verified hash and `session_epoch` under its row lock and answers `Unauthorized` if they changed.

## Current PHP behaviour

PHP reference: `src/Controllers/UserController.php`, `src/Views/user/dashboard.php`, `src/Views/user/profile.php`, `Auth::requestEmailChange/verifyEmailChange`, `UserRepository::initiateEmailChange/verifyEmailChange/findByPendingEmail/findByEmailVerificationToken`, `EmailService::sendEmailVerification`, `GroupRepository::findByUserId`, `GiftAssignmentRepository::findByGiverId/loadReceiver`, `src/Views/layouts/main.php` (nav), `public/assets/js/app.js`.

### Routes

| Method | Path | Handler | Auth | What it does |
|---|---|---|---|---|
| GET | `/user/dashboard` | `dashboard` | `requireAuth` | Welcome + groups card + assignments card + static tips card |
| GET | `/user/profile` | `showProfile` | `requireAuth` | Two forms (name+email, password), both POST to `/user/profile` |
| POST | `/user/profile` | `updateProfile` | `requireAuth` | Password change, name change and email change request, all in one handler |
| GET | `/user/verify-email/:token` | `verifyEmail` | **none** | Applies the pending email change (a state change on GET), then redirects to `/user/profile` |
| GET | `/api/user/:id` | `apiGetUser` | `requireAuth` | JSON `{id,name,email}` if `:id == session user_id`, else 403 JSON; 404 JSON if missing |

`requireAuth` stores `REQUEST_URI` as `intended_url`, sets the flash `auth.login.required` and redirects to `/auth/login`. For `/api/user/:id` this means an anonymous caller gets an HTML redirect instead of a 401. `HomeController::index` redirects logged-in users from `/` to `/user/dashboard`. Login and register redirect there too.

### Dashboard (`GET /user/dashboard`)
- Heading `user.dashboard.welcome` with `{name}` (escaped), subtitle `user.dashboard.subtitle`.
- **Groups card**: `GroupRepository::findByUserId` (JOIN `group_members`, `ORDER BY g.created_at DESC`). Each row links to `/groups/{id}`, has a Drawn/Not drawn badge (`group.status.*`) and an "Admin" badge if `admin_id == me`. The card also has "Join a Group" (`/groups/join`) and "Create a Group" (`/groups/create`) buttons, plus the empty text `user.dashboard.groups.empty`.
- **Assignments card**: `findByGiverId(me)`, so only the caller's own assignments. The group and receiver are loaded per row (N+1). Each row shows the group name and the receiver name, with a link to `/wishlist/view/{receiverId}/{groupId}`. Empty texts are `user.dashboard.assignments.empty` and `.emptyDescription`. Rows are not filtered by current membership.
- **Tips card**: three static strings `user.dashboard.tips.*`.
- Unused keys: `user.dashboard.quickActions.*`, `user.dashboard.groups.viewAll`.

### Profile page (`GET /user/profile`)
- Form 1: `name` (required) and `email` (type=email, required). If `pending_email` is set, it shows the `.alert` `user.profile.emailPendingVerification {email}`, even after the token has expired. Otherwise it shows the hint `user.profile.emailChangeNotice`.
- Form 2: `current_password`, `new_password` (minlength 8), `password_confirm`.
- Link back to the dashboard. The key `user.profile.emailNotice` ("Email address cannot be changed.") is unused.

### `POST /user/profile` (`updateProfile`), in order
1. If `current_password` **and** `new_password` are non-empty (PHP `empty`, so `"0"` counts as empty):
   - `password_verify` fails → flash error "Current password is incorrect" (hard-coded English) → redirect.
   - `strlen(new) < 8` → "New password must be at least 8 characters long".
   - `new !== confirm` → "New passwords do not match".
   - Otherwise it sets the bcrypt hash on the in-memory user (not saved yet).
   - If only one of the two fields is filled, the change is skipped without a message and the request ends with "Profile updated successfully".
2. If `name` is non-empty and differs, it is set in memory. An empty name is ignored. Whitespace-only names are accepted. There is no length limit (VARCHAR 255).
3. If `email` is non-empty and `!==` the current email (case-sensitive compare, no trim):
   - `FILTER_VALIDATE_EMAIL` fails → "Invalid email address" → redirect (the name change is lost).
   - `requestEmailChange`: if another user has this `email`, **or another user has it as `pending_email`**, it returns false → "Email is already in use" (the name change is lost). Otherwise `initiateEmailChange` generates `bin2hex(random_bytes(32))`, stores it **in plaintext** with expiry +24h (PHP local time) and `save()`s the user. That full-row save also persists the name change from step 2. It then sends the verification mail and flashes "Email verification sent". `sendEmail` always returns true, so the "in use" message on mail failure can never happen.
   - Redirect. Step 4 is skipped.
4. `save($user)` (full-row UPDATE of every column) → "Profile updated successfully" → redirect.

The i18n keys `flash.success.profile_updated`, `flash.success.email_verification_sent`, `flash.error.email_already_in_use`, `flash.success.email_verified`, `flash.error.email_verification_failed`, `flash.error.passwords_dont_match` and `flash.error.password_too_short` exist in en/de but are **not used**. All messages are hard-coded English.

### `GET /user/verify-email/:token`
`findByEmailVerificationToken(token)` (plaintext compare) → fail if not found, expired, no `pending_email`, or `pending_email` already used as `email` by another user. On success: `email = pending_email` and the three pending columns are set to NULL. Flash "Email verified successfully" / "Email verification failed" → redirect to `/user/profile`. That page requires login, so a logged-out user ends up on the login page with both flashes. No login is required to verify. No session is affected, the old address is not notified, and `reset_token` is not cleared.

### Email sent
`sendEmailVerification(user, newEmail, token)` goes to **the new address**. Subject "Verify Your New Email Address" (English only). The HTML body interpolates `{$user->getName()}` and `{$newEmail}` **without escaping**. It contains the link `APP_URL/user/verify-email/{token}` and says the link expires in 24 h. Before sending, `error_log` writes the full verification link (with token) to the log. With `APP_DEBUG=true` the mail is written to `storage/emails/` instead of being sent.

### `/api/user/:id` usage
Nothing calls it: `public/assets/js/app.js` only initializes Bootstrap tooltips/popovers and auto-dismisses alerts. No view uses `fetch`/XHR. It only returns the caller's own id/name/email, so there is no cross-user leak. Comparison is the loose `$id != userId()`.

## Problems in the PHP version
- **Critical: CSRF → account takeover.** `POST /user/profile` has no CSRF token and email change needs no password. A cross-site form can submit `email=attacker@evil` for a logged-in victim. The attacker gets the verification link, confirms it, and then uses "forgot password" on the now-attacker-owned address.
- Email change does not require the current password. A briefly hijacked session is enough to take the account over.
- The verification link changes state on GET.
- Verification tokens are stored in plaintext, and the full verification link is written to the error log in every environment.
- HTML injection in the verification mail: the user-controlled name and the new address are interpolated without escaping. The mail goes to an arbitrary address chosen by the requester, and there is no throttle, so it can be used to send spam or phishing from the app's domain.
- Account/pending-address enumeration: "Email is already in use" reveals that an address is registered **or that another user has it pending**.
- The combined name+email form loses the name change when the email change is rejected, but saves it when the email change succeeds (only as a side effect of `save()`).
- Name validation: an empty name is ignored without a message ("Profile updated successfully" is still shown), whitespace-only names are accepted, and there is no max length.
- Email: no trim; a case-sensitive "changed?" check.
- Password: a half-filled form is skipped without a message and still reports success. Changing the password keeps all other sessions alive, does not regenerate the session id and does not clear outstanding `reset_token`s.
- Completing an email change does not notify the old address and does not clear an outstanding password-reset token that was sent to the old address.
- No way to cancel a pending change. The "pending" banner is shown even after expiry, and `app.js` auto-dismisses it after 5 s (it lacks `alert-permanent`).
- `sendEmail` always returns true, so mail failures are never reported. If the failure branch were reachable, it would show the wrong message ("Email is already in use").
- Flash messages are hard-coded English although matching i18n keys exist.
- Full-row `save()` overwrites every column (lost updates between concurrent requests).
- Dashboard: N+1 queries. The tip "Invite friends and family using their email" describes a feature that does not exist (invites are by code).
- `/api/user/:id`: dead code. Anonymous callers get an HTML redirect instead of a 401.
- After a successful verification, a logged-out user is sent to `/user/profile`, which redirects to login with an extra "please log in" error.

## Redesign

### Decisions
- D1: **URLs stay** `/user/dashboard`, `/user/profile` and `/user/verify-email/:token`. The auth slice uses `/user/dashboard` as its post-login target (`DASHBOARD_PATH`).
- D2: **`/api/user/:id` is removed.** Nothing uses it, and server fns replace it. The path falls through to the normal 404 page.
- D3: The profile has **three independent forms/server fns**: name, email, password. No combined handler.
- D4: An **email change requires the current password**. It is **non-enumerating**: the request never says whether the address is taken. The check happens at confirm time and is only visible to whoever holds the token (the owner of that mailbox).
- D5: Verification is **GET page + POST confirm**. The GET renders a confirmation page and changes nothing. The button calls `confirm_email_change(token)`. This also protects against mail-scanner link prefetching. Confirming needs no login, because the token proves control of the new mailbox.
- D6: Tokens use the auth slice's token helper: 32 bytes from a CSPRNG, sent as 43 base64url characters without padding. Only `hex(sha256(token))` is stored in `users.email_verification_token`. Valid 24 h. Single use. One pending change per user: a new request replaces the old one.
- D7: **Throttle**: a new email-change request is rejected if the current pending request was issued less than 60 s ago. Issue time = `email_verification_expires - 24h`, so no new column is needed.
- D8: **Changing the password invalidates every other session** of the user, keeps the current one (with a new session id), and clears any outstanding password-reset token and any pending email change (amended 2026-11-09, #71). **Changing the email does not invalidate sessions**: credentials don't change, and requesting the change already required the password. Completing an email change clears any outstanding password-reset token and sends a notice to the **old** address.
- D9: Session invalidation uses a per-user counter `users.session_epoch`. It is stored in the session at login, and `require_user()` rejects sessions whose stored value differs from the DB. The column and the check are owned by the `auth` slice; this slice only increments it on password change.
- D10: Dashboard ownership: this slice builds the page shell (welcome, subtitle, tips). The groups card belongs to the **groups** slice (3). The assignments card belongs to the **draw** slice (6). See UI notes for the contract.
- D11: Never log tokens or verification links. Mail bodies HTML-escape every interpolated value and include a plain-text alternative.
- D12: All DB writes are targeted `UPDATE … SET <only the touched columns>` statements. `updated_at` is left to `ON UPDATE CURRENT_TIMESTAMP`.
- D13: Emails are normalised (trim, lowercase) before every write and every comparison, the same rule as the auth slice. `pending_email` and `email` therefore always hold lowercase addresses.

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/user/dashboard` | Dashboard | `require_user()`; anonymous → login redirect with return URL (auth slice behaviour) |
| `/user/profile` | Profile: name, email, password | `require_user()` |
| `/user/verify-email/:token` | Confirm email change | Anyone (logged in or not) |
| `/api/user/:id` | removed → 404 | – |

`/` → `/user/dashboard` for logged-in users is owned by auth/foundation. It is listed here only as the entry point.

### Server functions
All are POST (Leptos default) and go through the foundation's Origin check. "Auth" means `require_user()`: without a valid session the fn returns `AppError::Unauthorized` and changes nothing. The user id always comes from the session, never from arguments. The error names below are normative for tests. They are variants of the shared app error type, and each maps to an i18n key.

| Fn | Auth | Result | Errors (checked in this order; first failure wins) |
|---|---|---|---|
| `get_profile() -> ProfileView` | yes | `{ name, email, pending_email: Option<String>, pending_expires_at: Option<DateTime<Utc>> }`. Pending fields are `None` if no pending change exists **or it has expired** | Unauthorized |
| `update_name(name: String) -> ()` | yes | Stores `trim(name)` | Unauthorized, NameRequired (empty after trim), NameInvalid (control characters), NameTooLong (> 100 chars) |
| `change_password(current_password, new_password, new_password_confirm) -> PasswordChanged` | yes | See below. `PasswordChanged { pending_email_cancelled: bool }` (amended 2026-11-09, #71) | Unauthorized, PasswordTooShort, PasswordTooLong, PasswordsDontMatch, CurrentPasswordIncorrect |
| `request_email_change(new_email, current_password) -> ()` | yes | See below | Unauthorized, InvalidEmail, EmailUnchanged, CurrentPasswordIncorrect, EmailChangeTooSoon, MailSendFailed |
| `cancel_email_change() -> ()` | yes | Sets `pending_email`, `email_verification_token` and `email_verification_expires` to NULL. Idempotent (Ok if nothing is pending) | Unauthorized |
| `confirm_email_change(token: String) -> ConfirmResult` | **no** | `{ logged_in: bool }`, so the page can offer "Go to profile" or "Log in" | InvalidOrExpiredToken, EmailTaken |

**Validation rules**
- Name: the auth slice's shared name validator: trim leading/trailing whitespace; 1..=100 Unicode scalar values after trim; no control characters (otherwise NameInvalid → `flash.error.name_invalid`). Empty after trim → NameRequired; more than 100 characters → NameTooLong.
- Email: the auth slice's shared `validate_email` after normalising (trim, then lowercase): at most 255 chars, no whitespace or control characters, exactly one `@`, non-empty local part and domain, domain contains a `.` not at either end. The normalised value is what gets stored. Since every stored email is lowercase and every input is lowercased first, plain `=` comparisons are case-insensitive regardless of collation.
- Password: the auth slice's policy: 8..=256 Unicode scalar values, not trimmed, no other rules. The confirm field must match exactly. The new password may equal the old one.

**`change_password`**
1. Validate length (too short, then too long), then the confirm match, then verify `current_password` against the stored argon2id hash.
2. In one statement: `password = argon2id(new)`, `session_epoch = session_epoch + 1`, `reset_token = NULL`, `reset_token_expires = NULL`, and the three pending email columns `= NULL` (amended 2026-11-09, #71). `pending_email_cancelled` is whether a live (unexpired) pending change existed under the row lock just before.
3. Write the new `session_epoch` into the current session and cycle the session id. Every other session of the user fails `require_user()` on its next request.

**`request_email_change`**
1. Normalise and validate `new_email`. If it equals the current `email` → EmailUnchanged.
2. Verify `current_password` → CurrentPasswordIncorrect.
3. Under `SELECT ... FOR UPDATE` on the user row: if `password` or `session_epoch` differ from the values the password was verified against (a reset/change committed meanwhile) → Unauthorized, nothing written (amended 2026-11-09, #71). If a pending change exists, is not expired, and `email_verification_expires - 24h > now - 60s` → EmailChangeTooSoon. The existing pending change is not modified.
4. **No** check whether another account uses the address or has it pending (D4).
5. Generate the token. In one statement: `pending_email = new`, `email_verification_token = hex(sha256(token))`, `email_verification_expires = now + 24h` (UTC). Any earlier token is overwritten.
6. Send the verification mail to `new_email` in Alice's account language (`mail_i18n(users.locale, config)`; read `locale` together with the password hash in step 2). If sending fails, restore the three columns to their values before step 5 and return MailSendFailed.

**`confirm_email_change(token)`**
1. If `token` is not exactly 43 characters from the base64url alphabet (`A–Z`, `a–z`, `0–9`, `-`, `_`) → InvalidOrExpiredToken (no DB lookup).
2. Find the user with `email_verification_token = hex(sha256(token))` and `pending_email IS NOT NULL`. If none, or `email_verification_expires <= now` → InvalidOrExpiredToken. Expired rows are left as they are.
3. If another user has `email = pending_email` → clear the three pending columns for this user and return EmailTaken.
4. `UPDATE users SET email = pending_email, pending_email = NULL, email_verification_token = NULL, email_verification_expires = NULL, reset_token = NULL, reset_token_expires = NULL WHERE id = ? AND email_verification_token = ?`. If 0 rows are affected (a concurrent confirm) → InvalidOrExpiredToken. A duplicate-key error on the `email` UNIQUE index (concurrent race) → clear the pending columns and return EmailTaken.
5. Best effort: send the "email changed" notice to the **old** address in the token owner's account language (`mail_i18n(users.locale, config)`; read `locale` with the row in step 2), not in the locale of whoever clicks confirm. A failure is logged (without the address or token) and does not fail the confirm.
6. Sessions are not touched (D8). If the caller is logged in as a different user, that session is unaffected. The change applies to the token's owner.

Dashboard: no server fn of its own in this slice. The welcome name comes from the foundation's current-user resource. Slices 3 and 6 add `list_my_groups()` and `list_my_assignments()` respectively.

### Emails
Both use the foundation mailer (Mailpit in debug), send HTML + plain text, use the **account language** of the user they concern (`mail_i18n(users.locale, config)`, i18n-foundation F4: NULL or unknown → `APP_LOCALE`), set `<html lang>` to it, and HTML-escape every interpolated value.

- **Verify new address** → `new_email`. Subject `email.verifyEmail.subject`. Body: greeting with the name, the new address, a button and the plain URL `APP_URL/user/verify-email/{token}`, "valid for 24 hours", and "if you didn't request this, ignore it; your current address stays unchanged".
- **Email changed notice** → old address. Subject `email.emailChanged.subject`. Body: "The email address of your account was changed to {new_email}. If this wasn't you, reset your password and contact the group admin." There are no links with tokens.

### Data & migrations
Tables touched: `users` only. Columns read/written: `name`, `email`, `password`, `pending_email`, `email_verification_token`, `email_verification_expires`, `reset_token`, `reset_token_expires` (cleared only) and `session_epoch`. All columns keep their baseline types.

No migration in this slice:
- `users.session_epoch` is created by the auth slice's migration (single owner).
- `email_verification_token VARCHAR(255)` fits the 64-char hash. No new index on it (the table is small).

### UI notes
**Dashboard (`/user/dashboard`)**
- `user.dashboard.welcome {name}` (escaped by Leptos) and `user.dashboard.subtitle`.
- Tips card with `user.dashboard.tips.title`, `.createWishlist`, `.inviteFriends` and `.setExclusions`. **Reword** `user.dashboard.tips.inviteFriends`: en "Share your group's invitation code with friends and family", de "Teilen Sie den Einladungscode Ihrer Gruppe mit Freunden und Familie".
- Layout: three columns as in PHP. Until slices 3 and 6 land, the groups and assignments columns are simply not rendered (no placeholder text).
- Contract for slice 3 (groups card): the caller's groups, newest first, each with name → group page, Drawn/Not drawn badge, Admin badge if the caller is the admin, Join/Create buttons and an empty state. Reuse the `user.dashboard.groups.*`, `group.joinButton` and `group.status.*` keys. One query, not N+1.
- Contract for slice 6 (assignments card): **only the caller's own assignments**, each with group name, receiver name and a link to the receiver's wishlist. Reuse the `user.dashboard.assignments.*` keys. One query.
- Nav (foundation layout): "Dashboard" and "Profile" links. The user's name in the dropdown must update after `update_name` without a full reload (refetch the current-user resource).

**Profile (`/user/profile`)**: three cards. Forms use `ActionForm`, so they also work before hydration. Each card shows its own success/error message (i18n) next to the form. Messages are not auto-dismissed.
1. *Name*: `user.profile.name`, prefilled. Submit `user.profile.updateProfile`. Success `flash.success.profile_updated`.
2. *Email*: shows the current address (`user.profile.email`). If a pending change exists, it shows a persistent info box `user.profile.emailPendingVerification {email}` + `user.profile.emailPendingExpires {date}` (`date = LocalDateTime(pending_expires_at)`, so `app::format` renders it in `APP_TIMEZONE` and the page locale: en `1 Dec 2026, 11:00`, de `01.12.2026 11:00`) and a button `user.profile.changeEmail.cancel`. Form: `user.profile.changeEmail.new`, `user.profile.changeEmail.currentPassword`, submit `user.profile.changeEmail.submit`, hint `user.profile.emailChangeNotice`. Success `flash.success.email_verification_sent`, after which the pending box appears. Cancel success: `flash.success.email_change_cancelled`.
3. *Password*: `user.profile.changePassword.{title,current,new,confirm,submit}` and the hint `user.profile.changePassword.otherSessionsNotice`. Success `flash.success.password_changed`, or `flash.success.password_changed_email_change_cancelled` if the change cancelled a live pending email change (then the email card's pending box disappears; amended 2026-11-09, #71). Fields are cleared after both success and error.
- Back link `user.profile.backToDashboard`.

**Verify email (`/user/verify-email/:token`)**: title `user.verifyEmail.title`, text `user.verifyEmail.description`, button `user.verifyEmail.submit`. On success: `flash.success.email_verified` plus a link to `user.verifyEmail.goToProfile` (logged in) or `user.verifyEmail.goToLogin` (logged out). On error: the message for the error and a link to the profile/login page.

**i18n keys**
- Reused (copied from the PHP catalogs together with the code that uses them; i18n-foundation rule): `user.profile.*` (except `emailNotice`), `user.dashboard.*`, `nav.dashboard`, `nav.profile`, `flash.success.profile_updated`, `flash.success.email_verification_sent`, `flash.success.email_verified`, `flash.error.email_verification_failed` (= InvalidOrExpiredToken), `flash.error.email_already_in_use` (= EmailTaken, now only at confirm), `flash.error.passwords_dont_match`, `flash.error.password_too_short`. From the auth slice: `validation.password_too_long` (`{max}`) for PasswordTooLong.
- Added (en + de): `flash.error.name_required`, `flash.error.name_too_long`, `flash.error.name_invalid`, `flash.error.invalid_email`, `flash.error.email_unchanged`, `flash.error.current_password_incorrect`, `flash.error.email_change_too_soon`, `flash.error.mail_send_failed`, `flash.success.password_changed`, `flash.success.email_change_cancelled`, `user.profile.emailPendingExpires`, `user.profile.changeEmail.{title,new,currentPassword,submit,cancel}`, `user.profile.changePassword.otherSessionsNotice`, `user.verifyEmail.{title,description,submit,goToProfile,goToLogin}`, `email.verifyEmail.{subject,greeting,body,button,linkHint,expiry,ignore}`, `email.emailChanged.{subject,body}`.
- Not ported (never added to the catalogs): `user.profile.emailNotice` (it contradicts the feature), `user.dashboard.quickActions.*` and `user.dashboard.groups.viewAll` (unused keys fail the i18n-foundation scan).

## Acceptance criteria

**Access and authorization**
- AC1: Given no session, when GET `/user/dashboard`, then the user is redirected to the login page (auth slice behaviour), and after logging in lands on `/user/dashboard`.
- AC2: Given no session, when GET `/user/profile`, then the user is redirected to the login page, and after logging in lands on `/user/profile`.
- AC3: Given no session, when each of `get_profile`, `update_name`, `change_password`, `request_email_change` and `cancel_email_change` is called, then each returns `Unauthorized` and no `users` row changes and no mail is sent.
- AC4: Given a logged-in user with id 5 and an anonymous client, when either requests GET `/api/user/5`, then the response is the 404 page and contains no user data.
- AC5: Given a logged-in user, when `request_email_change` is POSTed with an `Origin` header of a foreign site, then the request is rejected by the Origin check, `pending_email` stays unchanged and no mail is sent.

**Dashboard**
- AC6: Given a logged-in user named `<b>Ann</b>`, when GET `/user/dashboard`, then the heading shows `Welcome, <b>Ann</b>!` as literal text (no bold element), with the subtitle and the three tips, including the reworded invite tip.
- AC7: Given the session locale `de`, when GET `/user/dashboard`, then the welcome text, subtitle and tips are in German.

**Profile view and name**
- AC8: Given a user with no pending change, when GET `/user/profile`, then the page shows the current name and email, and no pending box is shown.
- AC9: Given a user with `pending_email = new@example.com` and `email_verification_expires` in the future, when GET `/user/profile`, then a pending box shows `new@example.com`, the expiry time and a cancel button. Given the same row with an expiry in the past, then no pending box is shown and `get_profile` returns `pending_email = None`.
- AC10: Given a logged-in user, when `update_name("  Alice B  ")`, then `users.name = "Alice B"`, the success message `flash.success.profile_updated` is shown, and the nav dropdown shows "Alice B" without a page reload.
- AC11: When `update_name("   ")`, then `NameRequired` is returned and the name is unchanged.
- AC12: When `update_name` gets a 101-character name, then `NameTooLong` is returned. With a 100-character name (including multi-byte characters such as `ä`) it succeeds.
- AC13: Given a user with a pending email change, when `update_name("Bob")` succeeds, then `email`, `password`, `pending_email`, `email_verification_token` and `email_verification_expires` are unchanged.

**Change password**
- AC14: Given a user whose current password is `oldpass123`, when `change_password("oldpass123", "newpass123", "newpass123")`, then it succeeds, `users.password` starts with `$argon2id$`, logging in with `newpass123` works and logging in with `oldpass123` fails.
- AC15: When `change_password("wrong", "newpass123", "newpass123")`, then `CurrentPasswordIncorrect` is returned and `users.password` and `session_epoch` are unchanged.
- AC16: When `change_password("oldpass123", "short7c", "short7c")` (7 characters), then `PasswordTooShort` is returned and nothing changes. When the new password (and confirmation) has 257 characters, then `PasswordTooLong` is returned and nothing changes.
- AC17: When `change_password("oldpass123", "newpass123", "newpass124")`, then `PasswordsDontMatch` is returned and nothing changes.
- AC18: Given the same user logged in in browser contexts A and B, when A changes the password, then A's next request to `/user/profile` still succeeds, the session cookie value in A has changed, and B's next request to `/user/profile` redirects to login (and B's server fn calls return `Unauthorized`).
- AC19: Given a user with a non-NULL `reset_token`, when `change_password` succeeds, then `reset_token` and `reset_token_expires` are NULL and `session_epoch` has increased by 1.

**Request / cancel email change**
- AC20: Given user Alice (`alice@example.com`, password `alicepass1`), when `request_email_change("  Alice.New@Example.com ", "alicepass1")`, then: it succeeds; `pending_email = "alice.new@example.com"`; `email` is unchanged; `email_verification_expires` is within now+24h ±1 min; exactly one mail goes to `alice.new@example.com` with a link `APP_URL/user/verify-email/<43-char base64url token>`; `email_verification_token = hex(sha256(<token from the link>))` and is not equal to the raw token; the profile shows the pending box.
- AC21: When `request_email_change("not-an-email", "alicepass1")`, then `InvalidEmail` is returned, the pending columns are unchanged and no mail is sent.
- AC22: When `request_email_change("ALICE@Example.com", "alicepass1")`, then `EmailUnchanged` is returned and no mail is sent.
- AC23: When `request_email_change("alice.new@example.com", "wrong")`, then `CurrentPasswordIncorrect` is returned, the pending columns are unchanged and no mail is sent.
- AC24: Given `bob@example.com` belongs to another account, when Alice calls `request_email_change("bob@example.com", "alicepass1")`, then the result is identical to AC20 (success, pending set, mail sent to `bob@example.com`). Given a third user already has `pending_email = "x@example.com"`, Alice's request for `x@example.com` also succeeds.
- AC25: Given Alice requested a change less than 60 s ago, when she requests another change, then `EmailChangeTooSoon` is returned, no mail is sent, and the first link still works. Given the first request is older than 60 s (test: set `email_verification_expires` to now+24h−61s), when she requests `alice.other@example.com`, then it succeeds, the first link now yields `InvalidOrExpiredToken`, and the second link works.
- AC26: Given the mail transport fails, when `request_email_change` is called with valid input, then `MailSendFailed` is returned and the three pending columns equal their values before the call.
- AC27: Given Alice's name is `<script>x</script>` and her `users.locale` is `de` (her page may be English), when she requests an email change, then the mail's subject is the German `email.verifyEmail.subject`, the HTML body contains `&lt;script&gt;` and no `<script>` element, and the mail has a plain-text part.
- AC28: Given a pending change, when `cancel_email_change()`, then the three pending columns are NULL, the profile shows no pending box, and confirming the old link yields `InvalidOrExpiredToken`. Calling `cancel_email_change()` again with nothing pending succeeds.

**Confirm email change**
- AC29: Given a valid link, when it is opened with GET (logged in or not), then a confirmation page with a confirm button is shown, and the `users` row is unchanged after the GET.
- AC30: Given a valid link and no session, when the confirm button is clicked, then `email = "alice.new@example.com"`, the three pending columns and `reset_token`/`reset_token_expires` are NULL, the success message `flash.success.email_verified` is shown with a link to log in, logging in with `alice.new@example.com` works and with `alice@example.com` fails, and one notice mail goes to `alice@example.com` naming the new address and containing no token.
- AC31: Given a token that was already used successfully, when `confirm_email_change` is called with it again, then `InvalidOrExpiredToken` is returned.
- AC32: Given a pending change whose `email_verification_expires` is in the past, when `confirm_email_change(token)`, then `InvalidOrExpiredToken` is returned and `email` is unchanged.
- AC33: When `confirm_email_change` is called with an unknown well-formed 43-char base64url token, with `"abc"`, or with a 43-character string containing a character outside the base64url alphabet (e.g. `+`), then each returns `InvalidOrExpiredToken`.
- AC34: Given Alice has pending `bob@example.com` and that address now belongs to Bob, when Alice's token is confirmed, then `EmailTaken` is returned, Alice's `email` is unchanged, her three pending columns are NULL, and Bob's row is unchanged.
- AC35: Given user B is logged in and Alice's valid token is confirmed in B's browser, then Alice's email changes, B's email is unchanged, and B's session is still valid.
- AC36: Given Alice is logged in in two browser contexts, when her email change is confirmed, then both sessions remain valid (no session invalidation on email change).
- AC37: When `update_name` is called with a name containing a control character, then `NameInvalid` is returned and `users.name` is unchanged.

**Mail language and formatting** (added 2026-10-31, i18n-foundation)
- AC38: Given Alice's `users.locale = 'de'` and she is on an English page, when she requests an email change and the link is then confirmed from a browser with an English page, then the verification mail and the email-changed notice are both German. Given Bob's `users.locale` is NULL and `APP_LOCALE=en`, when he requests a change from a German page, then the verification mail is English.
- AC39: Given a pending change with `email_verification_expires = 2099-12-01 10:00:00 UTC` (a fixed future instant), when the profile is shown in English, then the pending box shows `1 Dec 2099, 11:00`; in German `01.12.2099 11:00`.
- AC40 (added 2026-11-01, review): Given Alice is logged in, when `change_password` or `request_email_change` is called with a wrong current password 10 times within 15 minutes (any mix of the two fns, also concurrently), then the next call returns `TooManyAttempts { minutes }` without verifying the password, even if it is correct, and nothing is changed. The limiter key is `account:pw:<user_id>` (10 / 15 min, reserved atomically before argon2 like auth's login limiter, cleared on a successful verification). It is independent of the login limiter. Validation failures are not counted.
- AC41 (added 2026-11-01, review): Given JavaScript is disabled (or the button is pressed before hydration), when Alice opens a valid verification link and presses Confirm, then the change is applied (no 403: the page sends `Referrer-Policy: same-origin`, so the form POST carries the real Origin), and the browser ends on `/user/profile` with a flash for the result (success or the error's message). The verify page never sends its URL to another origin.
- AC42 (added 2026-11-01, review): `/user/profile` and `/user/verify-email/<token>` respond with `Cache-Control: no-store`.
- AC43 (added 2026-11-01, review): The verification mail is subject to auth's per-recipient limit `mail:email:<normalised new email>`. When it is exceeded, no mail is sent, the pending columns are unchanged, and the call returns `EmailChangeTooSoon`.
- AC44 (added 2026-11-01, user): Given JavaScript is disabled, when Alice submits any profile form (`update_name`, `change_password`, `request_email_change`, cancel pending change), then the browser ends on `/user/profile` and shows that form's success message, or the error's message, as a flash rendered by the server. With JavaScript enabled the per-card messages stay as they are.

## Open questions

None.

## Resolved (2026-10-31, user, except where noted)
1. **Session invalidation mechanism**: aligned with the auth spec (orchestrator). The column is `users.session_epoch`, added by auth's migration, and checked by `current_user()`. `change_password` increments it and writes the new value into the current session.
2. **Current password for email change**: yes, it is required.
3. **Non-enumerating email change**: yes. A taken address is detected only at confirmation.
4. **Throttle between email-change requests**: 60 s accepted.
5. **Mail on password change**: no.
6. **URLs**: keep `/user/dashboard` and `/user/profile`.
7. **"Last login" on the dashboard**: no.
8. **`users.email` collation**: moot, there is no production DB. Emails are lowercased on write and compared lowercased, the same rule as auth (D13; AC20, AC22).
