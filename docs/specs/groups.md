---
slice: groups
status: approved
---
# Groups

Scope: create, edit and delete groups, list my groups, show a group, join via invitation code, leave, regenerate the invitation code. Added in the redesign: admin removes a member, admin transfers admin rights.
Not in scope (hooks only): wishlists, exclusions and the draw (`GroupRepository::performDraw` / `drawSecretSanta`, draw notifications). These are later slices.

Fresh start: no production data is migrated and users register again. The Rust app owns its schema; the PHP schema is only a reference for shapes and names.

Revised 2026-10-31: aligned with i18n-foundation (built before auth). Dates are displayed with `app::format` and read/written through the `tz` date-input helpers; `group.memberCount` and `flash.error.too_many_code_attempts` are plural keys. AC5 was revised; AC46 and AC47 were added.
Revised 2026-11-01 (implementation): a migration (next free number, 0005) changes `groups.invitation_code` to `utf8mb4_bin` (CLAUDE.md identity-column rule from the auth review; the baseline column is `utf8mb4_unicode_ci`). This replaces "no migration" for this slice.

## Current PHP behaviour

### Routes (`src/Core/Application.php`)

Every handler calls `requireAuth()`. When the user is not logged in, the handler stores `REQUEST_URI` as `intended_url`, sets flash `auth.login.required` and redirects to `/auth/login`. After a successful login the user is redirected to `intended_url`.

| Method | Path | Handler | Authorization | What it does |
|---|---|---|---|---|
| GET | `/groups` | `index` | logged in | Lists groups the user is a member of (`JOIN group_members`, `ORDER BY created_at DESC`) and loads each group's admin. |
| GET | `/groups/create` | `create` | logged in | Create form. |
| POST | `/groups/create` | `store` | logged in | Creates the group and the admin's membership. Redirects to `/groups/{id}`. |
| GET | `/groups/join` | `showJoin` | logged in | Join form. `?code=` pre-fills the input. |
| POST | `/groups/join` | `join` | logged in | Joins via `invitation_code`. |
| GET | `/groups/:id` | `show` | member | Group details, members, admin box, own assignment. |
| GET | `/groups/:id/edit` | `edit` | `admin_id == me` (membership not checked) | Edit form. |
| POST | `/groups/:id/edit` | `update` | `admin_id == me` | Updates name, description and dates. |
| GET | `/groups/:id/delete` | `delete` | `admin_id == me` | Deletes the group (children are deleted by hand in a transaction). |
| GET | `/groups/:id/leave` | `leave` | member, not admin | Deletes the `group_members` row. |
| GET | `/groups/:id/draw` | `draw` | `admin_id == me` | **draw slice** |
| GET | `/groups/:id/regenerate-invitation` | `generateInvitationLink` | `admin_id == me` | Sets a new invitation code. |

The router matches `:id` with `[^/]+`, and the handlers are typed `int $id`. `/groups/create` and `/groups/join` are registered before `/groups/:id`, so they win.

### Create (`store`) / update (`update`)
- Inputs: `name`, `description`, `registration_deadline` (`<input type=date>`) and `draw_date` (`<input type=date>`).
- Validation: `empty($name)` produces the hard-coded English error "Group name is required" (not i18n). Old input is flashed and the user is redirected back to the form. There is no length check and no trimming, so a name of only spaces passes.
- Dates are parsed with `strtotime`. An unparseable value is silently ignored. A valid value is stored as `Y-m-d 23:59:59` (deadline) or `Y-m-d 12:00:00` (draw date). PHP runs with timezone UTC (not configured), and the DB session runs in Europe/Berlin, so the stored wall-clock is 23:59:59 / 12:00:00 **Berlin**.
- Update: an empty date field leaves the old value. **A date can never be cleared once set.**
- Create: `createGroup` inserts the group with a code from `generateInvitationCode()` (it loops until the code is unused), then inserts the admin's `group_members` row. The dates are then set and the group is saved a second time. None of this runs in a transaction.
- Flash on success: `flash.success.group_created` / `group_updated`. The user is redirected to `/groups/{id}`.

