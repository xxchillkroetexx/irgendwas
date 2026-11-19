# Dashboard group cards and recipient disclosure (#47)

status: approved — user approved the dashboard, native disclosure, existing emails and tips; a human checks it on dev after the merge

Revised 2026-11-18 (review of #86): the cards come from `list_my_groups`, so archived
groups are left out (group-archive A3); a failing groups query leaves the section out
instead of turning the page into the 500 page; the disclosure's label says "Reveal" or
"Hide", and the disclosure stays open while the panel re-reads its data; only the fields a
card shows go into the hydration data. The human test runs on dev after the merge (there
is no other test deployment), so #47 stays open until it passed.

## Context and scope

[Issue #47](https://git.scheissndreck.de/irgendwas/irgendwas/issues/47) asks for one
card per group on the dashboard, removal of its recipient list, and a recipient who is
shown only on the group page after an explicit reveal. Its attached sketch shows group
cards beside a quick-actions panel; it does not specify the disclosure interaction.
The current Rust dashboard has tips plus an assignment card, but no group cards.

This specification supersedes the dashboard assignment-card contract in the approved
`account.md` UI notes and `draw.md` UI notes / AC31–AC32, and changes the initially
visible assignment presentation in `draw.md` UI notes / AC14 and AC46. Those specifications are amended alongside this implementation.
Other draw behavior and the authorization/privacy rules remain in force.

Issue #51 (dashboard entry for joining a group) is assigned separately and is outside
this change. Do not implement its join button or claim its issue; coordinate before
changing overlapping dashboard code. The existing group-management and join routes
remain available.

## Proposed behavior

- Show the signed-in user's current groups as individual cards, newest first, using the
  existing group-list semantics (`list_my_groups`: active groups only; archived groups are
  on `/groups/archive`, group-archive A3/U8). Each card identifies the group, its status and the
  caller's admin role if applicable (the same badges as `/groups`), and links to that group. Do not put a recipient's
  name, ID, or recipient-specific wishlist link on a dashboard card. The page maps the
  groups to the card's fields (id, name, admin flag, status) before they reach the
  hydration data: no description and no admin name.
- The dashboard is the default page after login: if the groups cannot be read, the
  "My groups" section is left out and the page keeps its status (200) and the rest of
  its content, like the draw panel on the group page. During SSR the page calls the
  flow function directly; in the browser the server fn.
- Show a useful empty state when the caller has no groups. Retain the existing welcome, subtitle and tips. The sketch's quick-actions panel is
  not added; #51 remains separate.
- Remove the dashboard's assignment card and its assignment-data fetch. On a drawn
  group page, keep the caller's own assignment but initially conceal its receiver and
  recipient-specific wishlist link behind an explicit, clearly labeled disclosure.
  The user can reveal and hide it again. Reset/draw state and admin controls continue
  to use the existing contracts.
- Use a native disclosure, initially closed, that can
  be operated before hydration and without scripts. This prevents accidental visual and
  assistive-technology disclosure while closed; it is not a security boundary against
  an authorized user inspecting the response. The control needs an accessible name,
  keyboard operation and an exposed expanded/collapsed state. Its label says what a
  click does: `group.draw.revealAssignment` while closed, `group.draw.hideAssignment`
  while open (CSS on `details[open]`, so it works without JavaScript too).
- While the page stays open, the disclosure keeps its state when the panel re-reads its
  data after an action on the page (for example resend, or a change of admin): the state
  lives at page level, not in the card. Perform and reset close it (a new or no
  assignment). A new page load starts closed; nothing is stored.
- Continue to return only the current user's own assignment and require group
  membership; an admin cannot see others' assignments. Do not place receiver data in
  the dashboard's SSR HTML or hydration payload. Keep personalized pages `no-store`.
  Do not log receiver names, receiver IDs linked to givers, or mail bodies.
- This is an on-screen change: existing result emails and the authorized
  assignment APIs remain unchanged. This does not claim to keep the result secret from the recipient's own email.

## Acceptance criteria

1. Anonymous dashboard and group access preserve the existing authentication and
   membership handling. A user sees cards only for groups they currently belong to;
   names are escaped and cards navigate to the correct group.
2. Zero, one and multiple groups render correctly in English and German; drawn and
   undrawn status and the admin indicator are correct, cards fit mobile and desktop,
   and keyboard users can open every group.
3. Dashboard SSR, hydration data and visible UI contain no assignment receiver name,
   assignment-specific receiver ID or recipient-specific wishlist link, including
   after a draw. The dashboard does not fetch the list of assignments. IDs independently
   needed for a group or the signed-in user are not treated as assignment data.
4. On a drawn group page the caller's recipient and recipient-specific link are hidden
   by default; the accessible disclosure exposes them on request and can hide them
   again. No other member's assignment is returned or shown, including to the admin.
5. Disclosure works with JavaScript disabled, before hydration and after hydration;
   keyboard and assistive-technology users can discover its label and state. After a
   fresh page load the disclosure starts closed. Reset/undrawn states show no stale
   recipient or stale link.
6. The dashboard and group pages retain the existing no-store and authorization
   behavior. Existing draw, wishlist, invite and notification behavior stays covered;
   no data migration is expected and existing rows must remain untouched.
7. SSR/integration tests cover membership isolation, escaped names, empty/drawn and
   reset cases, dashboard payload privacy and the group disclosure. Browser tests
   cover mobile (390 px) and desktop, keyboard navigation to the group links and the
   disclosure, no-JS and hydrated interactions, and the open state across a panel
   re-read; verify English and German (including the German empty state). Existing #51 functionality and work in progress are not removed or
   duplicated.

## Manual check on dev after merge (required)

There is no test deployment: the PR is merged into `dev` and a human then checks it on
the dev system, on mobile and desktop where available. #47 stays open until this passed:

1. With no groups, check the dashboard empty state; with multiple drawn and undrawn
   groups, check separate correctly labeled cards, admin/status badges and each link.
2. Confirm no recipient appears on the dashboard. Open a drawn group and verify the
   recipient and wishlist link are initially hidden, reveal and hide them, then reload
   and confirm they are hidden again.
3. Repeat the group-page disclosure without JavaScript and with keyboard navigation;
   verify screen-reader state if available. Verify another member's result remains
   inaccessible, and reset removes the old assignment.
4. Confirm the tips remain and draw-result emails still name the recipient; check
   English and German. Record the result in #47.

## Data and migration

This is expected to reuse `list_my_groups` and the existing per-group assignment flow,
with no schema migration or rewrite of production data. Result-email content and delivery remain unchanged.
