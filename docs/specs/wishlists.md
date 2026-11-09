---
slice: wishlists
status: approved
---
# Wishlists

Scope: one wishlist per member and group. The owner adds, edits, removes and reorders items and can clear the whole list. Members view the lists. This slice fills the groups slice's `<WishlistPanel group_id>` hook and provides the wishlist URL that the draw slice links to from its assignment card and from the dashboard assignments card (account D10 gives that card to the draw slice).

Not in scope: assignments and the draw (draw slice), exclusions, a global "one list for all my groups", reserving or marking items as bought.

Fresh start: no production data is migrated. The Rust app owns its schema, and `0001_baseline.sql` (the PHP schema) is the starting point.

Revised 2026-10-31: aligned with i18n-foundation (built before auth). `wishlist.view.lastUpdated` uses `LocalDate` (`app::format`); the panel item counts are plural keys. AC33 was added.

## Current PHP behaviour

### Routes (`src/Core/Application.php`, `WishlistController`)

Every existing handler calls `requireAuth()`: anonymous users are sent to `/auth/login` with `intended_url`.

| Method | Path | Handler | Authorization | What it does |
|---|---|---|---|---|
| GET | `/wishlist/view/:userId/:groupId` | `view` | caller is a member; target is a member; **after the draw**, a non-admin may only see their own list or their assigned receiver's | Read-only list. |
| GET | `/wishlist/edit/:groupId` | `edit` | member | Edit page. **Creates the `wishlists` row on GET** if missing. |
| POST | `/wishlist/:groupId/settings` | `updateSettings` | – | **Dead route.** The method was removed in commit `fb91bfa` ("Remove wishlist priority ordering feature"). The router throws, which gives a 500 page that prints `Method updateSettings not found in controller SecretSanta\Controllers\WishlistController`. It used to toggle `wishlists.is_priority_ordered`. |
| POST | `/wishlist/:groupId/item/add` | `addItem` | member | Adds an item. Creates the wishlist row if needed. |
| POST | `/wishlist/item/:itemId/update` | `updateItem` | owner of the item's wishlist (membership **not** checked) | Updates title, description and link. |
| GET | `/wishlist/item/:itemId/delete` | `deleteItem` | owner (membership not checked) | Deletes an item. |
| POST | `/wishlist/:groupId/priority` | `updatePriority` | – | **Dead route** (same 500 as `settings`). It used to set `position` for arbitrary item ids **without an ownership check**. |
| GET | `/wishlist/:groupId/delete` | `deleteWishlist` | member | Deletes the wishlist row (items cascade). |

### Viewing (`view`)
- Caller not a member: flash `flash.error.not_group_member`, redirect `/groups`. Target not a member: `flash.error.user_not_in_group`, redirect to the group.
- Group not drawn: **every member can view every member's list.** Group drawn: the admin can still view every list. Other members can view their own list and their receiver's only; anything else gives `flash.error.only_view_assigned_recipient`.
- A missing wishlist row renders as empty. The owner sees `wishlist.edit.addSome` and an "Add items now" button. Others see `wishlist.view.noPreference` and `wishlist.view.suggestGiftCard`.
- Items are rendered as cards: title, description (`nl2br(htmlspecialchars())`) and link (`<a href="…" target="_blank">`, escaped, no `rel`).

### Editing (`edit`, `addItem`, `updateItem`, `deleteItem`, `deleteWishlist`)
- Inputs: `title`, `description` and `link`, none of them trimmed.
- Validation: `empty($title)` gives the hard-coded English error "Item title is required". `!empty($link) && !filter_var($link, FILTER_VALIDATE_URL)` gives "Invalid URL format". There are no length limits and no item limit.
- On a validation error, `errors` and `old` are flashed and the user is redirected to the edit page. The `old` values pre-fill the **add** form, also when the error came from the edit modal.
- Editing happens in a Bootstrap modal that is filled by JS from `data-*` attributes. Without JS, items cannot be edited.
- Remove item and delete wishlist are GET links with a JS `confirm()`. The translated text is put into a single-quoted JS string without JS escaping.
- Editing is allowed before and after the draw.
- Flashes: `flash.success.item_added`, `item_updated`, `item_deleted`, `wishlist_deleted`, and `flash.error.item_not_found`, `no_permission_edit_item`, `no_permission_delete_item`, `wishlist_not_found`, `wishlist_delete_failed`.
- Hard-coded English: the validation messages, the table headers "Item" and "Actions", "Danger Zone", "This will permanently delete…", and the page titles "Edit Your Wishlist" and "…'s Wishlist". The edit page header link reads "Wishlist from: You".

