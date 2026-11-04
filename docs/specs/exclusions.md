---
slice: exclusions
status: approved
---
# Exclusions

Scope: a member manages their own exclusion rules for a group ("I should not draw X"). Also defined here, for the draw slice to reuse: the data contract the draw reads and a pure feasibility check.
Not in scope: the draw algorithm, reciprocal-pair avoidance, draw notifications and "reset draw" (draw slice). Membership cleanup on leave/removal is already specified in groups D5; this spec only depends on it.

Fresh start: no production data is migrated. The PHP schema is only a reference.

## Current PHP behaviour

### Routes (`src/Core/Application.php`, `ExclusionController`)

Every handler calls `requireAuth()` (redirects to login and remembers `intended_url`). The router matches `:groupId` with `[^/]+`, and the handlers are typed `int`. No `strict_types`, so a non-numeric id most likely throws a `TypeError`, which gives a 500.

| Method | Path | Handler | Authorization | What it does |
|---|---|---|---|---|
| GET | `/exclusions/:groupId` | `index` | member | Shows the caller's own rules for the group and a form to add one. Redirects to the group if it is drawn. |
| POST | `/exclusions/:groupId/add` | `add` | member | Adds the rule `(group, user_id = me, excluded_user_id = POST excluded_user_id)`. |
| GET | `/exclusions/:groupId/remove/:excludedUserId` | `remove` | member | Deletes the caller's rule for that excluded user. |

The order of checks is the same in all three handlers:
1. The caller is not a member (this includes a group that does not exist) → flash error "You are not a member of this group" (hard-coded English), redirect to `/groups`. Missing and foreign groups get the same message, so ids are not enumerable here.
2. The group does not exist → "Group not found" (unreachable after step 1).
3. `is_drawn` → flash error "Exclusions cannot be modified/added/removed after the draw has been performed" (three hard-coded English variants), redirect to `/groups/{id}`.

### Add
- `excluded_user_id` is cast with `(int)`. An empty or garbage value becomes `0`.
- The target is not a member of the group (including `0`) → "Selected user is not a member of this group".
- The target is the caller → "You cannot exclude yourself".
- `addExclusion` returns the existing rule if it already exists, so a duplicate add reports **success**. Self-exclusion is checked a second time there and returns `null` → "Failed to add exclusion rule" (unreachable).
- There is no limit on the number of rules and no feasibility check. Success flash: "Exclusion rule added successfully". Redirect to `/exclusions/{id}`.

### Remove
- GET link with a JS `confirm(t('exclusion.current.removeConfirm'))`.
- It deletes only the caller's own rule (`user_id = me`). A missing rule → "Failed to remove exclusion rule". Success → "Exclusion rule removed successfully". The target does not have to be a member.

### View (`exclusion/index.php`)
- Header `exclusion.title`, `exclusion.forGroup` + group link, `exclusion.backToGroup`.
- "Add" card: `exclusion.addNew.*`, a `<select>` of all members except the caller and except those already excluded.
- "Why" card: `exclusion.why.*`. The view prints a hard-coded `<strong>Note:</strong>` before `exclusion.why.note`, whose text already starts with "Note:"/"Hinweis:".
- "Current" card: `exclusion.current.*`, the caller's rules with the excluded user's name and a Remove link.
- Only the caller's **own outgoing** rules are shown. Nobody sees other members' rules, the admin included, and nobody sees rules that point at them.

### Group page hook
`group/show.php` shows "Manage Exclusions" (`group.show.manageExclusions`) to every member while the group is not drawn.

### Consumption by the draw (`GroupRepository::performDraw` / `drawSecretSanta`)
- Loads members from `group_members` and every `exclusion_rules` row of the group. It builds `exclusionMap[user_id][] = excluded_user_id`.
- A receiver is valid for a giver if it is not the giver, not in `exclusionMap[giver]`, and the pair is not reciprocal with an assignment already made (hard rule).
- Greedy random assignment, 10 attempts. Failure gives the generic `flash.error.draw_failed`, with no reason.
- Semantics, therefore: a rule is **one-way and giver-side**. `(user_id = A, excluded_user_id = B)` means "A must not give to B". It does not stop B from giving to A.
- Rules whose giver has left are ignored (the giver is no longer among the members). Rules pointing at a former member are harmless.

