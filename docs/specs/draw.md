---
slice: draw
status: approved
---
# Draw

Scope: the Secret Santa draw (algorithm, persistence, concurrency), revealing the caller's own assignment (group page and dashboard card), draw notification mails with retry, and the admin's "reset draw". Also covered: dropping the unused `email_templates` table, which the groups spec left to this slice.
Not in scope: managing exclusion rules and the feasibility check (the **exclusions** slice; this spec uses its `check_feasibility` and `load_rules` exactly as defined there), and viewing wishlists (the **wishlists** slice; this spec only links to its view page).

Fresh start: no production data is migrated. Migration numbers are placeholders (`NNNN_…`).

Revised 2026-10-31: aligned with i18n-foundation (built before auth). D12: each draw and reset mail uses the recipient's account language (`users.locale` via `mail_i18n`) instead of `APP_LOCALE`; panel dates use `app::format`; counts use plural keys. AC38 was revised; AC48 and AC49 were added. The optional "only one possible draw" admin warning is decided: not added (user).
Revised 2026-11-02 (exclusions implementation): `load_rules` is `app::server::exclusions::load_rules(&mut Transaction<'_, MySql>, group_id) -> Result<Vec<(UserId, UserId)>, AppError>` (it reads the DB); `check_feasibility` lives in `app::domain::feasibility`. Transactions that rely on REPEATABLE READ set their isolation level explicitly (CLAUDE.md).
Revised 2026-11-02 (tests): AC43/AC45 "no draw-result mail for the deleted rows is sent" is read together with the accepted in-flight case: at most the one mail already in flight when the reset commits may still be delivered; the worker re-checks each assignment row immediately before sending (no iteration over a stale row list). Draw-UI markers use `data-testid` attributes (contract in server/tests/it/common/draw.rs).

PHP reference: `GroupRepository::performDraw` / `drawSecretSanta`, `GiftAssignmentRepository`, `Models/GiftAssignment`, `GroupController::draw` / `sendDrawNotifications` / `show`, `EmailService::sendDrawNotification` / `sendEmail`, `UserController::dashboard`, `Views/group/show.php`, `Views/user/dashboard.php`, `WishlistController::view` (assignment check), `ExclusionController` (`is_drawn` checks), `src/Config/schema.sql`, `src/Localization/{en,de}/messages.json`.

## Current PHP behaviour

### Routes

| Method | Path | Handler | Auth | What it does |
|---|---|---|---|---|
| GET | `/groups/:id/draw` | `GroupController::draw` | logged in, `admin_id == me` | Runs the draw, then sends all mails synchronously, then redirects to `/groups/:id` |
| GET | `/groups/:id` | `GroupController::show` | member | If `is_drawn`: loads `findByGiverAndGroup(me, id)` and shows the receiver's name + link `/wishlist/view/{receiverId}/{groupId}` |
| GET | `/user/dashboard` | `UserController::dashboard` | logged in | Assignments card: `findByGiverId(me)`, then group and receiver per row (N+1) |
| GET | `/wishlist/view/:user/:group` | `WishlistController::view` | member | After the draw a non-admin may only open the wishlist of their own receiver (`flash.error.only_view_assigned_recipient`). **The admin may open every wishlist.** |

There is no undo or reset. There is no route that lists the full pairing.

### `draw`
1. Missing group: `flash.error.group_not_found` and a redirect to `/groups`. Not admin: `flash.error.only_admin_draw`. Already drawn: `flash.error.already_drawn`. Each of these redirects to the group page.
2. `performDraw($group)`:
   - Returns false if `isDrawn()` is set or there are fewer than 2 members. Members (N+1 queries) and all exclusion rules of the group are read **outside any transaction and without a lock**.
   - Exclusion map: `user_id → [excluded_user_id…]`, meaning "user_id must not give to excluded_user_id". The rules are one-directional and are not filtered by current membership.
   - `drawSecretSanta`: shuffles the receivers once, then makes up to 10 attempts. Each attempt walks the givers **in member order** and picks a random receiver (`array_rand`) from those that are not the giver, not excluded and **not reciprocal** (`assignments[receiver] === giver`). If a giver is left with no candidate, the attempt fails. After 10 failed attempts, it returns `[]`.
   - On success, it inserts one row per giver and sets `is_drawn = 1` in one transaction. Any exception rolls back and returns false.
3. False gives `flash.error.draw_failed` ("make sure there are enough members and the exclusion rules allow a valid draw"). No reason is given.
4. `sendDrawNotifications`: for each assignment, it loads the giver and receiver and calls `sendDrawNotification(giverEmail, giverName, receiverName, groupName, REQUEST_SCHEME://HTTP_HOST/groups/{id})`, then sets `notification_sent = 1`. Any exception gives the flash `flash.warning.draw_partial_success` with `{error}` = the exception message. Otherwise it flashes `flash.success.draw_success`.

### Email `sendDrawNotification`
- English only. Subject "Your Secret Santa Draw Results for {group}". HTML body: greeting, "You have been assigned to give a gift to: **{receiver}**", a link "View Group Details", "keep this a secret". Names and the group name are interpolated **without escaping**.
- `sendEmail` `error_log`s the recipient, the subject and the first 100 characters of the body. With `APP_DEBUG=true` it writes the full mail (receiver name included) to `storage/emails/*.html` and does not send it. Otherwise PHPMailer runs with `SMTPDebug = DEBUG_SERVER`, which writes the **full SMTP transcript, body included,** to the PHP error log. It returns `true` in every case, including failures.

### Views
- Group page: the dropdown entry "Perform draw" appears for the admin whenever the group is not drawn. A second "Perform draw" button appears only with 3 or more members. Both are GET links with the JS confirm `group.show.performDrawConfirm` ("…cannot be undone"). If drawn and an assignment exists: card `group.show.assignment`, `group.show.assignmentDescription`, receiver name, button `group.show.viewWishlist`. "Manage exclusions" is hidden after the draw.
- Dashboard: card `user.dashboard.assignments.title`, rows with `…group`, `…recipient`, `…viewWishlist`. The empty texts are `…empty` and `…emptyDescription`. The rows are not filtered by current membership.

### Data
- `gift_assignments(id, group_id FK CASCADE, giver_id FK users CASCADE, receiver_id FK users CASCADE, created_at, notification_sent BOOLEAN DEFAULT FALSE NULL, UNIQUE(group_id, giver_id), UNIQUE(group_id, receiver_id))`. Nothing stops `giver_id = receiver_id`.
- `groups.is_drawn BOOLEAN DEFAULT FALSE` is nullable. There is no "drawn at" timestamp.
- `email_templates` is never read or written by any code.
- `notification_sent` is written but never read.