### Ordering
- `schema.sql` has no ordering column. `fb91bfa` dropped `wishlist_items.position` and `wishlists.is_priority_ordered`, the controller methods and the drag-and-drop JS. It left behind the two routes, doc comments ("sorted by position", "next available position"), the de-only keys `wishlist.edit.priorityHelp`, `flash.error.invalid_item_positions`, `flash.success.priorities_updated` and `flash.error.priorities_update_failed`, the unused `flash.success.wishlist_settings_updated`, and `home.features.prioritizedWishlists`.
- `findByWishlistId` runs `SELECT * … WHERE wishlist_id = ?` with **no `ORDER BY`**. In practice InnoDB returns creation order, but nothing guarantees it. **Users cannot reorder anything today.**

### Elsewhere
- Group page (`group/show.php`): a "Your Wishlist" card (`group.show.yourWishlist`, `editWishlist`, `wishlistHelp`, `viewYourWishlist`, `viewYourWishlistHelp`) with links to edit and to view their own list. After the draw, the assignment card links to the receiver's list (`group.show.viewWishlist`, draw slice).
- Dashboard: each own assignment links to `/wishlist/view/{receiver}/{group}` (`user.dashboard.assignments.viewWishlist`), and a tip `user.dashboard.tips.createWishlist`.
- No emails are sent.

### Data
- `wishlists(id, user_id FK users CASCADE, group_id FK groups CASCADE, created_at, updated_at ON UPDATE, UNIQUE(user_id, group_id))`
- `wishlist_items(id, wishlist_id FK wishlists CASCADE, title VARCHAR(255) NOT NULL, description TEXT NULL, link VARCHAR(2048) NULL, created_at, updated_at)`
- `wishlists.updated_at` changes only when the wishlist row itself is updated, so item changes never touch it.

## Problems in the PHP version
1. **Stored XSS through links.** Only `FILTER_VALIDATE_URL` is checked, so the scheme is not restricted. `javascript://%0aalert(1)` passes and runs in the viewer's (the giver's) session when they click it.
2. `target="_blank"` without `rel="noopener noreferrer"`, so the app URL (with group id) leaks as the referrer to shops.
3. **CSRF**: remove item and delete wishlist are GET links. Add and update are POSTs without a token.
4. The dead `settings` and `priority` routes give a 500 page that **prints the exception message**. The removed `updatePriority` had an IDOR (any item id).
5. No `ORDER BY`, while comments and the home page promise prioritised wishlists.
6. Titles are not trimmed: a title of only spaces passes and the title `"0"` is rejected (`empty("0")`). There are no length limits (a title over 255 characters gives a DB error and a 500) and no item limit.
7. The edit page creates a DB row on GET.
8. The visibility rule is inconsistent: everything is visible before the draw, then hidden from non-admins after it, although members had already seen the lists. The admin keeps seeing every list.
9. Item fns check ownership but not current membership. Because PHP leave keeps the wishlist, a former member can still edit items nobody can see.
10. "Item not found" and "no permission" are different messages, so item ids can be enumerated.
11. Validation `old` values from the edit modal end up in the add form. Editing is impossible without JS.
12. There is no way for a giver to notice that the list changed after the draw.
13. Hard-coded English strings. Confirm texts are not JS-escaped.

## Redesign

### Decisions taken in this spec
- **W1 Visibility.** Every current member of a group can view the wishlist of every current member of that group, before and after the draw. The admin has no extra rights. The rule never depends on assignments, so no wishlist fn reads `gift_assignments`, and access patterns cannot reveal who gives to whom. (Resolved 1.)
- **W2 Authorization.** Every fn calls `require_user()` first (`Unauthorized` when anonymous), then `authz::require_member(group_id)`. A missing group and a group the caller is not in give the same `NotFound` (groups D2). A target user who is not a current member of the group, or does not exist, also gives `NotFound`. Item fns take only an `item_id`. The server loads `(wishlist.user_id, wishlist.group_id)` for it, and returns `NotFound` if the item does not exist, belongs to someone else, or the caller is no longer a member of its group. The three cases cannot be told apart.
  - Note for implementation: `rust/app/src/server/authz.rs::require_member` already returns `NotFound` for both "group missing" and "not a member" (groups D2; checked 2026-10-31). Use it as is; do not re-map errors per fn.