### Data (`exclusion_rules`)
`id, group_id FK groups CASCADE, user_id FK users CASCADE, excluded_user_id FK users CASCADE, created_at, UNIQUE(group_id, user_id, excluded_user_id)`. There is no constraint against `user_id = excluded_user_id`, and no link to `group_members`.

### Emails
None.

## Problems in the PHP version
1. **CSRF**: remove is a GET link, so any page can delete a member's rules. Add is a POST without a token.
2. All flash messages of this controller are hard-coded English (no i18n).
3. "Note:" is printed twice (hard-coded prefix plus the key text).
4. No cap on rules. One member can exclude everyone and make the draw impossible.
5. No feasibility feedback. When the rules (or the hard reciprocal ban) make a draw impossible, the admin gets the generic `draw_failed`, does not know why, and cannot see the rules to fix it. The greedy 10-attempt draw can also fail when a valid draw exists (draw slice).
6. The help text `exclusion.addNew.description` ("prevent you from being assigned to specific people") reads as if it worked in both directions. It only works giver-side.
7. Leavers' rules (both directions) stay behind and come back when they rejoin (fixed by groups D5).
8. No locking: a rule added while the admin draws can commit after the draw has read the rules. The user thinks it applies, but it does not.
9. Non-numeric ids in the URL probably give a 500.
10. Inherent to small groups (not a bug, but relevant to secrecy): with 3 members only two valid draws exist (the two 3-cycles), so **any** single rule determines the whole assignment, and its author can work out every pairing.

## Redesign

### Decisions taken in this spec
- **D1 Semantics.** A rule `(group_id, user_id = G, excluded_user_id = X)` means "in this group, G must not be assigned to give to X". It is one-way. Self is always implicitly excluded and is never stored.
- **D2 Ownership: self-service only.** A member creates and deletes only rules where they are the giver. The giver is always the session user. No server fn takes a giver id, so nobody can create or delete another member's rule. The admin has no extra powers here: they manage their own rules like any member, and there is no admin management or view of rules. Reason: someone who could set everybody's rules could force the whole draw and learn every assignment, which breaks "never expose another user's assignment, not to admins either" (CLAUDE.md). (Resolved 1.)
- **D3 Direction in the UI: one-way only.** There is no "mutual" or "couple" option. A couple adds one rule each. Reason: a mutual option would write a rule for another member without their consent. (Resolved 2.)
- **D4 Visibility.** A member sees only their own outgoing rules. Nobody (admin included) sees other members' rules or rules that point at them. `get_group` and every other fn of every slice never return rule data except the caller's own rules. The only aggregate visible to members is the 1-bit feasibility notice (D7). The draw slice's admin blocker is the same single bit (draw Resolved 4).
- **D5 Rules lock with the draw.** While `is_drawn` is true, add and remove return `AlreadyDrawn`, and the page shows the caller's rules read-only. Rules are **kept** after the draw. If the draw slice's "reset draw" sets `is_drawn` back to false, the same rules apply and are editable again.
- **D6 Cap.** At most `MAX_EXCLUSIONS_PER_MEMBER = 5` rules per giver per group (constant in the shared crate). An idempotent re-add of an existing rule does not count as new. (Resolved 5.)
- **D7 Feasibility.**
  - Pure function `check_feasibility` (below). It uses hard constraints only: no self-assignment, plus the rules. Reciprocal avoidance is a soft preference of the draw slice and is not part of it, so a 2-member group without rules is feasible.
  - `add_exclusion` **rejects** a new rule if the group would become infeasible with it (`WouldMakeDrawImpossible`). Removing a rule can never make a draw less feasible, so it is always allowed. (Resolved 3.)
  - Joining never makes a feasible group infeasible: a newcomer has no rules and is excluded by nobody, so they can be inserted into any cycle. Leaving or removal can (e.g. the only person someone may still draw leaves). Therefore the page also shows the **current** state: a generic banner for every member when the group is infeasible, and a specific banner for a member whose own rules leave them no possible receiver.
  - Accepted leak: the reject/banner is a 1-bit oracle about the combination of everyone's rules. It never names a person or a rule. The cap and the "only current members" constraint keep it small.
  - No predictability guard in this slice (Resolved 4): a rule that leaves only one possible draw is accepted like any other.
