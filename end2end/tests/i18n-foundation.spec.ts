import { expect, test } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";

// German strings of the home page (locales/de.json: home.title, home.subtitle).
const DE_TITLE = "Wichteln";
const DE_SUBTITLE = "Geschenke machen einfach Spaß!";

test.describe("i18n-foundation", () => {
  test.describe("browser language de-DE, no cookies", () => {
    test.use({ locale: "de-DE" });

    test("AC19: first visit follows Accept-Language and hydrates cleanly", async ({ page, context }) => {
      const errors = collectPageErrors(page);
      expect(await context.cookies()).toEqual([]);

      const res = await page.goto("/");
      expect(res?.status()).toBe(200);
      await expect(page.locator("html")).toHaveAttribute("lang", "de");
      await expect(page.getByRole("heading", { level: 1 })).toHaveText(DE_TITLE);
      await expect(page.getByText(DE_SUBTITLE)).toBeVisible();

      await waitForHydration(page);
      // Still German after hydration: the client read the same locale from <html lang>.
      await expect(page.locator("html")).toHaveAttribute("lang", "de");
      await expect(page.getByRole("heading", { level: 1 })).toHaveText(DE_TITLE);
      expect(errors).toEqual([]);
    });
  });

  test.describe("browser language en-US (the default context)", () => {
    test("AC19: the same page is English", async ({ page }) => {
      const errors = collectPageErrors(page);
      await page.goto("/");
      await expect(page.locator("html")).toHaveAttribute("lang", "en");
      await expect(page.getByRole("heading", { level: 1 })).toHaveText("Secret Santa");
      await expect(page.getByText("Make gift-giving fun and easy!")).toBeVisible();

      await waitForHydration(page);
      expect(errors).toEqual([]);
    });
  });
});
