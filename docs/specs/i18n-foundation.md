---
slice: i18n-foundation
status: approved
---
# i18n-foundation

Scope: the i18n building blocks every later slice uses, built **before auth**:
- shared date/number/plural formatting (`app/src/format.rs`, typed `t!` arguments, `t_plural!`, typed flash arguments);
- date-input helpers in `app::tz`;
- locale resolution per request (session → `locale` cookie → `Accept-Language` → `APP_LOCALE`), plus the helpers that let auth put the account language into the session at login;
- the `users.locale` column (account language) and the per-user mail-locale helpers;
- `safe_return_path` (shared validator for relative return paths);
- the static key-coverage / hard-coded-text check `server/tests/i18n_coverage.rs`, with unused keys failing the suite.

What stays for slice 7 (`i18n-polish`): the no-JS POST language switch (`set_locale`), the landing page port with the "Got an invitation code?" link, and the remaining strings.

PHP reference: `src/Core/I18n.php`, `src/Localization/functions.php`, `src/Localization/{en,de}/messages.json`, `src/Views/layouts/main.php`, and the views that format dates (`group/show.php`, `group/edit.php`).

Split out of the former `i18n-polish` draft on 2026-10-31 (user). The approved specs auth, account, groups, wishlists and draw were revised the same day to build on this spec (see "Ownership of the cross-slice parts").

## Current PHP behaviour

### I18n (`I18n.php`, `functions.php`)
- Singleton. Locale = session `locale`, else `APP_LOCALE`, else `en`. Calls `setlocale(LC_ALL, "<l>_<L>.UTF-8")` (`en_EN.UTF-8` does not exist) and `\Locale::setDefault`.
- `t(key, params)` looks up the key in the **current locale's** file only. A missing file falls back to the English file as a whole, but a key missing from the German file is **not** looked up in English: the raw key is shown. `{name}` placeholders are replaced with `str_replace`, without escaping.
- `format_date`, `format_datetime`, `format_number`, `format_currency` (ICU) exist but **no view calls them**. Views use `date('F j, Y', …)` (English month names in the German UI) for the deadline, draw date and created date, `date('Y-m-d', …)` for form values, and `date('Y')` in the footer.
- No plural support (messages avoid it with "Members ({count})").
- The locale lives only in the session: no cookie and nothing in `users`. Mails (`EmailService`) are English only.

### Catalogs
Flat JSON with dotted keys. `de` has four keys `en` lacks. Most controllers flash hard-coded English. Nothing checks for missing, unused or drifted keys.

### Already in the Rust foundation (Phase 1A)
- `app::i18n`: `Locale { En, De }`, `Locale::from_code` (lenient: `de-AT`, `EN_us`), embedded catalogs, `translate` (current locale → English → key), `translate_with`, `t!` (`to_string()` on each argument), `I18n { locale }`, `provide_i18n(locale)`, `use_i18n()`, `request_locale()` (server: request extension; client: `<html lang>`).
- `server/src/middleware.rs::locale`: session → `locale` cookie → `APP_LOCALE`.
- `app::server::i18n`: `session_locale`, `set_session_locale`, `cookie_locale`.
- `app::tz`: UTC ↔ `APP_TIMEZONE`, `format_local`, `datetime-local` helpers, `use_timezone()` (client: `<html data-timezone>`).
- `app::flash::FlashMessage { kind, key, params: Vec<(String, String)> }`; `FlashAlert` renders with `t_with`.
- `rust/locales/{en,de}.json` = the PHP catalogs plus 9 foundation keys. As of 2026-10-31 the Rust code uses 23 of their ~260 keys.
- `server/tests/i18n.rs`, including `every_english_key_exists_in_german` (one direction only).

## Problems in the PHP version
1. German UI shows English month names; the ICU helpers are dead code.
2. A key missing in `de` shows the raw key. Nothing detects missing or unused keys; the catalogs drifted.
3. Translation parameters are not escaped centrally.
4. No plurals.
5. The language choice does not survive the session and never reaches mails.
6. `setlocale("en_EN.UTF-8")` names a locale that does not exist.

## Redesign