### Show
- A missing group gives `flash.error.group_not_found` and a redirect to `/groups`. A non-member gets `flash.error.not_group_member` and a redirect to `/groups`. The two messages differ, so they reveal whether a group id exists.
- The page shows the name, description, status (drawn / not drawn), admin name, deadline, draw date and created date (`F j, Y`, English only). Every member's **name and email** is visible to all members, with badges for "Admin" and "You".
- Admin only: an edit button, and a dropdown with "Regenerate code", "Perform draw" (if not drawn) and "Delete" (each a GET link with a JS `confirm`). An invitation code box with a copy button, and the link `REQUEST_SCHEME://HTTP_HOST/groups/join?code=CODE`, which is built from the Host header and not escaped.
- Non-admins: a "Leave group" GET link with a confirm dialog.
- A second "Perform draw" button appears in the details card only when there are 3 or more members. The dropdown entry has no such threshold.
- If drawn: own assignment card (receiver name, link to their wishlist). **draw slice**
- "Manage exclusions" link (`/exclusions/{id}`) when not drawn. "Your wishlist" card with links to `/wishlist/edit/{id}` and `/wishlist/view/{me}/{id}`.

### Join
- An empty code gives `flash.error.invitation_code_required`. An unknown code gives `flash.error.invalid_invitation_code`. Both redirect to `/groups/join`.
- Already a member: flash **info** `flash.info.already_member`, redirect to the group. The layout only renders `success`, `error` and `errors`, so **this message is never shown**.
- If `registration_deadline` is set and `strtotime(deadline) < time()`, the user gets `flash.error.registration_deadline_passed`. The deadline string read from the DB is Berlin wall-clock but is interpreted as UTC, so the deadline actually ends 1–2 h late.
- **`is_drawn` is not checked: users can join a drawn group.**
- Inserts `group_members` and flashes `flash.success.group_joined`.
- The code lookup is `WHERE invitation_code = ?` with the default (case-insensitive, PAD SPACE) collation. In practice it ignores case and trailing spaces, but the help text says "case-sensitive".

### Leave
- A missing group goes to `/groups`. The admin gets `flash.error.admin_cannot_leave`. A non-member gets `flash.error.not_group_member`.
- Only the `group_members` row is deleted. The leaver's wishlist, exclusion rules (both directions) and gift assignments remain. **Leaving after the draw is allowed.** The leaver's giver still "gives" to someone outside the group, and the leaver still sees their assignment on the dashboard (`findByGiverId`, not filtered by membership). Rejoining revives all of it.

### Delete
- Admin only. A transaction deletes members, assignments and exclusion rules one by one, then the group. Wishlists and `email_templates` are removed only by `ON DELETE CASCADE`. On error the user gets `flash.error.group_delete_failed` **with the raw exception message**.

### Regenerate invitation
- Admin only. `substr(str_shuffle('ABCDEFGHJKLMNPQRSTUVWXYZ23456789'), 0, 8)` with **no uniqueness check** (a collision becomes a 500 from the UNIQUE constraint). Flash `flash.success.invitation_generated`.

### Emails
- None are sent by this slice. `EmailService::sendInvitation` is **never called**. It links to `/group/join/{code}` (a route that does not exist) and interpolates names into HTML without escaping.

### Data
- `groups(id, name VARCHAR(255) NOT NULL, description TEXT NULL, admin_id INT NOT NULL FK users ON DELETE CASCADE, invitation_code VARCHAR(32) NOT NULL UNIQUE, registration_deadline TIMESTAMP NULL, draw_date TIMESTAMP NULL, is_drawn BOOLEAN DEFAULT FALSE (nullable), created_at, updated_at)`
- `group_members(id, group_id FK CASCADE, user_id FK CASCADE, joined_at, UNIQUE(group_id, user_id))`
- Children of a group that use `ON DELETE CASCADE`: `group_members`, `gift_assignments`, `wishlists` (→ `wishlist_items`), `exclusion_rules`. PHP also has an unused `email_templates` table with a cascading FK; the draw slice's migration drops it.
- Invariant assumed but not enforced: the admin has a `group_members` row. If that row were missing, the admin could not open their own group.

## Problems in the PHP version
1. **CSRF**: delete, leave, regenerate and draw are GET links. Any page can trigger them.
2. Invitation codes come from `str_shuffle` (not a CSPRNG, no repeated characters). Regenerate does not check uniqueness, so a collision gives a 500.
3. The invite link is built from the `Host` header (host header injection) and printed without escaping.
4. Users can join after the draw. They get no assignment and nobody gives to them.
5. Members can leave after the draw. This breaks the assignment chain, leaves dangling assignments and the dashboard still shows them.
6. Leaving leaves orphaned wishlists and exclusions behind. Rejoining silently revives them.
7. The admin can neither leave nor hand the group over to someone else. There is no way to remove a member, so a leaked code cannot be cleaned up.
8. Dates cannot be cleared. Invalid dates are silently ignored. There is no check that the draw date comes after the deadline.
9. The deadline comparison is off by the UTC/Berlin offset.
10. The name is not trimmed and has no length limit (>255 chars gives a DB error and a 500). The validation messages are not i18n.
11. Show reveals whether a group id exists (different messages for "not found" and "not member").
12. All members see every member's email address.
13. The delete failure flash leaks the exception text.
14. `info`/`warning` flashes are never rendered (`already_member` is invisible).
15. Children are deleted by hand despite `ON DELETE CASCADE`. Group create is not transactional.
16. The join help text says "case-sensitive" but the lookup is not.
17. `draw_date` does nothing: it is display-only and nothing runs automatically. The UI ("When the draw will take place") suggests otherwise.
18. `sendInvitation` is dead code with a broken link.
19. There are two "Perform draw" entries with different preconditions (draw slice).

