import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

const en: Record<string, string> = JSON.parse(
  readFileSync(resolve(__dirname, "../../locales/en.json"), "utf8"),
);
const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";

async function register(page: Page, email: string, name: string): Promise<void> {
  await page.goto("/auth/register");
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Register" }).click();
  const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
  await page.goto(link);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill(name);
  await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
  await page.getByLabel("Confirm Password").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

test("query-only navigation preserves forms; path changes still recheck login (#81)", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = uniqueEmail("nav81");

  // A query-only change on the login page must not replace its typed input.
  await page.goto("/auth/login?next=%2Fgroups");
  await waitForHydration(page);
  const loginEmail = page.locator("#login-email");
  await loginEmail.fill("typed@example.test");
  await loginEmail.focus();
  await page.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>("#login-email")!;
    input.dataset.guardMarker = "original";
    const a = document.createElement("a");
    a.href = "/auth/login";
    document.body.appendChild(a);
    a.click();
    a.remove();
  });
  await expect(page).toHaveURL(/\/auth\/login$/);
  await expect(loginEmail).toHaveValue("typed@example.test");
  await expect(loginEmail).toHaveAttribute("data-guard-marker", "original");
  await expect(loginEmail).toBeFocused();

  await register(page, email, "Guard User");
  await page.goto("/groups/new");
  await waitForHydration(page);
  const groupName = page.getByLabel(en["group.form.name"], { exact: true });
  await groupName.fill("Unsubmitted name");
  await page.evaluate(async () => {
    // The app body is a flex column: prevent this artificial scroll area from
    // shrinking to the viewport and wait until the browser has laid it out.
    const spacer = document.createElement("div");
    spacer.style.height = "2000px";
    spacer.style.flexShrink = "0";
    document.body.appendChild(spacer);
    // Bootstrap enables smooth scrolling; use an immediate scroll so the
    // before/after assertion observes a settled, nonzero position.
    document.documentElement.style.scrollBehavior = "auto";
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    window.scrollTo(0, 200);
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  });
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(200);
  await page.evaluate(() => {
    const input = document.querySelector<HTMLInputElement>("#group-name")!;
    input.dataset.guardMarker = "original";
    input.focus({ preventScroll: true });
    const w = window as unknown as { __guardLoading: boolean; __guardScroll: number };
    w.__guardLoading = false;
    w.__guardScroll = window.scrollY;
    new MutationObserver(() => {
      if (document.querySelector("main .spinner-border")) w.__guardLoading = true;
    }).observe(document.querySelector("main")!, { childList: true, subtree: true });
    const a = document.createElement("a");
    a.href = "/groups/new?probe=one";
    // Normal links deliberately scroll to the top in Leptos Router. Suppress
    // that independent behavior so this checks whether the guard remounts.
    a.setAttribute("data-noscroll", "");
    document.body.appendChild(a);
    a.click();
    a.remove();
  });
  await expect(page).toHaveURL(/\/groups\/new\?probe=one$/);
  await expect(groupName).toHaveValue("Unsubmitted name");
  await expect(groupName).toHaveAttribute("data-guard-marker", "original");
  await expect(groupName).toBeFocused();
  const scroll = await page.evaluate(() => ({
    before: (window as unknown as { __guardScroll: number }).__guardScroll,
    after: window.scrollY,
  }));
  expect(scroll.before).toBeGreaterThan(0);
  expect(scroll.after).toBe(scroll.before);
  expect(await page.evaluate(() => (window as unknown as { __guardLoading: boolean }).__guardLoading)).toBe(false);

  // Logout in another tab revokes the session. A real path change must check
  // again, and its redirect must retain the destination query.
  const other = await context.newPage();
  await other.goto("/groups");
  await waitForHydration(other);
  await other.locator("nav button", { hasText: "Logout" }).dispatchEvent("click");
  await expect(other).toHaveURL(/\/$/);
  await page.evaluate(() => {
    const a = document.createElement("a");
    a.href = "/user/profile?source=guard";
    document.body.appendChild(a);
    a.click();
    a.remove();
  });
  await expect(page).toHaveURL(/\/auth\/login\?next=%2Fuser%2Fprofile%3Fsource%3Dguard$/);
  await deleteFor(email);
  await context.close();
});

test("wishlist edit and cancel keep the page, unsaved input and scroll position (#81)", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 600 } });
  const page = await context.newPage();
  const email = uniqueEmail("nav81-edit");
  try {
    await register(page, email, "Edit User");
    await page.goto("/groups/new");
    await waitForHydration(page);
    await page.getByLabel(en["group.form.name"], { exact: true }).fill("Navigation group");
    await page.getByRole("button", { name: en["group.form.createButton"] }).click();
    await expect(page).toHaveURL(/\/groups\/\d+$/);
    const group = page.url().match(/\/groups\/(\d+)$/)![1];
    await page.goto(`/groups/${group}/wishlist`);
    await waitForHydration(page);
    const draft = page.locator("#add-title");
    await draft.fill("Existing wish");
    await page.getByRole("button", { name: en["wishlist.edit.addItem"] }).click();
    const edit = page.locator('a[href*="?edit="]').first();
    await expect(edit).toBeVisible();
    // Consume the add-success flash first; removing a banner later would itself
    // shift the document's natural scroll anchor, independently of the router.
    await page.reload();
    await waitForHydration(page);
    await expect(edit).toBeVisible();
    await draft.fill("Unsubmitted next wish");
    await page.evaluate(async () => {
      const spacer = document.createElement("div");
      spacer.style.height = "2000px";
      spacer.style.flexShrink = "0";
      document.body.appendChild(spacer);
      document.documentElement.style.scrollBehavior = "auto";
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      window.scrollTo(0, 200);
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const input = document.querySelector<HTMLInputElement>("#add-title")!;
      input.dataset.guardMarker = "original";
      input.focus({ preventScroll: true });
    });
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(200);

    // Click the actual editor links without moving focus as a pointer click would.
    await edit.evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page).toHaveURL(/\?edit=\d+$/);
    await expect(draft).toHaveValue("Unsubmitted next wish");
    await expect(draft).toHaveAttribute("data-guard-marker", "original");
    await expect(draft).toBeFocused();
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(200);
    await page.getByRole("link", { name: en["wishlist.edit.cancel"] }).evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page).toHaveURL(new RegExp(`/groups/${group}/wishlist$`));
    await expect(draft).toHaveValue("Unsubmitted next wish");
    await expect(draft).toHaveAttribute("data-guard-marker", "original");
    await expect(draft).toBeFocused();
    await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(200);
  } finally {
    await deleteFor(email);
    await context.close();
  }
});