- **W3 Only the owner edits**, and only their own list in a group they are currently a member of. Nobody else can edit a list, the admin included.
- **W4 Editing after the draw is allowed** (add, edit, remove, reorder, clear). Every item mutation sets `wishlists.updated_at = CURRENT_TIMESTAMP`, and the read-only view shows "Last updated {date}" so a giver notices changes.
- **W5 Lazy creation.** The `wishlists` row is created by the first `add_item` (`INSERT … ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id)`). No row is created on join or on any GET. A missing row reads as an empty list with no "last updated". `clear_wishlist` deletes the row (items cascade).
- **W6 No wishlist settings.** `updateSettings` / `is_priority_ordered` is not ported. Every list is ordered by its owner (W7), and there is no toggle.
- **W7 Ordering.** New column `wishlist_items.position`. Reads use `ORDER BY position, id`. A new item goes to the end (`MAX(position) + 1`). The owner reorders with **Move up / Move down** buttons per item. Each button is a plain `<form method="post">` for the `move_item` server fn (Leptos `ActionForm`), so it works without JS: the server fn redirects back to the edit page. A move inside a transaction loads the list's items in order, swaps the item with its neighbour and rewrites positions `1..n`. Moving the first item up or the last item down is a successful no-op. The read-only view shows the hint "Most wanted first". There is no drag and drop. (Resolved 2.)
- **W8 Limits** (counted in Unicode scalar values after trimming):
  - `title`: required, 1..=255.
  - `description`: 0..=2000. Empty is stored as `NULL`.
  - `link`: 0..=2048 (column size) before and after normalisation. Empty is stored as `NULL`.
  - At most **50 items** per wishlist, checked under the lock in W11.
  - All errors are returned together, per field.
- **W9 Link validation**, done on the server; the client mirrors it only as a hint:
  1. Trim. If the empty string remains, store `NULL`.
  2. If the input contains no `://`, prefix it with `https://` (so `www.shop.de/x` works).
  3. Parse with the `url` crate. Reject the link (`validation.wishlist.link_invalid`) unless the scheme is `http` or `https`, the host is non-empty, and there is no username or password.
  4. Store `Url::as_str()` (the normalised form). Reject it with `validation.wishlist.link_too_long` if the input or the normalised form is longer than 2048.
- **W10 Safe rendering.**
  - Title and description are rendered as text nodes; there is no `inner_html` anywhere in this slice. Line breaks in the description are preserved via CSS `white-space: pre-line`.
  - Links render as `<a href={link} target="_blank" rel="noopener noreferrer nofollow">`, followed by the link's host (computed on the server, punycode as is) so viewers see where it goes.
  - Values read from the DB are not re-validated on render. Only W9-validated values can be stored.
- **W11 Concurrency.** Every mutation runs in one transaction, and the lock order is fixed to avoid deadlocks:
  1. `SELECT id FROM \`groups\` WHERE id = ? LOCK IN SHARE MODE`. For item fns, the group id is first read without a lock, then the group row is locked and the item is re-read under the lock.
  2. The membership check, inside the transaction.
  3. `SELECT id FROM wishlists WHERE user_id = ? AND group_id = ? FOR UPDATE`.
  - The groups slice's leave, remove and join (groups D16), the exclusions writes (exclusions D9) and the draw (draw D5) take `FOR UPDATE` on the same group row, always before any child row. So a wishlist write can never interleave with `membership::remove_member_data`, and no wishlist can be left behind for a non-member.
  - Two tabs of the same owner are serialised by the wishlist row lock.
- **W12 Departing members.** Groups D5 already deletes the member's `wishlists` row (items cascade) when they leave or are removed. This slice adds nothing to `remove_member_data`. Deleting a group or a user cascades.
- **W13 No emails** are sent by this slice.

