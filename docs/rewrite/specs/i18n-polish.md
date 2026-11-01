---
slice: i18n-polish
status: approved
---
# i18n-polish

Scope (slice 7): the no-JS POST language switch, the landing page port with a "Got an invitation code?" link, and the remaining strings.

Built on **`i18n-foundation`** (built before auth), which owns: `app::format` (dates, numbers), typed `t!` arguments, `t_plural!`, typed flash arguments, locale resolution (session → cookie → `Accept-Language` → `APP_LOCALE`), `users.locale` and the mail-locale helpers, `remember_locale` / `locale_cookie` / `append_set_cookie`, `safe_return_path`, and the static check `server/tests/i18n_coverage.rs` (unused keys fail). This spec does not re-specify them.

PHP reference: `HomeController::index` / `setLanguage`, `src/Core/Application.php` (route), `src/Views/layouts/main.php` (switcher), `src/Views/home/index.php`.

Split from the former draft on 2026-10-31 (user).

## Current PHP behaviour

### Routes

| Method | Path | Handler | Auth | What it does |
|---|---|---|---|---|
| GET | `/` | `HomeController::index` | none | Logged in → 302 `/user/dashboard`. Otherwise renders `home/index`. |
| GET | `/language/:locale` | `HomeController::setLanguage` | none | If `:locale` ∈ `['en','de']`: stores `locale` in the session. Then 302 to the raw `Referer` header, or `/` if it is missing. An unsupported locale is ignored silently and still redirects. |

### Switcher (`layouts/main.php`)
- `<html lang="en">` is hard-coded, whatever the locale.
- A Bootstrap dropdown labelled `strtoupper($session->get('locale', 'en'))`, with GET links `/language/en` "English" and `/language/de` "Deutsch". The label defaults to `en` while `I18n` defaults to `APP_LOCALE`, so with `APP_LOCALE=de` and no session value the label says "EN" over a German page.
- The locale lives only in the session.

### Home (`home/index.php`)
Hero (`home.title`, `home.subtitle`), two cards (login, register), "How it works" in three steps with icons (`home.howItWorks.*`), and a features row (`home.features.multipleGroups`, `emailNotifications`, `exclusionRules`). `home.features.prioritizedWishlists` exists but is not shown.

### In the Rust app when this slice starts
- The nav has a placeholder dropdown with `rel="external"` GET links to `/language/{en,de}` (not routed).
- The landing page has only the hero and the two cards.
- The catalogs contain only keys in use (i18n-foundation F5). The other `home.*` texts are in the PHP catalogs `src/Localization/{en,de}/messages.json`.

## Problems in the PHP version
1. **Open redirect**: `setLanguage` redirects to the raw `Referer` (any host).
2. **State change on GET**: any cross-site `<img src=".../language/de">` switches the victim's language (low impact, but it is a CSRF).
3. `<html lang>` is always `en` (fixed by the foundation).
4. The switcher label and the effective locale use different defaults.
5. The choice is not persisted beyond the session and does not reach mails (fixed by i18n-foundation + auth; this slice writes `users.locale` on a switch).
6. Home page text is wrong: step 3 "Automated Draw … Our system randomly assigns" (the admin starts the draw), step 1 "inviting participants" (invites are by link/code), and the prioritised-wishlists feature is not mentioned although wishlists W7 brings ordering back.

## Redesign

### Decisions
- **L1 The switch is a POST.** A Leptos server fn `set_locale`, submitted by a **native** `<form method="post">` (not `ActionForm`, not router `<Form>`), so it works without JS and is always a full page load (the locale is fixed per page load). CSRF: the foundation's Origin check. GET `/language/*` is not routed.
- **L2 Return target = validated `next`**, the current path+query, sent as a hidden field and checked with `safe_return_path` (i18n-foundation). The `Referer` is never read. Switching on the login, register and token pages returns to that page.
- **L3 The choice is remembered** in the session and the `locale` cookie (`remember_locale`), and, when logged in, in `users.locale` (the account language, i18n-foundation F3). A switch while logged in changes the account language, so later mails to that user follow it (F4).
- **L4 Landing page**: the full PHP landing page, with corrected texts, an extra "assignments stay secret" feature and a "Got an invitation code?" link. Logged-in visitors are redirected to the dashboard.

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/` | Landing page (UI notes) | Anonymous. Logged in → redirect to `DASHBOARD_PATH`. If an earlier slice already does this, unchanged. |
| `POST /api/set_locale` | server fn `set_locale` (`#[server(prefix = "/api", endpoint = "set_locale")]`) | Anyone |
| `GET /language/:locale` | not routed → 404 page | – |

### Server functions

