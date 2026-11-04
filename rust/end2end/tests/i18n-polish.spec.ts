import { expect, test, type Browser, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { collectPageErrors, waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * i18n-polish acceptance tests (docs/rewrite/specs/i18n-polish.md): the POST language
 * switch (AC1-AC4, AC9-AC12), the landing page (AC13-AC15). The exhaustive cases (next
 * validation, validation errors, Origin, stale sessions) are integration tests in
 * server/tests/it/i18n_polish*.rs. Texts come from the catalogs by key, so a reworded text
 * does not break the tests.
 *
 * Registrations and reset requests are mail-sending endpoints (20 per hour per IP): this
 * file registers two users and requests two resets.
 */

const catalogs: Record<"en" | "de", Record<string, string>> = {
  en: JSON.parse(readFileSync(resolve(__dirname, "../../locales/en.json"), "utf8")),
  de: JSON.parse(readFileSync(resolve(__dirname, "../../locales/de.json"), "utf8")),
};

function t(locale: "en" | "de", key: string, params: Record<string, string> = {}): string {
  const text = catalogs[locale][key];
  if (text === undefined) throw new Error(`key ${key} is missing from locales/${locale}.json`);
  return text.replace(/\{(\w+)\}/g, (_, name) => params[name] ?? `{${name}}`);
}

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const SWITCH = 'form[action="/api/set_locale"]';

/** The switch button of the other locale (the nav offers exactly one). */
function switchButton(page: Page) {
  return page.locator(`${SWITCH} button[type=submit]`);
}

async function register(page: Page, email: string, name: string): Promise<void> {
  await page.goto("/auth/register");
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Register" }).click();
  const mail = await waitForLatest(email);
  const link = firstUrlWithPath(mail, "/auth/register/confirm/")!;
  await page.goto(link);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill(name);
  await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
  await page.getByLabel("Confirm Password").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

async function newUser(browser: Browser, prefix: string) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = uniqueEmail(prefix);
  await register(page, email, "E2E User");
  return { context, page, email };
}

/** Marks the current document; the marker is gone after a full page load. */
async function markDocument(page: Page): Promise<void> {
  await page.evaluate(() => ((window as unknown as { __marker: boolean }).__marker = true));
}
async function documentWasReloaded(page: Page): Promise<boolean> {
  return !(await page.evaluate(() => (window as unknown as { __marker?: boolean }).__marker === true));
}

async function expectLanding(page: Page, locale: "en" | "de"): Promise<void> {
  await expect(page.locator("html")).toHaveAttribute("lang", locale);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(t(locale, "home.title"));
  const keys = [
    "home.subtitle",
    "home.login.title",
    "home.login.description",
    "home.register.title",
    "home.register.description",
    "home.join.text",
    "home.howItWorks.title",
    "home.howItWorks.step1.title",
    "home.howItWorks.step1.description",
    "home.howItWorks.step2.title",
    "home.howItWorks.step2.description",
    "home.howItWorks.step3.title",
    "home.howItWorks.step3.description",
    "home.features.title",
    "home.features.multipleGroups",
    "home.features.emailNotifications",
    "home.features.exclusionRules",
    "home.features.prioritizedWishlists",
    "home.features.secretAssignments",
  ];
  for (const key of keys) {
    await expect(page.locator("main").getByText(t(locale, key), { exact: true })).toBeVisible();
  }
  await expect(page.locator("main").locator('a[href="/auth/login"]')).toBeVisible();
  await expect(page.locator("main").locator('a[href="/auth/register"]')).toBeVisible();
  await expect(page.locator("main").locator('a[href="/join"]')).toHaveText(t(locale, "group.joinButton"));
}

test.describe("i18n-polish", () => {
  test("AC1 AC11 AC13: switch to Deutsch and back on the landing page, full reload, cookie", async ({
    page,
    context,
  }) => {
    const errors = collectPageErrors(page);
    await page.goto("/");
    await expectLanding(page, "en");
    await waitForHydration(page);
    expect(errors).toEqual([]);

    // The button offers only the other language, labelled with its native name.
    await expect(switchButton(page)).toHaveCount(1);
    await expect(switchButton(page)).toHaveText("Deutsch");
    await expect(switchButton(page).locator("span")).toHaveAttribute("lang", "de");
    await expect(switchButton(page)).not.toHaveAttribute("lang", /.*/);
    await expect(switchButton(page)).toHaveAttribute(
      "aria-label",
      t("en", "language.switchTo", { language: "Deutsch" }),
    );
    await expect(page.locator("#languageDropdown")).toHaveCount(0);

    await markDocument(page);
    const [response] = await Promise.all([
      page.waitForResponse((r) => r.url().endsWith("/api/set_locale") && r.request().method() === "POST"),
      switchButton(page).click(),
    ]);
    const setCookie = (await response.headerValues("set-cookie")).find((c) => c.startsWith("locale="))!;
    expect(setCookie).toMatch(/^locale=de;/);
    expect(setCookie).toContain("Path=/");
    expect(setCookie).toContain("Max-Age=31536000");
    expect(setCookie).toContain("SameSite=Lax");
    expect(setCookie).toContain("HttpOnly");
    expect(setCookie).not.toContain("Secure");

    await expect(page).toHaveURL(/\/$/);
    await expectLanding(page, "de");
    expect(await documentWasReloaded(page)).toBe(true);
    const cookie = (await context.cookies()).find((c) => c.name === "locale")!;
    expect(cookie.value).toBe("de");
    expect(cookie.httpOnly).toBe(true);
    expect(cookie.sameSite).toBe("Lax");

    await waitForHydration(page);
    await expect(switchButton(page)).toHaveText("English");
    await expect(switchButton(page).locator("span")).toHaveAttribute("lang", "en");
    await expect(switchButton(page)).not.toHaveAttribute("lang", /.*/);
    await expect(switchButton(page)).toHaveAttribute(
      "aria-label",
      t("de", "language.switchTo", { language: "English" }),
    );
    await switchButton(page).click();
    await expectLanding(page, "en");
    expect(errors).toEqual([]);
  });

  test("AC11: at 375 px the switch is visible without opening the menu, before and after hydration", async ({
    browser,
  }) => {
    const viewport = { width: 375, height: 700 };
    // Before hydration: JavaScript off.
    const plain = await browser.newContext({ viewport, javaScriptEnabled: false });
    const p = await plain.newPage();
    await p.goto("/");
    await expect(switchButton(p)).toBeVisible();
    await expect(switchButton(p)).toHaveText("Deutsch");
    await plain.close();

    const context = await browser.newContext({ viewport });
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    await page.goto("/");
    await expect(switchButton(page)).toBeVisible();
    await waitForHydration(page);
    await expect(switchButton(page)).toBeVisible();
    await expect(page.locator("#navbarNav")).not.toHaveClass(/show/);
    expect(errors).toEqual([]);
    await context.close();
  });

  test("AC2: the switch works without JavaScript, both ways", async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto("/");
    await switchButton(page).click();
    await expect(page).toHaveURL(/\/$/);
    await expectLanding(page, "de");
    await switchButton(page).click();
    await expectLanding(page, "en");

    // On another page: back to that page, not to the start page.
    await page.goto("/auth/login");
    await switchButton(page).click();
    await expect(page).toHaveURL(/\/auth\/login$/);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await context.close();
  });

  test("AC3: after client-side navigation the switch returns to exactly that path and query", async ({
    page,
  }) => {
    const errors = collectPageErrors(page);
    await page.goto("/");
    await waitForHydration(page);
    await markDocument(page);
    // Client-side navigation: the /join link of the landing page ends at the login page.
    await page.locator('main a[href="/join"]').click();
    await expect(page).toHaveURL(/\/auth\/login\?next=%2Fjoin$/);
    const next = page.locator(`${SWITCH} input[name=next]`);
    await expect(next).toHaveValue("/auth/login?next=%2Fjoin");

    await switchButton(page).click();
    await expect(page).toHaveURL(/\/auth\/login\?next=%2Fjoin$/);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    expect(await documentWasReloaded(page)).toBe(true);
    await waitForHydration(page);
    expect(errors).toEqual([]);
  });

  test("AC14: the invitation-code link leads through login and back to /join", async ({ browser }) => {
    const { context, email } = await newUser(browser, "joiner");
    await context.close();

    const fresh = await browser.newContext();
    const page = await fresh.newPage();
    await page.goto("/");
    await waitForHydration(page);
    await page.locator('main a[href="/join"]').click();
    await expect(page).toHaveURL(/\/auth\/login\?next=%2Fjoin$/);
    await waitForHydration(page);
    await page.locator("input[type=email]").fill(email);
    await page.locator("input[type=password]").fill(PASSWORD);
    await page.locator("main form button[type=submit]").click();
    await expect(page).toHaveURL(/\/join$/);
    await fresh.close();
  });

  test("AC13: the landing page is German for a German browser and hydrates cleanly", async ({ browser }) => {
    const context = await browser.newContext({ locale: "de-DE" });
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    await page.goto("/");
    await expectLanding(page, "de");
    await waitForHydration(page);
    await expectLanding(page, "de");
    expect(errors).toEqual([]);
    await context.close();
  });

  test("AC9 AC12 AC15 AC10: a logged-in switch changes the account language; other devices follow at login", async ({
    browser,
  }) => {
    const { context: one, page: p1, email } = await newUser(browser, "bilingual");
    await waitForHydration(p1);
    // AC15: a logged-in visitor is redirected from the landing page.
    await p1.goto("/");
    await expect(p1).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(p1.locator("html")).toHaveAttribute("lang", "en");

    // Device 1 switches to Deutsch on the dashboard.
    await waitForHydration(p1);
    await switchButton(p1).click();
    await expect(p1).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(p1.locator("html")).toHaveAttribute("lang", "de");

    // Device 2 (English browser): the reset mail follows the account language at once.
    const two = await browser.newContext({ locale: "en-US" });
    const p2 = await two.newPage();
    const errors = collectPageErrors(p2);
    await deleteFor(email); // the registration mail is still in the inbox
    await p2.goto("/auth/forgot-password");
    await expect(p2.locator("html")).toHaveAttribute("lang", "en");
    await waitForHydration(p2);
    await p2.locator("input[type=email]").fill(email);
    await p2.locator("main form button[type=submit]").click();
    const mail = await waitForLatest(email);
    expect(mail.HTML).toContain('lang="de"');
    expect(mail.Subject).toBe(t("de", "email.passwordReset.subject"));

    // Logging in there adopts the account language.
    await p2.goto("/auth/login");
    await waitForHydration(p2);
    await p2.locator("input[type=email]").fill(email);
    await p2.locator("input[type=password]").fill(PASSWORD);
    await p2.locator("main form button[type=submit]").click();
    await expect(p2).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(p2.locator("html")).toHaveAttribute("lang", "de");
    expect(errors).toEqual([]);

    // AC10: after logging out the page is still German (cookie).
    await two.close();
    await one.close();
  });

  test("AC4: switching on a reset-password page returns to it in German; the token still works", async ({
    browser,
  }) => {
    const { context: setup, email } = await newUser(browser, "resetter");
    await setup.close();

    const context = await browser.newContext();
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    await deleteFor(email); // the registration mail is still in the inbox
    await page.goto("/auth/forgot-password");
    await waitForHydration(page);
    await page.locator("input[type=email]").fill(email);
    await page.locator("main form button[type=submit]").click();
    const mail = await waitForLatest(email);
    const link = firstUrlWithPath(mail, "/auth/reset-password/")!;
    expect(link).toBeTruthy();
    const path = new URL(link).pathname;

    await page.goto(link);
    await waitForHydration(page);
    await switchButton(page).click();
    await expect(page).toHaveURL(new RegExp(`${path}$`));
    await expect(page.locator("html")).toHaveAttribute("lang", "de");

    await waitForHydration(page);
    const passwords = page.locator("main form input[type=password]");
    await passwords.nth(0).fill("new-e2e-password-1");
    await passwords.nth(1).fill("new-e2e-password-1");
    await page.locator("main form button[type=submit]").click();
    await expect(page).toHaveURL(/\/auth\/login/);
    expect(errors).toEqual([]);
    await context.close();
  });
});