### Pages / routes

| URL | Page | Who |
|---|---|---|
| `/groups/:id/wishlist` | Edit my wishlist for group `:id` | member (anyone else: 404 page) |
| `/groups/:id/wishlist/:user_id` | Read-only wishlist of `:user_id` in group `:id` | member, if `:user_id` is a current member (otherwise: 404 page) |
| `/groups/:id` | `<WishlistPanel>` (groups page hook) | member |

- `/groups/:id/wishlist/:me` is the read-only view of my own list ("how others see it"), with an Edit button.
- The draw slice's assignment card on the group page and its dashboard assignments card link to `/groups/{group_id}/wishlist/{receiver_id}`.
- Old PHP URLs (`/wishlist/...`) are not kept (fresh start).
- Non-numeric or non-positive `:id` / `:user_id` gives the 404 page. Logged-out visitors are redirected to login with `?next=` (auth slice) and land back on the same URL.
- Edit page query `?edit={item_id}`: renders that item's row as an inline edit form. This is a plain GET navigation, so it also works without JS. If the id is not one of my items in this group, the parameter is ignored.

### Server functions

| Fn | Auth | Result | Errors |
|---|---|---|---|
| `get_wishlist(group_id, user_id)` | `require_member`; target is a current member | `WishlistView` | `Unauthorized`, `NotFound` |
| `get_my_wishlist(group_id)` | `require_member` | `WishlistView` (owner = me) | `Unauthorized`, `NotFound` |
| `get_wishlist_panel(group_id)` | `require_member` | `WishlistPanel` | `Unauthorized`, `NotFound` |
| `add_item(group_id, input: ItemInput)` | `require_member` | `item_id`; flash `item_added`, redirect `/groups/{id}/wishlist` | `Unauthorized`, `NotFound`, `Validation{field→key}`, `Validation(too_many_items)` |
| `update_item(item_id, input: ItemInput)` | owner + member (W2) | `()`; flash `item_updated`, redirect `/groups/{id}/wishlist` | `Unauthorized`, `NotFound`, `Validation{field→key}` |
| `delete_item(item_id)` | owner + member | `()`; flash `item_deleted`, redirect `/groups/{id}/wishlist` | `Unauthorized`, `NotFound` |
| `move_item(item_id, direction: Up \| Down)` | owner + member | `()`; no flash, redirect `/groups/{id}/wishlist` | `Unauthorized`, `NotFound` |
| `clear_wishlist(group_id, confirm: bool)` | `require_member` | `()`; flash `wishlist_deleted`, redirect `/groups/{id}/wishlist`. Idempotent: succeeds when no list exists | `Unauthorized`, `NotFound`, `Validation{confirm→validation.wishlist.confirm_required}` when `confirm` is false |

Types:
- `ItemInput { title: String, description: String, link: String }`. Validation per W8 and W9. On any error nothing is written.
- `WishlistView { group: { id, name }, owner: { id, name }, is_own: bool, items: Vec<ItemView>, updated_at: Option<DateTime<Utc>> }`
- `ItemView { id, title, description: Option<String>, link: Option<String>, link_host: Option<String> }`. Items are ordered `position, id`.
- `WishlistPanel { my_item_count: u32, members: Vec<{ user_id, name, is_me, item_count: u32 }> }`. Members are ordered like the group page (`joined_at, id`). One query: `group_members` ⋈ `users` ⟕ `wishlists` ⟕ `wishlist_items`.
- None of these types contains email addresses or assignment data.
- `validation` keys use the same field→key error shape as the groups slice (`Validation{field→key}`).

`add_item` steps (W11 locks): lock the group shared → check membership → `INSERT INTO wishlists (user_id, group_id) VALUES (?, ?) ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id)` → `SELECT … FOR UPDATE` → `COUNT(*) < 50` else `too_many_items` → insert the item with `position = COALESCE(MAX(position), 0) + 1` → `UPDATE wishlists SET updated_at = CURRENT_TIMESTAMP`. Validation runs before the transaction.

### Data & migrations

Tables touched: `wishlists` (lazy insert, `updated_at` bump, delete), `wishlist_items` (CRUD). Read only: `groups`, `group_members`, `users`.