- **D8 Authorization.** Every fn calls `require_user()`, then `authz::require_member(group_id)`. A missing group and a group the caller is not a member of give the **same `NotFound`** (groups D2). Note for implementation: `rust/app/src/server/authz.rs::require_member` already returns `NotFound` for both cases (checked 2026-10-31); use it as is.
- **D9 Concurrency.** `add_exclusion` and `remove_exclusion` run in one transaction that starts with `SELECT is_drawn FROM \`groups\` WHERE id = ? FOR UPDATE` (the same row lock as groups D16 and draw D5). Membership, `is_drawn`, the cap and feasibility are all checked under that lock. This also serializes two members adding rules at the same time, so their combination cannot slip past the feasibility check. Lock order: the group row first, then `exclusion_rules`.
- **D10 Targets** must be current members of the group. Enforced in the fn and by the DB (migration below).

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/groups/:id/exclusions` | Manage my exclusions | member (anyone else: the same 404 page as `/groups/:id`) |

- Logged out: redirect to login with `?next=/groups/:id/exclusions` (auth slice mechanism).
- `:id` that is not a positive integer gives the 404 page.
- The old `/exclusions/...` URLs are not ported (they give 404).
- Hook `<ExclusionsLink group_id is_drawn>` on the group page (groups spec): renders a link `group.show.manageExclusions` → `/groups/{id}/exclusions` for members when `!is_drawn`, and nothing when drawn.

### Server functions

Errors are one enum `ExclusionError` (or the shared `AppError` with these variants). They map to the i18n keys in UI notes.

| Fn | Auth | Result | Errors |
|---|---|---|---|
| `get_exclusions(group_id: i32)` | `require_member` | `ExclusionsPage` (below) | `Unauthorized`, `NotFound` |
| `add_exclusion(group_id: i32, excluded_user_id: i32)` | `require_member` under lock | `()`; stay on page, refetch, flash `exclusion_added` | `Unauthorized`, `NotFound`, `AlreadyDrawn`, `CannotExcludeSelf`, `TargetNotMember`, `TooManyExclusions { max }`, `WouldMakeDrawImpossible` |
| `remove_exclusion(group_id: i32, excluded_user_id: i32)` | `require_member` under lock | `()`; stay on page, refetch, flash `exclusion_removed` | `Unauthorized`, `NotFound`, `AlreadyDrawn` |

`add_exclusion`, in this order and inside the transaction:
1. Lock the group row. It does not exist → `NotFound`.
2. `require_member` → `NotFound` for a non-member.
3. `is_drawn` (NULL counts as false) → `AlreadyDrawn`.
4. `excluded_user_id == me` → `CannotExcludeSelf`.
5. The target is not in `group_members` for this group (also covers non-existent and non-positive ids) → `TargetNotMember`.
6. The rule already exists → return `Ok(())`. No change and no second row (idempotent; covers double clicks).
7. The caller already has `MAX_EXCLUSIONS_PER_MEMBER` rules in this group → `TooManyExclusions { max: 5 }`.
8. `check_feasibility(current_members, current_rules + new_rule)` is not `Feasible` → `WouldMakeDrawImpossible`.
9. `INSERT INTO exclusion_rules (group_id, user_id, excluded_user_id) VALUES (?, ?, ?)`, then commit.

`remove_exclusion`: steps 1–3 as above, then `DELETE FROM exclusion_rules WHERE group_id = ? AND user_id = ? AND excluded_user_id = ?` with `user_id = me`. Zero rows affected is still `Ok(())` (idempotent). The target does not have to be a member.

`ExclusionsPage`:
- `group_id, group_name, is_drawn`
- `my_rules: Vec<MemberRef { user_id, name }>`: only `user_id = me` rules, joined to current members, ordered by `name, user_id`.
- `candidates: Vec<MemberRef>`: current members except me and except those in `my_rules`, ordered by `name, user_id`. Empty when `is_drawn`.
- `max_rules: u8` (= 5), `can_add: bool` (= `!is_drawn && my_rules.len() < max && !candidates.is_empty()`)
- `notice: Option<FeasibilityNotice>`. `None` when drawn or feasible. Otherwise `Some(DrawImpossible { no_receiver_for_me: bool })`. `no_receiver_for_me` is computed directly: the members minus me minus `my_rules` is empty. `TooFewMembers` (fewer than 2 members) gives `None`; the group page and the draw slice already cover that case.
- Computed on the server. It never contains another member's rules or any assignment.

### Feasibility function (shared, pure, reused by the draw slice)

Location: the shared `app` crate, e.g. `app/src/domain/feasibility.rs`. No `ssr` gate, no DB and no I/O, so it is unit-testable.

```rust
pub type UserId = i32;

