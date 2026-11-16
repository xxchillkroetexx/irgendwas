---
feature: copy-wishes
status: approved
---
# Copy wishes from my other wishlists (#76)

Issue #76 ("UX für 'Aus eigenen Wunschlisten übernehmen' ist schlecht"): rethink the flow
for copying one's own wishes from another group's list. PR #88 already stopped the source
choice from reloading the page; the flow as a whole is still open.

**Amends `wishlists.md`:**
- the section "Issue #57: copying own wishes": the source `<select>` + "Show wishes" step
  is replaced by a collapsible section that shows all other lists at once; the server rules
  of #57 (authorization, locking, atomicity, 50-item cap, source unchanged) stay, except
  for the changes listed under "Server functions" below;
- "UI notes" for `/groups/:id/wishlist` (edit): where the copy section sits and how it
  behaves; the edit page gets the query parameter `?copy=1`;
- "Flash / validation keys": the copy keys listed under "UI notes" below.

No new route. Everything else in `wishlists.md` (W1–W13) applies unchanged.

Revised 2026-11-15: the user answered the first draft's open questions (see "Decisions"):
a section on the edit page instead of a separate page; wishes already on the list are
greyed out; "already on the list" compares titles only.

## Decisions (user, 2026-11-15)
1. A collapsible section on the edit page, directly below the add form, is enough; no
   separate page.
2. Nothing is ticked at the start.
3. Wishes already on the target list are greyed out (disabled, with a hint) and cannot be
   selected.
4. "Already on this list" means the same title (trimmed, case-insensitive); the link does
   not matter.
5. After a successful copy: back on the edit page with a flash that gives the number.
6. Wishes that fail today's rules (migrated data) are greyed out, with a link to fix them in
   their list.
