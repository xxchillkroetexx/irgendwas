import { expect, test } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";

// Data-dependent member/nonmember cases are covered with isolated SQLx fixtures.
// This test covers real-router hydration and auth return-path retention in a browser.
test("an invitation survives the anonymous login and registration screens", async ({ page }) => {
  const errors = collectPageErrors(page);
  await page.goto("/join/ABCDEFGH");
  await expect(page).toHaveURL(/\/auth\/login\?next=%2Fjoin%2FABCDEFGH$/);
  await waitForHydration(page);
  await expect(page.locator('input[name="next"]')).toHaveValue("/join/ABCDEFGH");
  await page.getByRole("link", { name: "Register", exact: true }).last().click();
  await expect(page).toHaveURL(/\/auth\/register\?next=%2Fjoin%2FABCDEFGH$/);
  await waitForHydration(page);
  await expect(page.locator('input[name="next"]')).toHaveValue("/join/ABCDEFGH");
  expect(errors).toEqual([]);
});