#### `set_locale(locale: String, next: Option<String>) -> Result<(), AppError>`
No login required. Origin check applies (POST).
1. `Locale::from_code(&locale)`. `None` → `AppError::Validation("validation.locale_unsupported")`. Nothing is written and no cookie is set.
2. `remember_locale(&session, locale, config.secure_cookies())`, then `append_set_cookie(&cookie)`.
3. If `load_user` returns a user (valid session and current `session_epoch`): `UPDATE users SET locale = ? WHERE id = ?` with the canonical code (`en`/`de`) and the id from the session. An anonymous or stale session writes nothing to `users`.
4. `leptos_axum::redirect(next.as_deref().and_then(safe_return_path).unwrap_or("/"))`. No flash.
5. Tracing for this fn must not record its arguments (`next` can contain a reset or registration token).

### Data & migrations
No migration. Writes `users.locale` (column from i18n-foundation).

### UI notes

**Language switch (nav).** Rendered by the layout **outside `.navbar-collapse`** (next to the toggler), so it is visible on mobile and before hydration. It does not depend on the session resource. One form per locale other than the current one (with two locales: one button):
```html
<form method="post" action="/api/set_locale" class="d-inline">
  <input type="hidden" name="locale" value="de">
  <input type="hidden" name="next" value="<current path + query>">
  <button type="submit" class="btn btn-sm btn-outline-light" lang="de"
          aria-label="<language.switchTo with language = Deutsch>">Deutsch</button>
</form>
```
- The visible label is `Locale::native_name()` (not translated, by design) with `lang` set to that locale.
- `next` is `use_location()` pathname + search, bound with `prop:value` so it follows client-side navigation. On the server it is the request's path+query.
- The old dropdown, `id="languageDropdown"` and the `/language/*` links are removed.

**Landing page (`/`, anonymous).** In this order:
1. Hero: `home.title`, `home.subtitle`.
2. Two cards: login (`home.login.title`, `.description`, button `auth.login` → `/auth/login`) and register (`home.register.title`, `.description`, button `auth.register` → `/auth/register`).
3. A line `home.join.text` with a link `group.joinButton` → `/join` (protected; anonymous visitors go through login with `next=/join`).
4. "How it works" (`home.howItWorks.title`): three cards with the PHP SVG icons (people, list-check, gift) and `home.howItWorks.step{1,2,3}.{title,description}`.
5. Features (`home.features.title`): five items with the check icon: `multipleGroups`, `emailNotifications`, `exclusionRules`, `prioritizedWishlists`, `secretAssignments`, in a wrapping grid (`col-md-6 col-lg-4`).

SVG icons are markup only (no text nodes), so they pass the foundation's C9 rule.

**Footer.** Unchanged: `"© "` + `footer.copyright`, no year (hydration).

**i18n keys** (each added to both catalogs together with the code that uses it; foundation rule)

New (en / de):

| Key | en | de |
|---|---|---|
| `language.switchTo` | Switch language to {language} | Sprache wechseln zu {language} |
| `validation.locale_unsupported` | This language is not supported. | Diese Sprache wird nicht unterstützt. |
| `home.join.text` | Got an invitation code? | Sie haben einen Einladungscode? |
| `home.features.secretAssignments` | Assignments stay secret, even from the admin | Zuweisungen bleiben geheim, auch vor dem Admin |

Copied from the PHP catalogs unchanged: `home.howItWorks.title`, `home.howItWorks.step1.title`, `home.howItWorks.step2.title`, `home.howItWorks.step2.description`, `home.features.title`, `home.features.multipleGroups`, `home.features.emailNotifications`, `home.features.exclusionRules`, `home.features.prioritizedWishlists`. `group.joinButton` is already in the catalogs (groups slice).

Copied and reworded (en / de):

| Key | en | de |
|---|---|---|
| `home.howItWorks.step1.description` | Create a group and share its invitation link with family and friends. | Erstellen Sie eine Gruppe und teilen Sie den Einladungslink mit Familie und Freunden. |
| `home.howItWorks.step3.title` | 3. Draw Names | 3. Wichtel ziehen |
| `home.howItWorks.step3.description` | When everyone has joined, the admin starts the draw. Everyone gets their recipient by email, and exclusion rules are respected. | Sobald alle beigetreten sind, startet der Admin die Ziehung. Jeder erhält seinen Wichtelpartner per E-Mail, und Ausschlussregeln werden berücksichtigt. |

**Remaining strings.** At the end of this slice the foundation's scan has no findings, and a review pass over all pages checks that every count next to a counted noun uses `t_plural!` and every displayed date goes through `app::format` (foundation rules the scanner cannot fully detect). Anything found is fixed in the owning page's code and catalogs.

### Decision log entries (for PLAN.md)
- `i18n-polish | language switch = set_locale server fn via native POST form, return target = validated next (safe_return_path), Referer never read; a logged-in switch updates users.locale | CSRF-safe, no open redirect, works without JS`

## Acceptance criteria

Fixtures: `APP_LOCALE=en`, `APP_TIMEZONE=Europe/Berlin`. Users Alice, Bob (as in the auth spec), `users.locale` NULL unless an AC says otherwise. "Switch to X" means submitting the nav form for X. HTTP-level tests use `server::router_with`; browser tests use Playwright (which sends `Accept-Language: en-US`).

