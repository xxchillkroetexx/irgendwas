import { expect, test } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

// The copy data, authorization, atomicity and HTML form round trip are covered by
// server/tests/wishlist_copy.rs. These journeys exercise the new resource/guard creation
// order in the browser and the no-JavaScript return path after authentication.
for (const javaScriptEnabled of [true, false]) {
  test(`own wishlist navigation (JavaScript ${javaScriptEnabled})`, async ({ browser, baseURL }) => {
    const context = await browser.newContext({ javaScriptEnabled, locale: "en-US" });
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const email = uniqueEmail(javaScriptEnabled ? "wishlist-js" : "wishlist-html");
    const origin = new URL(baseURL!).origin;
    const headers = { Origin: origin, "Accept-Language": "en-US" };
    try {
      const registration = await context.request.post(`${origin}/api/register`, { form: { email }, headers });
      expect(registration.ok()).toBeTruthy();
      const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
      const token = new URL(link).pathname.split("/").pop()!;
      const completion = await context.request.post(`${origin}/api/complete_registration`, {
        headers,
        form: { token, name: "Wishlist User", password: "wishlist-password-1", password_confirm: "wishlist-password-1" },
      });
      expect(completion.ok()).toBeTruthy();
      await page.goto("/user/dashboard");
      if (javaScriptEnabled) await waitForHydration(page);
      await page.getByRole("link", { name: "Your wishlist", exact: true }).click();
      await expect(page).toHaveURL(/\/wishlists$/);
      await expect(page.getByRole("heading", { name: "Your wishlist", exact: true })).toBeVisible();
      if (javaScriptEnabled) await waitForHydration(page);
      expect(errors).toEqual([]);
    } finally {
      await context.close();
    }
  });
}

test("anonymous wishlist editor preserves its return URL", async ({ page }) => {
  await page.goto("/groups/1/wishlist?source=2");
  await expect(page).toHaveURL(/\/auth\/login\?next=/);
  const next = new URL(page.url()).searchParams.get("next");
  expect(next).toBe("/groups/1/wishlist?source=2");
});