### Decisions (user, 2026-10-31)
- **F1 Formats** are fixed per locale: en `10 Dec 2026` / `10 Dec 2026, 14:30`; de `10.12.2026` / `10.12.2026 14:30`. 24-hour clock, no zone suffix. Integers grouped by thousands (en `,`, de `.`).
- **F2 Locale resolution** per request: session `locale` → `locale` cookie → `Accept-Language` → `APP_LOCALE`.
- **F3 Account language.** `users.locale` (nullable). Auth writes it when registration completes and puts it into the session (and the `locale` cookie) on login, so a logged-in user sees their account language on every device. Slice 7's switch updates it while logged in.
- **F4 Mail language.** Every mail to an existing account uses that account's `users.locale`. NULL or an unknown value falls back to `APP_LOCALE`. Only mails to an address without an account (auth's registration mail) use the request locale.
- **F5 Unused catalog keys fail the test suite.** The catalogs contain exactly the keys the code uses. This slice removes every key that is unused today. Each later slice adds its keys in the same change as the code that uses them.
- **F6 One formatter.** Display dates, date-times and numbers go only through `app::format` (via `I18n`). Slices never call chrono `.format("%…")` or `tz::format_local` for display.

### Locale resolution

`resolve_locale(session: Option<&str>, cookie: Option<&str>, accept_language: Option<&str>, default: Locale) -> Locale` (pure, `app::i18n`): the first of session, cookie, `accept_language(header)`, `default` that yields a locale. Invalid values are skipped, not fatal.

`accept_language(header: &str) -> Option<Locale>` (pure, `app::i18n`): split on `,`; each item is `tag[;q=v]` (whitespace around parts ignored); `q` defaults to 1; a malformed `q` or one outside `0..=1` drops the item; `q=0` and `*` are ignored. Order by `q` descending, stable for ties. Return the first tag for which `Locale::from_code` is `Some`.

The `locale` middleware joins all `Accept-Language` headers with `,` and passes the result as the third input. Nothing else changes: the result is the `RequestLocale` request extension, written into `<html lang>` and read back by the client.

The account language reaches the session only through the helpers below, called by auth's `login` (auth AC47). There is no per-request `users` lookup. A session that already holds a locale wins over everything else.

### Server-only locale helpers (`app::server::i18n`)

- `LOCALE_COOKIE_MAX_AGE: i64 = 31_536_000` (1 year).
- `locale_cookie(locale: Locale, secure: bool) -> String`: exactly `locale=<code>; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly`, plus `; Secure` iff `secure`. HttpOnly is fine: the client reads `<html lang>`, never the cookie.
- `remember_locale(session: &Session, locale: Locale, secure: bool) -> Result<String, AppError>`: `set_session_locale(session, locale)`, then returns `locale_cookie(locale, secure)`. The caller attaches the cookie (`secure` = `config.secure_cookies()`).
- `adopt_account_locale(session: &Session, stored: Option<&str>, secure: bool) -> Result<Option<String>, AppError>`: if `stored` parses with `Locale::from_code`, `remember_locale` with it and return `Some(cookie)`; otherwise change nothing and return `None`.
- `append_set_cookie(value: &str)`: appends a `Set-Cookie` header to the current server fn's `leptos_axum::ResponseOptions`. A thin wrapper for server fns; it is exercised by auth AC47 and i18n-polish AC1, not by this slice.
- `account_locale(stored: Option<&str>, default: Locale) -> Locale` (pure): `stored.and_then(Locale::from_code).unwrap_or(default)`.
- `mail_i18n(stored: Option<&str>, config: &Config) -> I18n`: `I18n::new(account_locale(stored, config.app_locale), config.app_timezone)`. Every mail to an account builds its texts with this. The registration mail (no account) uses `I18n::new(request_locale(), config.app_timezone)`.
- `load_account_locale<'e>(executor: impl sqlx::MySqlExecutor<'e>, user_id: i32) -> Result<Option<String>, AppError>`: `SELECT locale FROM users WHERE id = ?`. No row → `NotFound`. For slices that do not already read `locale` with the user's row; slices that do just add the column to their query.

Mail HTML sets `<html lang="<mail locale code>">`. Every interpolated value stays HTML-escaped (`escape_html`), as the slices already require.

