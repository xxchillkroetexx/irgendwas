# PWA installation (#56)

status: approved (implementation authorized in the 2026-11-03 issue task)

## Decisions

- Applies to the Rust rewrite. The PHP application remains a reference.
- A stable manifest identity `/`, scope `/`, and start URL `/user/dashboard`.
  Existing authentication redirects guests to login with the intended URL intact.
- `display: standalone`, matching theme color, opaque 192/512 PNG icons,
  a separate maskable declaration and a 180px Apple touch icon.
- Public `/install` instructions in English and German, linked from every footer,
  describe iPhone/iPad Safari, Android Chrome and supported desktop browsers.
- Hide installation links in standalone, minimal-ui and window-controls-overlay
  display modes, and in older iOS Home Screen apps reporting `navigator.standalone`.
  Keep the help URL available and keep the links visible in normal fullscreen tabs.
- HTTPS is required in production; localhost is suitable for development.
- Offline behavior is outside scope. No service worker or response cache is
  introduced; private pages continue to use the network and existing sessions.

## Acceptance criteria

1. Every SSR page links to a public, valid web app manifest and Apple touch icon.
2. Manifest icons are reachable PNGs with their declared sizes and purposes.
3. Installation uses an app icon and standalone display where supported.
4. Launching the start URL uses the dashboard's existing authentication flow.
5. Installation instructions work without login or JavaScript and fit mobile.
6. Unsupported browsers can continue using the normal website.

Browser acceptance tests cover the asset contract, launcher URL, localized help,
no-JavaScript rendering and mobile hydration. Native OS installation must also be
checked manually on actual iPhone, Android and supported desktop browsers; a
Playwright tab does not perform an operating-system installation.

## Platform references (checked 2026-11-03)

- [MDN: Making PWAs installable](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Guides/Making_PWAs_installable): manifest criteria, HTTPS, service worker not required for installation.
- [WebKit: Home Screen web apps](https://webkit.org/blog/13878/): standalone manifest and Apple touch icons.
- [WebKit: Safari 26](https://webkit.org/blog/17333/webkit-features-in-safari-26-0/): keep Open as Web App enabled when adding to Home Screen.
