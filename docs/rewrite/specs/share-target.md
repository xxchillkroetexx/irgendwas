# Product link sharing (#58)

status: approved (implementation authorized by the issue and 2026-11-03 user request)

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
