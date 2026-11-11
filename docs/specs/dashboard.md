# Dashboard group cards and recipient disclosure (#47)

status: approved — user approved the dashboard, native disclosure, existing emails and tips; a human test is required before merge

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
  existing group-list semantics. Each card identifies the group, its status and the
  caller's admin role if applicable, and links to that group. Do not put a recipient's
  name, ID, or recipient-specific wishlist link on a dashboard card.
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
  keyboard operation and an exposed expanded/collapsed state.
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
   cover mobile/desktop, keyboard, no-JS and hydrated interactions; verify English and
   German. Existing #51 functionality and work in progress are not removed or
   duplicated.

## Human test before merge (required)

A human must verify the PR before merge, on mobile and desktop where available:

1. With no groups, check the dashboard empty state; with multiple drawn and undrawn
   groups, check separate correctly labeled cards, admin/status badges and each link.
2. Confirm no recipient appears on the dashboard. Open a drawn group and verify the
   recipient and wishlist link are initially hidden, reveal and hide them, then reload
   and confirm they are hidden again.
3. Repeat the group-page disclosure without JavaScript and with keyboard navigation;
   verify screen-reader state if available. Verify another member's result remains
   inaccessible, and reset removes the old assignment.
4. Confirm the tips remain and draw-result emails still name the recipient; check
   English and German. Record the human test result in the PR before merging.

## Data and migration

This is expected to reuse `list_my_groups` and the existing per-group assignment flow,
with no schema migration or rewrite of production data. Result-email content and delivery remain unchanged.