7. A "Start from last year's list" entry on the group page comes later (see "Later
   options").

## Current behaviour

### Where and how
- `/groups/:id/wishlist` (`app/src/pages/wishlists/edit.rs`, `WishlistEditPage`) renders,
  in this order: title, group link, description, "Add a shared link", "View your wishlist",
  the add form, the item list, the "Remove all items" card (only with items), and **last**
  the copy card `CopySection` (heading `wishlist.copy.title`).
- `CopySection` is shown only when `get_wishlist_sources(group_id)` returns something:
  the caller's own **non-empty** lists in other groups they are still a member of, ordered
  by group name (`wishlists_flow::get_sources`). Drawn groups are included.
- Step 1, `CopySourceForm`: a GET form with a `<select name="source">` (label
  `wishlist.copy.source`, first option `wishlist.copy.choose`, then the group names) and the
  button `wishlist.copy.load` ("Show wishes"). Without JS it loads
  `/groups/:id/wishlist?source=<group id>`; with JS the submit handler navigates there
  client-side with `scroll: false` (#81/#88), so the page stays mounted.
- Step 2: `?source=` makes the page call `get_my_wishlist(source)` and render an
  `ActionForm` for `copy_wishlist_items`: hidden `group_id` and `source_group_id`, the text
  `wishlist.copy.instructions`, one checkbox per source item (`name="item_ids[<index>]"`,
  label = **title only**), and the button `wishlist.copy.submit`, disabled while pending.
- `?source=` equal to the current group renders nothing. A group the caller is not a member
  of (or a missing one) gets `NotFound`, which renders the `<NotFound/>` page component
  inside the copy card; `NotFound` calls `set_status(404)`, so the whole edit page answers
  404. A non-numeric `source` is ignored.

### Server fn `copy_wishlist_items(group_id, source_group_id, item_ids: Option<Vec<i32>>)`
(`app/src/wishlists_api.rs` → `wishlists_flow::copy_items`)
- Requires a session (`Unauthorized`), then `require_member` for **both** groups
  (`NotFound`).
- `Validation[(Items, SelectionRequired)]` when the two groups are the same, the
  (deduplicated) selection is empty, longer than 50, or has a non-positive id.
- In one READ COMMITTED transaction, behind the per-user write queue, with deadlock
  retries: lock both group rows (and member rows) in ascending group id order, create the
  target `wishlists` row if missing, lock both lists, read the source items `FOR UPDATE` in
  `position, id` order. If not every selected id is in the source list: `NotFound`
  (foreign, missing and other-list ids look the same). If target count + selection > 50:
  `Validation[(Items, TooManyItems)]`. Each item is re-validated with W8/W9 (`validate`);
  a failure aborts the whole copy with that field's error (`Title`/`Description`/`Link`).
  Items are appended in source order with new ids; the target's `updated_at` is bumped;
  the source and its timestamps are untouched.
- `outcome(Some(group_id), …, "flash.success.wishlist_copied")`: success stores the flash
  "The selected wishes were copied." (no count). A plain form post gets 302 to
  `/groups/:id/wishlist` on success; on failure 302 to `/groups/:id/wishlist` with the
  error flash, **to `/groups` for `NotFound`**, and to login for `Unauthorized`.
- With JS (`edit.rs`, effect on `copy.value()`): success refetches the session (flash) and
  the list; an error is pushed as a toast. The source stays loaded and **the checkboxes
  stay ticked**.

### Data
`wishlists`, `wishlist_items` (with `position`, migration 0006). No copy-specific columns.
Production holds the lists migrated from PHP (52 wishes in the 2025 data, 3 of 4 groups
drawn); the migration trimmed text and mapped empty values to `NULL` but did not re-apply
W8/W9.

### Tests that pin the current UI
- `end2end/tests/wishlists.spec.ts` "#57 copies selected own wishes with and without
  JavaScript" (select + "Show wishes" + checkboxes; it ends with the duplicates
  `Copy A, Copy B, Copy A, Copy B` in the target list).
- `end2end/tests/navigation_query.spec.ts` "a query-only change after the session ended
  goes to the login page (#81)": its first half uses the `?source=` chooser.
- `server/tests/it/wishlist_copy.rs`: `sources_are_own_nonempty_current_lists_…` (calls
  `get_wishlist_sources`, checks the `?source=` page for `copy-source`) and
  `plain_form_posts_copy_sparse_checkbox_selection_and_redirect_with_flash` (expects the
  failure redirect to `/groups/:id/wishlist` and the old `selection_required` text).

## Problems
1. **Hard to find.** The copy card is the last thing on the edit page, below the danger
   zone "Remove all items". On a phone with a filled list it is several screens down; with
   an empty list (the main use case: a new group) it still comes after the add form.
2. **Two steps before seeing a single wish**: choose a list, press "Show wishes", even when
   there is only one other list (the common case: last year's group).
3. **Only titles.** Two wishes with the same title (e.g. "Book" with different links) cannot
   be told apart; description and link host are not shown.
4. **Duplicates.** Nothing shows which wishes are already on the target list. After a
   successful copy with JS the boxes stay ticked and the copied items appear far above,
   out of view, so a second click copies everything again (the e2e test itself ends with
   duplicates).
5. **Weak feedback.** The flash says "The selected wishes were copied." without a number.
6. **No select-all.** Copying a 20-item list takes 20 taps.
7. **Full target list.** The card is offered at 50 items too; the user learns only after
   submitting.
8. **Odd `?source=` states.** A source list emptied meanwhile renders the instructions and a
   copy button without checkboxes (submitting gives `selection_required`). `?source=` equal
   to the current group renders nothing. A foreign group id renders the full "Page not
   found" block inside the card, below a working editor, and the page answers HTTP 404.
9. **No-JS dead ends.** A failed plain post (empty selection, too many items) returns to
   the edit page without `?source=`: the user has to pick the list again. `NotFound` (a
   source item deleted in another tab, or the user left the source group meanwhile) sends
   them to `/groups` with "The page you are looking for does not exist."
10. **Migrated wishes may not copy.** A migrated item that fails today's W8/W9 rules (a
    description over 2000 characters, a link PHP's `FILTER_VALIDATE_URL` accepted but W9
    rejects, e.g. `ftp://…` or with user info) aborts the whole copy with a field message
    about input the user did not type ("Enter a web address starting with http:// or
    https://") and no hint which wish is meant. How many rows are affected is unknown.
11. **Order by name.** Sources are sorted alphabetically, so the list one usually wants
    (the most recently used one) is not necessarily first; two groups with the same name
    (e.g. "Family" of two years) are indistinguishable in the select.
12. **Edit page complexity.** The `?source=` navigation is part of the edit page's
    focus/scroll handling (#81/#88 had to special-case query-only navigation for it). The
    section below needs no navigation to show a list, which removes that case; the copy
    action still counts as a page submission for `settled` (#69).
13. Wording (not fixed here, i18n-polish): the editor says "items"/"Artikel", the copy
    texts say "wishes"/"Wünsche".

## Design

A native `<details id="copy">` section on the edit page, directly below the add form. It
shows all of the caller's other non-empty lists at once, each as a nested `<details>` (the
most recently changed one open), with its wishes as checkboxes (title, description, link
host). Wishes already on the list and wishes that can't be copied are greyed out. Each
list has a JS "Select all" and its own submit button. A successful copy closes the section
and shows a flash with the number. Everything works without JS.

### Pages / routes
- No new route. `/groups/:id/wishlist` (edit page) gains the query parameter `copy`:
  `?copy=1` renders the copy section open. Any other value of `copy` is treated like `1`.
  The parameter combines with `?edit=` and changes nothing else.
- New path helper `paths::wishlist_copy_open_path(group_id)` →
  `/groups/{id}/wishlist?copy=1`.
- `?source=` no longer means anything: the parameter is ignored and the edit page renders
  normally (old bookmarks keep working).
- No-JS redirects after a failed copy land on `?copy=1` **without** a `#copy` fragment:
  without JS the flash is not sticky (it is `position: sticky` only once hydrated, #81),
  so a fragment would scroll the error message out of view. The page opens at the top
  with the flash, and the open section follows directly below the add form.

### Server functions

**New: `get_copy_sources(group_id: i32) -> Result<CopySources, WishlistError>`**
(`POST /api/get_copy_sources`, body in `wishlists_flow`). Replaces `get_wishlist_sources`,
which is removed (its only caller was the edit page).
- `require_user` (`Unauthorized`), then `authz::require_member(group_id)` (`NotFound` for
  a missing group and a non-member alike).
- Read-only; creates no row (W5).
- Types (in `wishlists_api.rs`):
  - `CopySources { free: u32, sources: Vec<CopySource> }`; `free` = 50 − the number of
    items currently on the caller's list in `group_id` (50 without a row).
  - `CopySource { group: NamedRef, updated_at: Option<DateTime<Utc>>, items: Vec<CopyItem> }`
  - `CopyItem { id: i32, title: String, description: Option<String>, link_host: Option<String>, on_target: bool, copyable: bool }`
- `sources`: the caller's own lists (`wishlists.user_id` = caller) in groups the caller is
  a current member of, other than `group_id`, with at least one item (the same set as
  `get_sources` today). Ordered by `wishlists.updated_at DESC, group id DESC`. Items in
  `position, id` order.
- `on_target`: the caller's list in `group_id` has an item with the same **title key**.
  Title key = the title trimmed (Unicode whitespace) and lowercased with Rust's
  `str::to_lowercase`; no other folding (accents, inner whitespace and punctuation
  count). Description and link are ignored. Computed in Rust.
- `copyable`: the item passes the same W8/W9 validation the copy runs (`validate(title,
  description, link)` is `Ok`).
- Never contains emails, other users' items or assignment data; reads no
  `gift_assignments`.

**Changed: `copy_wishlist_items(group_id, source_group_id, item_ids) -> Result<u32, WishlistError>`**
- Same arguments, authorization, locking, atomicity and checks as today (#57), plus:
  1. Returns the number of items copied.
  2. **Greyed-out ids are rejected, not skipped.** Under the locks, after the `NotFound`
     check: if a selected item's title key equals the title key of an item already on the
     target list → `Validation[(Items, AlreadyOnList)]` (new `WishlistFieldError`, key
     `validation.wishlist.already_on_list`). Then, if a selected item fails W8/W9 →
     `Validation[(Items, ItemNotCopyable)]` (new, key
     `validation.wishlist.item_not_copyable`) instead of the item's field error. Then the
     50-item cap as today. The first failing check is returned; nothing is written.
     Rejecting keeps the count in the flash equal to the selection and needs no "0 copied"
     case. The page only sends such ids when it is stale (another tab added a wish with
     that title) or the request was crafted.
  3. The check compares against the items on the target list **before** the copy: two
     selected wishes with the same title key, neither on the target list, are both copied.
  4. Success flash: `flash.success.wishes_copied` with plural `.one` / `.other` and
     `{count}` (`FlashMessage::success(…).with_plural(n).with_number("count", n)`),
     replacing `flash.success.wishlist_copied`.
  5. Plain form posts (no JS): success → 302 `/groups/:id/wishlist` (unchanged, section
     closed). Failure → 302 `/groups/:id/wishlist?copy=1` with the error flash, also for
     `NotFound`, whose flash is `wishlist.copy.changed` instead of `error.notFound.message`
     (if the caller is not a member of `:id`, that URL shows the 404 page; nothing is
     revealed that a GET would not show). `Unauthorized` → login with
     `next=/groups/:id/wishlist?copy=1`.
- Errors: `Unauthorized`, `NotFound`, `Validation{Items→selection_required |
  already_on_list | item_not_copyable | too_many_items}`, `Internal`.

### Data & migrations

No schema change, no migration. Tables read: `groups`, `group_members`, `wishlists`,
`wishlist_items`. Tables written (unchanged from #57): `wishlists` (target row created if
missing, `updated_at`), `wishlist_items` (insert).

### UI notes

**Removed from the edit page:** `CopySection` at the bottom, `CopySourceForm`, the
`source_group` memo, the `source_list` resource and the `get_wishlist_sources` resource.

**The copy section** (only when `get_copy_sources` returns at least one source):
- Position: directly below the add form card, above the `wishlist.edit.itemsHeading` list.
- Outer element `<details id="copy">` with `<summary>` `wishlist.copy.title` (styled as a
  heading-sized summary). It is **open** when the URL has `copy` or the caller's list in
  this group was empty when the page was rendered; otherwise closed. The `open` attribute
  is decided once when the page renders; later query changes (Edit/Cancel, `?edit=`) do
  not open or close it, and neither do re-reads of the item list (see stability below).
- When `free == 0`: inside the section only the text `validation.wishlist.too_many_items`;
  no source lists, no forms.
- Otherwise: the text `wishlist.copy.instructions`, then **one nested `<details>` per
  source**, the first one `open`, the others closed. Its `<summary>`: the group name, its
  item count (`wishlist.panel.itemCount`, plural) and `wishlist.view.lastUpdated` with
  `{date}` = `LocalDate(updated_at)` (omitted when `updated_at` is `None`).
- Inside each nested `<details>` one `ActionForm` for `copy_wishlist_items` with hidden
  `group_id` and `source_group_id`, and:
  - "Select all" (`wishlist.copy.selectAll`), kept although greyed-out items exist: the
    main use case is copying most of last year's list into an empty one, and this turns
    10–20 taps into one. It is a checkbox without `name` (never submitted), rendered with
    the `hidden` attribute and revealed by an effect after hydration (no-JS users never
    see a control that does nothing). Ticking it ticks every **enabled** item checkbox of
    this source; unticking it unticks them. It never touches disabled checkboxes or other
    sources.
  - One row per item: a checkbox `name="item_ids[<index>]" value=<id>` whose label is the
    title (`fw-semibold`, `text-break`); below it the description (small, muted,
    `white-space: pre-line`, clamped to 2 lines with CSS) and the link host as plain text
    (no link: a tap on the row must not leave the page). No checkbox is ticked initially.
  - `on_target` (greyed out): the checkbox is `disabled`, the row is muted, and the hint
    `wishlist.copy.alreadyOnList` follows the title.
  - `!copyable` (greyed out): the checkbox is `disabled`, the row is muted, with the text
    `wishlist.copy.notCopyable` and a link `wishlist.copy.fixInSource` →
    `wishlist_item_edit_path(source_group, item_id)` (`?edit=ID#item-ID`). An item that is
    both shows only `alreadyOnList`.
  - Submit button `wishlist.copy.submit`, `aria-label` = `row_label(i18n,
    "wishlist.copy.submit", group name)` (several such buttons on the page); disabled
    while its action is pending; full width below the `sm` breakpoint. A source whose
    items are all greyed out still shows its rows, but no "Select all" and no submit
    button.
- **Errors, with JS:** shown as an alert inside the failing source's form, above its submit
  button (`WishlistError::text`; `NotFound` as `wishlist.copy.changed`). The section and the
  source stay open. For `selection_required` and `too_many_items` the ticks stay. For
  `NotFound`, `already_on_list` and `item_not_copyable` (the shown data is stale) the
  section's data is read again: a vanished item disappears, a new duplicate is greyed out,
  ticks may reset, the alert stays.
- **Errors, without JS:** the 302 to `/groups/:id/wishlist?copy=1` (see routes): flash at
  the top, section open; ticks are lost (accepted degradation).
- **Success, with JS:** the list and the section's data are read again, the section is
  closed, the flash is shown (session re-read), and the scroll position stays (the item
  list with the appended wishes follows directly below). If the URL has `copy`, it is
  replaced by `/groups/:id/wishlist` without scrolling, the same end state as the no-JS
  redirect.
- **Stability:** the section's data is read with the page and again only after a copy (as
  above). Add, edit, move, remove and clear on the page do not re-read it and must not
  close the section or clear its ticks. ("Already on this list" can be stale meanwhile;
  the server check in item 2 keeps that harmless.) Implementation hint: render it from its
  own resource outside the list's `Transition`, e.g. by splitting `EditView` into the part
  up to the add form and the item list.
- The copy action stays in the page's `busy` list for `settled` (#69).
- All user text (group names, titles, descriptions) as text nodes. The edit page has no
  horizontal scrolling at 390 px width with the section open.

**i18n (en / de).**
- New:
  - `wishlist.copy.selectAll`: "Select all" / "Alle auswählen"
  - `wishlist.copy.alreadyOnList`: "Already on this list" / "Schon auf dieser Liste"
  - `wishlist.copy.notCopyable`: "Can't be copied as it is." / "Kann so nicht übernommen
    werden."
  - `wishlist.copy.fixInSource`: "Edit it in that list" / "In dieser Liste bearbeiten"
  - `wishlist.copy.changed`: "Some of the selected wishes no longer exist. Please check
    your selection." / "Einige der ausgewählten Wünsche gibt es nicht mehr. Bitte prüfen
    Sie Ihre Auswahl."
  - `flash.success.wishes_copied.one`: "{count} wish was copied." / "{count} Wunsch wurde
    übernommen."
  - `flash.success.wishes_copied.other`: "{count} wishes were copied." / "{count} Wünsche
    wurden übernommen."
  - `validation.wishlist.already_on_list`: "One of the selected wishes is already on this
    list." / "Einer der ausgewählten Wünsche steht schon auf dieser Liste."
  - `validation.wishlist.item_not_copyable`: "One of the selected wishes can't be copied
    as it is. Edit it in its list first." / "Einer der ausgewählten Wünsche kann so nicht
    übernommen werden. Bearbeiten Sie ihn zuerst in seiner Liste."
- Reworded:
  - `wishlist.copy.instructions`: "Choose wishes from your other wishlists. They are added
    to the end of this list; the other lists stay unchanged." / "Wählen Sie Wünsche aus
    Ihren anderen Wunschlisten. Sie werden am Ende dieser Liste angefügt; die anderen
    Listen bleiben unverändert."
  - `validation.wishlist.selection_required`: "Select at least one wish to copy." /
    "Wählen Sie mindestens einen Wunsch aus."
- Reused unchanged: `wishlist.copy.title`, `wishlist.copy.submit`,
  `wishlist.edit.itemAction`, `wishlist.panel.itemCount`, `wishlist.view.lastUpdated`,
  `validation.wishlist.too_many_items`.
- Removed (unused afterwards; the i18n coverage test C7 rejects unused keys):
  `wishlist.copy.source`, `wishlist.copy.choose`, `wishlist.copy.load`,
  `flash.success.wishlist_copied`.

### Tests to replace
The test writer replaces or adapts the tests listed under "Current behaviour":
- the e2e "#57 copies…" test (by AC9/AC17 and the others below);
- the copy half of the #81 navigation test: choosing a list no longer navigates, so its
  "query-only change after the session ended" case for copying goes away; AC18 covers an
  ended session on submit. The shared-wish half stays;
- `sources_are_own_nonempty_current_lists_…` (now `get_copy_sources`, no `?source=` page);
- `plain_form_posts_copy_sparse_checkbox_selection_and_redirect_with_flash` (failure
  redirect `?copy=1`, reworded text).

### Later options (not in this spec)
- "Start from last year's list" on the group page's wishlist panel when my list there is
  empty and I have other lists.
- Copying from several source lists in one submit (needs a server fn change and a lock on
  every involved group).
- A no-JS "Copy all" (e.g. a second submit button carrying all eligible ids).
- Undo of a copy.

## Acceptance criteria

Fixtures: users Alice, Bob, Carol, Dave. Group **T** ("Family 2026", not drawn) with
members Alice, Bob, Carol; Bob's list in T has one item "Socks" (no link). Group **S1**
("Family 2025", drawn) with members Alice, Bob; Bob's list in S1 has, in this order,
"Book" (description "Paperback", link `https://shop.example/book`), "socks" (link
`https://shop.example/socks`), "Tea"; its `updated_at` is 2025-12-01. Alice's
list in S1 has "Scarf". Group **S2** ("Office") with members Bob, Carol; Bob's list in S2
has "Mug"; its `updated_at` is 2025-06-01. Group **E** with member Bob and no list. Carol
has no list anywhere. Dave is in none of these groups. These hold unless an AC says
otherwise.

**Section: presence and placement**
- AC1: Given Bob opens `/groups/{T}/wishlist`, then a `<details id="copy">` with the summary
  `wishlist.copy.title` comes after the add form and before the `itemsHeading` list, and it
  is closed (his T list has an item). There is no source `<select>`, no "Show wishes"
  button and no copy card after "Remove all items".
- AC2: Given Carol opens `/groups/{T}/wishlist` (no non-empty list in another group), then
  there is no copy section.
- AC3: Given Bob opens `/groups/{E}/wishlist` (his list there is empty), then the copy
  section is open on load. Given he opens `/groups/{T}/wishlist?copy=1`, it is open too.
  Given `/groups/{T}/wishlist?source={S1}` (with and without JS), the response is 200 and
  the page renders like `/groups/{T}/wishlist`.
- AC4: `get_copy_sources(T)` for Dave and `get_copy_sources(999999)` for Bob return
  `NotFound`; without a session it returns `Unauthorized`. Bob's call never creates a
  `wishlists` row (also not for E), and nothing in the database changes.

**Section: content**
- AC5: Given Bob opens the section on `/groups/{T}/wishlist`, then he sees the instructions
  and two sources in the order S1, S2. S1's summary shows its name, "3 items" and its
  last-updated date; S1 is open (Book, socks, Tea visible, in that order, with "Paperback"
  and `shop.example` under Book); S2 is closed (its "Mug" checkbox is not visible until
  its summary is clicked). No checkbox is ticked. T, E, Alice's "Scarf" and Carol's data
  do not appear, and the response contains no email address.
- AC6: Given AC5, then "socks" is greyed out: its checkbox is disabled and it shows
  `wishlist.copy.alreadyOnList` (title key "socks" equals "Socks"; the different link does
  not matter). "Book" and "Tea" are enabled. Given Bob's T list also had an item "BOOK", then
  "Book" is greyed out too; given it had "Books" instead, "Book" is
  enabled.
- AC7: Given S1 additionally holds an item "Old" with link `ftp://example.com/x` (written
  directly in SQL, as migrated data could be), then its checkbox is disabled, it shows
  `wishlist.copy.notCopyable` and a link to `/groups/{S1}/wishlist?edit={id}#item-{id}`,
  and `get_copy_sources(T)` returns it with `copyable: false`.
- AC8: Given Bob's list in T has 50 items, then the open section shows
  `validation.wishlist.too_many_items` and no source lists or submit buttons;
  `get_copy_sources(T)` returns `free: 0`.

**Copying**
- AC9: Given Bob ticks "Book" and "Tea" in S1 and presses that source's
  `wishlist.copy.submit`, then (with JS and without JS) he is on `/groups/{T}/wishlist`
  without `copy` in the URL, the copy section is closed, he sees "2 wishes were copied."
  once, and his T list is Socks, Book, Tea, where Book keeps "Paperback" and
  `https://shop.example/book`; T's `updated_at` is bumped; S1's items, positions and
  `updated_at` are unchanged.
- AC10: Given Bob copies only "Mug" from S2, then the flash reads "1 wish was copied."
  (en) / "1 Wunsch wurde übernommen." (de), and `copy_wishlist_items` returns 1.
- AC11: Given Bob presses S1's submit with nothing ticked, then nothing is written. With JS
  the page does not navigate, the section and S1 stay open, and S1's form shows "Select at
  least one wish to copy."; without JS he lands on `/groups/{T}/wishlist?copy=1` with that
  text as a flash and the section open.
- AC12: Given Bob's T list has 49 items and he submits "Book" and "Tea", then nothing is
  written and `validation.wishlist.too_many_items` shows (JS: in S1's form, both still
  ticked; no JS: flash on `?copy=1`).
- AC13: Given Bob has the section open and "Book" is deleted from S1 in another tab, when he
  submits "Book" and "Tea", then nothing is copied and he sees `wishlist.copy.changed`
  (JS: in S1's form, and the re-read S1 no longer lists "Book"; no JS: as a flash on
  `?copy=1`).
- AC14: Given Bob has the section open and adds "tea" to his T list in another tab, when he
  submits "Book" and "Tea", then nothing is copied, the result is
  `Validation[(Items, AlreadyOnList)]` with the text `validation.wishlist.already_on_list`
  (JS: in S1's form, and after the re-read "Tea" is greyed out), and a direct call of
  `copy_wishlist_items(T, S1, [id of "socks"])` returns the same error.
- AC15: Given a direct call of `copy_wishlist_items(T, S1, [id of "Old"])` (AC7's item),
  then it returns `Validation[(Items, ItemNotCopyable)]` and nothing is written; a plain
  form post of it lands on `/groups/{T}/wishlist?copy=1` with
  `validation.wishlist.item_not_copyable`.
- AC16: Given S2 holds "Lamp" and "lamp" and neither is on Bob's T list, when he copies
  both, then both are added and the flash counts 2.
- AC17: The #57 rules still hold for `copy_wishlist_items`, with nothing written in each
  case: Alice's "Scarf" in the selection, or an S2 item id with `source_group_id = S1`,
  gives `NotFound`; `group_id = source_group_id` or an empty selection gives
  `Validation[(Items, SelectionRequired)]`; Dave as caller, or Bob once his S1 membership
  row is gone (deleted in SQL: S1 is drawn, so he cannot leave), gives `NotFound`; a GET
  or a cross-origin POST changes nothing.
- AC18: Given Bob has the section open with JS and his session ends (logout in another
  tab), when he submits a selection, then he is sent to login with
  `next=/groups/{T}/wishlist?copy=1`; a plain post without a session gets 302 to that login
  URL. Nothing is written.

**Select all, repeated copies, stability**
- AC19: Given JS is enabled and Bob ticks `wishlist.copy.selectAll` in S1, then "Book" and
  "Tea" are ticked, the disabled "socks" (and AC7's "Old") stay unticked, and S2's
  checkboxes are unchanged; unticking it unticks "Book" and "Tea". Given JS is disabled,
  the "Select all" control is not visible, and ticking items by hand and submitting works
  (AC9). Given every item of a source is greyed out, that source shows neither "Select
  all" nor a submit button.
- AC20: Given JS is enabled and Bob submits "Book" in S1, then the submit button is
  disabled until the response arrives, "Book" is added exactly once, and when he opens the
  section again, S1's "Book" is greyed out with `wishlist.copy.alreadyOnList`.
- AC21: Given JS is enabled and Bob has the section open with "Book" ticked in S1, when he
  adds an item through the add form, moves an item, or opens and cancels an inline edit,
  then the section and S1 stay open and "Book" stays ticked.

**Layout, safety, i18n**
- AC22: Given a 390 × 844 viewport, the section open, and an S1 item whose title is 255
  characters without spaces, then the edit page has no horizontal scrolling
  (`document.documentElement.scrollWidth <= window.innerWidth`), the title wraps, and
  tapping the title text of an enabled item toggles its checkbox.
- AC23: Given an S1 item titled `<script>alert(1)</script>` with description
  `<img src=x onerror=alert(1)>`, then the section shows both as literal text and no
  script runs.
- AC24: Every key this spec adds, rewords or reuses exists in `locales/en.json` and
  `locales/de.json`; the removed keys exist in neither and nothing references them; the
  edit page with the section open renders without raw keys in both locales; e2e runs with
  JS report no page errors (`collectPageErrors`, no hydration panic).

## Open questions

None. The first draft's questions were answered on 2026-11-15 (see "Decisions").