New migration `NNNN_wishlist_item_position.sql`:
```sql
ALTER TABLE wishlist_items
    ADD COLUMN position INT NOT NULL DEFAULT 0 AFTER link;
CREATE INDEX idx_wishlist_items_order ON wishlist_items (wishlist_id, position, id);
```
- Positions are unique within a wishlist by construction (append = max + 1, moves rewrite `1..n`), but correctness only relies on `ORDER BY position, id`. There is no UNIQUE constraint, so that swaps need no temporary values.
- There is no `is_priority_ordered` column and no other schema change. Queries name their columns explicitly.

### UI notes

**`<WishlistPanel group_id>`** on the group page:
- Card `group.show.yourWishlist` with `group.show.wishlistHelp`, my item count (`wishlist.panel.myItemCount` (new, plural `.one` / `.other`, `{count}`; en "{count} item" / "{count} items", de "{count} Artikel" / "{count} Artikel"; `t_plural!`)), a button `group.show.editWishlist` → `/groups/:id/wishlist`, and the link `group.show.viewYourWishlist` + `group.show.viewYourWishlistHelp` → `/groups/:id/wishlist/:me`.
- Below it, "Wishlists in this group" (`wishlist.panel.membersTitle` (new)): one row per other member with their name, item count (`wishlist.panel.itemCount` (new, plural `.one` / `.other`, `{count}`, same texts as `myItemCount`; `t_plural!`), with the "empty" wording when 0) and a link `wishlist.panel.view` (new) → `/groups/:id/wishlist/:user_id`.

**`/groups/:id/wishlist` (edit):**
- Title `wishlist.edit.title`, then `wishlist.edit.forGroup` and a group link.
- Visibility hint: reword `wishlist.edit.description` to en "Add items you'd like to receive. All members of this group can see this list.", de "Fügen Sie Artikel hinzu, die Sie sich wünschen. Alle Mitglieder dieser Gruppe können diese Liste sehen."
- Link `group.show.viewYourWishlist` to the read-only view. This replaces the odd "Wishlist from: You".
- **Add form**: `wishlist.edit.itemTitle` (placeholder `itemTitlePlaceholder`, `maxlength=255`, required), `wishlist.edit.itemDescription` (placeholder `descriptionPlaceholder`, help `descriptionHelp`, `maxlength=2000`), and `wishlist.edit.link` (placeholder `linkPlaceholder`, help `linkHelp`). The link input is `type="text" inputmode="url" maxlength=2048`, not `type=url`, because scheme-less input is accepted. Submit `wishlist.edit.addItem`. Show `wishlist.edit.itemCount` (new, "{count} of {max} items"). At 50 items, disable the form and show `validation.wishlist.too_many_items`.
- **Item list** (heading `wishlist.edit.itemsHeading` (new)), in order. Each row has the title, description, link + host, and these buttons:
  - Move up (`wishlist.edit.moveUp` (new), hidden on the first row).
  - Move down (`wishlist.edit.moveDown` (new), hidden on the last row).
  - Edit (`wishlist.edit.edit`, link to `?edit=ID`).
  - Remove (`wishlist.edit.removeItem`, confirm `wishlist.edit.removeItemConfirm` with JS; without JS it removes directly).
  - The buttons need accessible labels that include the item title.