### Formatting (`app/src/format.rs`, shared ssr + hydrate)

Free functions hold the logic:

| Function | en | de |
|---|---|---|
| `date(locale, tz, dt: DateTime<Utc>)` | `%-d %b %Y` → `10 Dec 2026` | `%d.%m.%Y` → `10.12.2026` |
| `datetime(locale, tz, dt: DateTime<Utc>)` | `%-d %b %Y, %H:%M` → `10 Dec 2026, 14:30` | `%d.%m.%Y %H:%M` → `10.12.2026 14:30` |
| `number(locale, n: i128)` | `,` every three digits → `2,000`, `-1,234` | `.` every three digits → `2.000`, `-1.234` |

- Dates are converted to `tz` first (`tz::to_local`). `%b` is chrono's English abbreviation, used only for `en` (no `unstable-locales` feature). No currency or decimal helpers (the app has no amounts).
- Only `format.rs` and `tz.rs` may call chrono's `.format` with a `%` pattern or `tz::format_local` (rule C10).

**`I18n` carries the time zone and its catalogs:** `I18n { locale: Locale, tz: Tz, catalogs: &'static Catalogs }`, still `Copy`.
- `I18n::new(locale, tz)` uses the embedded catalogs. `I18n::with_catalogs(locale, tz, catalogs: &'static Catalogs)` is public and meant for tests (synthetic catalogs, e.g. via `Box::leak`). `Catalogs { en, de }` with `Catalogs::embedded()` and `Catalogs::from_json(en: &str, de: &str) -> Result<Catalogs, serde_json::Error>`.
- Methods: `locale()`, `tz()`, `t(key)`, `t_with(key, params)`, `t_plural(key, n, params)`, `date(dt)`, `datetime(dt)`, `number(n)`. The free `translate` / `translate_with` functions stay and use the embedded catalogs.
- Lookup everywhere: current locale → English → the key itself.
- `provide_i18n(locale, tz)`. `App` passes `request_locale()` and `use_timezone()`. `use_i18n()` falls back to `I18n::new(request_locale(), use_timezone())`, so it also works inside server fns.
- Hydration: locale and tz are fixed per page load (`<html lang>`, `data-timezone`), so server and client produce the same text. Components receive `DateTime<Utc>` and format with the page's `I18n`. Formatting on the server and sending the string is also allowed if it uses the same `I18n`.

**Message arguments.** `t!(i18n, "key", name = value, …)` converts each value through the trait `MessageArg { fn render(&self, i18n: &I18n) -> String }` instead of `to_string()`:
- `&str`, `String`, `&String`, `Cow<'_, str>` → unchanged;
- every primitive integer type → `i18n.number(..)` (through `i128`);
- `LocalDate(DateTime<Utc>)` → `i18n.date(..)`; `LocalDateTime(DateTime<Utc>)` → `i18n.datetime(..)` (newtypes in `app::format`).
- Nothing else implements it, so a bare `DateTime<Utc>` or another `Display` type does not compile (write `.to_string()` explicitly if a raw value is really meant).

**Plurals.** Catalog entries `<base>.one` and `<base>.other`; there is no plain `<base>` key next to them. `I18n::t_plural(base, n: i64, params)` and `t_plural!(i18n, "base", n, name = value, …)` look up `<base>.one` if `n == 1`, else `<base>.other`, each with the usual fallback (English form, then the key `<base>.one` / `<base>.other` itself). `n` only selects the form; placeholders come from the named arguments as in `t!` (so pass `count = n` when the text shows the number). en and de share this rule; there are no other plural categories. Rule for every slice: a message that puts a count next to a counted noun uses `t_plural!` (review rule; the scanner cannot detect it).

**Flash messages.** `FlashMessage.params: Vec<(String, FlashArg)>` with `enum FlashArg { Text(String), Number(i64), Date(DateTime<Utc>), DateTime(DateTime<Utc>) }`, plus `plural: Option<i64>`. Builders: `with_param(name, text)` (unchanged meaning), `with_number(name, n)`, `with_date(name, dt)`, `with_datetime(name, dt)`, `with_plural(n)`. New `FlashMessage::text(&self, i18n: &I18n) -> String` renders the args with that `I18n` (number/date/datetime formatted by it) and uses `t_plural` when `plural` is set. `FlashAlert` uses `text`. So a flash renders in the locale of the page that shows it, not the one that queued it. Old session payloads need no compatibility (fresh start).