**Language switch**
- AC1: Given an anonymous visitor on `/` in English, when they switch to Deutsch, then the browser does a full page load of `/`, `<html lang="de">`, the hero shows the `de` text of `home.title`, and the response to the POST has `Set-Cookie: locale=de` with `Path=/`, `Max-Age=31536000`, `SameSite=Lax` and `HttpOnly`, and without `Secure` (http `APP_URL`). With an https `APP_URL` the cookie also has `Secure` (integration test).
- AC2: Given JavaScript is disabled in the browser, when the visitor switches to Deutsch on `/` and back to English, then each switch works as in AC1.
- AC3: Given the visitor navigated client-side (after hydration) from `/` to `/auth/login?next=%2Fjoin%2FABC234`, when they switch to Deutsch, then they are on exactly `/auth/login?next=%2Fjoin%2FABC234` in German, and the hidden `next` field held that path+query before submitting.
- AC4: Given a pending reset link `/auth/reset-password/<token>`, when the visitor opens it and switches to Deutsch, then they are back on that URL in German and the token still works for a reset.
- AC5: When `set_locale("de", next)` is POSTed with each `next` below, then the redirect `Location` is: `/groups/5?tab=a` → `/groups/5?tab=a`; `/auth/register` → `/auth/register`; `//evil.example/` → `/`; `https://evil.example/` → `/`; `/\evil.example` → `/`; `/api/set_locale` → `/`; `/api` → `/`; `/a b` → `/`; a value containing a raw CR LF → `/`; a 513-byte path → `/`; empty → `/`; field missing → `/`. With no `next` and `Referer: https://evil.example/x`, the target is `/`.
- AC6: When `set_locale("fr", "/")` is POSTed, then the result is `Validation("validation.locale_unsupported")`, the response sets no `locale` cookie, and the session locale and every `users.locale` are unchanged. When `set_locale("de-AT", "/")` is POSTed, then the cookie and session hold `de`.
- AC7: Given a POST to `/api/set_locale` with `Origin: https://evil.example`, then the response is 403, and no session value, cookie or `users` row changes.
- AC8: When `GET /language/de` is requested, then the 404 page is shown and no `locale` cookie is set. When `GET /api/set_locale?locale=de&next=/` is requested, then the response is neither a 2xx nor a redirect, and nothing changes.
- AC9: Given Alice is logged in, when she switches to Deutsch, then Alice's `users.locale = 'de'` and Bob's row is unchanged. Given a session whose `session_epoch` is stale (Alice changed her password elsewhere), when that session switches to Deutsch, then cookie and session are set and Alice's `users.locale` is unchanged.
- AC10: Given an anonymous visitor switched to Deutsch, when their session record is deleted but the `locale` cookie is kept, then the next page is German. Given a logged-in user switched to Deutsch, when they log out, then the page after logout is German.
- AC11: Given a 375 px wide viewport, then the switch button is visible without opening the mobile menu, both before and after hydration. Its text is the other locale's native name ("Deutsch" on an English page, "English" on a German page), its `lang` attribute is that locale's code, its `aria-label` is `language.switchTo` in the current locale, and the current locale is not offered. The rendered HTML contains no `languageDropdown` and no `/language/` link.
- AC12: Given Alice (`users.locale` NULL) is logged in on browser context 1 and switches to Deutsch there, when a password reset for Alice is then requested from an English page in browser context 2, then the reset mail is German (auth AC50), and when Alice logs in on context 2 (cookie `locale=en`), the dashboard is German (auth AC47).

**Landing page**
- AC13: Given an anonymous visitor, when they open `/` in English, then they see, in order: `home.title`, `home.subtitle`, the login card with a link to `/auth/login`, the register card with a link to `/auth/register`, `home.join.text` with a link to `/join`, the "How it works" title with three steps (step 3 title "3. Draw Names"), and the features title with five items including `secretAssignments` and `prioritizedWishlists`. With German, all of these are the `de` texts. The page has no hydration errors (`collectPageErrors`) in either locale.
- AC14: Given an anonymous visitor follows the `/join` link on the landing page, then they get the login page with `next=/join`, and after logging in they are on `/join`.
- AC15: Given Alice is logged in, when she opens `/`, then she is redirected to `/user/dashboard`.

**Strings**
- AC16: At the end of this slice, the foundation's real-tree scan (i18n-foundation AC16) has no findings, and every key in this spec's tables exists in both `en.json` and `de.json` with the texts given here.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **English date format**: `10 Dec 2026, 14:30`. Moved to i18n-foundation (F1).
2. **Login adopts `users.locale`** (option A). Moved to i18n-foundation (F3) and auth (AC47).
3. **`Accept-Language` for first visits**: yes. Moved to i18n-foundation (F2).
4. **Unused keys fail** the coverage test. Moved to i18n-foundation (F5).
5. **Split**: the shared parts (formatting, plurals, locale resolution, `users.locale`, mail-locale helpers, `safe_return_path`, the static scan) moved to `i18n-foundation`, which is built before auth. The other slices' specs were revised the same day to use them directly, so this slice no longer changes their code.