/// `members`: current group members. `exclusions`: (giver, excluded) pairs.
pub fn check_feasibility(members: &[UserId], exclusions: &[(UserId, UserId)]) -> Feasibility;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feasibility {
    Feasible,
    Infeasible(Infeasibility),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Infeasibility {
    /// Fewer than 2 distinct members.
    TooFewMembers,
    /// This giver may draw nobody (excluded every other member).
    NoReceiverFor { giver: UserId },
    /// Nobody may draw this member (every other member excluded them).
    NoGiverFor { receiver: UserId },
    /// Hall violation: these givers together may only draw these receivers,
    /// and there are fewer receivers than givers. Both sorted ascending.
    Blocked { givers: Vec<UserId>, receivers: Vec<UserId> },
}
```

Contract:
- "Feasible" means that a bijection `assign: members → members` exists with `assign(m) != m` for every m and `assign(g) != x` for every rule `(g, x)`. Cycles of length 2 are allowed.
- Input normalisation: duplicate members are de-duplicated. Rules whose giver or excluded member is not in `members`, self-rules and duplicate rules are ignored. The result depends only on the sets, not on input order (sort ids internally).
- Reason priority (deterministic): `TooFewMembers`, then `NoReceiverFor` with the smallest giver id, then `NoGiverFor` with the smallest receiver id, then `Blocked`.
- `Blocked` witness: from a maximum bipartite matching (giver → allowed receiver; Kuhn or Hopcroft–Karp), take the first unmatched giver in ascending id order. `givers` = the givers reachable from it via alternating paths, `receivers` = the union of their allowed receivers. Guaranteed: `receivers.len() < givers.len()`, and every allowed receiver of every giver in `givers` is in `receivers`.
- Complexity: O(n · E) at most. Fine for any realistic group (groups have no size cap; the draw itself is capped at 200 members, draw D3).
- Disclosure rule for callers: reasons carry user ids for logic and tests only. **No UI may render the ids, names or rules from an `Infeasibility`.** Members see only the `FeasibilityNotice` of this slice. The draw slice shows the admin the generic `exclusion.warning.drawImpossible` (reused from this slice) and never names anyone, because `NoGiverFor { Bob }` would reveal that everyone excluded Bob. Logs may record at most the variant kind, never the ids (draw S5).

### Data contract for the draw slice
- Rules are read inside the draw's group-row lock with `SELECT user_id, excluded_user_id FROM exclusion_rules WHERE group_id = ? ORDER BY user_id, excluded_user_id`. They are exposed as `exclusions::load_rules(tx, group_id) -> Vec<(UserId, UserId)>`, a server-only function of this slice.
- Each pair is a hard constraint: the giver `user_id` must not be assigned to `excluded_user_id`. The draw calls `check_feasibility(members, rules)` first and must produce an assignment that respects every rule whenever the result is `Feasible`.
- The draw spec (`draw.md`, algorithm step 2 and `perform_draw` step 3) uses exactly `load_rules` and `check_feasibility` as defined here.
- After the migration below, every row is guaranteed to reference two current members of the group, so no filtering is needed. `check_feasibility` ignores stray pairs anyway.
- Reset draw (draw slice) must not delete rules.

### Data & migrations

Tables touched: `exclusion_rules` (CRUD for own rows), read-only `group_members`, `users.name` and `` `groups` ``.

New migration `NNNN_exclusion_rules_member_fks.sql`. Intent: rules can only exist between current members, and removing a membership removes the rules in both directions at DB level. This backs up groups D5, which still deletes them explicitly.

```sql
ALTER TABLE exclusion_rules
  ADD CONSTRAINT fk_exclusion_giver_member
    FOREIGN KEY (group_id, user_id) REFERENCES group_members (group_id, user_id) ON DELETE CASCADE,
  ADD CONSTRAINT fk_exclusion_excluded_member
    FOREIGN KEY (group_id, excluded_user_id) REFERENCES group_members (group_id, user_id) ON DELETE CASCADE;
```

- Fresh start: the table is empty when this migration runs. (Implementation 2026-11-02: the migration still deletes rules whose giver or excluded user is not a member before adding the FKs — a no-op on a fresh DB; see the decision log.)
- `group_members` already has `UNIQUE(group_id, user_id)`, which is a valid FK target. `(group_id, user_id)` is covered by the prefix of `unique_exclusion`. InnoDB creates the index for `(group_id, excluded_user_id)`.
- The existing single-column FKs and `unique_exclusion` stay. Self-rules are rejected by the server fn (step 4) and ignored by `check_feasibility`. No CHECK constraint is added for them.
- No other schema change. The cap is enforced in the fn under the lock (D9).

### UI notes

**`/groups/:id/exclusions`** (calls `get_exclusions`). Layout as in PHP, Bootstrap cards.
- Header: `exclusion.title`, `exclusion.forGroup` + group name linking to `/groups/{id}`, button `exclusion.backToGroup`.
- Privacy line under the header: `exclusion.privacy` (new).
- `notice` banner (warning style) above the cards: `exclusion.warning.drawImpossible` (new). If `no_receiver_for_me` is true, also show `exclusion.warning.noReceiverLeft` (new).
- Not drawn, "Add" card: `exclusion.addNew.title`, `exclusion.addNew.description` (**reworded**, below), `exclusion.addNew.limit` (new, `{count}`, `{max}`).
  - With `can_add`: select labelled `exclusion.addNew.selectPerson`, placeholder `exclusion.addNew.selectPlaceholder`, options = `candidates`, and submit `exclusion.addNew.submit` (disabled while no person is selected or the call is pending).
  - At the cap: `exclusion.addNew.limitReached` (new, `{max}`) instead of the form. No candidates: `exclusion.addNew.noCandidates` (new).
- "Why" card: `exclusion.why.title`, `.intro`, `.reason1`–`.reason3`, `.note`. **No hard-coded "Note:" prefix**; the key text is **reworded** (below).
- "Current" card: `exclusion.current.title`, the empty state `exclusion.current.empty`, and per rule the name plus a Remove button (`exclusion.current.remove`) behind a confirm (`exclusion.current.removeConfirm`) that calls `remove_exclusion`.
- Drawn: a notice `exclusion.locked` (new). No Add card. The Current card lists the rules without Remove buttons, and the banner is not shown.
- Errors from the fns are shown as an error toast with the key below. `NotFound` renders the 404 page.

**Reworded existing keys**
| Key | en | de |
|---|---|---|
| `exclusion.addNew.description` | "Choose people you should not draw. This only works one way: they can still draw you." | "Wählen Sie Personen, die Sie nicht ziehen sollen. Das gilt nur in eine Richtung: Diese Personen können Sie trotzdem ziehen." |
| `exclusion.why.note` | "Note: Too many exclusions can make a draw impossible. You will be told if an exclusion would do that." | "Hinweis: Zu viele Ausschlüsse können eine Ziehung unmöglich machen. Sie werden informiert, wenn ein Ausschluss das bewirken würde." |

**New keys**
| Key | en | de |
|---|---|---|
| `exclusion.privacy` | "Only you can see your exclusions. Other members and the group admin cannot." | "Nur Sie sehen Ihre Ausschlüsse. Andere Mitglieder und der Gruppenadmin sehen sie nicht." |
| `exclusion.addNew.limit` | "You can exclude up to {max} people ({count} used)." | "Sie können bis zu {max} Personen ausschließen ({count} verwendet)." |
| `exclusion.addNew.limitReached` | "You have reached the maximum of {max} exclusions. Remove one to add another." | "Sie haben das Maximum von {max} Ausschlüssen erreicht. Entfernen Sie einen, um einen neuen hinzuzufügen." |
| `exclusion.addNew.noCandidates` | "There is nobody else in this group you could exclude." | "In dieser Gruppe gibt es niemanden mehr, den Sie ausschließen könnten." |
| `exclusion.locked` | "The draw has been performed. Exclusions can no longer be changed." | "Die Ziehung wurde durchgeführt. Ausschlüsse können nicht mehr geändert werden." |
| `exclusion.warning.drawImpossible` | "With the current exclusions in this group, no valid draw is possible. Some exclusions must be removed before the draw." | "Mit den aktuellen Ausschlüssen in dieser Gruppe ist keine gültige Ziehung möglich. Vor der Ziehung müssen einige Ausschlüsse entfernt werden." |
| `exclusion.warning.noReceiverLeft` | "Your exclusions leave nobody you could draw. Please remove at least one." | "Durch Ihre Ausschlüsse bleibt niemand übrig, den Sie ziehen könnten. Bitte entfernen Sie mindestens einen." |
| `flash.success.exclusion_added` | "Exclusion added." | "Ausschluss hinzugefügt." |
| `flash.success.exclusion_removed` | "Exclusion removed." | "Ausschluss entfernt." |
| `flash.error.exclusion_self` | "You cannot exclude yourself." | "Sie können sich nicht selbst ausschließen." |
| `flash.error.exclusion_limit_reached` | "You can exclude at most {max} people in a group." | "Sie können in einer Gruppe höchstens {max} Personen ausschließen." |
| `flash.error.exclusion_would_block_draw` | "This exclusion would make a valid draw impossible, so it was not added." | "Dieser Ausschluss würde eine gültige Ziehung unmöglich machen und wurde daher nicht hinzugefügt." |
| `flash.error.exclusions_locked` | "Exclusions cannot be changed after the draw." | "Ausschlüsse können nach der Ziehung nicht mehr geändert werden." |

`exclusion.warning.drawImpossible` is also used by the draw slice's admin panel (draw UI notes); keep its text suitable for both members and the admin.

**Error → key mapping**: `AlreadyDrawn` → `flash.error.exclusions_locked`, `CannotExcludeSelf` → `flash.error.exclusion_self`, `TargetNotMember` → `flash.error.user_not_in_group` (reused; groups' `TargetNotMember` may reuse the same key, which groups allows), `TooManyExclusions{max}` → `flash.error.exclusion_limit_reached`, `WouldMakeDrawImpossible` → `flash.error.exclusion_would_block_draw`, `NotFound` → 404 page.

**Reused unchanged**: `exclusion.title`, `exclusion.forGroup`, `exclusion.backToGroup`, `exclusion.addNew.title`, `.selectPerson`, `.selectPlaceholder`, `.submit`, `exclusion.why.title`, `.intro`, `.reason1`–`.reason3`, `exclusion.current.*`, `group.show.manageExclusions`, `flash.error.user_not_in_group`, `user.dashboard.tips.setExclusions`, `home.features.exclusionRules`.

All new and reworded keys exist in en and de.

## Acceptance criteria

Fixtures: users Alice, Bob, Carol, Dave, Eve. **G** = group with admin Alice and members Alice, Bob, Carol, Dave, not drawn. **G3** = group with members Alice (admin), Bob, Carol. **G2** = group with members Alice (admin), Bob. **G8** = group with 8 members including Bob. Eve is in none of these groups. "Rule X→Y" means the row `(group, user_id = X, excluded_user_id = Y)`.

**Viewing**
- AC1: Given Bob has no rules in G, when he opens `/groups/{G}/exclusions`, then he sees `exclusion.current.empty`, the privacy line, `exclusion.addNew.limit` with count 0 and max 5, and a select with exactly Alice, Carol and Dave (not himself), ordered by name.
- AC2: Given G is not drawn, the group page shows the "Manage exclusions" link to `/groups/{G}/exclusions` for every member. Given G is drawn, the link is not shown.

**Add**
- AC3: Given Bob adds Carol in G, then the rule Bob→Carol exists, `exclusion_added` is shown, Carol appears in his Current list and is no longer in the select. `get_exclusions` returns `my_rules = [Carol]`.
- AC4: Given the rule Bob→Carol, when Carol opens her exclusions page, then her list is empty and Bob is in her select (rules are one-way and not shown to the excluded person).
- AC5: Given the rules Bob→Carol and Carol→Dave, when Alice (admin) calls `get_exclusions(G)` or opens the page, then she sees only her own rules. No server fn of any slice returns Bob's or Carol's rules to Alice, Dave or each other (`get_group` included).
- AC6: Given Bob calls `add_exclusion(G, Bob)`, then `CannotExcludeSelf` and no row is created.
- AC7: Given Bob calls `add_exclusion(G, Eve)`, `add_exclusion(G, 0)`, `add_exclusion(G, -1)` or `add_exclusion(G, 999999)`, then `TargetNotMember` each time and no row is created.
- AC8: Given the rule Bob→Carol exists, when Bob calls `add_exclusion(G, Carol)` again, then `Ok`, exactly one such row exists, and the call does not count towards the cap.
- AC9: Given Bob has 5 rules in G8 and the group is feasible, when he adds a 6th member, then `TooManyExclusions { max: 5 }`, no row is created, and the page shows `exclusion.addNew.limitReached` instead of the form. After he removes one rule, he can add one again.

**Feasibility on add**
- AC10: Given the rules Alice→Dave and Bob→Dave in G, when Carol adds Dave, then `WouldMakeDrawImpossible` and no row is created (Dave would have no possible giver). Carol adding Bob instead succeeds.
- AC11: Given the rule Bob→Alice in G3, when Bob adds Carol, then `WouldMakeDrawImpossible` (Bob would have nobody to draw).
- AC12: Given G2 with no rules, `get_exclusions` shows no notice (a 2-member group is feasible because reciprocal pairs are allowed). When Alice adds Bob, then `WouldMakeDrawImpossible`.

**Remove**
- AC13: Given the rule Bob→Carol, when Bob confirms Remove for Carol, then the row is gone, `exclusion_removed` is shown and Carol is back in the select.
- AC14: Given only the rule Alice→Carol exists, when Bob calls `remove_exclusion(G, Carol)`, then `Ok` and Alice's rule still exists (the giver is always the caller). `remove_exclusion(G, Eve)` by Bob is also `Ok` with no change.

**Feasibility notice**
- AC15: Given the rules Bob→Alice and Bob→Carol in G (Bob can only draw Dave), when Dave leaves G (groups D5), then on Bob's page `notice = DrawImpossible { no_receiver_for_me: true }`, and he sees both `exclusion.warning.drawImpossible` and `exclusion.warning.noReceiverLeft`. Alice's and Carol's pages show only `drawImpossible`. No page names Bob or any rule. After Bob removes Alice, no page shows a notice.
- AC16: Given G is drawn, `get_exclusions` returns `notice = None` and `candidates = []`.

**After the draw**
- AC17: Given G is drawn and the rule Bob→Carol exists, when Bob opens the exclusions page, then he sees `exclusion.locked` and Carol in a read-only list, with no Add form and no Remove buttons. `add_exclusion(G, Dave)` and `remove_exclusion(G, Carol)` return `AlreadyDrawn`, and the rules are unchanged.
- AC18: Given G is drawn with the rule Bob→Carol, when `is_drawn` is set back to false (as the draw slice's reset does), then the rule Bob→Carol still exists and Bob can remove it.

**Authorization and transport**
- AC19: Given Eve opens `/groups/{G}/exclusions`, or anyone opens `/groups/999999/exclusions` or `/groups/abc/exclusions`, then the same 404 page is shown. `get_exclusions`, `add_exclusion` and `remove_exclusion` by Eve on G, and by anyone on group 999999, return `NotFound`, and nothing changes.
- AC20: Given a logged-out visitor opens `/groups/{G}/exclusions`, then they are redirected to login and, after logging in as Bob, land on that page. All three fns called without a session return `Unauthorized`.
- AC21: Given a plain GET to the old `/exclusions/{G}/remove/{Carol}` URL, or a GET or cross-origin POST to the `remove_exclusion`/`add_exclusion` endpoints, then no rule is created or deleted.

**Data integrity**
- AC22: Given the rules Bob→Carol and Carol→Bob in G, when Carol's `group_members` row is deleted directly in SQL (without the D5 helper), then both rules are gone (composite FK cascade). An `INSERT` of a rule whose `excluded_user_id` is not a member of the group fails with an FK error.

**Concurrency**
- AC23: Given the rules Alice→Dave in G, and Bob and Carol each call `add_exclusion(G, Dave)` at the same time, then exactly one succeeds and the other returns `WouldMakeDrawImpossible`. (Integration test: hold the group-row `FOR UPDATE` lock in one transaction and assert that the second call waits and then sees the first rule.)
- AC24: Given a transaction holds the group-row lock and sets `is_drawn = 1`, then a concurrent `add_exclusion` waits and then returns `AlreadyDrawn`.

**Feasibility function (unit tests)**
- AC25: `check_feasibility(&[], &[])` and `check_feasibility(&[1], &[])` → `Infeasible(TooFewMembers)`. `check_feasibility(&[1, 1], &[])` → `TooFewMembers` (de-duplicated). `check_feasibility(&[1, 2], &[])` → `Feasible`.
- AC26: `[1, 2]` with `[(1, 2)]` → `NoReceiverFor { giver: 1 }`. `[1, 2, 3]` with `[(1, 3), (2, 3)]` → `NoGiverFor { receiver: 3 }`. `[1, 2, 3]` with `[(1, 2), (1, 3), (2, 3), (1, 2)]`: both `NoReceiverFor { giver: 1 }` and `NoGiverFor { receiver: 3 }` apply, and the result is `NoReceiverFor { giver: 1 }` (priority).
- AC27: `[1, 2, 3, 4, 5]` with `[(1, 2), (1, 3), (2, 1), (2, 3), (3, 1), (3, 2)]` → `Blocked { givers: [1, 2, 3], receivers: [4, 5] }`.
- AC28: Self-rules, rules with ids not in `members` and duplicate rules are ignored: `[1, 2]` with `[(1, 1), (1, 9), (9, 2)]` → `Feasible`. Permuting `members` or `exclusions` never changes the result.
- AC29: Property test: for every member count 0..=6 and random rule sets (at least 1000 cases), `Feasible` holds exactly when a brute-force search finds a fixed-point-free permutation that respects all rules. Every `Blocked` result satisfies `receivers.len() < givers.len()`, and every allowed receiver of each listed giver is in `receivers`.
- AC30: `load_rules(tx, G)` returns the `(user_id, excluded_user_id)` pairs of G only, ordered by `user_id, excluded_user_id`.

**i18n**
- AC31: Given the locale is de, the exclusions page shows only German strings (no "Note:" prefix, no English fallbacks). Every key listed as new or reworded exists in both `en.json` and `de.json`.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **Admin manages rules for others?** No. Self-service only: each member manages only their own rules, and there is no admin management or view of other members' rules (D2).
2. **Mutual ("we are a couple") option?** No. One-way only, no "couple" checkbox; each partner adds their own rule (D3).
3. **Infeasible rule: reject or warn?** Reject: `add_exclusion` returns `WouldMakeDrawImpossible` for a rule that would make the draw impossible (D7).
4. **Predictability guard.** (a): nothing in the exclusions slice (D7). The draw slice may show a generic admin warning (no names) when only one draw is possible; see the draw spec.
5. **Cap value.** 5 rules per member per group (`MAX_EXCLUSIONS_PER_MEMBER`, D6).
- Edit 2026-10-31 (cross-spec review, fresh start): the migration no longer deletes pre-existing invalid rows (there are none), so the former AC23 about that cleanup was removed and AC24–AC32 became AC23–AC31.
