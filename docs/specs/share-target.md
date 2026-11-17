# Product link sharing (#58)

status: approved (implementation authorized by the issue and 2026-11-03 user request)

Revised 2026-11-16 (group-archive, #61; see `docs/specs/group-archive.md`): the group picker lists active groups only (`list_my_groups`), and saving into an archived group answers `WishlistError::Archived`.

Revised 2026-11-17 (#75, PR #89): the entry point **Add a shared link** is now called **Use a product link from another app** (German „Produktlink aus einer anderen App übernehmen“), as link text and page heading alike; the wishlist editor explains it below the link (not on a full list), and the iPhone steps name it and ask for a title. The fallback below keeps its original wording.

## Platform feasibility

The installed PWA registers a GET `share_target` for title, text and URL. Android Chromium and supported Chromium desktop installations can offer it in the system share picker. Availability depends on browser, OS, installation and app updates; do not promise every desktop combination. iPhone/iPad Safari does **not** support receiving shares through this manifest member. `navigator.share()` is an outgoing API and does not solve this.

Sources checked 2026-11-04:
- https://developer.chrome.com/docs/capabilities/web-apis/web-share-target (manifest, installation and Android often placing URLs in text).
- https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest/Reference/share_target (GET reception and compatibility).
- https://bugs.webkit.org/show_bug.cgi?id=194593 (incoming Web Share Target implementation is still NEW, unresolved).

Practical iPhone fallback: copy a product link, open the installed app, choose **Add a shared link**, paste, choose a wishlist and confirm. A native iOS Share Extension would require a separate native application and is outside this issue. An optional user-created Shortcuts workflow can open `/share-target?url=<URL-encoded shared URL>`; the app does not install or require such a shortcut.

## Acceptance criteria

1. Manifest maps GET `/share-target` fields `title`, `text`, `url`; receiving a share never writes a wish automatically.
2. Intake accepts bounded untrusted data and extracts HTTP(S) URLs from URL, text or title (in that order). It never fetches remote products.
3. Incoming data is kept server-side in the existing session, under an opaque short draft id. The redirect and login `next` contain only that id so long product URLs do not exceed auth's 512-byte safe-next limit.
4. After login, the review page offers all current own wishlist groups, including empty lists. Title, description and link can be edited. Only a deliberate POST creates a wish, through the existing wishlist authorization/validation path.
5. Invalid input retains the edited draft for correction; successful submission clears that draft. No-JS use works. Drafts belong to the receiving browser session, expire with it, and logout clears them. Up to five pending drafts are retained per session.
6. Blank `/wishes/shared` offers the same form for manual pasting, with a visible link in wishlist navigation and installation help. iPhone restrictions and the fallback are explained in English and German.
7. Unit/integration tests cover text URLs, unsafe URLs, long login continuation, lack of mutation on GET, session isolation, authorization, invalid correction and success; browser tests cover manifest and review/fallback pages.

## Scope

Text and URL shares only; no binary files, scraping, offline queues or automatic creation. OS picker registration must additionally be checked on real devices after installation/update; Playwright tests the receiving HTTP flow.

## Amendments

Amended 2026-11-17 (#84 review, single-use drafts): the acceptance criteria above keep their wording; this note records what the implementation adds to AC3-AC5.

- A staged draft creates at most one wish, also when concurrent requests loaded the same session data. Its submit stores a one-way hash of the draft id (with the user id) in `consumed_share_drafts` (migration `0012_consumed_share_drafts.sql`) in the same transaction as the wish, after the group's membership and archive checks. A submit that fails (validation, too many items, archived group) stores no claim, so the draft can still be corrected. Claims are kept for the life of the account (sessions are rolling) and are deleted with it.
- Only a staged draft is claimed: a logged-in submit whose id names a pending draft of this session. A submit without an id (a wish typed on the blank `/wishes/shared` page) goes through the normal add path and stores no claim; a logged-out submit keeps the entered values as a new draft for the login and claims nothing either.
- A logged-in submit whose id is not a pending draft of this session (already consumed, e.g. a double submit after the first finished, or never staged here), or whose draft another request claimed first, creates no wish. A plain form post (no JS) gets the `error.notFound.message` flash and a redirect to the review page `/wishes/shared?id=…`, like the other failures (instead of a bare 404); that page shows the not-found alert with the "start again" link, because reading a claimed or unknown draft answers not found. The server fn client (JS) still gets the 404 (`NotFound`). A failed submit keeps the edited values as the draft again only if the draft was not claimed meanwhile.