## Redesign

### Decisions taken in this spec
- **D1 Admin model.** One admin per group (`groups.admin_id`). The admin is always also a member: `create_group` inserts the admin's membership in the same transaction, `transfer_admin` only targets current members, and the admin can neither leave nor be removed.
- **D2 Authorization.** Every server fn calls `require_user()` and then `authz::require_member(group_id)` or `authz::require_admin(group_id)`. A missing group and a group the user is not a member of give **the same `NotFound` error**, so group ids cannot be enumerated. A member calling an admin-only fn gets `Forbidden`. IDs from the client are only used as lookup keys and are never trusted for ownership.
- **D3 Joining** requires all of: a valid code, not yet a member, `is_drawn = false`, and either no deadline or `now ≤ registration_deadline`. Joining a drawn group is rejected.
- **D4 Leaving** is allowed for non-admin members **only before the draw**. After the draw, nobody can leave or be removed. The recovery path for a dropout after the draw is the draw slice's "reset draw" (see Resolved 2).
- **D5 Data of a departing member** (leave or removal, both only possible before the draw) is deleted in the same transaction: their `group_members` row, their `wishlists` row for the group (items cascade), every `exclusion_rules` row in the group where they are `user_id` **or** `excluded_user_id`, and, defensively, any `gift_assignments` row in the group where they are giver or receiver. One shared function `membership::remove_member_data(tx, group_id, user_id)` does this. Later slices that add per-member group data must extend it.
- **D6 Admin rights can be transferred** to another current member at any time (before or after the draw). The old admin stays a member. Transferring does not reveal any assignment. After a transfer, the former admin can leave (before the draw) like any member.
- **D7 Admin removes a member** (not themselves), only before the draw. The same data deletion as D5 applies. No email is sent.
- **D8 `draw_date` is informational only.** Nothing is automatic. It is shown on the group page with a hint that the admin performs the draw.
- **D9 Dates.** The UI uses a date picker (date only) in `APP_TIMEZONE` (default Europe/Berlin). For storage, the deadline date D becomes `D 23:59:59` in `APP_TIMEZONE` → UTC, and the draw date D becomes `D 12:00:00` in `APP_TIMEZONE` → UTC. For display, the UTC value is shown with the page's `I18n::date` (i18n-foundation `app::format`: converted to `APP_TIMEZONE`, date only; en `10 Dec 2026`, de `10.12.2026`). Form values come from `tz::to_date_input_value` and are parsed with `tz::parse_date_input`; no ad-hoc chrono formatting (foundation rule C10). An empty field **clears** the value (`NULL`). If both are set, the draw date must not be earlier than the deadline date. Past dates are allowed (no check), so editing an old group never fails because of its old dates.
- **D10 Invitation code.** 8 characters drawn uniformly with a CSPRNG (`rand::rngs::OsRng` or `getrandom`) from `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` (32 symbols, 40 bits, repeated characters allowed). On a UNIQUE collision, retry up to 5 times, then return an internal error. Input normalisation: trim, remove spaces and `-`, uppercase. Lookup is exact on the normalised value.
- **D11 Invitation link format.** The link is `{APP_URL}/join/{CODE}`, built from config and never from request headers. Only the admin sees the code and link. Opening the link **never joins on GET**: existing members go directly to the group (#59); nonmembers see a confirmation page with a POST button.
- **D12 Regenerating** the code invalidates the old one immediately. It is allowed at any time (admin only). Members who already joined stay; removal is a separate action (D7).
- **D13 Delete** is admin only, allowed at any time (also after the draw), and is a single `DELETE FROM \`groups\` WHERE id = ?` that relies on `ON DELETE CASCADE`. No emails are sent.
- **D14 Member emails** are shown to the admin only. Other members see names only.
- **D15 No emails** are sent by this slice. `sendInvitation` is not ported.
- **D16 Concurrency.** `join_group`, `leave_group`, `remove_member` and `transfer_admin` run in a transaction that first runs `SELECT … FROM \`groups\` WHERE id = ? FOR UPDATE` and re-checks `is_drawn` under that lock. The draw slice must take the same lock, so a join or leave cannot interleave with a draw. A duplicate-key error on the membership insert counts as `AlreadyMember`.
- **D17** `groups` is always backtick-quoted in SQL. `is_drawn IS NULL` is treated as false.

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/groups` | My groups list | logged in |
| `/groups/new` | Create group form | logged in |
| `/groups/:id` | Group page | member (anyone else: 404 page) |
| `/groups/:id/edit` | Edit form | admin (member: 403 page; anyone else: 404 page) |
| `/join` | Enter-code form | logged in |
| `/join/:code` | Invitation preview + "Join" button | logged in |

- A logged-out user on any of these pages is redirected to login with a return path (auth slice `?next=` mechanism; the return path must be a same-origin relative path). After login **or registration** they land back on the same URL (auth AC6, AC29, AC30). For `/join/:code` this is required for the invite flow.
- `:id` that is not a positive integer gives the 404 page.
- Mutations are server fns only (POST). After a successful mutation the client navigates as listed per fn, and a flash/toast shows the message key.

### Server functions

All server fns call `require_user()` first. Without a session they return `Unauthorized` (the auth slice's error for anonymous calls), and the UI redirects to login. The error names below map to i18n keys in UI notes.

| Fn | Auth | Result | Errors |
|---|---|---|---|
| `list_my_groups()` | user | `Vec<GroupSummary { id, name, description, is_admin, admin_name, member_count, status, created_at }>`, ordered `created_at DESC, id DESC` | – |
| `get_group(group_id)` | `require_member` | `GroupDetail` (below) | `NotFound` |
| `create_group(input: GroupInput)` | user | `group_id`; navigate to `/groups/{id}`, flash `group_created` | `Validation{field→key}` |
| `update_group(group_id, input: GroupInput)` | `require_admin` | `()`; navigate to `/groups/{id}`, flash `group_updated` | `NotFound`, `Forbidden`, `Validation` |
| `delete_group(group_id)` | `require_admin` | `()`; navigate to `/groups`, flash `group_deleted` | `NotFound`, `Forbidden` |
| `regenerate_invitation(group_id)` | `require_admin` | new code; stay on page, flash `invitation_generated` | `NotFound`, `Forbidden` |
| `preview_invitation(code)` | user | `InvitationPreview { group_id, name, admin_name, member_count, already_member, closed: Option<ClosedReason{Drawn, DeadlinePassed}> }` | `CodeRequired`, `InvalidCode`, `TooManyAttempts{minutes}` |
| `join_group(code)` | user | `group_id`; navigate to `/groups/{id}`, flash `group_joined` | `CodeRequired`, `InvalidCode`, `TooManyAttempts{minutes}`, `AlreadyMember{group_id}` (UI navigates to the group with info flash `already_member`), `RegistrationClosed` (deadline), `AlreadyDrawn` |
| `leave_group(group_id)` | `require_member` | `()`; navigate to `/groups`, flash `left_group` | `NotFound`, `AdminCannotLeave`, `AlreadyDrawn` |
| `remove_member(group_id, user_id)` | `require_admin` | `()`; stay, flash `member_removed` | `NotFound`, `Forbidden`, `CannotRemoveSelf`, `TargetNotMember`, `AlreadyDrawn` |
| `transfer_admin(group_id, user_id)` | `require_admin` | `()`; stay (now as a member), flash `admin_transferred` | `NotFound`, `Forbidden`, `TargetIsSelf`, `TargetNotMember` |

`GroupInput { name: String, description: String, registration_deadline: Option<NaiveDate>, draw_date: Option<NaiveDate> }`. Validation, with all errors returned together:
- `name`: trimmed. Empty gives `validation.group.name_required`. More than 255 characters gives `validation.group.name_too_long`.
- `description`: trimmed. Empty is stored as `NULL`. More than 2000 characters gives `validation.group.description_too_long`.
- Both dates set and `draw_date < registration_deadline` gives `validation.group.draw_before_deadline` on `draw_date`.
- A date string that `tz::parse_date_input` rejects (anything but a real `YYYY-MM-DD` date) gives `validation.group.invalid_date` on that field. It is never silently ignored.

`GroupDetail`:
- `id, name, description, created_at, registration_deadline (UTC|null), draw_date (UTC|null), is_drawn`
- `status`: `Drawn` | `RegistrationClosed` (deadline passed, not drawn) | `Open`. Computed **on the server** (no `Utc::now()` during render).
- `is_admin`, `admin { id, name }`
- `members: Vec<{ user_id, name, email: Option<String> (Some only when the viewer is admin), is_admin, is_me }>`, ordered by `joined_at, id`
- `invitation: Option<{ code, link }>` (Some only for the admin; `link` = `{APP_URL}/join/{code}`)
- `can_leave` (= not admin and not drawn)
- It never contains assignment data. The viewer's own assignment comes from a draw-slice fn.

`create_group` runs in one transaction: insert the group with a fresh code (D10), then insert `group_members(group_id, user_id = me)`.

### Data & migrations

Tables touched: `groups` (CRUD), `group_members` (CRUD). Delete-only through D5: `wishlists` (→ `wishlist_items` cascade), `exclusion_rules`, `gift_assignments`. The unused `email_templates` table is dropped by the draw slice's migration and plays no part in this slice.

No new columns. One migration (0005, see the revision note at the top) changes `groups.invitation_code` to `utf8mb4_bin`. `delete_group` relies on the `ON DELETE CASCADE` foreign keys of the child tables `group_members`, `gift_assignments`, `wishlists` (→ `wishlist_items`) and `exclusion_rules`; AC22 checks them.

### UI notes

**`/groups` list.** Header with "Create new group" (`group.createButton`) → `/groups/new` and "Join a group" (`group.joinButton`) → `/join`. Empty state: `group.emptyMessage` + `group.emptyMessageAction`. Each card shows the name, description (or `group.noDescription`), an Admin badge (`group.admin`) if I am admin, a status badge (`group.status.drawn` / `group.status.registrationClosed` (new) / `group.status.open` (new)), the member count (`group.memberCount` (new, plural: `.one` / `.other`, `{count}`; en "{count} member" / "{count} members", de "{count} Mitglied" / "{count} Mitglieder"; rendered with `t_plural!(i18n, "group.memberCount", n, count = n)`)), the admin name (`group.adminLabel`) and a link (`group.viewDetails`).

**`/groups/new`, `/groups/:id/edit`.** Fields and help texts reuse `group.form.*`. Edit pre-fills the current values. Dates are shown as local dates, and an empty input means "none". Field errors appear inline under the field. Buttons: `group.form.createButton` / `group.form.updateButton`, and cancel `group.form.cancel`. Titles: `group.create.title` / `group.edit.title`. Update the text of `group.form.drawDateHelp` to say that the date is informational and the admin starts the draw.

**`/groups/:id` group page.**
- Header: name, description (line breaks preserved, escaped).
- Admin actions: Edit (`group.show.editButton`), Regenerate code (`group.show.regenerateCode`), Delete (`group.show.deleteGroup`, confirm `group.show.deleteGroupConfirm`). Each mutation is a button that calls its server fn after a confirm dialog (delete and regenerate confirm; regenerate confirm `group.show.regenerateCodeConfirm` (new): "The old code and link stop working.").
- Non-admin, not drawn: Leave (`group.show.leaveGroup`). Change the confirm text of `group.show.leaveGroupConfirm` to mention that their wishlist and exclusions for this group are deleted. After the draw, show the hint `group.show.cannotLeaveAfterDraw` (new) instead of the Leave button.
- Details card: status, admin, deadline, draw date with hint `group.show.drawDateInfo` (new), created on. Labels reuse `group.show.*`.
- Invitation card (admin only): `group.show.invitationCode`, `group.show.shareCode`, the code, a copy button (`group.show.copy`, then a toast `group.show.copied` (new)), `group.show.shareLink` + link. If the status is not `Open`, also show `group.show.registrationClosedHint` (new).
- Members card: `group.show.members` (`{count}`). Each row shows the name, badges `group.admin` / `group.show.you`, and the email (admin only). Admin, per other member: "Make admin" (`group.show.makeAdmin` (new), confirm `group.show.makeAdminConfirm` (new)), and before the draw "Remove" (`group.show.removeMember` (new), confirm `group.show.removeMemberConfirm` (new)).
- **Hooks for later slices** (render nothing until that slice exists; groups tests must not depend on them):
  - `<WishlistPanel group_id>` (wishlists slice): own wishlist card. Old keys `group.show.yourWishlist`, `editWishlist`, `wishlistHelp`, `viewYourWishlist*` belong to that slice.
  - `<ExclusionsLink group_id is_drawn>` (exclusions slice): "Manage exclusions" (`group.show.manageExclusions`), members only, before the draw.
  - `<DrawPanel group_id is_admin is_drawn member_count>` (draw slice): the single "Perform draw" action for the admin before the draw. After the draw it shows the viewer's own assignment via a draw-slice fn that only returns the caller's own row and requires current membership. Old keys `group.show.performDraw*`, `group.show.assignment*`, `group.show.viewWishlist` belong to that slice.

**`/join`.** `group.join.title`, `group.join.description`, input `group.join.code` with placeholder `group.join.codePlaceholder`. Change the text of `group.join.codeHelp` to "not case-sensitive". Submit (`group.join.submit`) navigates to `/join/{normalised code}`. Cancel: `group.join.cancel`.

**`/join/:code`.** Calls `preview_invitation`.
- Valid and open: show `group.join.previewTitle` (new, `{name}`), admin name, member count, and a "Join group" button (`group.join.submit`) that calls `join_group`.
- Already a member: show `flash.info.already_member` and a link to the group.
- Closed: show `flash.error.registration_deadline_passed` or `flash.error.join_closed_drawn` (new), with no button.
- Invalid: show `flash.error.invalid_invitation_code` and the enter-code form.

**Flash/toast keys.** The foundation renders success/info/warning/error.
- Reused: `flash.success.group_created`, `group_updated`, `group_deleted`, `group_joined`, `left_group`, `invitation_generated`, `flash.info.already_member`, `flash.error.group_not_found`, `invitation_code_required`, `invalid_invitation_code`, `registration_deadline_passed`, `admin_cannot_leave` (new text: "…transfer admin rights to another member or delete the group").
- New: `flash.error.join_closed_drawn`, `flash.error.cannot_leave_after_draw`, `flash.error.cannot_remove_after_draw`, `flash.error.cannot_remove_self`, `flash.error.target_not_member` (or reuse `flash.error.user_not_in_group`), `flash.error.target_is_self`, `flash.error.forbidden_admin_only`, `flash.success.member_removed`, `flash.success.admin_transferred`, `validation.group.name_required`, `validation.group.name_too_long`, `validation.group.description_too_long`, `validation.group.draw_before_deadline`, `validation.group.invalid_date`.
- Removed (not ported): `flash.error.group_delete_failed` (leaked `{error}`; a generic 500 is shown instead), `flash.error.not_group_member` for group pages (replaced by the 404), and `flash.error.not_authorized_edit` / `not_authorized_delete` / `only_admin_invitation` (replaced by `forbidden_admin_only`).

All new keys exist in en and de, and are added together with the code that uses them; reused keys are copied from the PHP catalogs (i18n-foundation rule). Dates are formatted with `app::format` (date only), created date included. Any message that puts a count next to a counted noun uses `t_plural!`.

## Acceptance criteria

Fixtures: users Alice, Bob, Carol, Dave. G = group with admin Alice and members Alice and Bob, not drawn, no dates, unless an AC says otherwise.

**Create**
- AC1: Given Alice is logged in, when she submits create with name "  Family 2026  ", a description and no dates, then a group "Family 2026" exists with `admin_id = Alice`, Alice is its only member, she is on `/groups/{id}` and sees `group_created`.
- AC2: Given a create submit with a name that is empty or only spaces, then no group is created and the name field shows `validation.group.name_required`.
- AC3: Given a create submit with a 256-character name, then no group is created and `validation.group.name_too_long` is shown. A 255-character name succeeds.
- AC4: Given a create submit with a description of 2001 characters, then `validation.group.description_too_long` is shown. An empty description is stored as `NULL`.
- AC5: Given a create submit with deadline 2026-12-10 and draw date 2026-12-12 (`APP_TIMEZONE` Europe/Berlin), then the DB holds `registration_deadline = 2026-12-10 22:59:59 UTC` and `draw_date = 2026-12-12 11:00:00 UTC`, and the group page shows the dates `10 Dec 2026` and `12 Dec 2026` in English (`10.12.2026` and `12.12.2026` in German).
- AC6: Given a create submit with deadline 2026-12-12 and draw date 2026-12-10, then nothing is saved and the draw date field shows `validation.group.draw_before_deadline`.
- AC7: Given a call to `create_group` with an unparseable date, then `validation.group.invalid_date` is returned for that field and nothing is saved.
- AC8: The invitation code of a new group is 8 characters from `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` and is unique. When a code collision is forced in a test, the retry yields a different unique code.

**List / show**
- AC9: Given Alice is admin of G and a member of H (admin Carol, created later), when she opens `/groups`, then she sees H before G, the Admin badge only on G, member counts and admin names, and no group she is not a member of.
- AC10: Given Dave has no groups, when he opens `/groups`, then he sees the empty-state message and the Create and Join buttons.
- AC11: Given Bob is a member of G, when he opens `/groups/{G}`, then he sees the name, status `Open`, admin Alice, and member names with "You" on himself, but **no emails, no invitation code, no admin actions**, and a Leave button.
- AC12: Given Alice opens `/groups/{G}`, then she sees the member emails, the invitation code, the link `{APP_URL}/join/{code}` and the admin actions, and no Leave button.
- AC13: Given Dave (not a member) opens `/groups/{G}`, or anyone opens `/groups/999999`, then the same 404 page is shown. `get_group` returns `NotFound` in both cases.
- AC14: Given a logged-out visitor opens `/groups/{G}`, then they are redirected to login and, after logging in as Bob, land on `/groups/{G}`. Every server fn of this slice called without a session returns `Unauthorized`.
- AC15: Given a stored deadline in the past and G not drawn, then G's status is `RegistrationClosed` on the list and the group page. Given `is_drawn = 1`, the status is `Drawn`.
- AC16: `get_group` never returns assignment data for any caller, admin included.

**Edit**
- AC17: Given Alice edits G to a new name, a new description and a deadline, then the values are saved, she lands on `/groups/{G}` and sees `group_updated`.
- AC18: Given G has a deadline and a draw date, when Alice saves the edit form with both date fields empty, then both columns become `NULL`.
- AC19: Given G has `registration_deadline = '2025-12-01 22:59:59'` UTC (2025-12-01 23:59:59 Berlin, a past date), when the edit form opens, then the deadline field shows 2025-12-01. Saving unchanged succeeds and keeps the same stored value.
- AC20: Given Bob (member, not admin) opens `/groups/{G}/edit` or calls `update_group(G, …)`, then he gets the 403 page / `Forbidden` and nothing changes. Dave (non-member) gets `NotFound`.
- AC21: Edit validation follows AC2–AC7 and leaves the stored row unchanged on error.

**Delete**
- AC22: Given G has members, wishlists with items, exclusion rules and (drawn) gift assignments, when Alice confirms Delete, then the `groups` row and all child rows in `group_members`, `wishlists`, `wishlist_items`, `exclusion_rules` and `gift_assignments` for G are gone, and she lands on `/groups` with `group_deleted`.
- AC23: Given Bob calls `delete_group(G)`, then `Forbidden` and G still exists. Given Dave calls it, then `NotFound`.
- AC24: Given a plain GET or a cross-origin POST to the delete endpoint, then nothing is deleted.

**Invitation code**
- AC25: Given Alice clicks Regenerate and confirms, then G has a new code in the D10 format, the page shows the new code and link, `invitation_generated` is shown, and `/join/{old code}` now shows `invalid_invitation_code`.
- AC26: Given Bob calls `regenerate_invitation(G)`, then `Forbidden` and the code is unchanged.

**Join**
- AC27: Given Carol is logged in and opens `/join/{G code}`, then she sees G's name, admin Alice, member count 2 and a Join button, **and she is not yet a member**. When she clicks Join, she is a member, lands on `/groups/{G}` and sees `group_joined`.
- AC28: Given Carol enters the code lowercased with surrounding spaces or a dash (e.g. ` abcd-efgh `) on `/join`, then she reaches the preview for G.
- AC29: Given an empty code, `join_group` returns `CodeRequired`. Given an unknown code, `/join/XXXXXXXX` shows `invalid_invitation_code` and `join_group` returns `InvalidCode`.
- AC30: Given Bob (already a member) opens `/join/{G code}`, then he is redirected directly to G (including after login and after the draw/deadline), without offering a join or writing group data (#59). `join_group` returns `AlreadyMember{G}` and no second membership row exists.
- AC31: Given G's deadline is in the past, when Carol opens the link, then she sees `registration_deadline_passed` and no Join button. `join_group` returns `RegistrationClosed`.
- AC32: Given G's deadline is today (stored as today 23:59:59 Berlin) and it is 23:00 Berlin, then Carol can join.
- AC33: Given G is drawn, then Carol sees `join_closed_drawn` and `join_group` returns `AlreadyDrawn`. The membership count is unchanged.
- AC34: Given a logged-out visitor opens `/join/{code}`, then they are sent to login. After logging in, or after registering and completing registration via the mail link, they land on `/join/{code}` (the preview, not an automatic join).

**Leave**
- AC35: Given Bob is in G (not drawn) with a wishlist with items, an exclusion rule Bob→Alice and a rule Alice→Bob, when he confirms Leave, then his membership, his wishlist and items for G, and both exclusion rules are deleted, Alice's own data is untouched, and he lands on `/groups` with `left_group`. G no longer appears in his list.
- AC36: Given Alice (admin) calls `leave_group(G)`, then `AdminCannotLeave` and nothing changes. The page shows no Leave button for her.
- AC37: Given G is drawn, then Bob sees no Leave button and `leave_group(G)` returns `AlreadyDrawn`. His membership and all assignments are unchanged.
- AC38: Given Dave calls `leave_group(G)`, then `NotFound`.

**Remove member**
- AC39: Given G not drawn, when Alice removes Bob, then the same data as in AC35 is deleted for Bob, the page shows `member_removed`, and Bob then gets the 404 page for `/groups/{G}`.
- AC40: Given Alice calls `remove_member(G, Alice)`, then `CannotRemoveSelf`. `remove_member(G, Dave)` gives `TargetNotMember`. When G is drawn, `remove_member(G, Bob)` gives `AlreadyDrawn`. Bob calling `remove_member(G, Alice)` gives `Forbidden`. Nothing changes in any of these cases.

**Transfer admin**
- AC41: Given Alice transfers admin to Bob, then `groups.admin_id = Bob` and Alice is still a member. On reload, Bob sees the admin actions and code, Alice sees member view and a Leave button (if not drawn), and `admin_transferred` is shown.
- AC42: Given G is drawn, transfer still succeeds, and neither the old nor the new admin can obtain anyone else's assignment.
- AC43: `transfer_admin(G, Alice)` by Alice gives `TargetIsSelf`. `transfer_admin(G, Dave)` gives `TargetNotMember`. Bob calling `transfer_admin(G, Bob)` gives `Forbidden`.

**Concurrency**
- AC44: Given a join and a draw for G running at the same time, then either the joiner is a member and the draw includes them, or the join fails with `AlreadyDrawn`. There is never a member without an assignment in a drawn group created this way. (Integration test: hold the `FOR UPDATE` lock in one transaction and assert that the second waits and then sees the updated `is_drawn`.)
- AC45: Given Carol has submitted 10 invalid codes (via `preview_invitation` or `join_group`) within 10 minutes, then her 11th call, even with G's valid code, returns `TooManyAttempts{minutes}` with `1 <= minutes <= 10` (UI: `flash.error.too_many_code_attempts`, new en+de, plural `.one` / `.other` on `{minutes}` like auth's `flash.error.too_many_attempts`) and creates no membership. Another user is unaffected. After the window passes (injectable clock), Carol can join with the valid code. Valid codes do not count towards the limit. The limiter is in memory, per user id, and reuses the auth slice's rate-limiter type.

**i18n** (added 2026-10-31, i18n-foundation)
- AC46: The groups list and the invitation preview show `1 member` / `1 Mitglied` for a one-member group and `2 members` / `2 Mitglieder` for two. The `TooManyAttempts{minutes: 1}` message of AC45 uses the singular form ("1 minute" / "1 Minute").
- AC47: Given G has deadline date 2026-12-10, a draw date 2026-12-12 and was created on 2026-11-01 (Berlin), when Bob opens `/groups/{G}` in German, then the details card shows `10.12.2026`, `12.12.2026` and `01.11.2026`, and in English `10 Dec 2026`, `12 Dec 2026` and `1 Nov 2026`. The browser console shows no hydration error on that page in either locale (`collectPageErrors`). The edit form's date inputs hold `2026-12-10` and `2026-12-12` in both locales.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **Orphan cleanup migration** (delete wishlists/exclusions that PHP "leave" left behind): moot. Fresh start, so there is no legacy data and no cleanup migration. Going forward, D5 deletes a departing member's data in the same transaction.
2. **Dropout after the draw**: yes, the draw slice provides an admin "reset draw".
3. **Automatic draw on `draw_date`**: no. `draw_date` is informational only. No background job.
4. **Invite by email**: no. Sharing the link is enough.
5. **Brute force on codes**: yes, a small in-memory limit per user on invalid codes (10 per 10 minutes), in this slice.
6. **Who may see and share the invite link**: admin only.
7. **Member emails**: visible to the admin only.

2026-10-31: AC22 no longer asserts email_templates; the table is dropped by the draw migration.
