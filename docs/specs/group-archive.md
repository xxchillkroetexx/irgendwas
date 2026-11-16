---
feature: group-archive
status: approved
---
# Group archive (#61)

Issue #61 "archive old draw groups after 6 months since draw date": "and give users an
archive of old happenings". Maintainer comment: "The admin should be notified and the owner
should be able to archive the group as well."

Scope: an archived state for groups (read-only, kept out of "My groups"), an archive page
listing them, automatic archiving (drawn groups 6 months after the draw, never-drawn groups
after 9 months) with a warning mail to the group admin 14 days before, "Keep active", and
manual archive/restore by the group admin.
Not in scope: deleting old data automatically (archived groups keep all their rows), any
change to the draw algorithm or the secrecy rules, a site-wide admin role.

Builds on the approved specs `groups`, `draw`, `wishlists`, `exclusions`, `share-target` and
(if it lands first) `dashboard` (#47, branch `kw-codex/47-dashboard`). Their rules stay in
force; "Effect on other specs" lists what changes in them.

Revised 2026-11-16: the user answered the first draft's open questions (section "Decisions
(user, 2026-11-16)"). Changes: a warning mail 14 days before the automatic archive replaces the
mail on the archive date; never-drawn groups are archived too (after 9 months, a later user
decision the same day); "Keep active"; a third column; AC numbering rewritten. Third round
the same day: members never see the archive date (U9, AC37 added, the i18n AC is now AC38),
the undrawn backfill is kept (U10), and the migration number is no longer fixed (P1).

Amended 2026-11-16 after the review of the implementation (#61): the PHP import kept each
group's `updated_at` from the PHP app (it is not the import time), so a backfill from
`updated_at` would let the first sweep archive an old undrawn group without a warning. The
backfill now sets `kept_active_at = CURRENT_TIMESTAMP` for every group that is not drawn (U10,
Data & migrations, AC1). The rollback note is corrected (an older image refuses to start), and
the read-only view of the caller's own list in an archived group has no edit links (UI notes,
AC35).

Amended 2026-11-16 after merging #76 (copy-wishes, `docs/specs/copy-wishes.md`): the copy
chooser (`get_sources`, `?source=`) is replaced by the edit page's copy section
(`get_copy_sources`, `copy_wishlist_items`, `?copy=1`). Read `get_sources` as
`get_copy_sources` (it still lists the caller's own archived lists as sources) and "copy
chooser" as "copy section" (AC28). The edit page of an archived group has no copy section
(AC35); `copy_wishlist_items` into an archived group answers `Archived` before the copy's own
errors (`AlreadyOnList`, `ItemNotCopyable`), and its no-JS failure goes back to `?copy=1` like
every other copy error.

## Current behaviour

**Roles.** A group has exactly one admin, `groups.admin_id` (groups D1). The admin is always a
member and can transfer the role (groups D6). There is no separate owner and no site-wide admin.

**Routes** (all `SsrMode::Async`, behind `RequireUser`): `/groups` (list), `/groups/new`,
`/groups/:id`, `/groups/:id/edit`, `/groups/:id/wishlist` (own list, editable),
`/groups/:id/wishlist/:user_id` (read-only view), `/groups/:id/exclusions`, `/join`,
`/join/:code`, `/user/dashboard`, `/wishes/shared` (share target). `/groups/new` is a static
segment next to `/groups/:id`, so a static `/groups/archive` route works the same way.

**Group lifecycle.** `groups_flow`: create, update (name, description, both dates; an empty
date clears it), delete (any time, cascade), regenerate code, join (closed once drawn or after
the deadline), leave/remove (only before the draw), transfer admin (any time). Every change
takes `SELECT … FROM \`groups\` WHERE id = ? FOR UPDATE` and re-checks rights under the lock
(decisions 2026-11-02 "Every group mutation"). `list_my_groups()` returns every group the
caller is a member of, newest first; it feeds `/groups` and the share-target group picker
(`pages/share.rs`), and #47's dashboard cards reuse it.

**Draw.** `perform_draw` sets `is_drawn = TRUE, drawn_at = CURRENT_TIMESTAMP`; `reset_draw`
deletes the assignments and sets `is_drawn = FALSE, drawn_at = NULL` (draw D6/D7). A group
counts as drawn when `is_drawn AND drawn_at IS NOT NULL` (decisions 2026-11-02 draw).
`groups.draw_date` is informational only, optional and editable (groups D8). Receiver data
leaves the server only through `get_my_assignment(group_id)` and `list_my_assignments()`,
both filtered on `giver_id = <session user>` and current membership (draw S1–S3).

**Wishlists / exclusions.** Wishlist writes take the group row `LOCK IN SHARE MODE`, then the
member row and list rows (wishlists W11, decisions 2026-11-02). Exclusion writes take the group
row `FOR UPDATE`. Exclusion writes are refused after the draw; wishlist writes are allowed
before and after it. `get_sources`/`copy_items` copy own items from any other group the caller
is a member of (#57).

**Background work.** The server process runs two kinds of background task, both started in
`server::run()` (not in `app_state`, so tests do not get them): the session cleanup
(`MySqlSessionStore::spawn_cleanup`, `tokio::time::interval` every 300 s) and the draw mail
worker (`DrawWorker`, spawned in `app_state`, plus `recover()` at startup). There is no other
scheduler. The deployment runs one app process (decisions 2026-11-02 wishlists).

**Data.** `groups(id, name, description, admin_id, invitation_code, registration_deadline,
draw_date, is_drawn BOOLEAN NOT NULL, drawn_at TIMESTAMP NULL, created_at, updated_at ON
UPDATE CURRENT_TIMESTAMP)` after migrations 0001–0008. No archive column exists. `updated_at`
changes on every UPDATE of the row (also on `reset_draw`). Production holds the migrated 2025
data (decisions 2026-11-03: 4 groups, 3 drawn; `drawn_at` = newest assignment, i.e. around
December 2025).

**Mail.** `state.mailer.send(Message)` sends off the request path; `MailHandle::outcome()`
waits for the delivery result. The draw worker sends through the `MailTransport` trait
(`draw_worker.rs`), which tests use to inject failures. Account mails use the recipient's
`users.locale` via `mail_i18n` (`APP_TIMEZONE` for dates) and go to the stored address.

## Problems

- P1 **Migration number.** `dev` ends at `0008_draw.sql`, but the unmerged `kw-codex/dev` /
  PR #84 adds 0009, 0011 and 0012. The migration of this feature therefore has no fixed number:
  it takes the next free number after the highest migration on `dev` at implementation time
  (`0009` if it lands before #84, `0013` if #84 merges first). Neither branch is deployed, so
  whichever lands second simply renames its file before merging; nothing already on `dev` is
  edited (U10). This spec calls the file `NNNN_group_archive.sql`.
- P2 **"Draw date" is ambiguous.** The issue says "6 months since draw date", but
  `groups.draw_date` is an optional, editable, informational date (groups D8). For drawn groups
  this spec uses `drawn_at` (decision U7). For never-drawn groups `draw_date` is one input of
  the reference date (A4).
- P3 **Dates can move backwards.** `update_group` can clear or move the deadline and draw
  date into the past, and `reset_draw` turns a drawn group (clock: `drawn_at`) into an undrawn
  one (clock: its old dates). Without care either would make a group due at once and archive
  it without a warning. A4 handles this with `kept_active_at`.
- P4 **Groups reset since the cutover.** A 2025 group whose draw was reset after the cutover
  (to reuse it for 2026) is undrawn and its dates are from 2025. It would be archived by the
  first sweep without a warning, during the 2026 season. The migration's backfill (Data &
  migrations) protects it.
- P5 `resend_draw_notifications` reads `is_drawn` without the group row lock and does not
  re-check the admin under a lock (the other draw writes do). Harmless today (a concurrent
  reset leaves 0 rows → `NothingToResend`); with this feature the same unlocked read also
  decides `Archived`, so a resend racing an archive can still queue mails. Accepted: the
  mails only repeat the giver's own assignment, which stays visible in the archive.
- P6 (oddity, not changed) `groups_flow::LockedGroup`, `SummaryRow` and `DetailRow` still read
  `is_drawn` as `Option<bool>` although migration 0008 made it `NOT NULL`.
- P7 (coordination) #47 (`kw-codex/47-dashboard`, approved, unmerged) replaces the dashboard
  assignments card with group cards built from `list_my_groups`. If #47 lands first, archived
  groups drop out of the dashboard through `list_my_groups` and AC33's assignment-card part
  falls away; if this lands first, #47 must keep using `list_my_groups` (not its own query).

## Decisions (user, 2026-11-16)

- U1 "Owner" is the group's one admin (`groups.admin_id`); only that role exists.
- U2 Notification: **only** a warning mail to the admin 14 days **before** the automatic
  archive; **no** mail on the archive date itself. The warning is sent once per archive date.
- U3 No mail to members.
- U4 Only the admin may archive and restore.
- U5 Groups that are **not drawn** (never drawn, or drawn and reset) are archived too, after
  **9 months** (drawn groups: 6 months), counted from a reference date that cannot archive a
  group in preparation before its planned draw (A4). They get the same warning 14 days before.
- U6 Rollout: archive right away. Groups already past their archive date at the deploy are
  archived by the first sweep **without** a warning (their warning date has passed); there is
  no "start counting at the deploy" for them. (Confirmed by the user after the second draft.)
- U7 Reference for drawn groups: when the draw actually ran (`drawn_at`).
- U8 Assignments of archived groups leave the dashboard.
- U9 Members do **not** see the archive date. Only the admin sees it (details card, warning
  alert with "Keep active", warning mail); `get_group` returns it to the admin only.
- U10 Backfill kept: groups that are not drawn (never drawn, or reset) get a fresh start at
  the deploy (`kept_active_at = CURRENT_TIMESTAMP` at the migration, `updated_at` itself
  unchanged; Data & migrations; amended 2026-11-16, it was `updated_at`). Accepted cost: such
  groups, including those from the PHP data, are archived only 9 months after the deploy
  (August 2027), after a warning; their admins can archive them by hand this year. The
  migration takes the next free number on `dev` at implementation time (P1).

## Design

### Decisions taken in this spec

- **A1 What "archived" means.** A group is archived iff `groups.archived_at IS NOT NULL`.
  An archived group is **read-only for everybody**: every group, wishlist, exclusion and draw
  mutation returns `Archived`, except `delete_group` (the admin can still delete it for good)
  and `restore_group`. Membership rows, wishlists, exclusion rules and gift assignments are
  kept unchanged. Nothing is deleted by archiving.
- **A2 Who sees what.** An archived group stays visible to its **current members** exactly
  as before, minus every control that changes something: the group page with members (emails
  still only for the admin, groups D14), the details, each member's own assignment (draw S1–S3
  unchanged: only `get_my_assignment`, only the caller's own row, admins included), every
  member's wishlist (read-only), the caller's own exclusion rules (read-only). Non-members get
  the 404 page as before (groups D2).
- **A3 Where it is listed.** `list_my_groups()` (the `/groups` page, the share-target picker,
  #47's dashboard cards) returns only active groups. A new page `/groups/archive` lists the
  caller's archived groups, newest archive first. The dashboard assignments card
  (`list_my_assignments`) leaves archived groups out (U8); the assignment stays on the archived
  group page.
- **A4 The archive clock.** Every active group has an **archive date**
  `auto_archive_at = reference + N calendar months` (UTC, chrono
  `checked_add_months(Months::new(N))`, clamped to the month's last day) and a **warning date**
  `warn_at = auto_archive_at − 14 days`, for drawn and undrawn groups alike. Reference and N:
  - drawn group (`is_drawn AND drawn_at IS NOT NULL`): N = **6**; reference = the latest of
    `drawn_at` and `kept_active_at` (U7);
  - any other group (never drawn, or reset): N = **9**; reference = the latest of
    `created_at`, `registration_deadline`, `draw_date` and `kept_active_at` (NULL values are
    skipped) (U5). A deadline or draw date that is set keeps a group in preparation out of the
    archive until 9 months after that date; a group without dates gets 9 months after its
    creation or the admin's last clock restart, which covers a May group drawn in December.
    Only a group drawn more than 9 months after its last reference (and whose admin ignores the
    warning) is archived before its draw; the admin can restore it.

  `kept_active_at` (new column) is the moment the admin last restarted the clock. It is set to
  `CURRENT_TIMESTAMP` by `keep_group_active`, `restore_group`, `update_group` and `reset_draw`
  (the two changes that can move the reference backwards, P3), in the same UPDATE as their
  other columns. `perform_draw` needs nothing (it sets `drawn_at` to now). Joining, wishlist,
  exclusion and other admin actions do not touch the clock. With these writes the archive date
  never jumps into the past after the deploy (a draw moves it to now + 6 months, a reset or an
  edit to now + 9 months), so a group always passes through its 14-day warning
  window before it is archived, unless the server is down for that whole window or the group
  was already past its date at the deploy (U6).
  One pure function `archive_dates(&GroupClock) -> (warn_at, auto_archive_at)` in
  `app::domain` (no I/O, testable without a DB) computes both, for the sweep and for
  `get_group`.
- **A5 Trigger.** An in-process sweep, `archive_flow::run_archive_sweep(pool, config,
  transport, now)`, runs once right after startup and then every hour (`tokio::time::interval`,
  like the session cleanup), spawned in `server::run()` next to `spawn_cleanup`, never in
  `app_state` (so `#[sqlx::test]` routers have no background sweeper). It runs outside any
  request: it takes the pool, config and a `MailTransport` explicitly and never calls
  `state()`, `session()` or other context helpers (AGENTS.md server-fn context rule). Tests call
  it directly with an explicit `now`.
- **A6 What one sweep does.** It reads the clock columns of every active group
  (`archived_at IS NULL`; a small table) without locks and decides in Rust, per group:
  1. `now >= auto_archive_at` → **archive** (no mail, U2/U6), whether or not a warning was
     sent.
  2. else `now >= warn_at` and no warning for this archive date yet (`archive_warned_at IS NULL
     OR archive_warned_at < warn_at`) → **warn**.
  3. else nothing.

  Each archive or warning runs in its own short READ COMMITTED transaction: `SELECT … FROM
  \`groups\` WHERE id = ? FOR UPDATE` (gone → skip), recompute A4 and the step under the lock
  (restored, kept active, reset, edited or archived meanwhile → skip or do the other step),
  then
  - archive: `UPDATE \`groups\` SET archived_at = ? WHERE id = ? AND archived_at IS NULL`
    (bound `now`);
  - warn: `UPDATE \`groups\` SET archive_warned_at = ? WHERE id = ?` (bound `now`), read the
    current admin's `email, name, locale`, commit, **then** send the warning mail (no DB
    connection is held during SMTP) and await its outcome. On a delivery failure the sweep
    writes the previous value back (`UPDATE … SET archive_warned_at = <previous> WHERE id = ?
    AND archive_warned_at = <now>`), so the next hourly sweep tries again while the window is
    open; it logs group id, admin user id and `MailError::kind()`.

  The warning state needs no reset anywhere: a warning counts only for the archive date it was
  sent for (`archive_warned_at >= warn_at`). Any restart of the clock (keep active, restore,
  edit, reset, a new draw) moves `warn_at` past the old `archive_warned_at`, so the next archive
  date gets its own warning. A failure on one group is logged and the sweep goes on. Two sweeps
  at once (two processes, a test calling it twice) archive a group once and warn once (the
  second one finds the warning under the lock). The sweep returns `SweepResult { warned,
  archived }`.
- **A7 The warning mail** (U2). To the group's admin at the time of the warning
  (`groups.admin_id`, stored address, `users.locale` via `mail_i18n`): the group will be
  archived on `{date}` (`auto_archive_at` as a local date in `APP_TIMEZONE`), what archived
  means, that "Keep active" on the group page restarts the period (6 months for a drawn group,
  9 for an undrawn one), that otherwise
  nothing needs to be done, and that an archived group can be restored at any time. Link to
  `{APP_URL}/groups/{id}`. Members get no mail (U3); manual archive, restore and keep-active
  send no mail. The archive itself happens at the first sweep at or after `auto_archive_at`
  (at most one hour later).
- **A8 Manual archive, restore and keep active** (U4). `archive_group`, `restore_group` and
  `keep_group_active` are admin-only (`authz::require_admin`: members `Forbidden`, others
  `NotFound`).
  - Archive is allowed in any state (drawn or not).
  - Restore sets `archived_at = NULL, kept_active_at = CURRENT_TIMESTAMP`. The group is exactly
    as before archiving (still drawn if it was); its next archive date is 6 months (drawn) or
    9 months (not drawn) after the restore, with a new warning 14 days before.
  - Keep active sets `kept_active_at = CURRENT_TIMESTAMP` on an active group; on an archived
    group it returns `Archived` (restore it instead). It is allowed at any time; the UI offers
    it to the admin once the warning window has begun.
  - Archive and restore are idempotent: archiving an archived group or restoring an active one
    succeeds without changing any column (no-JS double submits). Keep active always moves the
    clock to now.
- **A9 Error order.** Every mutation first runs its existing authorization (Unauthorized →
  NotFound → Forbidden), then checks `archived`, then its own state errors. So a member
  calling `update_group` on an archived group still gets `Forbidden`, a non-member `NotFound`,
  and the admin calling `leave_group` gets `Archived` (not `AdminCannotLeave`). The archived
  check runs **under the lock the mutation already takes** (groups/exclusions/draw: the group
  row `FOR UPDATE`; wishlists: the group row `LOCK IN SHARE MODE`), so a mutation that gets the
  lock after an archive committed sees it, and archive/restore (`FOR UPDATE`) wait for running
  wishlist writes. Lock order stays "group row first".
- **A10 Joining.** An archived group's invitation code stays stored (a restore brings it back)
  but cannot be used: `preview_invitation` reports `closed: Some(Archived)` (before `Drawn` and
  `DeadlinePassed`), `join_group` returns `Archived`. Existing members opening the link are
  still redirected to the group (#59, groups AC30). The admin's group page shows no invitation
  card while archived.
- **A11 Copying wishes from the archive.** An archived group is still a valid **source** for
  `get_sources`/`copy_items` (copy last year's wishes into this year's group); only the
  destination must be active.
- **A12 Authorization helper.** `authz::GroupAccess` gains `archived: bool`, read in the same
  query as admin/membership (`g.archived_at IS NOT NULL AS archived`). Flows check
  `access.archived` from the `require_member`/`require_admin` call they already make under the
  lock and map it to their own `Archived` variant. The draw flow's `lock_group_as_admin` reads
  `archived_at` in its locking select.
- **A13 Logging.** Sweep logs carry group ids, the counts and error kinds only; never names,
  addresses or mail bodies (as draw S5).

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/groups/archive` (new) | Archived groups list | logged in (logged out: login with `next=/groups/archive`) |
| `/groups` | My groups (active only) + link to the archive | logged in |
| `/groups/:id` | Group page; archived: read-only view (UI notes) | member (others: 404 page) |
| `/groups/:id/edit` | Edit form; archived: the form is replaced by the `flash.error.group_archived` notice and a link back to the group | admin (member: 403, others: 404) |
| `/groups/:id/wishlist` | Own list; archived: read-only, no forms | member |
| `/groups/:id/wishlist/:user_id` | Unchanged; archived: the caller's own list has no edit / "add now" links (amended 2026-11-16) | member |
| `/groups/:id/exclusions` | Archived: read-only like after the draw | member |
| `/join/:code` | Archived: `flash.error.join_closed_archived`, no Join button | logged in |

The new route is registered as `<Route path=path!("/groups/archive") view=GroupArchivePage
ssr=SsrMode::Async/>`; the path constant `ARCHIVE_PATH` goes into `paths.rs`.

### Server functions

All call `require_user()` first (anonymous → `Unauthorized`).

| Fn | Auth | Result | Errors |
|---|---|---|---|
| `list_archived_groups()` (new) | user | `Vec<ArchivedGroupSummary { id, name, is_admin, admin_name, member_count, drawn_at: Option<DateTime<Utc>>, archived_at: DateTime<Utc> }>`, member's groups with `archived_at IS NOT NULL`, ordered `archived_at DESC, id DESC`. No assignment data | – |
| `archive_group(group_id)` (new) | `require_admin` | `()`; stay on `/groups/{id}`, flash `group_archived` (plain post: 302 to the group) | `NotFound`, `Forbidden` |
| `restore_group(group_id)` (new) | `require_admin` | `()`; stay, flash `group_restored` | `NotFound`, `Forbidden` |
| `keep_group_active(group_id)` (new) | `require_admin` | `()`; stay, flash `group_kept_active` | `NotFound`, `Forbidden`, `Archived` |
| `list_my_groups()` | user | unchanged type; adds `WHERE g.archived_at IS NULL` | – |
| `get_group(group_id)` | `require_member` | `GroupDetail` + `archived_at: Option<DateTime<Utc>>` and `archive_schedule: Option<ArchiveSchedule { auto_archive_at: DateTime<Utc>, archive_soon: bool }>` (A4; `archive_soon` = server-side `now >= warn_at`). `archive_schedule` is `Some` only for the admin of an active group, `None` for members and for archived groups (U9). When archived: `status = Archived`, `can_leave = false`, `invitation = None` | `NotFound` |
| `update_group` | unchanged | also sets `kept_active_at = CURRENT_TIMESTAMP` | adds `Archived` (A9) |
| `preview_invitation(code)` | user | `ClosedReason` gains `Archived` | unchanged |
| `join_group(code)` | user | | adds `Archived` (after `AlreadyMember`) |
| `regenerate_invitation`, `leave_group`, `remove_member`, `transfer_admin` | unchanged | | add `Archived` |
| `delete_group` | unchanged | allowed on archived groups | unchanged |
| `reset_draw` | unchanged | also sets `kept_active_at = CURRENT_TIMESTAMP` | adds `Archived` |
| `perform_draw`, `resend_draw_notifications` | unchanged | | add `Archived` |
| `list_my_assignments()` | user | leaves out groups with `archived_at IS NOT NULL` | – |
| `get_my_assignment(group_id)` | unchanged | unchanged (works on archived groups) | unchanged |
| `add_exclusion`, `remove_exclusion` | unchanged | | add `Archived` |
| `get_exclusions(group_id)` | unchanged | `ExclusionsPage` + `archived: bool`; when archived `can_add = false`, `candidates = []`, `notice = None` | unchanged |
| `add_item`, `update_item`, `delete_item`, `move_item`, `clear_wishlist`, `copy_items` (destination), share `add_wish` | unchanged | | add `Archived`; `copy_items` with an archived **source** and an active destination succeeds (A11) |
| `get_my_wishlist`, `get_wishlist` | unchanged | `WishlistView` + `archived: bool` | unchanged |
| `get_sources(group_id)` | unchanged | still lists archived groups as sources | unchanged |

`Archived` is a new variant of `GroupError`, `DrawApiError`, `ExclusionError` and
`WishlistError`: HTTP 409, message key `flash.error.group_archived` (for `GroupError` raised by
`join_group`/`preview_invitation`: `flash.error.join_closed_archived`). No-JS posts that fail
with `Archived` follow each slice's existing plain-post error path (flash + 302 back).

`archive_group` / `restore_group` / `keep_group_active` steps: `require_admin` (fast fail) →
READ COMMITTED transaction → `lock_group` (`FOR UPDATE`; gone → `NotFound`) → `require_admin`
again under the lock → archive: `UPDATE \`groups\` SET archived_at = CURRENT_TIMESTAMP WHERE
id = ? AND archived_at IS NULL`; restore: `UPDATE \`groups\` SET archived_at = NULL,
kept_active_at = CURRENT_TIMESTAMP WHERE id = ? AND archived_at IS NOT NULL`; keep active:
archived → `Archived`, else `UPDATE \`groups\` SET kept_active_at = CURRENT_TIMESTAMP WHERE
id = ?` → commit. Logged with user id and group id. Outcomes reuse
`groups_flow::stay_outcome`.

**Sweep** (`app::server::archive_flow`, ssr only; the pure date logic in `app::domain`):
```rust
pub const ARCHIVE_AFTER_MONTHS_DRAWN: u32 = 6;
pub const ARCHIVE_AFTER_MONTHS_NOT_DRAWN: u32 = 9;
pub const ARCHIVE_WARNING_DAYS: i64 = 14;
pub const ARCHIVE_SWEEP_PERIOD: Duration = Duration::from_secs(3600);
pub struct GroupClock { is_drawn: bool, drawn_at, created_at, registration_deadline, draw_date, kept_active_at } // Option<DateTime<Utc>> except is_drawn/created_at
pub fn archive_dates(clock: &GroupClock) -> (DateTime<Utc> /* warn_at */, DateTime<Utc> /* auto_archive_at */);
pub async fn run_archive_sweep(pool: &MySqlPool, config: &Config, mail: &dyn MailTransport, now: DateTime<Utc>) -> SweepResult;
pub fn spawn_archive_sweep(pool: MySqlPool, config: Arc<Config>, mail: Arc<dyn MailTransport>, period: Duration) -> JoinHandle<()>;
```
`spawn_archive_sweep` ticks immediately, then every `period`, calling `run_archive_sweep(…,
Utc::now())`. `server::run()` calls it with the production `Mailer` and `ARCHIVE_SWEEP_PERIOD`.
`created_at` NULL (the baseline column is nullable; no code writes NULL) counts as
`DateTime::UNIX_EPOCH`, as `groups_flow` already does. The mail is built by
`archive_mail::archive_warning(i18n, config, to, admin_name, group_id, group_name,
auto_archive_at) -> Message` (escaped values, HTML + text, `lang` of the mail locale, link via
`app_link`, like `draw_mail`).

### Data & migrations

Tables touched: `groups` (three new columns; read/written by archive, restore, keep active,
`update_group`, `reset_draw` and the sweep; `archived_at` read by every mutation's lock). No
other table changes; no rows are deleted.

`NNNN_group_archive.sql`, where `NNNN` is the next free number after the highest migration on
`dev` at implementation time (P1, U10; `0009` today, `0013` if #84 merges first):
```sql
-- NNNN group archive (spec: group-archive). New columns start NULL: nothing is archived
-- or warned by the migration; the first sweep after startup archives the groups that are
-- past their archive date (decision U6) and warns those inside the 14-day window.
ALTER TABLE `groups`
    ADD COLUMN archived_at TIMESTAMP NULL DEFAULT NULL AFTER drawn_at,
    ADD COLUMN archive_warned_at TIMESTAMP NULL DEFAULT NULL AFTER archived_at,
    ADD COLUMN kept_active_at TIMESTAMP NULL DEFAULT NULL AFTER archive_warned_at;

-- Groups that are not drawn (never drawn, or reset): a fresh start at the deploy (U10, P4).
-- Assigning updated_at to itself keeps ON UPDATE from touching it.
UPDATE `groups`
SET kept_active_at = CURRENT_TIMESTAMP, updated_at = updated_at
WHERE is_drawn = FALSE;
```
- Forward only, additive; the only write is the backfill of the new column, existing values
  (including `updated_at`) stay unchanged. Applies to the populated production DB.
  `TIMESTAMP NULL DEFAULT NULL` is spelled out so no column gets an implicit
  `CURRENT_TIMESTAMP` default or `ON UPDATE`.
- Drawn groups get no backfill: their clock is `drawn_at` (U6/U7), so the three groups drawn in
  December 2025 are archived by the first sweep, without a warning. Every group that is not
  drawn gets 9 months from the deploy: a group reset for 2026 since the cutover stays active,
  and an undrawn group from the PHP data is due in August 2027, with a warning first. Not
  `updated_at` (amended 2026-11-16): the PHP import kept each group's last change in the PHP
  app, so an undrawn PHP group last changed in December 2025 would have been archived by the
  first sweep without a warning. Without any backfill such groups would be archived by the
  first sweep too (their 2025 dates are more than 9 months old). Decided by the user (U10);
  admins can archive such groups by hand.
- No index: the sweep reads the active groups (a few rows), and the list queries start from
  `group_members`.
- `groups.updated_at` (`ON UPDATE CURRENT_TIMESTAMP`) changes whenever a group is warned,
  archived, restored or kept active. Nothing reads it after the migration.
- Rollback path: an older image refuses to start (sqlx `VersionMissing(9)`, the number of this
  migration) until the `_sqlx_migrations` row for that version is deleted by hand; it then
  ignores the columns, and archived groups show as active. To go forward again, save the row
  before deleting it (`SELECT * FROM _sqlx_migrations WHERE version = 9`) and put it back
  unchanged before the new image is deployed; otherwise the new image re-runs 0009, its
  `ALTER TABLE` fails on the existing columns, and the three columns must be dropped first,
  which loses the archive state.

### UI notes

**`/groups`.** Header gets a third link, `group.archive.title` → `/groups/archive`
(`btn btn-outline-secondary`), always shown (no extra query). The list shows active groups only.

**`/groups/archive`.** Title `group.archive.title`, intro `group.archive.intro`, link back to
`/groups` (`nav.groups`). One card per archived group: name, Admin badge (`group.admin`) if
the caller is admin, status badge `group.status.archived`, member count (`group.memberCount`,
plural), admin name (`group.adminLabel`), `group.draw.drawnOn {date}` if `drawn_at` is set,
`group.archive.archivedOn {date}`, link `group.viewDetails` → `/groups/{id}`. Empty:
`group.archive.empty`. No assignment data on this page. `Cache-Control: no-store` like
`/groups/:id`. Dates via `I18n::date`.

**`/groups/:id`, active.**
- Details card, **admin only** (U9): `group.show.autoArchiveOn {date}` (`auto_archive_at`) as
  a `form-text` under the status. Members see no archive date anywhere (page, hydration data,
  server fn responses).
- Admin header actions gain "Archive group" (`group.show.archiveGroup`, confirm
  `group.show.archiveGroupConfirm`, `btn-outline-secondary`) before Delete.
- Admin only, when `archive_soon`: an `alert alert-warning` (`role="status"`) at the top with
  `group.show.archiveSoon {date}` and a "Keep active" button (`group.show.keepActive`, no
  confirm dialog) that posts `keep_group_active`.

**`/groups/:id`, archived.**
- At the top: `<div class="alert alert-secondary" role="status">` with
  `group.show.archivedNotice {date}` (`date` = `archived_at`).
- Header actions: admin → Restore (`group.show.restoreGroup`, confirm
  `group.show.restoreGroupConfirm`) and Delete (unchanged). No Edit, no Regenerate. Members →
  nothing (no Leave button, no `cannotLeaveAfterDraw` text).
- Details card: status badge `group.status.archived`, no archive-date line; rest unchanged.
- Members card: names (emails for the admin); no Make admin / Remove buttons.
- Invitation card: not shown. Exclusions link: not shown.
- Draw panel: only the caller's own assignment card if drawn (`get_my_assignment`); no
  `group.draw.pending`, no admin section (no perform/reset/resend, no notification summary).
  `draw_resource` does not fetch `get_draw_admin_status` for archived groups.
- Wishlist panel: unchanged except that the "Edit my wishlist" button is not shown (the
  "view your wishlist" link and the members' view links stay). `WishlistPanel` gets an
  `archived` prop from `GroupDetail`.

All new buttons are `ActionForm`s (Archive and Restore with `ConfirmSubmit`); without JS a plain
POST answers 302 back to the group with a flash. `GroupStatus` gains `Archived` (`label_key`
`group.status.archived`, badge `badge bg-dark`). Every date shown comes from the server (no
`Utc::now()` during render).

**`/groups/:id/wishlist`, archived.** Notice `flash.error.group_archived` at the top, the items
listed read-only (as on the view page), no add/edit/move/remove/clear/copy controls.

**`/groups/:id/wishlist/:user_id`, archived** (amended 2026-11-16). Unchanged, except that the
caller's own list shows no `wishlist.edit.title` button and, when empty, no
`wishlist.view.addNow` button (like the group page's wishlist card).

**`/groups/:id/exclusions`, archived.** Same read-only rendering as after the draw (rules
listed, no add card, no remove buttons) plus the notice `flash.error.group_archived`.

**`/join/:code`, archived.** `flash.error.join_closed_archived`, no Join button.

**Warning mail.** Subject `email.archiveWarning.subject`; blocks: `…greeting {name}`,
`…body {group} {date}`, `…keep`, `…nothing`, button `…button` → `{APP_URL}/groups/{id}`,
`email.common.signature`. `date` = `auto_archive_at` via `I18n::date` (mail locale,
`APP_TIMEZONE`).

**i18n.** Reused: `group.admin`, `group.adminLabel`, `group.memberCount.*`, `group.viewDetails`,
`group.draw.drawnOn`, `nav.groups`, `email.common.signature`. New (every key in `en.json` and
`de.json`, each added with the code that uses it; the coverage test fails on unused keys):

| Key | en | de |
|---|---|---|
| `group.status.archived` | Archived | Archiviert |
| `group.archive.title` | Archived groups | Archivierte Gruppen |
| `group.archive.intro` | Groups that are no longer in use. They are read-only: you can still see the members, your own assignment and the wishlists. | Gruppen, die nicht mehr verwendet werden. Sie können nicht mehr geändert werden: Mitglieder, Ihre eigene Zuweisung und die Wunschlisten bleiben sichtbar. |
| `group.archive.empty` | You have no archived groups. | Sie haben keine archivierten Gruppen. |
| `group.archive.archivedOn` | Archived on {date} | Archiviert am {date} |
| `group.show.archivedNotice` | This group was archived on {date} and is read-only. | Diese Gruppe wurde am {date} archiviert und kann nicht mehr geändert werden. |
| `group.show.autoArchiveOn` | This group moves to the archive automatically on {date}. | Diese Gruppe wird am {date} automatisch archiviert. |
| `group.show.archiveSoon` | This group will be moved to the archive on {date}. If you still need it, keep it active. | Diese Gruppe wird am {date} archiviert. Wenn Sie sie noch brauchen, lassen Sie sie aktiv. |
| `group.show.keepActive` | Keep active | Aktiv lassen |
| `group.show.archiveGroup` | Archive group | Gruppe archivieren |
| `group.show.archiveGroupConfirm` | Move this group to the archive? Nobody can change it until you restore it. Members can still see it under Archived groups. | Diese Gruppe archivieren? Bis Sie sie wiederherstellen, kann niemand etwas ändern. Die Mitglieder finden sie weiterhin unter „Archivierte Gruppen“. |
| `group.show.restoreGroup` | Restore group | Gruppe wiederherstellen |
| `group.show.restoreGroupConfirm` | Restore this group from the archive? It appears under My Groups again and can be changed again. It moves to the archive again automatically after six months (nine if it has not been drawn); you get an email 14 days before. | Diese Gruppe aus dem Archiv holen? Sie erscheint wieder unter „Meine Gruppen“ und kann wieder geändert werden. Nach sechs Monaten (neun, wenn sie nicht gezogen ist) wird sie erneut automatisch archiviert; Sie erhalten 14 Tage vorher eine E-Mail. |
| `flash.success.group_archived` | The group was moved to the archive. | Die Gruppe wurde archiviert. |
| `flash.success.group_restored` | The group was restored from the archive. | Die Gruppe wurde aus dem Archiv geholt. |
| `flash.success.group_kept_active` | The group stays active. Its new archive date is shown in the group details. | Die Gruppe bleibt aktiv. Das neue Archivierungsdatum steht in den Gruppendetails. |
| `flash.error.group_archived` | This group is archived and can no longer be changed. | Diese Gruppe ist archiviert und kann nicht mehr geändert werden. |
| `flash.error.join_closed_archived` | This group is archived. Nobody can join it any more. | Diese Gruppe ist archiviert, ein Beitritt ist nicht mehr möglich. |
| `email.archiveWarning.subject` | {group} will be archived on {date} | {group} wird am {date} archiviert |
| `email.archiveWarning.greeting` | Hello {name}, | Hallo {name}, |
| `email.archiveWarning.body` | Your Secret Santa group {group} will be moved to the archive automatically on {date}. Its members can still open it under Archived groups, but nothing can be changed any more. | Ihre Wichtelgruppe {group} wird am {date} automatisch archiviert. Die Mitglieder können sie weiterhin unter „Archivierte Gruppen“ öffnen, aber nichts mehr ändern. |
| `email.archiveWarning.keep` | Do you still need the group? Open it and choose "Keep active". The period until the archive then starts again (six months for a drawn group, nine otherwise). | Brauchen Sie die Gruppe noch? Öffnen Sie sie und wählen Sie „Aktiv lassen“. Dann beginnt die Frist bis zur Archivierung von vorn (sechs Monate für eine gezogene Gruppe, sonst neun). |
| `email.archiveWarning.nothing` | Otherwise you don't need to do anything. You can restore an archived group at any time. | Andernfalls müssen Sie nichts tun. Eine archivierte Gruppe können Sie jederzeit wiederherstellen. |
| `email.archiveWarning.button` | Open the group | Gruppe öffnen |

### Effect on other specs

When this spec is approved, the implementer adds a dated revision line pointing here to
`groups.md` (A1, A3, A4 for `update_group`, A9, A10, `GroupStatus::Archived`,
`/groups/archive`), `draw.md` (`Archived` errors, `reset_draw` restarts the archive clock,
`list_my_assignments` filter, panel when archived; secrecy rules unchanged), `wishlists.md`
(`Archived`, read-only edit page, archived sources), `exclusions.md` (`Archived`, read-only
page) and `share-target.md` (archived groups are not offered and cannot receive). If #47 has
landed, also `dashboard.md` (cards come from `list_my_groups`, so archived groups are left out).
Decision-log lines for U1–U10 and A4–A6 are added on approval.

## Acceptance criteria

Fixtures: users Alice, Bob, Carol, Dave. G = group with admin Alice and members Alice, Bob,
Carol, drawn (3 assignment rows) with `drawn_at` as stated, `kept_active_at` NULL, not
archived, unless an AC says otherwise. Clock columns are set directly in the DB where an AC
gives times. "The sweep at T" is `run_archive_sweep(pool, config, transport, T)` called
directly, with a transport that delivers to Mailpit unless stated. Mail tests use
`Mailpit::unique_email()` addresses. `APP_TIMEZONE` is Europe/Berlin.

**Migration**
- AC1: Given a database at migration 0008 holding drawn and undrawn groups (with members,
  wishlists, rules and assignments), when the new migration applies, then every existing value
  is unchanged (including `updated_at`), `archived_at`, `archive_warned_at` and `kept_active_at`
  exist as nullable `TIMESTAMP` without default value expression or `ON UPDATE`,
  `archived_at` and `archive_warned_at` are NULL on every row, `kept_active_at` is the time of
  the migration on every group that is not drawn (never drawn or reset) and NULL on every
  drawn group (amended 2026-11-16: was `updated_at`).

**Archive clock (pure)**
- AC2: `archive_dates` gives, for a drawn group, `auto_archive_at = max(drawn_at,
  kept_active_at) + 6 months` and `warn_at = auto_archive_at − 14 days`; `2026-08-31 12:00 UTC`
  maps to `2027-02-28 12:00 UTC`.
- AC3: For a group that is not drawn, `auto_archive_at` = the latest of `created_at`,
  `registration_deadline`, `draw_date` and `kept_active_at`, plus 9 months: created
  `2026-05-01 10:00 UTC` without dates → archive `2027-02-01 10:00 UTC`, warning
  `2027-01-18 10:00 UTC`; the same group with `draw_date = 2026-12-12 11:00 UTC` → archive
  `2027-09-12 11:00 UTC`; with only `registration_deadline = 2026-12-01 22:59:59 UTC` → archive
  `2027-09-01 22:59:59 UTC`; `2026-05-31 12:00 UTC` → `2027-02-28 12:00 UTC`. A
  group with `is_drawn = TRUE` but `drawn_at` NULL is treated as not drawn.

**Clock writes**
- AC4: `keep_group_active`, `restore_group` (on an archived group), `update_group` and
  `reset_draw` each set `kept_active_at` to the current time. `join_group`, `add_item`,
  `add_exclusion`, `regenerate_invitation`, `transfer_admin` and `perform_draw` leave it
  unchanged.
- AC5: Given an undrawn group with `created_at` and `kept_active_at` 10 months ago and no
  dates, when Alice saves the edit form with both dates in the past (before any sweep runs),
  then `get_group` for Alice reports `archive_schedule.auto_archive_at` nine months after the save, and a sweep right after
  the save neither warns nor archives it.

**Sweep: warning and archive**
- AC6: Given G with `drawn_at = 2026-05-14 10:00 UTC` (archive `2026-11-14 10:00 UTC`, warning
  `2026-10-31 10:00 UTC`): the sweep at `2026-10-31 09:59:59` does nothing; the sweep at
  `2026-10-31 10:00:00` sends Alice one warning mail, sets `archive_warned_at` and does not
  archive; the sweep at `2026-11-01 10:00` sends no second mail; the sweep at
  `2026-11-14 09:59:59` does not archive; the sweep at `2026-11-14 10:00:00` sets
  `archived_at = now` and sends **no** mail. The sweep results are `{warned: 1, archived: 0}`,
  then zeros, then `{warned: 0, archived: 1}`.
- AC7: The warning mail of AC6 goes to Alice's stored address, has subject
  `email.archiveWarning.subject` with the group name and the date `14 Nov 2026`, contains the
  keep-active and restore hints and a link to `{APP_URL}/groups/{G}`, has a plain-text part, and
  contains no receiver name of any assignment. Bob and Carol get no mail. Given G's name is
  `<b>x</b>`, it appears HTML-escaped. Given Alice's `users.locale = 'de'`, subject and body are
  German with `14.11.2026` and the HTML root has `lang="de"`.
- AC8: Given Alice transferred the admin role to Bob before the warning date, then Bob (not
  Alice) gets the warning.
- AC9 (deploy backlog, U6): Given G with `drawn_at = 2025-12-10 18:00 UTC` and no warning, when
  the first sweep runs at `2026-11-20 10:00 UTC`, then G is archived at once and no mail is
  sent. Given an undrawn group whose reference is more than 9 months before `now` (set
  directly, no warning), the same.
- AC10: Given G was warned at W (inside its window) and Alice calls `keep_group_active` at K
  (before the archive date), then the sweep at the old archive date does not archive G; exactly
  one new warning is sent at `K + 6 months − 14 days`; G is archived at `K + 6 months`.
- AC11: Given G was archived and Alice restores it at R, then the sweep at `R + 1 hour` neither
  warns nor archives; it warns once at `R + 6 months − 14 days` and archives at `R + 6 months`.
- AC12: Given G was drawn 7 months before `now` and Alice resets the draw (`reset_draw`) at X
  before any sweep runs, then the sweep does not archive or warn G; if G is not drawn again it
  is warned at `X + 9 months − 14 days` and archived at `X + 9 months`. When Alice instead
  draws again at T, the next warning comes at `T + 6 months − 14 days` and the archive at
  `T + 6 months`.
- AC13 (a group in preparation is not archived before its draw): given an undrawn group
  created `2026-05-01 10:00 UTC` without dates, the sweeps at `2026-11-01 10:00 UTC` and
  `2026-12-11 10:00 UTC` neither warn nor archive it; when it is drawn at
  `2026-12-12 11:00 UTC`, its archive date becomes `2027-06-12 11:00 UTC`. Left undrawn, the
  sweep at `2027-01-18 10:00 UTC` warns once (the same warning mail as AC7, with
  `1 Feb 2027`) and the sweep at `2027-02-01 10:00 UTC` archives it without a mail. With
  `draw_date = 2026-12-12 11:00 UTC` and still undrawn, the sweeps at `2027-01-18` and
  `2027-02-01` do nothing.
- AC14: Given G inside its warning window, when two sweeps run concurrently, then Alice receives
  exactly one warning; given G past its archive date, two concurrent sweeps archive it once and
  `archived_at` keeps the first value.
- AC15: Given G inside its warning window and a transport that fails for Alice's address, when
  the sweep runs, then `archive_warned_at` is back to its previous value (NULL) and the result
  counts no warning; when the address works again, the next sweep sends exactly one warning.
  Given every warning attempt failed, G is still archived at its archive date. A failing group
  does not stop the sweep from handling a second due group H.
- AC16: `spawn_archive_sweep` with a period of 50 ms archives a due group without any other call
  (the first run happens at once), and keeps running after a sweep that hit a failing mail.
- AC17: The captured `tracing` output of sweeps that warn (one injected mail failure) and
  archive contains the group ids and counts, but no group name, user name, email address or
  mail text.

**Manual archive, restore and keep active**
- AC18: Given G (drawn or not drawn), when Alice confirms "Archive group", then `archived_at`
  is set, she stays on `/groups/{G}` with `flash.success.group_archived` and the archived view,
  G is gone from `/groups` and listed on `/groups/archive`, and no mail is sent.
- AC19: Given Bob calls `archive_group(G)`, `restore_group(G)` or `keep_group_active(G)`, then
  `Forbidden`; Dave gets `NotFound`; an anonymous call gets `Unauthorized`; a plain GET or a
  cross-origin POST changes nothing. In every case `archived_at` and `kept_active_at` are
  unchanged.
- AC20: Given G is archived, `archive_group(G)` succeeds and leaves `archived_at` unchanged, and
  `keep_group_active(G)` returns `Archived`. Given G is active, `restore_group(G)` succeeds and
  leaves `kept_active_at` NULL.
- AC21: Given G is archived, when Alice confirms "Restore group", then `archived_at` is NULL,
  `kept_active_at` is set, she sees `flash.success.group_restored`, G is back on `/groups` and
  gone from `/groups/archive`, and G's draw state, assignments, wishlists, exclusion rules,
  members and invitation code are unchanged. Afterwards `update_group` and `add_item` work again.
- AC22: Given G is inside its warning window, Alice sees `group.show.archiveSoon` with the
  archive date and a "Keep active" button; clicking it shows `flash.success.group_kept_active`,
  the alert disappears and the details card shows the new date (six months later). Bob sees
  neither the alert, nor the button, nor any archive date. Outside the window Alice sees no
  alert, only the `group.show.autoArchiveOn` line.
- AC23: Without JavaScript, the Archive, Restore and Keep active buttons are plain forms; the
  POST answers 302 to `/groups/{G}`, and that page shows the success flash.

**Read-only**
- AC24: Given G is archived, then each of these calls returns `Archived` (HTTP 409, message
  `flash.error.group_archived`) and changes no row: Alice: `update_group`,
  `regenerate_invitation`, `remove_member(G, Bob)`, `transfer_admin(G, Bob)`, `leave_group`,
  `reset_draw`, `resend_draw_notifications`, and `perform_draw` on an archived group that is
  not drawn; Bob: `leave_group`, `add_exclusion`, `remove_exclusion`, `add_item`,
  `update_item`, `delete_item`, `move_item`, `clear_wishlist`, `copy_items` with G as
  destination, and the share target's `add_wish` into G.
- AC25: Given G is archived, then authorization errors win: Bob's `update_group(G, …)` gives
  `Forbidden`, Dave's `add_item(G, …)` and `archive_group(G)` give `NotFound`.
- AC26: Given G is archived, when Alice deletes G, then G and all its child rows are gone
  (groups AC22) and she lands on `/groups` with `group_deleted`.
- AC27: Given G is archived, when Dave opens `/join/{G code}`, then he sees
  `flash.error.join_closed_archived` and no Join button; `preview_invitation` returns
  `closed: Some(Archived)` and `join_group` returns `Archived` and adds no membership. When Bob
  opens the link, he is redirected to `/groups/{G}` (groups AC30).
- AC28: Given Bob has items in his wishlist in archived G and is a member of active H, then the
  copy chooser on `/groups/{H}/wishlist` lists G, `copy_items(H, G, ids)` copies the items into
  H, and Bob's list in G is unchanged.
- AC29: Given a test transaction that holds G's row `FOR UPDATE` and sets `archived_at`, when
  Bob's `add_item(G, …)` starts and the transaction then commits, then `add_item` returns
  `Archived` and no item is written. Given a transaction holding G's row in share mode (as a
  wishlist write does), `archive_group(G)` waits until it ends.

**Visibility**
- AC30: Given G is archived, when Bob opens `/groups/{G}`, then he sees the archived notice
  with the date, the status `Archived`, the member names (no emails), his own assignment card
  for his own receiver, and the wishlist card with view links but no "Edit my wishlist" button;
  and no Leave button, no exclusions link, no `group.draw.pending`, no invitation code, no
  archive-date line. `get_group` returns `status = Archived`, `can_leave = false`,
  `invitation = None`, `archived_at` set and `archive_schedule = None`.
- AC31: Given G is archived, when Alice opens `/groups/{G}`, then she additionally sees the
  member emails and the Restore and Delete buttons, and sees no Edit, Regenerate, Make admin,
  Remove, Keep active, Perform/Reset draw, resend button or notification summary.
  `/groups/{G}/edit` shows the `flash.error.group_archived` notice instead of the form.
- AC32: Given G is archived, then the response bodies (HTML and hydration data) of
  `/groups/{G}` for Alice and for Bob each contain exactly one assignment, the viewer's own
  (draw AC27 holds for archived groups), and `list_archived_groups` returns no receiver id or
  name. No new server fn takes a user id and returns assignment data.
- AC33: Given Bob is a member of archived groups G (archived later) and H (archived earlier),
  active K, and Dave's archived group J where Bob is not a member, when Bob opens
  `/groups/archive`, then he sees G then H, each with name, member count, admin name, drawn
  date (when drawn), archived date and a link to the group, and an Admin badge only where he is
  admin; K and J are not listed. `/groups` lists only K and links to `/groups/archive`. The
  dashboard assignments card (if present) lists K's assignment only (U8), and the share-target
  picker offers only K.
- AC34: Given Dave has no archived groups, `/groups/archive` shows `group.archive.empty`. A
  logged-out visitor is redirected to login and, after logging in, lands on `/groups/archive`.
  The response has `Cache-Control: no-store`.
- AC35: Given G is archived, `/groups/{G}/wishlist` for Bob shows the notice and his items
  without any add, edit, move, remove, clear or copy control, and `get_my_wishlist(G)` has
  `archived = true`. `/groups/{G}/wishlist/{Bob}` for Bob links nowhere to
  `/groups/{G}/wishlist` (amended 2026-11-16). `/groups/{G}/exclusions` shows his rules without add or remove controls
  and the notice; `get_exclusions(G)` has `archived = true` and `can_add = false`.
- AC36: Given G (active) has `drawn_at = 2026-12-10 10:00 UTC`, then Alice's group page shows
  `group.show.autoArchiveOn` with `10 Jun 2027` in English and `10.06.2027` in German, and
  `get_group` for Alice returns `archive_schedule.auto_archive_at = 2027-06-10 10:00 UTC`. Given
  additionally `kept_active_at = 2027-07-01 10:00 UTC`, the date is `1 Jan 2028`. An undrawn
  group created `2026-11-01 10:00 UTC` without dates shows `1 Aug 2027` / `01.08.2027` to its
  admin.
- AC37: Given G is active (inside or outside its warning window), then `get_group` for Bob
  returns `archive_schedule = None`, and Bob's response body for `/groups/{G}` (HTML and
  hydration data) contains neither the archive date (in any format) nor the texts
  `group.show.autoArchiveOn` and `group.show.archiveSoon` (U9).

**i18n / hydration**
- AC38: Every key listed in UI notes exists in `en.json` and `de.json`, and the archive page,
  the archived group page, the warning alert and the new buttons and flashes render in German
  for a `de` session. `/groups/archive`, an archived `/groups/{G}` and an active `/groups/{G}`
  inside its warning window show no hydration error in either locale (`collectPageErrors`),
  with and without the admin role.

## Open questions

None. The backfill question of the second draft is decided (U10).