## Problems in the PHP version
1. **CSRF on an irreversible action**: the draw is a GET link, so any page (or a link prefetcher) can trigger it.
2. **2-person groups can never be drawn**, because reciprocal pairs are a hard rule. Groups of 3 with one exclusion often fail for the same reason.
3. **Random greedy with 10 attempts** can fail even when a valid assignment exists. It is also not uniform (the givers are walked in a fixed order). It gives no reason for a failure.
4. **No lock or transaction around the read**: a join or leave racing with the draw can produce a member without an assignment, or an assignment for an ex-member. Two concurrent draws both pass the `isDrawn` check, and the second fails with a UNIQUE error that is reported as "draw failed".
5. Two "Perform draw" entries have different thresholds (≥ 2 members vs. ≥ 3).
6. **Mails are sent synchronously in the request**: up to 30 s SMTP timeout per member. A failure halfway leaves the rest unsent, with no retry. `notification_sent` is set even when nothing was sent, because `sendEmail` always returns true.
7. **Secrecy leaks through logs and files**: the SMTP debug transcript (full body, receiver name included) goes to the error log, and debug mode writes all draw mails to `storage/emails/`. Anyone with log access can rebuild the whole pairing.
8. The mail link is built from `REQUEST_SCHEME` and `Host` (host header injection; also `http` behind Caddy). Mail HTML is not escaped. Mails are English only.
9. `draw_partial_success` shows the raw exception text.
10. There is no undo. A dropout after the draw breaks the group for good. The confirm text says "cannot be undone".
11. The dashboard shows assignments of groups the user has left (no membership filter). It also makes N+1 queries.
12. `email_templates` is dead schema. `is_drawn` and `notification_sent` are nullable.
13. Self-assignment is not prevented at the DB level.

## Redesign

### Decisions taken in this spec
- **D1 Minimum group size: 2.** A 2-person group is drawn as A→B, B→A. For groups of 2 or 3 the panel shows a hint that members can work out who gives to whom. (With 3, a 3-cycle lets each member infer everything.)
- **D2 The draw may happen before the registration deadline.** The admin decides when everyone is in. If the deadline is still in the future, the confirm dialog warns that drawing closes registration (groups D3 rejects joins once `is_drawn = 1`).
- **D3 Maximum group size for the draw: 200** (`MAX_DRAW_MEMBERS`; Resolved 3). Larger groups get `TooManyMembers`. Joining is not capped (groups spec).
- **D4 Hard constraints**: nobody gives to themselves, and every exclusion rule `(user_id → excluded_user_id)` between two current members holds. **Soft preference**: avoid reciprocal pairs (A→B and B→A). The algorithm is specified below.
- **D5 One transaction under the group row lock.** `perform_draw` and `reset_draw` start with `SELECT admin_id, is_drawn FROM \`groups\` WHERE id = ? FOR UPDATE`, the same lock as groups D16 (join, leave, remove, transfer), exclusions D9 (add and remove rule) and the wishlists writes (W11, shared mode). They re-check `admin_id` and `is_drawn` under the lock. Members and rules (`exclusions::load_rules`) are read under that lock, so a rule written concurrently is either included or rejected with `AlreadyDrawn` (exclusions AC24). Lock order in every slice: the group row first, then child rows.
- **D6 Invariant**: `is_drawn = TRUE` ⇔ `drawn_at IS NOT NULL` ⇔ the group has exactly one `gift_assignments` row per current member as giver and one as receiver. Groups D4/D5 (no leave or removal after the draw) keep this invariant true.
- **D7 Reset draw** (admin only, any time after the draw): one transaction deletes all of the group's assignments and sets `is_drawn = FALSE, drawn_at = NULL`. Exclusion rules and wishlists are kept (exclusions D5). **Every former giver except the admin who reset gets a "the draw was reset, your assignment is void" mail. It contains no names of receivers.** The mail also makes a reset visible to all members, which discourages an admin from re-rolling until they get the receiver they want. After a reset the group behaves like a never-drawn group: join (if the deadline allows), leave, remove and exclusions work again. The old pairing is not kept anywhere.
- **D8 Notifications are sent off the request path** by a bounded in-process worker. Assignment rows and reset notices are durable, workers coordinate through database leases, and retries are recovered periodically. The admin sees counts, and the names of members whose mail failed, and can resend failed mails. Details below.
- **D9 Secrecy** (rules S1–S9 below) is the core requirement. It overrides convenience everywhere in this slice.
- **D10 Admin feedback never depends on the result.** `perform_draw` returns `()`. It never says whether reciprocal pairs were unavoidable or how many the result contains, because that leaks information about the pairing. Blockers (too few, too many, impossible) depend only on members and rules. "Impossible" is one generic value: the admin is never told which members or rules make the draw impossible (Resolved 4, exclusions D4).
- **D11 `email_templates` is dropped** (unused; mail texts live in the i18n files).
- **D12 Mail language**: each draw and reset mail uses its recipient's account language: `mail_i18n(giver.locale, config)` (i18n-foundation F4; NULL or unknown → `APP_LOCALE`). The mail's HTML root has `lang="<mail locale>"`. Mails are HTML plus plain text. Every interpolated value is HTML-escaped. Links are built from `APP_URL`.
- **D13 The draw mail contains the receiver's name** (Resolved 1) and a link to `{APP_URL}/groups/{id}`.

### Secrecy rules
- **S1** Only two server fns return receiver data: `get_my_assignment(group_id)` and `list_my_assignments()`. Both filter on `giver_id = <session user id>` in SQL. Neither takes a user id argument. This slice exposes no receiver-lookup helper to other slices: the wishlists slice does not read `gift_assignments` (wishlists W1), and groups D5 only deletes rows.
- **S2** No server fn, page or mail ever returns or shows **who gives to a given user**, the full pairing, a per-member receiver, or any count derived from the result (for example, reciprocal pairs). This includes the admin.
- **S3** Both fns require **current membership**. `list_my_assignments` joins `group_members` on the giver and requires `is_drawn = TRUE`.
- **S4** SSR and hydration: assignment data is only fetched inside the requesting user's own request. It is never cached in memory across requests or users. `/groups/:id` and `/user/dashboard` responses carry `Cache-Control: no-store`.
- **S5** Logs: never log receiver ids or names, giver→receiver pairs, mail bodies, mail recipient addresses, the RNG seed or state, or the user ids inside an `Infeasibility`. Allowed: group id, member count, assignment row count, attempt counts, the giver's user id on a mail failure, and the infeasibility kind at `debug` level. The mail transport's SMTP debug/transcript logging must be off. `tracing` spans for draw-slice fns must not record their return values.
- **S6** Production uses a fresh OS-seeded RNG per draw (`StdRng::from_os_rng()` or `ChaCha20Rng::from_os_rng()`). The seed is never stored, logged or returned.
- **S7** No files: there is no "write mails to disk" debug mode. In dev and test, mails go to Mailpit only (foundation).
- **S8** Error responses of these fns do not depend on the result: a member without an assignment (impossible under D6) and an undrawn group both give `Ok(None)`.
- **S9** Out of scope, stated explicitly: whoever has direct DB access (operator, backups) can read the pairing. The app does not encrypt assignments.

