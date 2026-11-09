import { expect, test } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";

test("install metadata and all launcher icons are served without login", async ({ page, request }) => {
  await page.goto("/");
  await expect(page.locator('link[rel="manifest"]')).toHaveAttribute("href", "/manifest.webmanifest");
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute("content", "#dc3545");
  const response = await request.get("/manifest.webmanifest");
  expect(response.status()).toBe(200);
  expect(response.headers()["content-type"]).toMatch(/application\/(manifest\+json|json)/);
  const manifest = await response.json();
  expect(manifest).toMatchObject({
    id: "/", name: "Secret Santa", start_url: "/user/dashboard", scope: "/", display: "standalone",
  });
  expect(manifest.icons).toEqual(expect.arrayContaining([
    expect.objectContaining({ sizes: "192x192", purpose: "any" }),
    expect.objectContaining({ sizes: "512x512", purpose: "any" }),
    expect.objectContaining({ sizes: "512x512", purpose: "maskable" }),
  ]));

  const appleIcon = await page.locator('link[rel="apple-touch-icon"]').getAttribute("href");
  expect(appleIcon).toBeTruthy();
  for (const icon of [...manifest.icons, { src: appleIcon, sizes: "180x180" }]) {
    const image = await request.get(icon.src);
    expect(image.status()).toBe(200);
    expect(image.headers()["content-type"]).toContain("image/png");
    const bytes = await image.body();
    expect(bytes.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
    expect(`${bytes.readUInt32BE(16)}x${bytes.readUInt32BE(20)}`).toBe(icon.sizes);
  }

  // The launcher reaches the existing login flow without changing its intended URL.
  await page.goto(manifest.start_url);
  await expect(page).toHaveURL(/\/auth\/login\?next=/);
});

test("installation help is linked, localized and available without JavaScript", async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false, locale: "de-DE" });
  const page = await context.newPage();
  await page.goto("/");
  await page.getByRole("link", { name: "App installieren" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("App installieren");
  await expect(page.getByRole("heading", { level: 2 })).toHaveText(["iPhone und iPad", "Android", "Desktop", "Produktlinks empfangen"]);
  await expect(page.getByText(/Zum Home-Bildschirm und Hinzufügen/)).toBeVisible();
  await expect(page.getByText(/Eine Internetverbindung wird benötigt/)).toBeVisible();
  await expect(page.locator("main")).toContainText("Legen Sie Wichteln");
  await expect(page.locator("main")).not.toContainText("Secret Santa");
  await context.close();
});

test("installation help hydrates on mobile", async ({ page }) => {
  const errors = collectPageErrors(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/install");
  await waitForHydration(page);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Install the app");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});

// Issue #74: inside the installed app the install links are pointless and hidden.
test("install link is shown in the browser but hidden in the installed app", async ({ page }) => {
  const errors = collectPageErrors(page);
  await page.goto("/");
  await waitForHydration(page);
  const installLink = page.locator("footer").getByRole("link", { name: "Install the app", exact: true });
  await expect(installLink).toBeVisible();

  // Chromium cannot emulate `display-mode` (CDP `Emulation.setEmulatedMedia` silently
  // ignores it, checked with Chromium 156), so the test finds the app stylesheet's rule
  // that hides `.install-link`, checks its display-mode conditions, and then makes it
  // match by appending the medium `all`, as an installed app's display mode would.
  const media = await page.evaluate(() => {
    for (const sheet of Array.from(document.styleSheets)) {
      if (!sheet.href?.endsWith("/assets/css/styles.css")) continue;
      for (const rule of Array.from(sheet.cssRules)) {
        if (rule instanceof CSSMediaRule && rule.cssText.includes(".install-link")) {
          const original = rule.media.mediaText;
          rule.media.appendMedium("all");
          return original;
        }
      }
    }
    return null;
  });
  expect(media).toContain("(display-mode: standalone)");
  expect(media).toContain("(display-mode: minimal-ui)");
  expect(media).toContain("(display-mode: window-controls-overlay)");
  expect(media).not.toContain("fullscreen");
  await expect(installLink).toBeHidden();
  await expect(page.locator("footer")).toContainText("©");

  // The help page itself stays reachable by URL.
  await page.goto("/install");
  await waitForHydration(page);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Install the app");
  expect(errors).toEqual([]);
});

test("install link is hidden in an older iOS Home Screen app (navigator.standalone)", async ({ page }) => {
  const errors = collectPageErrors(page);
  await page.goto("/");
  await waitForHydration(page);
  await expect(page.locator("html[data-standalone]")).toHaveCount(0);
  await expect(page.locator("footer").getByRole("link", { name: "Install the app" })).toBeVisible();

  await page.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, "standalone", { configurable: true, get: () => true });
  });
  await page.goto("/");
  await waitForHydration(page);
  await expect(page.locator("html[data-standalone]")).toHaveCount(1);
  await expect(page.locator("footer").getByRole("link", { name: "Install the app" })).toBeHidden();
  expect(errors).toEqual([]);
});
