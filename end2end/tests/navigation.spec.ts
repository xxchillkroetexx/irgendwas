import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { collectPageErrors, waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * Regression tests for issue #72: a logged-in user who navigates (client-side) to a page
 * for logged-out visitors (landing, login, registration) must be sent on without that page ever being
 * rendered, and the brand in the navbar leads a logged-in user straight to the dashboard.
 *
 * Registrations are mail-sending endpoints (20 per hour per IP): this file registers one
 * user.
 */

const en: Record<string, string> = JSON.parse(
  readFileSync(resolve(__dirname, "../../locales/en.json"), "utf8"),
);

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const BRAND = "nav a.navbar-brand";

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

/**
 * Records whether logged-out content ever shows up in the document, however briefly.
 */
async function watchForLoggedOutContent(page: Page): Promise<void> {
  await page.evaluate((landingText) => {
    const w = window as unknown as { __seen: string[] };
    w.__seen = [];
    const check = () => {
      const main = document.querySelector("main");
      if (main?.textContent?.includes(landingText) && !w.__seen.includes("landing")) w.__seen.push("landing");
      if (document.querySelector("#login-email") && !w.__seen.includes("login")) w.__seen.push("login");
      if (document.querySelector("#register-email") && !w.__seen.includes("register")) w.__seen.push("register");
    };
    check();
    new MutationObserver(check).observe(document.body, { childList: true, subtree: true, characterData: true });
  }, en["home.howItWorks.title"]);
}

async function seenLoggedOutContent(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __seen: string[] }).__seen);
}

/** Logs the paths the router pushes or replaces into the history. */
async function recordHistory(page: Page): Promise<void> {
  await page.evaluate(() => {
    const w = window as unknown as { __paths: string[] };
    w.__paths = [];
    for (const method of ["pushState", "replaceState"] as const) {
      const original = history[method].bind(history);
      history[method] = (data: unknown, unused: string, url?: string | URL | null) => {
        if (url != null) w.__paths.push(new URL(url, location.href).pathname);
        original(data, unused, url);
      };
    }
  });
}

async function historyPaths(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __paths: string[] }).__paths);
}

/**
 * Clicks a link to `href` added to the page (a client-side navigation via the router) and
 * waits until the router has gone there and then on to the dashboard.
 */
async function navigateClientSide(page: Page, href: string): Promise<void> {
  const before = (await historyPaths(page)).length;
  await page.evaluate((href) => {
    document.getElementById("e2e-nav-link")?.remove();
    const a = document.createElement("a");
    a.href = href;
    a.id = "e2e-nav-link";
    a.textContent = "e2e";
    document.body.appendChild(a);
  }, href);
  await page.locator("#e2e-nav-link").click();
  await expect
    .poll(async () => (await historyPaths(page)).slice(before))
    .toEqual(expect.arrayContaining([href, DASHBOARD]));
  const paths = (await historyPaths(page)).slice(before);
  expect(paths.lastIndexOf(DASHBOARD)).toBeGreaterThan(paths.indexOf(href));
}

/** The current document was not reloaded since `markDocument`. */
async function markDocument(page: Page): Promise<void> {
  await page.evaluate(() => ((window as unknown as { __marker: boolean }).__marker = true));
}
async function sameDocument(page: Page): Promise<boolean> {
  return page.evaluate(() => (window as unknown as { __marker?: boolean }).__marker === true);
}

test.describe("navigation (#72)", () => {
  test("logged in, the brand and logged-out pages never flash logged-out content", async ({
    browser,
  }) => {
    const context = await browser.newContext();
    const page = await context.newPage();
    const errors = collectPageErrors(page);

    // Logged out, the brand leads to the landing page.
    await page.goto("/");
    await waitForHydration(page);
    await expect(page.locator(BRAND)).toHaveAttribute("href", "/");

    const email = uniqueEmail("nav72");
    await register(page, email, "Nav User");

    // Logged in: rendered by the server and after hydration, the brand leads to the dashboard.
    await page.goto(DASHBOARD);
    await waitForHydration(page);
    await expect(page.locator(BRAND)).toHaveAttribute("href", DASHBOARD);
    await markDocument(page);
    await recordHistory(page);
    await watchForLoggedOutContent(page);

    // Clicking the brand stays on the dashboard, client-side.
    await page.locator(BRAND).click();
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));

    // A client-side navigation to logged-out pages goes on to the dashboard without
    // rendering the logged-out page first.
    await navigateClientSide(page, "/");
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(page.locator(BRAND)).toHaveAttribute("href", DASHBOARD);
    await navigateClientSide(page, "/auth/login");
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(page.locator(BRAND)).toHaveAttribute("href", DASHBOARD);
    await navigateClientSide(page, "/auth/register");
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(page.locator(BRAND)).toHaveAttribute("href", DASHBOARD);

    expect(await sameDocument(page), "the navigations were client-side").toBe(true);
    expect(await seenLoggedOutContent(page)).toEqual([]);
    expect(errors).toEqual([]);

    await deleteFor(email);
    await context.close();
  });
});

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