- Inline edit form: the same fields pre-filled, `wishlist.edit.save`, and `wishlist.edit.cancel` (link back to the page without `?edit`). Errors from the inline form appear under its own fields, never in the add form.
- Empty state: `wishlist.edit.noItems` + `wishlist.edit.addSome`.
- **Clear list**: a bordered section with `wishlist.edit.deleteWishlist` (reword en "Remove all items", de "Alle Artikel entfernen"), a required checkbox `wishlist.edit.deleteWishlistConfirm` (reword to en "Yes, remove all items from this list. This cannot be undone.", de "Ja, alle Artikel dieser Liste entfernen. Das kann nicht rückgängig gemacht werden."), and a submit button. The checkbox is the confirmation, so it works without JS. It is shown only when the list has items.
- Field errors appear inline (with JS). Without JS, the page reloads and shows the error through the foundation's no-JS server fn error path, and entered values may be lost (accepted degradation).
- Amended 2026-11-09 (#69): with JS, a successful add empties the add form (title, description, link) and focuses its title; a failed add keeps the entered values. Every item row is `<li id="item-{id}">`; Edit links to `?edit=ID#item-ID`, Cancel to the page without `?edit` at `#item-ID` (without JS the browser jumps to the row; it does not focus a field). With JS the inline title is focused when the form opens, but a later re-read of the list (add, move, remove) does not take the focus from elsewhere; after Cancel or a successful save the row is scrolled into view. A save returns to the URL without `?edit` and without a fragment, like the no-JS redirect.

**`/groups/:id/wishlist/:user_id` (view):**
- Title `wishlist.view.title` (`{name}`), then `wishlist.view.forGroup` and a group link.
- When the list has items, show `wishlist.view.lastUpdated` (new, `{date}` = `LocalDate(updated_at)`: `app::format` date in `APP_TIMEZONE` and the page locale, en `10 Dec 2026`, de `10.12.2026`; never `Utc::now()` at render) and `wishlist.view.orderHint` (new, en "Most wanted first", de "Größte Wünsche zuerst").
- Item cards: the title, the description, and the link button `wishlist.view.viewItem` followed by the host.
- Empty: `wishlist.view.empty`. The owner also sees `wishlist.edit.addSome` + `wishlist.view.addNow` → edit page. Others also see `wishlist.view.noPreference` + `wishlist.view.suggestGiftCard`.
- The owner sees an Edit button (`wishlist.edit.title`). Back link: `wishlist.view.backToGroup`.

**Flash / validation keys.**
- Reused: `flash.success.item_added`, `item_updated`, `item_deleted`, `wishlist_deleted` (reword en "All items were removed from your wishlist", de "Alle Artikel wurden von Ihrer Wunschliste entfernt").
- New (en + de): `validation.wishlist.title_required`, `title_too_long`, `description_too_long`, `link_invalid` (en "Enter a web address starting with http:// or https://"), `link_too_long`, `too_many_items` (`{max}`), `confirm_required`; `wishlist.panel.myItemCount`, `wishlist.panel.membersTitle`, `wishlist.panel.itemCount`, `wishlist.panel.view`; `wishlist.edit.itemCount`, `wishlist.edit.itemsHeading`, `wishlist.edit.moveUp`, `wishlist.edit.moveDown`; `wishlist.view.lastUpdated`, `wishlist.view.orderHint`.
- Removed (not ported): `flash.error.only_view_assigned_recipient`, `no_permission_edit_item`, `no_permission_delete_item`, `item_not_found`, `wishlist_not_found`, `wishlist_delete_failed` (all replaced by the 404 page / `NotFound`), `flash.success.wishlist_settings_updated`, `wishlist.edit.update`, and the de-only leftovers `wishlist.edit.priorityHelp`, `flash.error.invalid_item_positions`, `flash.success.priorities_updated`, `flash.error.priorities_update_failed`.
- `flash.error.user_not_in_group` is not used by this slice (the groups and exclusions slices use it).
- `home.features.prioritizedWishlists` stays (accurate again with W7). The home page is not this slice.
- `user.dashboard.assignments.viewWishlist` and `group.show.viewWishlist` belong to the draw slice and link to the URL above.

## Acceptance criteria

Fixtures: users Alice, Bob, Carol and Dave. G is a group with admin Alice and members Alice, Bob and Carol, not drawn. H is a group with admin Bob and members Bob and Dave. Bob has a list in G with items A, B, C (in that order) and a list in H with item X. Alice has a list in G with one item. Carol has no wishlist row. These hold unless an AC says otherwise.

**View**
- AC1: Given Carol opens `/groups/{G}/wishlist/{Bob}`, then she sees Bob's name, the group link, items A, B, C in that order with title, description and link, `wishlist.view.orderHint`, and `wishlist.view.lastUpdated` with the date of Bob's last change. She sees no edit controls.
- AC2: Given Bob opens `/groups/{G}/wishlist/{Carol}`, then he sees `wishlist.view.empty`, `noPreference` and `suggestGiftCard`, and no "last updated". No `wishlists` row exists for Carol in G afterwards.
- AC3: Given Dave (not in G) opens `/groups/{G}/wishlist/{Bob}`, or anyone opens `/groups/999999/wishlist/{Bob}`, then the same 404 page is shown, and `get_wishlist` returns `NotFound` in both cases.
- AC4: Given Bob calls `get_wishlist(G, Dave)` (Dave exists but is not in G) or `get_wishlist(G, 999999)`, then `NotFound`. His items in H are never returned through G.
- AC5: Given G is drawn with Bob → Carol, then Bob, Carol and Alice can each view every member's list in G. For Bob, `get_wishlist(G, Alice)` and `get_wishlist(G, Carol)` succeed alike. No wishlist fn result contains assignment data, and no wishlist fn reads `gift_assignments` (integration test: revoking SELECT on that table for the test user does not break any wishlist fn, or an equivalent code-level check).
- AC6: Given Bob opens `/groups/{G}/wishlist/{Bob}`, then he sees the read-only view and an Edit button. Carol on the same URL sees no Edit button.
- AC7: Given a logged-out visitor opens `/groups/{G}/wishlist` or `/groups/{G}/wishlist/{Bob}`, then they are redirected to login and, after logging in as Carol, land on the same URL. Every server fn of this slice called without a session returns `Unauthorized`.
- AC8: Given Bob opens `/groups/{G}`, then the wishlist panel shows his count 3, links to edit and to his view, and rows for Alice (1 item) and Carol (empty) with links to their lists. It shows no emails.

**Add**
- AC9: Given Carol (no wishlist row) submits title "  Book  ", description "Paperback", link "https://example.com/book", then a `wishlists` row for (Carol, G) exists, the item is stored with title "Book", it is last in her list, `wishlists.updated_at` is set, she is on `/groups/{G}/wishlist` and sees `item_added`.
- AC10: Given Carol opens `/groups/{G}/wishlist` and leaves without adding anything, then no `wishlists` row exists for her in G.
- AC11: Given an add with a title that is empty or only spaces, then nothing is saved and the title field shows `validation.wishlist.title_required`. A 256-character title gives `title_too_long`, and 255 characters succeed. A 2001-character description gives `description_too_long`. An empty description and an empty link are stored as `NULL`. The title "0" is accepted.
- AC12: Given an add with each of these links: `javascript:alert(1)`, `javascript://%0aalert(1)`, `data:text/html,<b>x</b>`, `ftp://example.com/x`, `https://user:pw@example.com/`, `https://`, `http:// example.com`. Then each is rejected with `validation.wishlist.link_invalid` and nothing is saved. A link longer than 2048 characters gives `link_too_long`.
- AC13: Given an add with `http://example.com`, `https://example.com/a?b=c#d` or `www.example.com/x`, then each is accepted. The last one is stored as `https://www.example.com/x`.
- AC14: Given Bob's list in G has 50 items, when he adds another, then `validation.wishlist.too_many_items` is returned and the count stays 50. His list in H is unaffected by the limit.
- AC15: Given Dave calls `add_item(G, …)` or `add_item(999999, …)`, then `NotFound`, and no `wishlists` or `wishlist_items` row is created.

**Update / delete**
- AC16: Given Bob opens `/groups/{G}/wishlist?edit={B}`, changes B's title, description and link and saves, then the values are stored, B keeps its position, `updated_at` is bumped, and he sees `item_updated`.
- AC17: Given Carol calls `update_item`, `delete_item` or `move_item` with the id of Bob's item B, or with a non-existent id, then she gets `NotFound` in every case and B is unchanged. Alice (admin of G) gets the same result.
- AC18: Given `update_item` with invalid input (per AC11–AC12), then the stored item is unchanged, and the error is shown under the inline edit form, not the add form.
- AC19: Given Bob opens `?edit={Alice's item id}`, then no edit form is rendered and Alice's item is not shown on his edit page.
- AC20: Given Bob removes B, then B is gone, the list shows A, C, and he sees `item_deleted`.
- AC21: Given Bob submits Clear list in G with the checkbox ticked, then his `wishlists` row and items in G are gone, his list in H still has X, Alice's list is unchanged, and he sees `wishlist_deleted`. Calling `clear_wishlist(G, true)` again succeeds. `clear_wishlist(G, false)` returns `validation.wishlist.confirm_required` and deletes nothing.

**Reorder**
- AC22: Given A, B, C, when Bob moves C up, then the order is A, C, B for Bob and for Carol's view. Moving A up and moving B down (now last) change nothing and return success. The first row has no Move up button and the last row has no Move down button.
- AC23: Given the browser has JavaScript disabled (Playwright `javaScriptEnabled: false`), when Bob clicks Move down on A, then the page reloads with the order B, A, C. Adding an item and Clear list also work without JS.
- AC24: Given Bob reorders his list in G, then the order of his list in H and of Alice's list in G is unchanged.
- AC25: After any sequence of add, delete and move operations on one list, the positions read back in `ORDER BY position, id` match the order shown on the page (property-style integration test over random operation sequences).

**After the draw**
- AC26: Given G is drawn, then Bob can still add, update, delete, move and clear in G. After his change, Carol's view shows the new content and a `lastUpdated` date that is not earlier than the change.

**Rendering safety**
- AC27: Given an item with link `https://shop.example/x`, then the view renders `<a href="https://shop.example/x" target="_blank" rel="noopener noreferrer nofollow">` and shows the host `shop.example`.
- AC28: Given an item with title `<script>alert(1)</script>` and description `<img src=x onerror=alert(1)>` followed by a line break and "line 2", then both are shown as literal text, no script runs, and "line 2" appears on its own line.

**Protocol / authorization plumbing**
- AC29: Given a plain GET or a cross-origin POST to the endpoint of `add_item`, `delete_item`, `move_item` or `clear_wishlist`, then nothing changes.

**Membership interplay**
- AC30: Given Bob leaves G (groups slice), then his `wishlists` row and items for G are gone, his list in H is intact, and Alice gets the 404 page for `/groups/{G}/wishlist/{Bob}`.
- AC31: Given a transaction holds `FOR UPDATE` on G's group row (as `remove_member` does), when Bob calls `add_item(G, …)`, then the call waits until that transaction ends. If it removed Bob, `add_item` returns `NotFound` and no `wishlists` row for (Bob, G) exists afterwards. (Integration test as in groups AC44.)

**i18n**
- AC32: Every i18n key this slice uses (reused and new) exists in both `rust/locales/en.json` and `rust/locales/de.json`, and the edit and view pages render without missing-key markers in both locales.
- AC33 (added 2026-10-31, i18n-foundation): Given Bob's list in G has `updated_at = 2026-12-10 22:30 UTC`, then Carol's view shows the `lastUpdated` text with `10 Dec 2026` in English and `10.12.2026` in German. On the group page, a member with 1 item shows `1 item` / `1 Artikel` and one with 2 items `2 items` / `2 Artikel`.

## Open questions

None.

## Resolved (2026-10-31, user)
1. **Who may view whose wishlist?** (a): every current member sees every current member's list, before and after the draw (W1). The admin has no extra rights, and no wishlist fn reads `gift_assignments`. Rejected: (b) the PHP rule (after the draw only own + receiver's list, admin sees all) and (c) only giver and owner after the draw.
2. **Keep item reordering?** (a): yes, an always-on order with Move up / Move down buttons and the `wishlist_items.position` migration (W7). Rejected: (b) creation order only.


## Issue #57: copying own wishes (approved 2026-11-03)

The user requested implementation of #57, extending the original exclusion of copying. The editor at `/groups/:id/wishlist` provides own source-list selection and checkbox selection of one or more items. Copy preserves source order, title, description and normalized link, generates new IDs and timestamps, and appends to the destination. Source rows and timestamps remain unchanged. Historical/drawn groups remain available while the user is still a member; departed/deleted lists are unavailable under W12.

Both groups require current membership, and the source must belong to the caller. The server rejects missing/foreign item IDs together, duplicate IDs are deduplicated, same-list copying is rejected, and the 50-item cap applies atomically to the complete selection. All group locks are taken in ascending ID order, then both wishlist locks in that order; destination creation happens in that order too. Copy validates item contents under W8/W9 before insert. No GET creates data.

Copy extends the existing wishlist editor and server flow; the original wishlist CRUD, panel, locking and validation remain authoritative. No additional schema migration is needed beyond the existing position migration.