### Algorithm

A pure function, server-only and without I/O, in `app` behind `ssr`. It uses the exclusions slice's feasibility module (`app/src/domain/feasibility.rs`) unchanged:

```rust
use crate::domain::feasibility::{check_feasibility, Feasibility, Infeasibility, UserId};

pub const MIN_DRAW_MEMBERS: usize = 2;
pub const MAX_DRAW_MEMBERS: usize = 200;

pub struct DrawConfig {
    pub exact_max_members: usize,      // default 9
    pub rejection_attempts: u32,       // default 100_000
}
pub fn draw<R: rand::Rng + ?Sized>(
    members: &[UserId],                // user ids, any order, duplicates ignored
    exclusions: &[(UserId, UserId)],   // (giver, forbidden receiver), any order, duplicates ignored
    cfg: &DrawConfig,
    rng: &mut R,
) -> Result<Vec<(UserId, UserId)>, DrawError>   // (giver, receiver), sorted by giver
pub enum DrawError {
    TooFewMembers { count: usize, min: usize },    // min = MIN_DRAW_MEMBERS
    TooManyMembers { count: usize, max: usize },   // max = MAX_DRAW_MEMBERS
    Infeasible(Infeasibility),                     // exactly the reason check_feasibility returned
}
```

Steps:
0. **Normalise.** Sort and deduplicate `members`. Drop exclusions where either side is not in `members`, and drop self pairs. Deduplicate. `allowed(g, r)` ⇔ `g ≠ r` and `(g, r)` is not an exclusion. A *reciprocal pair* in an assignment `p` is an unordered pair {a, b} with `p(a) = b` and `p(b) = a`. A *valid* assignment is a permutation `p` of the members with `allowed(m, p(m))` for every m.
1. `n < MIN_DRAW_MEMBERS` → `TooFewMembers`. `n > MAX_DRAW_MEMBERS` → `TooManyMembers`.
2. `check_feasibility(&members, &exclusions)`. `Feasibility::Infeasible(reason)` → `Err(DrawError::Infeasible(reason))`. Step 1 already guarantees at least 2 distinct members, so `Infeasibility::TooFewMembers` does not occur here. On `Feasibility::Feasible` a valid assignment is known to exist.
3. **Exact tier, `n ≤ exact_max_members`.** Enumerate every valid assignment by depth-first search: givers in ascending id order, candidate receivers in ascending id order. Let k* be the minimum number of reciprocal pairs over all of them, and let S be the list of assignments with exactly k* reciprocal pairs, in DFS order. Return `S[rng.random_range(0..S.len())]`. Cost: at most 133,496 leaves (the derangements of 9).
4. **Large tier, `n > exact_max_members`.**
   - 4a **Rejection sampling.** Repeat up to `rejection_attempts` times: copy the sorted members into `receivers`, `receivers.shuffle(rng)` (Fisher–Yates), set `p(members[i]) = receivers[i]`. Accept the first `p` that is valid **and has no reciprocal pair**.
   - 4b **Fallback** (reached only if 4a found nothing): a matching-guided greedy that always succeeds. Compute a perfect matching M of the bipartite graph giver→receiver over the `allowed` edges (Hopcroft–Karp; one exists by step 2). Shuffle the givers. For each giver g in that order, take the receivers that are not yet fixed with `allowed(g, r)`, shuffle them, then stable-partition them so that **non-reciprocal candidates** (r has not already been fixed to give to g) come first. For each candidate r in order, check whether M can be re-routed so that g→r holds while the unfixed rest stays perfectly matched. The check is one alternating-path search in the unfixed subgraph, from `M⁻¹(r)` to `M(g)`, avoiding g and r. Take the first candidate that passes, update M, and fix g→r. `r = M(g)` always passes, so every giver is fixed. Return M.
5. A step-3/4 failure after step 2 succeeded is a bug: the server fn logs it (without pairs) and returns `Internal`.

**Guarantees (testable):**
- **G1 Validity**: the result is a permutation of the normalised members that satisfies every hard constraint. A result exists exactly when step 1 passes and step 2 returns `Feasible`.
- **G2 Determinism**: for the same normalised inputs, the same `DrawConfig`, the same seed and the same build (same `rand` version), the output is identical. Input order and duplicates do not matter. Tests assert properties and distributions, not hard-coded pairings, because a `rand` upgrade may change the streams.
- **G3 Fairness, n ≤ 9**: exactly uniform over the valid assignments with the minimum possible number of reciprocal pairs. Reciprocal pairs therefore appear only when unavoidable, and then as few as possible.
- **G4 Fairness, n ≥ 10**: if 4a succeeds, the result is exactly uniform over the valid assignments without reciprocal pairs (the first accepted i.i.d. uniform permutation is uniform over the accepted set). With no exclusions the acceptance rate is about e^(-1.5) ≈ 22%, so 4a practically never falls through. 4b runs only under very dense exclusions or when no reciprocal-free valid assignment exists. Its result is valid and random, but not exactly uniform, and reciprocal pairs are avoided greedily, not minimised.
- **G5 Limits**: 2 ≤ n ≤ 200. Step 3 is bounded by the enumeration above. 4a costs at most `rejection_attempts · n` checks. 4b is polynomial: at most n·deg·E edge visits, typically n·E.

