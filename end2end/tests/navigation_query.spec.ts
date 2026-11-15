import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { clientNavigate, collectPageErrors, scrollPageTo, waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

const en: Record<string, string> = JSON.parse(
  readFileSync(resolve(__dirname, "../../locales/en.json"), "utf8"),
);
const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";

/** Matches a URL ending in `path` (path and query, taken literally). */
const endsWith = (path: string) => new RegExp(`${path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);

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

async function login(page: Page, email: string): Promise<void> {
  await page.goto("/auth/login");
  await waitForHydration(page);
  await page.locator("input[type=email]").fill(email);
  await page.locator("input[type=password]").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

/** Creates a group and returns its id. */
async function createGroup(page: Page, name: string): Promise<string> {
  await page.goto("/groups/new");
  await waitForHydration(page);
  await page.getByLabel(en["group.form.name"], { exact: true }).fill(name);
  await page.getByRole("button", { name: en["group.form.createButton"] }).click();
  await expect(page).toHaveURL(/\/groups\/\d+$/);
  return page.url().match(/\/groups\/(\d+)$/)![1];
}

/** Ends the context's session the way a logout in another tab does. */
async function logoutElsewhere(context: BrowserContext): Promise<void> {
  const other = await context.newPage();
  await other.goto("/groups");
  await waitForHydration(other);
  await other.locator("nav button", { hasText: "Logout" }).dispatchEvent("click");
  await expect(other).toHaveURL(/\/$/);
  await other.close();
}

/** Runs `step` after the session ended and drops the "status of 401" lines Chromium logs for its rejected requests. */
async function withoutSession(errors: string[], step: () => Promise<void>): Promise<void> {
  const before = errors.length;
  await step();
  const rejected = /^console: Failed to load resource: the server responded with a status of 401\b/;
  errors.push(...errors.splice(before).filter((e) => !rejected.test(e)));
}

/** Marks `selector` and focuses it without scrolling, so a remount (a new element) is detectable. */
async function markAndFocus(page: Page, selector: string): Promise<void> {
  await page.locator(selector).evaluate((input: HTMLInputElement) => {
    input.dataset.guardMarker = "original";
    input.focus({ preventScroll: true });
  });
}

test("query-only navigation preserves forms; path changes still recheck login (#81)", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = collectPageErrors(page);
  const email = uniqueEmail("nav81");
  try {
    // A query-only change on the login page must not replace its typed input.
    await page.goto("/auth/login?next=%2Fgroups");
    await waitForHydration(page);
    const loginEmail = page.locator("#login-email");
    await loginEmail.fill("typed@example.test");
    await markAndFocus(page, "#login-email");
    await clientNavigate(page, "/auth/login");
    await expect(page).toHaveURL(/\/auth\/login$/);
    await expect(loginEmail).toHaveValue("typed@example.test");
    await expect(loginEmail).toHaveAttribute("data-guard-marker", "original");
    await expect(loginEmail).toBeFocused();

    // Leaving a public page sends no login check whose result nobody uses.
    const checks: string[] = [];
    page.on("request", (request) => {
      if (new URL(request.url()).pathname.startsWith("/api/current_user")) checks.push(request.url());
    });
    await clientNavigate(page, "/install");
    await expect(page.getByRole("heading", { level: 1, name: en["app.install.title"] })).toBeVisible();
    expect(checks).toEqual([]);

    await register(page, email, "Guard User");
    await page.goto("/groups/new");
    await waitForHydration(page);
    const groupName = page.getByLabel(en["group.form.name"], { exact: true });
    await groupName.fill("Unsubmitted name");
    await scrollPageTo(page, 200);
    await markAndFocus(page, "#group-name");
    await page.evaluate(() => {
      const w = window as unknown as { __guardLoading: boolean };
      w.__guardLoading = false;
      new MutationObserver(() => {
        if (document.querySelector("main .spinner-border")) w.__guardLoading = true;
      }).observe(document.querySelector("main")!, { childList: true, subtree: true });
    });
    // Normal links deliberately scroll to the top in Leptos Router. Suppress that
    // independent behavior so this checks whether the guard remounts.
    await clientNavigate(page, "/groups/new?probe=one", true);
    await expect(page).toHaveURL(/\/groups\/new\?probe=one$/);
    await expect(groupName).toHaveValue("Unsubmitted name");
    await expect(groupName).toHaveAttribute("data-guard-marker", "original");
    await expect(groupName).toBeFocused();
    expect(await page.evaluate(() => window.scrollY)).toBe(200);
    expect(await page.evaluate(() => (window as unknown as { __guardLoading: boolean }).__guardLoading)).toBe(false);

    // Logout in another tab revokes the session. A real path change must check
    // again, and its redirect must retain the destination query.
    await logoutElsewhere(context);
    await withoutSession(errors, async () => {
      await clientNavigate(page, "/user/profile?source=guard");
      await expect(page).toHaveURL(/\/auth\/login\?next=%2Fuser%2Fprofile%3Fsource%3Dguard$/);
    });
    expect(errors).toEqual([]);
  } finally {
    await deleteFor(email);
    await context.close();
  }
});

test("wishlist edit, cancel and save keep the page, unsaved input and scroll position (#81)", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 600 } });
  const page = await context.newPage();
  const errors = collectPageErrors(page);
  const email = uniqueEmail("nav81-edit");
  try {
    await register(page, email, "Edit User");
    const group = await createGroup(page, "Navigation group");
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
    await scrollPageTo(page, 200);
    await markAndFocus(page, "#add-title");

    // Click the actual editor links without Playwright scrolling them into view first.
    // Opening the form focuses its title (#69), which may scroll it into view; nothing
    // else may move the page.
    await edit.evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page).toHaveURL(/\?edit=\d+$/);
    await expect(draft).toHaveValue("Unsubmitted next wish");
    await expect(draft).toHaveAttribute("data-guard-marker", "original");
    const inlineTitle = page.locator('input[id^="edit-"][id$="-title"]');
    await expect(inlineTitle).toBeFocused();
    await expect(inlineTitle).toBeInViewport();
    const opened = await page.evaluate(() => window.scrollY);
    expect(opened).toBeGreaterThan(0);
    await page.getByRole("link", { name: en["wishlist.edit.cancel"] }).evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page).toHaveURL(new RegExp(`/groups/${group}/wishlist$`));
    await expect(draft).toHaveValue("Unsubmitted next wish");
    await expect(draft).toHaveAttribute("data-guard-marker", "original");
    await expect(inlineTitle).toHaveCount(0);
    expect(await page.evaluate(() => window.scrollY)).toBe(opened);

    // Saving keeps the scroll position too; its confirmation must still be in view.
    await edit.evaluate((link: HTMLAnchorElement) => link.click());
    await expect(page).toHaveURL(/\?edit=\d+$/);
    await page.getByRole("button", { name: en["wishlist.edit.save"] }).evaluate((button: HTMLButtonElement) => button.click());
    await expect(page).toHaveURL(new RegExp(`/groups/${group}/wishlist$`));
    await expect(page.getByRole("alert").filter({ hasText: en["flash.success.item_updated"] })).toBeInViewport();
    // The stuck alert stays below the navbar's user menu and keeps focused elements clear.
    const layering = await page.evaluate(() => ({
      flash: Number(getComputedStyle(document.querySelector(".flash-messages")!).zIndex),
      // Bootstrap's dropdown layer (the menu itself is static in the collapsed mobile navbar).
      menu: Number(getComputedStyle(document.querySelector("nav .dropdown-menu")!).getPropertyValue("--bs-dropdown-zindex")),
      padding: getComputedStyle(document.documentElement).scrollPaddingTop,
    }));
    expect(layering.flash).toBeLessThan(layering.menu);
    expect(layering.padding).not.toBe("auto");
    expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
    expect(errors).toEqual([]);
  } finally {
    await deleteFor(email);
    await context.close();
  }
});

test("a query-only change after the session ended goes to the login page (#81)", async ({ browser }) => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const errors = collectPageErrors(page);
  const email = uniqueEmail("nav81-expired");
  try {
    await register(page, email, "Expired User");
    const source = await createGroup(page, "Source group");
    await page.goto(`/groups/${source}/wishlist`);
    await waitForHydration(page);
    await page.locator("#add-title").fill("Source wish");
    await page.getByRole("button", { name: en["wishlist.edit.addItem"] }).click();
    await expect(page.locator(".list-group-item .fw-semibold").getByText("Source wish", { exact: true })).toBeVisible();
    const target = await createGroup(page, "Target group");

    // Choosing a copy source changes only the query: the guard does not check again,
    // so the page itself must send the visitor to the login page (and back here).
    await page.goto(`/groups/${target}/wishlist`);
    await waitForHydration(page);
    await logoutElsewhere(context);
    await withoutSession(errors, async () => {
      await page.getByLabel(en["wishlist.copy.source"]).selectOption(source);
      await page.getByRole("button", { name: en["wishlist.copy.load"] }).click();
      await expect(page).toHaveURL(
        endsWith(`/auth/login?next=${encodeURIComponent(`/groups/${target}/wishlist?source=${source}`)}`),
      );
    });

    // The shared-wish page reloads its data when only `?id=` changes.
    await login(page, email);
    await page.goto("/wishes/shared");
    await waitForHydration(page);
    await logoutElsewhere(context);
    await withoutSession(errors, async () => {
      await clientNavigate(page, "/wishes/shared?id=probe");
      await expect(page).toHaveURL(endsWith(`/auth/login?next=${encodeURIComponent("/wishes/shared?id=probe")}`));
    });
    expect(errors).toEqual([]);
  } finally {
    await deleteFor(email);
    await context.close();
  }
});