**Escaping.** Translated strings are rendered as text nodes or attribute values only, never through `inner_html`.

### Date-input helpers (`app::tz`)
Input values stay ISO and come only from `app::tz`:
- existing `to_input_value` / `parse_input_value` (`datetime-local`);
- new `to_date_input_value(dt: DateTime<Utc>, tz: Tz) -> String` (`YYYY-MM-DD` of the local date in `tz`);
- new `parse_date_input(value: &str) -> Result<NaiveDate, TzError>`: trims, accepts exactly `%Y-%m-%d` with a real calendar date, otherwise `TzError::Invalid`.

The groups slice's conversion of a date to `23:59:59` / `12:00:00` local stays in groups and uses `tz::from_local`.

### `safe_return_path` (`app/src/paths.rs`, shared, pure)
`safe_return_path(raw: &str) -> Option<String>` returns `Some(raw.to_string())` only if all hold:
- length 1..=512 bytes;
- starts with `/`, and the second character is neither `/` nor `\`;
- contains no `\`, no ASCII control characters (incl. CR, LF, TAB) and no spaces;
- is not `/api` and does not start with `/api/`.

The redirect target is exactly the returned string, never prefixed with a host. Auth's `safe_next` = `safe_return_path` plus "not `/auth` and not under `/auth/`" (auth spec). Slice 7's `set_locale` uses `safe_return_path` directly.

### Key-coverage and hard-coded-text check (`server/tests/i18n_coverage.rs`)

Dev-dependencies of `server`: `proc-macro2` (feature `span-locations`, for line numbers) and `syn` (string-literal unescaping). The scanner is a function in the test file, `scan(files: &[(PathBuf, String)], en_json: &str, de_json: &str) -> Vec<Finding>` with `Finding { file, line, rule, detail }`, so each rule can be tested on synthetic input. The real-tree test fails if `scan` returns any finding and prints all of them (`file:line rule detail`).

**Inputs.** Catalogs `rust/locales/{en,de}.json` (raw text, so duplicates can be detected). Sources: every `.rs` file under `rust/app/src`, `rust/server/src` and `rust/frontend/src`, except `app/src/test_support.rs` and `app/src/test_support/**`. Test code (`server/tests`, `end2end`) is not scanned and does not count as usage. Keys appear only in `.rs` files (mail texts are built in Rust with the `t!` family).

**Tokenising.** Each file is parsed with `proc_macro2::TokenStream::from_str` and walked recursively through all groups (so `view!`, `t!` and `t_plural!` bodies are seen). Comments are dropped by the lexer. `#[doc = …]` / `#![doc = …]` attributes (doc comments) are skipped. String literals (`"…"`, `r"…"`, `r#"…"#`) are unescaped with `syn::LitStr`; byte and char literals are ignored. A file that does not tokenise is a finding.

**Namespaces** = the constant `NAMESPACES = [app, auth, common, email, error, exclusion, flash, footer, group, home, language, nav, user, validation, wishlist]` united with the first segments of all catalog keys. The constant makes C5 catch a typo even before a namespace has its first key. A literal is a **key literal** if it matches `^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9_]+)+$` and its first segment is a namespace. `NOT_KEYS: &[&str]` in the test file exempts literals that look like keys but are not (empty at first; each entry gets a comment).

Rules (each violation is one finding):
- **C1 Catalog shape.** Both files: a JSON object of string → string; **no duplicate keys within a file** (custom map visitor, because `serde_json::Value` keeps only the last duplicate); no empty or whitespace-only value; every key contains a `.`.
- **C2 Same key sets.** `en` keys == `de` keys (both directions).
- **C3 Placeholder parity.** For every key, the set of `{name}` placeholders in `en` equals the one in `de`.
- **C4 Plural shape.** For every `x.one` there is an `x.other` and vice versa, in both catalogs; no plain key `x` exists next to them.
- **C5 Used keys exist.** Every key literal `k` is satisfied if `k` is in the catalogs, or `k.one` and `k.other` are.
- **C6 No dynamic keys.** A string literal that starts with `<namespace>.` and contains `{` or ends with `.` is a finding (e.g. `format!("group.status.{}", s)`). Keys are complete literals.
- **C7 No unused keys.** Every catalog key is used: `k` is used if the literal `k` appears, or if `k` ends with `.one` / `.other` and its base appears.
- **C8 Arguments match placeholders.** For each `t!(…)` / `t_plural!(…)` invocation whose key argument is a literal, the set of named arguments (`name = …` at the top level of the macro's token list) equals the key's `en` placeholder set (plurals: the union over `.one` and `.other`). For each method call `.t("k")` (ident `t` after `.`, first argument a literal), `k` has no placeholders.
- **C9 No hard-coded text in `view!`.** Inside each `view! { … }` (nested `view!` inside `{}` blocks included), considering only literals directly in the view's token list (not inside any `{}`, `()` or `[]` group): a literal not preceded by `=` is a text node and must not contain any `char::is_alphabetic` character; a literal that is the value of an attribute named `aria-label`, `title`, `placeholder`, `alt`, `text` or `label` (the name rebuilt from the ident / `-` / `:` tokens before `=`) must not contain one either. `"© "` passes. Locale endonyms come from `Locale::native_name()` inside `{}` and are not affected.
- **C10 No ad-hoc date formatting.** Outside `app/src/format.rs` and `app/src/tz.rs`: no call `.format(<string literal containing %>)` and no call to `format_local(`.

`every_english_key_exists_in_german` in `server/tests/i18n.rs` is replaced by C2. The other tests in that file move to the new signatures; those that used keys this slice removes use synthetic catalogs (`I18n::with_catalogs`) instead.

**How later slices keep it green**
1. A slice adds each key to both catalogs **in the same change** as the code that uses it. It adds only keys its code uses, even if its spec lists more.
2. A key a spec lists as "reused" is copied from the PHP catalogs `src/Localization/{en,de}/messages.json` (unchanged until cutover), unless the spec rewords it.
3. Removing or replacing code removes its keys in the same change.
4. Tests must not rely on keys only they use. They use keys that production code uses, or synthetic catalogs.
5. A literal that looks like a key but is not goes into `NOT_KEYS` with a comment.

### Data & migrations

Tables touched: `users` (new column `locale`; read by `load_account_locale`; written by auth and slice 7, not by this slice).

New migration `rust/migrations/NNNN_user_locale.sql` (placeholder number, the next free one):
```sql
ALTER TABLE users ADD COLUMN locale VARCHAR(8) NULL DEFAULT NULL;
```
- NULL = no stored choice: `APP_LOCALE` for mails, nothing adopted at login. Code writes only canonical codes (`en`, `de`). No CHECK constraint, so adding a language later is a code-only change; readers treat unknown values like NULL.
- Existing rows (dev databases, fixtures) get NULL. No data migration (fresh start).
- Writing `locale` bumps `users.updated_at` (`ON UPDATE CURRENT_TIMESTAMP`); acceptable.
- Queries that need it name the column. `CurrentUser` does not carry it.
- Test fixture (`app::test_support`): `set_user_locale(pool, user_id, Option<&str>)`.

### Catalog clean-up in this slice
- Remove every key that C7 reports as unused, and the four `de`-only keys. As of 2026-10-31 the remaining set is these 23 keys (the actual set is whatever the code uses at implementation time): `app.name`, `auth.login`, `auth.logout`, `auth.register`, `common.close`, `error.backHome`, `error.forbidden`, `error.generic.message`, `error.generic.title`, `error.notFound.message`, `error.notFound.title`, `error.unauthorized`, `footer.copyright`, `home.login.description`, `home.login.title`, `home.register.description`, `home.register.title`, `home.subtitle`, `home.title`, `nav.dashboard`, `nav.groups`, `nav.profile`, `nav.toggle`. Keys that only appear in doc comments (`auth.login.title`, `user.dashboard.welcome` in `app/src/i18n.rs`) do not count.
- No new keys: this slice adds no UI text.
- The nav's placeholder language dropdown (`/language/{en,de}` links) stays until slice 7 replaces it.

### UI notes
No visible change except that a first visit follows the browser's `Accept-Language`. Page texts and catalogs otherwise stay as they are.

### Ownership of the cross-slice parts

| Part | Delivered here | Exercised end to end by |
|---|---|---|
| `adopt_account_locale` + `append_set_cookie` at login | helpers, AC11 | auth AC47 |
| `users.locale` written at registration completion | column, AC12 | auth AC48 |
| Mails in the account language (`mail_i18n`) | helpers, AC13–AC14 | auth AC49–AC50, account AC38, draw AC48 |
| `LocalDate` / `LocalDateTime` / `t_plural!` in pages | AC1–AC7 | auth AC2/AC51, account AC39, groups AC5/AC46/AC47, wishlists AC33, draw AC49 |
| `safe_return_path` | AC15 | auth AC7 (via `safe_next`), i18n-polish AC5 |
| `users.locale` written by the language switch | column | i18n-polish AC9, AC12 |

### Decision log entries (for PLAN.md)
- `i18n-foundation | split from i18n-polish and built before auth: format.rs, typed t! args, plurals, locale resolution incl. Accept-Language, users.locale + mail-locale helpers, safe_return_path, static i18n scan | earlier slices build on the final rules instead of being reworked`
- `i18n-foundation | fixed per-locale formats (en "10 Dec 2026, 14:30", de "10.12.2026 14:30"), integers grouped; plurals as .one/.other | user; unambiguous, hydration-stable`
- `i18n-foundation | locale resolution session → cookie → Accept-Language → APP_LOCALE; users.locale enters the session at login (auth) | user`
- `i18n-foundation | users.locale (nullable) = account language; every mail to an account uses it, else APP_LOCALE; only the registration mail uses the request locale | mails reach people other than the requester`
- `i18n-foundation | unused catalog keys fail the suite; catalogs cut to the keys in use, each slice adds its keys with its code, reused texts copied from the PHP catalogs | user; catalog drift was a PHP problem`

## Acceptance criteria

Fixtures: `APP_LOCALE=en`, `APP_TIMEZONE=Europe/Berlin`, `APP_URL=http://localhost:3000` unless an AC says otherwise. "Synthetic catalogs" are built in the test with `Catalogs::from_json` and used through `I18n::with_catalogs`. HTTP-level tests use `server::router_with`.

**Formatting**
- AC1: `format::date` / `format::datetime` with tz Europe/Berlin give:

  | UTC instant | en date | en datetime | de date | de datetime |
  |---|---|---|---|---|
  | 2026-12-01 10:00 | 1 Dec 2026 | 1 Dec 2026, 11:00 | 01.12.2026 | 01.12.2026 11:00 |
  | 2026-07-05 08:05 | 5 Jul 2026 | 5 Jul 2026, 10:05 | 05.07.2026 | 05.07.2026 10:05 |
  | 2026-12-31 23:30 | 1 Jan 2027 | 1 Jan 2027, 00:30 | 01.01.2027 | 01.01.2027 00:30 |
  | 2026-12-10 22:59:59 | 10 Dec 2026 | 10 Dec 2026, 23:59 | 10.12.2026 | 10.12.2026 23:59 |
- AC2: `I18n::new(Locale::De, Europe::Berlin).datetime(2026-12-01 10:00 UTC)` is `01.12.2026 11:00`, and with tz `America/New_York` it is `01.12.2026 05:00`. `I18n::date`, `datetime` and `number` return exactly what the free functions return for the same locale and tz.
- AC3: `format::number` gives en/de: `0` → `0`/`0`; `999` → `999`/`999`; `2000` → `2,000`/`2.000`; `1234567` → `1,234,567`/`1.234.567`; `-1234` → `-1,234`/`-1.234`.
- AC4: Given synthetic catalogs with `t.max` = en "Max {max}" / de "Höchstens {max}" and `t.when` = en "On {date} at {time}" / de "Am {date} um {time}", then `t!(i18n_de, "t.max", max = 2000_u32)` is `Höchstens 2.000`, `t!(i18n_en, "t.max", max = 2000)` is `Max 2,000`, `t!(i18n_de, "t.when", date = LocalDate(dt), time = LocalDateTime(dt))` with dt = 2026-12-01 10:00 UTC is `Am 01.12.2026 um 01.12.2026 11:00`, and a `&str` / `String` argument is inserted unchanged.
- AC5: `tz::to_date_input_value` with Europe/Berlin gives `2026-12-10` for 2026-12-10 22:59:59 UTC and `2027-01-01` for 2026-12-31 23:30 UTC. `tz::parse_date_input` accepts `2026-12-10` and ` 2026-12-10 ` (both → 2026-12-10), and returns `TzError::Invalid` for `""`, `10.12.2026`, `2026-02-30`, `2026-12-10T10:00` and `2026-1-5x`.

**Plurals**
- AC6: Given synthetic catalogs with `t.items.one` / `t.items.other` = en "{count} item" / "{count} items", de "{count} Artikel" / "{count} Artikel", then `t_plural!(i18n, "t.items", n, count = n)` gives en `1 item`, `0 items`, `2 items`, `-1 items`, `2,000 items`, and de `1 Artikel`, `2.000 Artikel`.
- AC7: Given synthetic catalogs where `t.x.one` / `t.x.other` exist only in `en`, then `t_plural` on a German `I18n` returns the English forms. Given a base with no forms at all, then it returns `t.none.one` for n = 1 and `t.none.other` otherwise.

**Flash messages**
- AC8: Given synthetic catalogs with `t.flash` = en "{n} on {d} at {dt}" / de "{n} am {d} um {dt}", when `FlashMessage::success("t.flash").with_number("n", 2000).with_date("d", t0).with_datetime("dt", t0)` (t0 = 2026-12-01 10:00 UTC) is serialised to JSON and back (as through the session), then `text()` with a German `I18n` is `2.000 am 01.12.2026 um 01.12.2026 11:00`, and with an English one `2,000 on 1 Dec 2026 at 1 Dec 2026, 11:00`. A message with `with_plural(1)` on the base `t.items` (AC6 catalogs) and `with_number("count", 1)` renders `1 item`. A message built with `with_param("name", "<b>x</b>")` renders the text `<b>x</b>` unchanged, and `FlashAlert` shows it as literal text (no `<b>` element in the HTML).

**Locale resolution**
- AC9: `accept_language` gives: `fr-CH, fr;q=0.9, de;q=0.8, en;q=0.7` → `de`; `en-US,en;q=0.9` → `en`; `fr` → `None`; `de;q=0, en;q=0.5` → `en`; `de;q=abc` → `None`; `de;q=2` → `None`; `*` → `None`; `""` → `None`; `en;q=0.5, de` → `de`; `de-AT;q=0.8, en-GB;q=0.8` → `de` (stable for ties).
- AC10: `resolve_locale` picks session over cookie over `Accept-Language` over default, and skips invalid values at each step (`Some("xx")` session → next step). HTTP-level, with no session value and no cookie: `GET /` with `Accept-Language: de-DE,de;q=0.9` renders `<html lang="de">` and the German `home.title`; with `Accept-Language: fr` and `APP_LOCALE=de` it renders `lang="de"`; with `APP_LOCALE=de` and `Accept-Language: en-US` it renders `lang="en"`; a `locale=en` cookie beats `Accept-Language: de`; two separate headers `Accept-Language: fr` and `Accept-Language: de` give `lang="de"`.
- AC11: `locale_cookie(De, false)` is exactly `locale=de; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly`, and `locale_cookie(De, true)` ends with `; Secure`. Through a test route mounted with `router_with` that calls `adopt_account_locale(session, <the fixture user's stored locale>, config.secure_cookies())` and returns the cookie as `Set-Cookie`: for a user with `locale = 'de'` the response sets that cookie (with `Secure` when `APP_URL` is https), and a following `GET /` with only the session cookie and `Accept-Language: en` renders `lang="de"`. For stored NULL, `'fr'` or `''` the route sets no `locale` cookie and the session's locale value is unchanged.

**Account language and mail locale**
- AC12: After all migrations, `users.locale` exists as a nullable `VARCHAR(8)` with default NULL. A user inserted by `test_support::create_user` has `locale` NULL, and `set_user_locale(pool, id, Some("de"))` stores `de`.
- AC13: `account_locale(None, En)` = `En`; `account_locale(Some("de"), En)` = `De`; `account_locale(Some("DE-at"), En)` = `De`; `account_locale(Some("fr"), De)` = `De`; `account_locale(Some(""), En)` = `En`. `mail_i18n(Some("de"), &config)` has locale `De` and tz = `config.app_timezone`; with `APP_LOCALE=de` and `APP_TIMEZONE=America/New_York`, `mail_i18n(None, &config)` has `De` and `America/New_York`.
- AC14: `load_account_locale(&pool, id)` returns `Some("de")`, `None` and `Some("fr")` for fixture users with stored `de`, NULL and `fr` (the raw value; `account_locale` interprets it), and `NotFound` for an id with no row.

**Return paths**
- AC15: `safe_return_path` returns the input unchanged for `/`, `/groups/5?tab=a`, `/auth/register`, `/auth/reset-password/abc_DEF-123` and `/join/ABCD2345`, and `None` for `""`, `groups`, `//evil.example/`, `https://evil.example/`, `/\evil.example`, `/a\b`, `/api`, `/api/set_locale`, `/a b`, a value containing CR LF, a value containing TAB, and a 513-byte path (a 512-byte path is accepted).

**Static checks**
- AC16: On the real tree after this slice, `scan` returns no findings; `en.json` and `de.json` have identical key sets, and every key in them is referenced by non-test code (the 23 keys listed under "Catalog clean-up", or whatever the code uses at implementation time). Adding an unused key to both catalogs, or a `t!(i18n, "nav.nope")` call, makes the real-tree test fail with a message that names the key, the rule and (for code findings) `file:line`.
- AC17: On synthetic input, each rule produces a finding naming the file and the key or literal: C1 a duplicate key, an empty value, a key without a dot; C2 a key only in `de`; C3 `{name}` in `en` but `{user}` in `de`; C4 `x.one` without `x.other`, and `x` next to `x.one`/`x.other`; C5 `i18n.t("home.nope")`; C6 `format!("group.status.{}", s)`; C7 a catalog key never referenced; C8 `t!(i18n, "t.max")` without `max`, `t!(i18n, "t.max", max = 1, foo = 2)`, and `i18n.t("t.max")`; C9 `view! { <p>"Hello"</p> }` and `view! { <input placeholder="Search"/> }`; C10 `dt.format("%d.%m.%Y")` and `format_local(dt, tz, "%Y")` in a slice file.
- AC18: On synthetic input, none of these produce a finding: a key literal only inside `// comment`, `/* block */` or a `///` doc comment (and it does not count as usage for C7 either); a literal in `NOT_KEYS`; `view! { <p>"© "</p> }`; `view! { <div class="mb-3" role="alert"/> }`; `view! { <p>{if x { "yes" } else { "no" }}</p> }` (inside a block); `t_plural!(i18n, "t.items", n, count = n)` with `.one`/`.other` present; chrono `%` formatting inside `app/src/format.rs` and `app/src/tz.rs`.

**Hydration**
- AC19: Given a Playwright browser context with locale `de-DE` (so `Accept-Language: de-DE`) and no cookies, when it opens `/`, then `<html lang="de">`, the hero shows the German `home.title`, and after `waitForHydration` `collectPageErrors` reports no errors. The same page with the default `en-US` context shows `lang="en"`.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **English date format**: `10 Dec 2026` / `10 Dec 2026, 14:30` (F1). German `10.12.2026` / `10.12.2026 14:30`.
2. **Login adopts `users.locale`** (option A): the session and cookie take the account language at login (F3; auth AC47).
3. **`Accept-Language` on a first visit** (no session or cookie locale): yes (F2).
4. **Unused keys fail** the key-coverage test (F5).
5. **Split**: the shared parts form this slice, built before auth; slice 7 (`i18n-polish`) keeps the language switch, the landing page and the remaining strings.