**Exclusions interface** (owned by the exclusions spec, used unchanged):
- `check_feasibility(members: &[UserId], exclusions: &[(UserId, UserId)]) -> Feasibility`, with `Feasibility::{Feasible, Infeasible(Infeasibility)}` and `Infeasibility::{TooFewMembers, NoReceiverFor { giver }, NoGiverFor { receiver }, Blocked { givers, receivers }}`. Pure, no I/O, de-duplicates its inputs and ignores rules that name non-members or are self pairs.
- `exclusions::load_rules(tx, group_id) -> Vec<(UserId, UserId)>`: the group's `(user_id, excluded_user_id)` pairs ordered by `user_id, excluded_user_id`, server-only, called inside the draw's group-row lock.
- A rule `(user_id, excluded_user_id)` means "user_id must not give to excluded_user_id" (one-directional). Self is always forbidden. Reciprocal avoidance is not part of feasibility; it is this slice's soft preference.
- The ids inside an `Infeasibility` are never shown in any UI and never logged (exclusions disclosure rule, S5).

### Pages / routes

No new URLs.

| URL | What this slice adds | Who |
|---|---|---|
| `/groups/:id` | `<DrawPanel group_id is_admin is_drawn member_count>` (hook defined by the groups spec) | member (others: groups' 404) |
| `/user/dashboard` | Assignments card (column defined by the account spec D10) | logged in |

Both pages send `Cache-Control: no-store` (S4). The wishlist buttons on both link to the wishlists slice's view page `/groups/:id/wishlist/:user_id` with `:user_id` = the caller's own receiver.

### Server functions

All call `require_user()` first (anonymous → `Unauthorized`), then the shared `authz` helper. Missing group and non-member both give `NotFound` (groups D2). A member calling an admin fn gets `Forbidden`.

| Fn | Auth | Result | Errors |
|---|---|---|---|
| `get_my_assignment(group_id)` | `require_member` | `Option<MyAssignment { receiver_id, receiver_name, drawn_at }>`. `None` if not drawn | `NotFound` |
| `list_my_assignments()` | user | `Vec<MyAssignmentSummary { group_id, group_name, receiver_id, receiver_name, drawn_at }>`, ordered `drawn_at DESC, group_id DESC`. One query | – |
| `get_draw_admin_status(group_id)` | `require_admin` | `DrawAdminStatus` (below) | `NotFound`, `Forbidden` |
| `perform_draw(group_id)` | `require_admin` | `()`. Stay on the page, refetch the panel, flash `draw_success` | `NotFound`, `Forbidden`, `AlreadyDrawn`, `TooFewMembers{count,min}`, `TooManyMembers{count,max}`, `DrawImpossible` |
| `reset_draw(group_id)` | `require_admin` | `()`. Stay, refetch, flash `draw_reset` | `NotFound`, `Forbidden`, `NotDrawn` |
| `resend_draw_notifications(group_id)` | `require_admin` | `u32` (rows re-queued). Stay, flash `draw_notifications_requeued {count}` | `NotFound`, `Forbidden`, `NotDrawn`, `NothingToResend` |

```text
DrawAdminStatus {
  is_drawn: bool,
  drawn_at: Option<DateTime<Utc>>,
  member_count: u32,
  blocker: Option<DrawBlocker>,                     // only when !is_drawn
  registration_open_until: Option<DateTime<Utc>>,   // Some iff !is_drawn and deadline set and deadline > now (computed on the server)
  notifications: Option<NotificationSummary>,       // Some iff is_drawn
}
DrawBlocker = TooFewMembers{count,min} | TooManyMembers{count,max} | DrawImpossible   // DrawImpossible carries no data
NotificationSummary { total, sent, pending, failed, failed_member_names: Vec<String> }   // names sorted; no receiver data
```
`DrawAdminStatus` has no field that refers to a receiver (S2), and the blocker never names a member or a rule (D10). The blocker comes from the same steps 0–2 as the draw: `get_draw_admin_status` reads the members and calls `exclusions::load_rules` in a short read transaction **without** `FOR UPDATE`, then applies step 1 and `check_feasibility`. `perform_draw` re-evaluates them under the lock.

**`perform_draw(group_id)`**
1. `require_admin` (fast fail).
2. `BEGIN`. `SELECT admin_id, is_drawn FROM \`groups\` WHERE id = ? FOR UPDATE`. If there is no row → `NotFound`. If `admin_id ≠ me` → `Forbidden` (the admin was transferred meanwhile). If `is_drawn` → `AlreadyDrawn`.
3. `SELECT user_id FROM group_members WHERE group_id = ? ORDER BY user_id` and `exclusions::load_rules(&mut tx, group_id)`.
4. `draw(members, rules, &DrawConfig::default(), &mut os_seeded_rng)`. On error, map `DrawError` and roll back:
   - `TooFewMembers{count,min}` → `TooFewMembers{count,min}`; `TooManyMembers{count,max}` → `TooManyMembers{count,max}`.
   - `Infeasible(reason)` → `DrawImpossible`. Log once at `debug` with the group id and the variant kind only (`"too_few_members"`, `"no_receiver_for"`, `"no_giver_for"` or `"blocked"`, from a local helper `infeasibility_kind(&Infeasibility) -> &'static str`). The ids inside `reason` are never logged or returned.
5. `DELETE FROM gift_assignments WHERE group_id = ?` (defensive, normally 0 rows). Insert one row per pair (`notification_sent = FALSE`, `notification_attempts = 0`). `UPDATE \`groups\` SET is_drawn = TRUE, drawn_at = CURRENT_TIMESTAMP WHERE id = ?`. `COMMIT`.
6. After the commit, send `DrawResult { group_id }` to the mail worker. Return without waiting for SMTP.

**`reset_draw(group_id)`**
1. `require_admin`. `BEGIN`. Lock the group row as above and re-check the admin. If `!is_drawn` → `NotDrawn`.
2. Read `(u.id, u.email, u.name, u.locale)` of all givers of the group (`JOIN users u ON u.id = ga.giver_id`; giver data only), plus the group name and the admin's name.
3. `DELETE FROM gift_assignments WHERE group_id = ?`. `UPDATE \`groups\` SET is_drawn = FALSE, drawn_at = NULL WHERE id = ?`. `COMMIT`. Exclusion rules and wishlists are not touched.
4. The transaction also records a reset notice for each giver except the caller; it contains no assignment or receiver information. After commit, wake the worker; periodic reconciliation also recovers notices if the wakeup is lost.

**`resend_draw_notifications(group_id)`**: `require_admin`. If `!is_drawn` → `NotDrawn`. `UPDATE gift_assignments SET notification_attempts = 0 WHERE group_id = ? AND notification_sent = FALSE AND notification_attempts >= 3`. If 0 rows → `NothingToResend`. Otherwise send `DrawResult { group_id }` to the worker and return the row count.

### Notification worker
- **Bounded workers** start at server boot. The mpsc channel is a wakeup hint, with at most four concurrent jobs per process. Database claims with expiration coordinate each result row and its group across processes, so at most one result per group is normally in flight. An SMTP attempt has a shorter timeout than its lease. A slow group need not block other groups.
- **`DrawResult { group_id }`**: `SELECT ga.id, giver.email, giver.name, giver.locale, receiver.name, g.name FROM gift_assignments ga JOIN \`groups\` g … JOIN users giver … JOIN users receiver … WHERE ga.group_id = ? AND g.is_drawn = TRUE AND ga.notification_sent = FALSE AND ga.notification_attempts < 3`. For each row, send the draw mail. On success: `UPDATE gift_assignments SET notification_sent = TRUE, notification_attempts = notification_attempts + 1, notification_last_attempt_at = CURRENT_TIMESTAMP WHERE id = ?`. On failure: the same without `notification_sent`, a `warn` log with group id, giver user id and attempt number (S5), and a retry of the group scheduled after `retry_delays[attempts - 1]` (default 60 s, then 300 s). Rows deleted by a reset are simply no longer found, so their pending mails are dropped.
- **Max attempts: 3.** Row state as shown to the admin: **sent** = `notification_sent`. **failed** = not sent and `attempts >= 3`. **pending** = otherwise.
- **Reset notices**: one durable row per recipient, with up to 3 attempts and persisted retry times. A draw generation invalidates unsent notices when another draw occurs; notices for members who have since left are removed. The final status is not shown to the admin; failures are logged with group id and user id only. A message already in SMTP cannot be recalled after reset, redraw or a recipient leaving.
- **Recovery** runs at startup and periodically for eligible rows, including expired claims and persisted retry deadlines. Delivery is at-least-once: a crash or database error after SMTP success but before recording it can resend a mail. Expired leases or SMTP/network failures can also produce duplicates, and SMTP cannot guarantee message ordering. The reset mail directs users to the current group status instead of promising that a new draw mail will arrive later.
- A draw mail may already be in flight while a reset commits. The giver can then receive the old result mail followed by the reset mail. This is accepted.
- **Test hooks**: the mailer is behind a trait, so tests can inject failures per recipient or block sending. `retry_delays` are configurable (0 in tests). The worker exposes a "wait until idle" handle for tests.

### Emails
Language per D12. HTML + plain text. All values HTML-escaped. Links from `APP_URL`.
- **Draw result** → each giver. Subject `email.drawResult.subject {group}`. Body: `email.drawResult.greeting {name}`, `email.drawResult.body {group} {receiver}`, button `email.drawResult.button` → `{APP_URL}/groups/{id}`, `email.drawResult.secret`.
- **Draw reset** → each former giver except the admin who reset. Subject `email.drawReset.subject {group}`. Body: `email.drawReset.greeting {name}`, `email.drawReset.body {group} {admin}`, `email.drawReset.next`, link → `{APP_URL}/groups/{id}`. It contains **no receiver names**.

### Data & migrations

Tables touched: `gift_assignments` (insert/delete/read own; worker updates delivery columns), `groups` (`is_drawn`, `drawn_at`; locked), read-only `group_members`, `exclusion_rules` (through `exclusions::load_rules`), `users`. `email_templates` is dropped.

`NNNN_draw.sql` (forward only; fresh start, so there are no existing rows to backfill or clean):
```sql
ALTER TABLE `groups`
    MODIFY is_drawn BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN drawn_at TIMESTAMP NULL DEFAULT NULL AFTER is_drawn;

ALTER TABLE gift_assignments
    MODIFY notification_sent BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN notification_attempts TINYINT UNSIGNED NOT NULL DEFAULT 0 AFTER notification_sent,
    ADD COLUMN notification_last_attempt_at TIMESTAMP NULL DEFAULT NULL AFTER notification_attempts,
    ADD CONSTRAINT chk_gift_assignments_not_self CHECK (giver_id <> receiver_id);

DROP TABLE email_templates;
```
- The existing `UNIQUE(group_id, giver_id)` and `UNIQUE(group_id, receiver_id)` plus the new CHECK enforce "permutation without self" at the DB level. The FK index on `giver_id` serves `list_my_assignments`.
- Implementation note on the CHECK: MySQL 8 rejects CHECK constraints on columns with cascading FK actions (`giver_id`, `receiver_id` have `ON DELETE CASCADE`). The app targets MariaDB, and AC44 shows whether the CHECK works there. If the MariaDB version in use rejects it, drop the CHECK and its AC44 assertion, rely on the UNIQUE keys plus G1, and record that in the decision log.
- **Effect on the groups slice**: groups AC22 no longer asserts `email_templates` (groups spec edited 2026-10-31). If a groups test written earlier still references the table, this slice removes that reference.
- `notification_last_attempt_at` is informational (debugging, future UI). The derived state above does not use it.

### UI notes

**`<DrawPanel>`** on `/groups/:id` (rendered between the details card and the members card):
- **Not drawn, non-admin**: `group.draw.pending`.
- **Not drawn, admin** (`get_draw_admin_status`): title `group.draw.title`, `group.draw.ready` (plural on `member_count`, `{count}`). If `member_count <= 3`, also `group.draw.smallGroupHint`.
  - If `blocker` is set: the "Perform draw" button (`group.show.performDraw`) is disabled, and the blocker text is shown below it: `group.draw.blocked.tooFewMembers {count} {min}`, `group.draw.blocked.tooManyMembers {max}`, or, for `DrawImpossible`, `exclusion.warning.drawImpossible` (exclusions slice) + `group.draw.impossible.hint`. No member names are shown.
  - Otherwise the button calls `perform_draw` after a confirm dialog: `group.show.performDrawConfirm` (reworded), plus `group.draw.confirmRegistrationOpen {date}` (`date = LocalDate(registration_open_until)`) if `registration_open_until` is set.
  - Errors returned by `perform_draw` (for example, a race) are shown with the same keys.
  - This is the **only** draw entry point (no second button, no dropdown entry).
- **Drawn, any member** (`get_my_assignment`): card `group.show.assignment`, `group.show.assignmentDescription`, receiver name (large), `group.draw.drawnOn {date}` (`date = LocalDate(drawn_at)`), and button `group.show.viewWishlist` → `/groups/{group_id}/wishlist/{receiver_id}` (wishlists view page).
- **Drawn, admin**, in addition: `group.draw.notifications.title`, `group.draw.notifications.summary {sent} {total}` (plural, selected by `total`), `…pending {count}` (plural) if > 0, `…failed {names}` if failed > 0, and button `group.draw.notifications.resend` (only if failed > 0). Then a danger button `group.draw.reset` with confirm `group.draw.resetConfirm`. The admin's panel shows **only their own** assignment card, the same as any member.
- Dates are shown with `app::format` (`I18n::date`: `APP_TIMEZONE`, date only; en `10 Dec 2026`, de `10.12.2026`). The "registration open" state comes from the server (no `Utc::now()` during render).

**Dashboard assignments card** (`list_my_assignments`): title `user.dashboard.assignments.title`. One row per assignment: `user.dashboard.assignments.group`: group name → `/groups/{id}`; `user.dashboard.assignments.recipient`: receiver name; button `user.dashboard.assignments.viewWishlist` → `/groups/{group_id}/wishlist/{receiver_id}`. Empty: `…empty` + `…emptyDescription`. It does not list undrawn groups (the groups card shows their status).

Implementation note: the wishlists slice comes before this one in the tracker, so `/groups/:id/wishlist/:user_id` normally exists. Only if it is not implemented yet when this slice is built, leave both wishlist buttons out and add them when the wishlists slice lands.

**Error → key mapping**: `NotFound` → 404 page; `Forbidden` → `flash.error.forbidden_admin_only` (groups); `AlreadyDrawn` → `flash.error.already_drawn`; `NotDrawn` → `flash.error.not_drawn`; `NothingToResend` → `flash.error.nothing_to_resend`; `TooFewMembers` → `group.draw.blocked.tooFewMembers`; `TooManyMembers` → `group.draw.blocked.tooManyMembers`; `DrawImpossible` → `exclusion.warning.drawImpossible` + `group.draw.impossible.hint`.

**i18n.** Reused unchanged: `group.show.performDraw`, `group.show.assignment`, `group.show.assignmentDescription`, `group.show.viewWishlist`, `group.status.drawn`, `flash.error.already_drawn`, `flash.error.forbidden_admin_only` (groups), `exclusion.warning.drawImpossible` (exclusions), `user.dashboard.assignments.*`.
Reworded:

| Key | en | de |
|---|---|---|
| `group.show.performDrawConfirm` | Perform the draw now? Every member gets a recipient and an email. Nobody can join or leave until the draw is reset. | Jetzt die Ziehung durchführen? Jedes Mitglied erhält einen Wichtelpartner und eine E-Mail. Bis die Ziehung zurückgesetzt wird, kann niemand beitreten oder austreten. |
| `flash.success.draw_success` | The draw is done. All members are being notified by email. | Die Ziehung ist erfolgt. Alle Mitglieder werden per E-Mail benachrichtigt. |

New (en + de required; the en text is listed, de follows the same meaning; each added together with the code that uses it). Keys marked *plural* are `.one` / `.other` pairs rendered with `t_plural!` (i18n-foundation); `.other` is the text listed, `.one` its singular:

| Key | en |
|---|---|
| `group.draw.title` | Secret Santa draw |
| `group.draw.pending` | The admin has not performed the draw yet. You will get an email when it is done. |
| `group.draw.ready` (*plural*) | {count} members will take part. (`.one`: {count} member will take part.) |
| `group.draw.smallGroupHint` | In a group this small, members can work out who gives to whom. |
| `group.draw.confirmRegistrationOpen` | Registration is open until {date}. Drawing now closes it. |
| `group.draw.blocked.tooFewMembers` | At least {min} members are needed for the draw (currently {count}). |
| `group.draw.blocked.tooManyMembers` | The draw supports at most {max} members. |
| `group.draw.impossible.hint` | Ask the members to remove some exclusion rules, then try again. |
| `group.draw.drawnOn` | Drawn on {date} |
| `group.draw.notifications.title` | Email notifications |
| `group.draw.notifications.summary` (*plural*, selector `total`) | {sent} of {total} emails sent. (`.one`: {sent} of {total} email sent.) |
| `group.draw.notifications.pending` (*plural*) | {count} are still being sent. (`.one`: {count} is still being sent.) |
| `group.draw.notifications.failed` | Could not be delivered to: {names}. They can still see their assignment on this page. |
| `group.draw.notifications.resend` | Resend failed emails |
| `group.draw.reset` | Reset draw |
| `group.draw.resetConfirm` | Reset the draw? All assignments are deleted and every member is told by email that their assignment is no longer valid. You can then change members and exclusions and draw again. |
| `flash.success.draw_reset` | The draw was reset. All assignments were deleted. |
| `flash.success.draw_notifications_requeued` (*plural*) | {count} emails are being sent again. (`.one`: {count} email is being sent again.) Pushed with `with_plural(n)` + `with_number("count", n)`. |
| `flash.error.not_drawn` | The draw has not been performed yet. |
| `flash.error.nothing_to_resend` | There are no failed emails to resend. |
| `email.drawResult.subject` | {group}: your Secret Santa recipient |
| `email.drawResult.greeting` | Hello {name}, |
| `email.drawResult.body` | The draw for {group} is done. You are giving a gift to {receiver}. |
| `email.drawResult.button` | Open the group and their wishlist |
| `email.drawResult.secret` | Keep it a secret! |
| `email.drawReset.subject` | {group}: the draw was reset |
| `email.drawReset.greeting` | Hello {name}, |
| `email.drawReset.body` | {admin} has reset the draw for {group}. Your previous assignment is no longer valid. |
| `email.drawReset.next` | Open the group to see the current draw status. |

Removed (not ported): `flash.error.draw_failed` (replaced by the blocker keys above), `flash.warning.draw_partial_success` (leaked `{error}`; replaced by the admin notification summary), `flash.error.only_admin_draw` (replaced by groups' `forbidden_admin_only`).

## Acceptance criteria

Fixtures: users Alice, Bob, Carol, Dave, Erin. G = group with admin Alice, members Alice, Bob, Carol, not drawn, no dates, no rules, unless an AC says otherwise. "Valid" means G1. Algorithm ACs call `draw()` directly with `StdRng::seed_from_u64(seed)` (or `ChaCha20Rng`) and the default `DrawConfig` unless stated.

**Algorithm (pure)**
- AC1: For every n in 2..=12 without rules, and for seeds 0..200, `draw` returns a valid assignment: each member appears exactly once as giver and once as receiver, and nobody gives to themselves.
- AC2: Given the same members and rules in a different order and with duplicates, and the same seed, the output is identical. For 4 members, seeds 0..50 produce at least 2 different outputs.
- AC3: Given 2 members [A, B], then the result is A→B, B→A for every seed.
- AC4: Given 4 members without rules, over seeds 0..60,000, then every result is one of the 6 four-cycles (never two reciprocal pairs), and the chi-square statistic of the 6 counts against 10,000 each is below 20.52 (df 5, p = 0.001).
- AC5: Given 4 members with couples excluded both ways (A↔B, C↔D), over seeds 0..10,000, then every result is one of the two 4-cycles A→C→B→D→A or A→D→B→C→A, and each appears 50 % ± 2 % of the time.
- AC6: Given 3 members with rule A→B forbidden, then for every seed the result is A→C, C→B, B→A.
- AC7: Given 6 members where A and B have each excluded C, D, E, F (so A↔B is forced) and C–F have no rules, over seeds 0..1,000, then every result has exactly 1 reciprocal pair (A↔B, and C–F always form a 4-cycle).
- AC8: Given 10 members without rules, over seeds 0..20,000, then no result has a reciprocal pair, and the receiver counts of the first member over the other 9 pass a chi-square test against uniform (statistic < 26.12, df 8).
- AC9: Given `rejection_attempts = 0` (fallback 4b forced) and 500 random rule sets on 10–30 members (seeded generator): for each rule set where `check_feasibility` returns `Feasible`, the result is valid. For each rule set where it returns `Infeasible(r)`, `draw` returns `DrawError::Infeasible(r)` with the same `r`.
- AC10: Given 1 member, then `DrawError::TooFewMembers{count:1, min:2}`. Given 201 members, then `DrawError::TooManyMembers{count:201, max:200}`.
- AC11: Given 3 members where Bob has excluded Alice and Carol, then `DrawError::Infeasible(NoReceiverFor{giver: Bob})`. Given Alice and Carol have both excluded Bob, then `DrawError::Infeasible(NoGiverFor{receiver: Bob})`.
- AC12: Given rules that name a non-member, or self pairs, then they are ignored (the result equals the result without them for the same seed).
- AC13: Given 200 members where each excludes 10 others (seeded, feasible), then `draw` completes in under 5 s in the `cargo leptos test` profile, both with the default config and with `rejection_attempts = 0`.

**Perform draw**
- AC14: Given G, when Alice confirms "Perform draw", then G has 3 `gift_assignments` rows forming a valid assignment, `is_drawn = TRUE`, `drawn_at` is set, she sees `draw_success`, and her panel shows only her own assignment card, whose `group.show.viewWishlist` button links to `/groups/{G}/wishlist/{her receiver's id}`. After the worker is idle, Mailpit holds exactly one draw mail per member. Each one names that member's own receiver and links to `{APP_URL}/groups/{G}`.
- AC15: Given a 2-member group (Alice, Bob), when Alice performs the draw, then it succeeds with Alice→Bob and Bob→Alice. The panel showed `group.draw.smallGroupHint` before the draw.
- AC16: Given Bob calls `perform_draw(G)`, then `Forbidden`. Given Dave calls it, then `NotFound`. Given an anonymous call, then `Unauthorized`. In each case no rows are written and no mail is sent.
- AC17: Given G is drawn, when `perform_draw(G)` is called, then `AlreadyDrawn`, the rows are unchanged and no mail is sent.
- AC18: Given a group with only Alice, then the panel shows a disabled button with `group.draw.blocked.tooFewMembers` (count 1, min 2), and `perform_draw` returns `TooFewMembers`.
- AC19: Given Dave has joined G, Bob has added the rules Bob→Alice and Bob→Carol (accepted while Dave was a member), and Dave then leaves G (so Bob can draw nobody, as in exclusions AC15), then `get_draw_admin_status(G).blocker` is `DrawImpossible`, Alice's panel shows a disabled button with `exclusion.warning.drawImpossible` and `group.draw.impossible.hint`, and `perform_draw(G)` returns `DrawImpossible` without writing anything. Neither the serialized status, the error nor the rendered panel contains Bob's (or any member's) name or id in the blocker, and the captured `tracing` output contains the infeasibility kind but no user ids or names.
- AC20: Given G has a registration deadline in the future, then `registration_open_until` is set and the confirm dialog includes `group.draw.confirmRegistrationOpen` with that date. The draw succeeds, and afterwards `join_group` for Dave returns `AlreadyDrawn`.
- AC21: Given a plain GET or a cross-origin POST to the `perform_draw` or `reset_draw` endpoint, then nothing changes.
- AC22: Given two `perform_draw(G)` calls at the same time, then exactly one succeeds, the other returns `AlreadyDrawn`, G has exactly 3 rows, and exactly 3 draw mails are sent.
- AC23: Given a transaction holding G's row lock (as a join does), then `perform_draw` waits for it and draws the member set as it is after that transaction commits. Every member of the drawn group has exactly one row as giver and one as receiver (complements groups AC44).
- AC24: Given `perform_draw` runs while the mailer is blocked, then the call returns `Ok` before any mail is sent.

**Secrecy**
- AC25: After drawing G, `get_my_assignment(G)` returns, for each of Alice, Bob and Carol, exactly their own receiver. Dave gets `NotFound`. For an undrawn group it returns `None`.
- AC26: The response types of `get_draw_admin_status`, `perform_draw`, `reset_draw` and `resend_draw_notifications` contain no receiver id or name and no giver→receiver data. This is checked on the types and on a serialized `get_draw_admin_status` response for Alice (admin) with one injected mail failure for Bob: the only member name in it is "Bob" inside `failed_member_names`, and no field refers to a receiver.
- AC27: Given G is drawn, with members named with unique markers, when Alice loads `/groups/{G}` and `/user/dashboard`, then the full response bodies (HTML + hydration data) contain exactly one assignment element and exactly one serialized assignment (`receiver_id`/`receiver_name`), both for her own receiver. Other members' names appear only in the members card and the wishlists panel (which lists every member alike), never in an assignment context. The same holds for Bob.
- AC28: Responses for `/groups/{G}` and `/user/dashboard` have `Cache-Control: no-store`.
- AC29: Given a captured `tracing` subscriber during `perform_draw`, worker delivery (including one injected failure), `get_my_assignment`, `list_my_assignments` and `reset_draw`, then the captured output contains no receiver names, no receiver ids paired with givers, no email addresses and no mail bodies.
- AC30: There is no server fn that accepts a user id and returns assignment data. (Review check, plus a test listing the slice's registered server fns.)

**Dashboard**
- AC31: Given Bob is in drawn groups G (drawn first) and H (drawn later) and undrawn K, when he opens `/user/dashboard`, then the card lists H then G, each with the group name linked to the group, his own receiver's name, and a wishlist button linking to `/groups/{group}/wishlist/{his receiver's id}`. K is not listed.
- AC32: Given Dave has no assignments, then the card shows `user.dashboard.assignments.empty` and `.emptyDescription`.
- AC33: Given a `gift_assignments` row for Bob in a group where he has no `group_members` row (inserted directly), then `list_my_assignments` does not return it and `get_my_assignment` returns `NotFound` for that group.

**Notifications**
- AC34: Given the mailer fails for Bob's address and retry delays are 0, when Alice draws G, then after the worker is idle Bob's row has `notification_sent = FALSE` and `notification_attempts = 3`, the other rows are sent, and Alice's panel shows `summary` 2 of 3, `failed` 1 with name "Bob", and the resend button.
- AC35: Given AC34, and Bob's address works again, when Alice clicks "Resend failed emails", then `resend_draw_notifications` returns 1 and the flash shows `draw_notifications_requeued` (1). Bob gets exactly one draw mail, his row becomes sent, and Alice and Carol get no second mail.
- AC36: Given no failed rows, `resend_draw_notifications(G)` returns `NothingToResend`. Given an undrawn group, then `NotDrawn`. Given Bob calls it, then `Forbidden`.
- AC37: Given a drawn group with rows `notification_sent = FALSE, notification_attempts < 3` at server start, then the worker sends each of those mails once. Rows with `notification_attempts >= 3` are not retried.
- AC38: Given the group name is `<b>x</b>` and a member name is `<script>y</script>`, then the draw mail's HTML contains them escaped, the mail has a plain-text part, and for a giver whose `users.locale` is `de` the subject and body come from the `de` messages (HTML root `lang="de"`).
- AC39: While mails are pending, Alice's panel shows `group.draw.notifications.pending` with the pending count.

**Reset**
- AC40: Given G is drawn and all mails are sent, when Alice confirms "Reset draw", then G has 0 assignment rows, `is_drawn = FALSE`, `drawn_at = NULL`, and Alice sees `draw_reset`. Bob and Carol each get one reset mail that contains no receiver name; Alice gets none. `get_my_assignment(G)` returns `None` for everyone, and the dashboard cards no longer list G.
- AC41: Given AC40, then Bob can leave G, Alice can remove Carol, Dave can join (no deadline), exclusions can be edited again, and a new `perform_draw` succeeds and sends new draw mails to the current members.
- AC42: Given Bob calls `reset_draw(G)`, then `Forbidden`. Given Dave calls it, then `NotFound`. Given G is not drawn, then `NotDrawn`. Nothing changes in any case.
- AC43: Given draw mails for G are still pending (mailer blocked) when Alice resets, then after unblocking no draw-result mail for the deleted rows is sent, and the reset mails are sent.

**Data**
- AC44: After all migrations: `email_templates` does not exist; `groups.is_drawn` is `NOT NULL DEFAULT FALSE`; `groups.drawn_at` exists; `gift_assignments` has `notification_attempts` and `notification_last_attempt_at`; inserting a row with `giver_id = receiver_id` fails the CHECK; a second row for the same `(group_id, giver_id)` or `(group_id, receiver_id)` fails the UNIQUE.
- AC45: Deleting a drawn group (groups `delete_group`) removes its assignment rows, and the worker does not send its pending mails afterwards.

**UI / i18n**
- AC46: Before the draw, Bob's panel shows `group.draw.pending` and no draw or reset buttons. Alice's panel shows exactly one "Perform draw" button. After the draw, Bob sees his assignment card and no admin section.
- AC47: Given the session locale `de`, then all panel texts, blocker reasons, the confirm dialogs and the dashboard card come from the `de` messages. Every new key in this spec exists in `en.json` and `de.json`.

**Mail language and formatting** (added 2026-10-31, i18n-foundation)
- AC48: Given G with Alice (admin, `users.locale = 'de'`), Bob (`'en'`) and Carol (NULL), and `APP_LOCALE=en`, when Alice draws, then Alice's draw mail is German and Bob's and Carol's are English, whatever locale Alice's page shows. When Alice then resets, Bob's and Carol's reset mails are English. With `APP_LOCALE=de` instead, Carol's mails are German. Given Carol's stored `locale` is `'fr'` (written directly), her mails use `APP_LOCALE`.
- AC49: Given G was drawn at 2026-12-10 22:30 UTC, then the assignment card shows "Drawn on 10 Dec 2026" in English and the `de` text with `10.12.2026` in German. Given a 1-member group, the admin panel shows the singular `group.draw.ready` form. Given 1 of 1 mails sent, the summary uses the singular form; with 1 pending, `pending` uses the singular form; and `resend_draw_notifications` returning 1 shows "1 email is being sent again." (en) after the flash is rendered.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **Receiver's name in the draw mail**: included (D13). The mail goes only to the giver, and the reset mail clears up stale ones.
2. **Language of draw and reset mails**: originally `APP_LOCALE`. Superseded 2026-10-31 (user, i18n-foundation): each recipient's account language `users.locale`, falling back to `APP_LOCALE` (D12).
3. **Member cap for the draw**: 200 members (`MAX_DRAW_MEMBERS`, `TooManyMembers`; D3).
4. **Naming members in the "draw impossible" reason**: no. Generic message only. The admin is never told which members or rules make the draw impossible; `perform_draw` returns a single `DrawImpossible` and the blocker is a data-less `DrawImpossible` (D10). The internal `Infeasibility` variant may be logged at debug level as its kind only, without names or ids (S5). This follows the exclusions spec's visibility rule (exclusions D4).
- Edit 2026-10-31 (cross-spec review): the algorithm now uses the exclusions slice's `check_feasibility -> Feasibility` and `load_rules` exactly; the pure function's error type is `DrawError` (to keep the name `DrawImpossible` for the generic server error). The `draw::receiver_of` helper was removed (wishlists Resolved 1 needs no assignment lookup), and the wishlist buttons link to `/groups/:id/wishlist/:user_id`. The migration's backfill steps were removed (fresh start). The former AC24 (draw ignores a rule naming a non-member, inserted directly) was removed because the exclusions migration's composite FKs make that row impossible; AC12 still covers stray rules at the pure level. AC25–AC48 became AC24–AC47.
- Decided 2026-10-31 (user): **not added**. Exclusions Resolved 4 allowed this slice an optional generic admin warning (no names) when only one draw is possible. There is no such warning; `group.draw.smallGroupHint` covers groups of 2–3. Adding it later is a spec change.
