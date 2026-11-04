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
  await expect(page.getByRole("heading", { level: 2 })).toHaveText(["iPhone und iPad", "Android", "Desktop"]);
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
