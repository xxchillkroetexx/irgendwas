import { expect, test, type Browser, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, latestFor, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * auth slice acceptance tests (docs/rewrite/specs/auth.md). Only the main journeys and
 * everything that depends on hydration or redirects; the exhaustive cases (validation,
 * rate limits, token edge cases, mail content) are integration tests in server/tests/auth_*.rs.
 *
 * Every test registers its own user through the UI with a unique address. Mail-sending
 * endpoints are limited to 20 calls per hour per IP, so keep the number of register /
 * forgot-password calls in this file low.
 *
 * The protected page used is the dashboard: it is the only protected page that exists
 * before the later slices.
 */

/**
 * Runs `step`, a deliberately failing request (wrong password: 401, validation: 422, rate
 * limit: 429), and removes from `errors` at most one console line it caused: Chromium's
 * "Failed to load resource ... status of 401|422|429". Everything else logged during the
 * step, and everything outside it, stays in `errors` and fails the test at its end.
 */
async function expectingFailedRequest(errors: string[], step: () => Promise<void>): Promise<void> {
  const before = errors.length;
  await step();
  const expected = /^console: Failed to load resource: the server responded with a status of (401|422|429)\b/;
  const during = errors.splice(before);
  const index = during.findIndex((e) => expected.test(e));
  if (index >= 0) during.splice(index, 1);
  errors.push(...during);
}

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";

// English texts (PHP catalog, unchanged keys) and the spec's revised texts.
const T = {
  loggedIn: "You have successfully logged in",
  lastLogin: /Your last login was on \d{1,2} \w{3} \d{4}, \d{2}:\d{2}/,
  loggedOut: "You have been successfully logged out",
  invalidCredentials: "Invalid email or password",
  loginRequired: "Please log in to access this page",
  registrationInstructions: "Check your inbox: we have sent you an email to continue.",
  resetInstructions: "If your email exists in our system, you will receive password reset instructions",
  resetDone: "Your password has been reset, you can now login",
  resetInvalid: "The password reset link has expired or is invalid. Please request a new one.",
  linkInvalid: "registration link", // case-insensitive fragment of flash.error.registration_link_invalid
};

/** Step 1 of registration up to the mail; returns the confirmation link. */
async function startRegistration(page: Page, email: string, url = "/auth/register"): Promise<string> {
  await page.goto(url);
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Register" }).click();
  await expect(page.getByText(T.registrationInstructions)).toBeVisible();
  const mail = await waitForLatest(email);
  const link = firstUrlWithPath(mail, "/auth/register/confirm/");
  expect(link, "registration mail has a confirmation link").toBeTruthy();
  return link!;
}

/** Step 2: name + password on the confirmation page. */
async function completeRegistration(page: Page, link: string, name: string, password = PASSWORD) {
  await page.goto(link);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill(name);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByLabel("Confirm Password").fill(password);
  await page.locator("main form button[type=submit]").click();
}

async function register(page: Page, email: string, name: string): Promise<void> {
  const link = await startRegistration(page, email);
  await completeRegistration(page, link, name);
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

async function logout(page: Page): Promise<void> {
  // The button lives in the user dropdown; dispatch the click so it works with or
  // without Bootstrap's dropdown JS (loaded from a CDN).
  await page.locator("nav button", { hasText: "Logout" }).dispatchEvent("click");
  await expect(page).toHaveURL(/\/$/);
}

async function login(page: Page, email: string, password: string, url = "/auth/login") {
  await page.goto(url);
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Login" }).click();
}

async function newUser(browser: Browser, prefix: string) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = uniqueEmail(prefix);
  await register(page, email, "E2E User");
  return { context, page, email };
}

test.describe("auth", () => {
  test("AC21 AC24 AC25 AC17 AC1: register by mail link, log out, log back in", async ({ page }) => {
    const errors = collectPageErrors(page);
    const email = uniqueEmail("carol");

    // Step 1: only an email; the same "check your inbox" state for everyone.
    const link = await startRegistration(page, email.toUpperCase());

    // Step 2: the page names the address; opening it twice does not consume the token.
    await page.goto(link);
    await expect(page.getByText(email)).toBeVisible();
    await page.goto(link);
    await waitForHydration(page);
    await expect(page.getByText(email)).toBeVisible();
    await page.getByLabel("Full Name").fill("  Carol  ");
    await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await page.getByLabel("Confirm Password").fill(PASSWORD);
    await page.locator("main form button[type=submit]").click();

    // Logged in on the dashboard, name in the navbar.
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(page.locator("nav").getByText("Carol")).toBeVisible();

    // #60: registration/login cookies persist when the browser/app is closed and reopened.
    const stored = await page.context().storageState();
    const session = stored.cookies.find((cookie) => cookie.name === "irgendwas_session");
    expect(session).toBeTruthy();
    expect(session!.expires).toBeGreaterThan(Date.now() / 1000 + 89 * 24 * 60 * 60);
    const reopened = await page.context().browser()!.newContext({ storageState: stored });
    const reopenedPage = await reopened.newPage();
    await reopenedPage.goto(DASHBOARD);
    await expect(reopenedPage).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(reopenedPage.locator("nav").getByText("Carol")).toBeVisible();
    await reopened.close();

    // Logout is a POST: back on the start page with the flash.
    await logout(page);
    await expect(page.getByText(T.loggedOut)).toBeVisible();
    await expect(page.locator("nav").getByRole("link", { name: "Login" })).toBeVisible();

    // The link cannot be used a second time.
    await page.goto(link);
    await expect(page).toHaveURL(/\/auth\/register$/);
    await expect(page.getByText(new RegExp(T.linkInvalid, "i"))).toBeVisible();

    // Wrong password: inline error, nothing else changes.
    await expectingFailedRequest(errors, async () => {
      await login(page, email, "not the password");
      await expect(page.getByText(T.invalidCredentials)).toBeVisible();
    });
    await expect(page).toHaveURL(/\/auth\/login/);

    // Right password (email typed in another case, with a space): dashboard + flash with the
    // previous login (registration completion set it).
    await login(page, ` ${email.toUpperCase()} `, PASSWORD);
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(page.getByText(T.lastLogin)).toBeVisible();

    expect(errors).toEqual([]);
  });

  test("AC6 AC29 AC8: protected page -> login with next -> register -> back to the page", async ({ page }) => {
    const errors = collectPageErrors(page);
    const email = uniqueEmail("dave");

    await page.goto(`${DASHBOARD}?via=invite`);
    await expect(page).toHaveURL(/\/auth\/login\?next=%2Fuser%2Fdashboard%3Fvia%3Dinvite$/);
    await expect(page.getByText(T.loginRequired)).toBeVisible();
    await waitForHydration(page);
    // No "remember me" control (AC10).
    await expect(page.locator("input[type=checkbox]")).toHaveCount(0);

    // "Register" (in the card, not the navbar) carries next through.
    await page.locator("main").getByRole("link", { name: "Register" }).click();
    await expect(page).toHaveURL(/\/auth\/register\?next=/);
    const mailRequested = page.getByLabel("Email");
    await waitForHydration(page);
    await mailRequested.fill(email);
    await page.getByRole("button", { name: "Register" }).click();
    await expect(page.getByText(T.registrationInstructions)).toBeVisible();

    // The link is opened in a different browser context: next lives server-side.
    const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
    const other = await page.context().browser()!.newContext();
    const otherPage = await other.newPage();
    await completeRegistration(otherPage, link, "Dave");
    await expect(otherPage).toHaveURL(new RegExp(`${DASHBOARD}\\?via=invite$`));

    // A logged-in visitor is bounced away from the login and register pages (AC8).
    await otherPage.goto("/auth/login");
    await expect(otherPage).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await otherPage.goto("/auth/register");
    await expect(otherPage).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await other.close();

    expect(errors).toEqual([]);
  });

  test("AC7: an open-redirect next is ignored after login", async ({ browser }) => {
    const { context, page, email } = await newUser(browser, "frank");
    await logout(page);
    await login(page, email, PASSWORD, `/auth/login?next=${encodeURIComponent("//evil.example/x")}`);
    await expect(page).toHaveURL(new RegExp(`^http://[^/]+${DASHBOARD}$`));
    await context.close();
  });

  test("AC33 AC36 AC38 AC37: forgot password, reset link, old sessions die", async ({ browser, page }) => {
    const errors = collectPageErrors(page);
    const { context: aContext, page: browserA, email } = await newUser(browser, "alice");

    // Browser B asks for a reset.
    await page.goto("/auth/forgot-password");
    await waitForHydration(page);
    await page.getByLabel("Email").fill(email);
    await page.getByRole("button", { name: "Send Reset Link" }).click();
    await expect(page).toHaveURL(/\/auth\/login/);
    await expect(page.getByText(T.resetInstructions)).toBeVisible();

    // An unknown address gets the same page (and no mail).
    const ghost = uniqueEmail("ghost");
    await page.goto("/auth/forgot-password");
    await waitForHydration(page);
    await page.getByLabel("Email").fill(ghost);
    await page.getByRole("button", { name: "Send Reset Link" }).click();
    await expect(page).toHaveURL(/\/auth\/login/);
    await expect(page.getByText(T.resetInstructions)).toBeVisible();
    expect(await latestFor(ghost)).toBeUndefined();

    const mail = await waitForLatest(email);
    const link = firstUrlWithPath(mail, "/auth/reset-password/")!;
    expect(link).toBeTruthy();

    // Reset with a new password.
    const fresh = "brand-new-password-2";
    await page.goto(link);
    await waitForHydration(page);
    await page.getByLabel("New Password", { exact: true }).fill(fresh);
    await page.getByLabel("Confirm New Password").fill(fresh);
    await page.getByRole("button", { name: "Reset Password" }).click();
    await expect(page).toHaveURL(/\/auth\/login/);
    await expect(page.getByText(T.resetDone)).toBeVisible();

    // The link is single-use.
    await page.goto(link);
    await expect(page).toHaveURL(/\/auth\/forgot-password$/);
    await expect(page.getByText(T.resetInvalid)).toBeVisible();

    // Old password is dead, new one works.
    await expectingFailedRequest(errors, async () => {
      await login(page, email, PASSWORD);
      await expect(page.getByText(T.invalidCredentials)).toBeVisible();
    });
    await login(page, email, fresh);
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));

    // Browser A (logged in before the reset) is logged out.
    await browserA.goto(DASHBOARD);
    await expect(browserA).toHaveURL(/\/auth\/login\?next=/);
    await aContext.close();

    expect(errors).toEqual([]);
  });

  test("AC18: GET /auth/logout is not routed and does not log out", async ({ browser }) => {
    const { context, page } = await newUser(browser, "gina");
    const res = await page.request.get("/auth/logout");
    expect(res.status()).toBe(404);
    await page.goto(DASHBOARD);
    await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await context.close();
  });

  test.describe("German browser", () => {
    test.use({ locale: "de-DE" });

    test("AC48 AC47: registering on a German page makes a German account", async ({ page }) => {
      const errors = collectPageErrors(page);
      const email = uniqueEmail("dora");

      await page.goto("/auth/register");
      await expect(page.locator("html")).toHaveAttribute("lang", "de");
      await waitForHydration(page);
      await page.locator("input[type=email]").fill(email);
      await page.locator("main form button[type=submit]").click();
      const mail = await waitForLatest(email);
      const link = firstUrlWithPath(mail, "/auth/register/confirm/")!;
      expect(mail.HTML).toContain('lang="de"');

      await page.goto(link);
      await waitForHydration(page);
      await page.locator("main form input[type=text]").fill("Dora");
      const passwords = page.locator("main form input[type=password]");
      await passwords.nth(0).fill(PASSWORD);
      await passwords.nth(1).fill(PASSWORD);
      await page.locator("main form button[type=submit]").click();
      await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
      await expect(page.locator("html")).toHaveAttribute("lang", "de");

      // A different browser (English) logging in adopts the account language.
      const english = await page.context().browser()!.newContext({ locale: "en-US" });
      const englishPage = await english.newPage();
      await englishPage.goto("/auth/login");
      await expect(englishPage.locator("html")).toHaveAttribute("lang", "en");
      await waitForHydration(englishPage);
      await englishPage.locator("input[type=email]").fill(email);
      await englishPage.locator("input[type=password]").fill(PASSWORD);
      await englishPage.locator("main form button[type=submit]").click();
      await expect(englishPage).toHaveURL(new RegExp(`${DASHBOARD}$`));
      await expect(englishPage.locator("html")).toHaveAttribute("lang", "de");
      await english.close();

      expect(errors).toEqual([]);
    });
  });

  test("token pages work without JavaScript (no Origin: null 403): register confirm, reset password", async ({
    browser,
  }) => {
    const email = uniqueEmail("nojs");
    const setup = await browser.newContext();
    const setupPage = await setup.newPage();
    const link = await startRegistration(setupPage, email);

    // Registration completion as a plain form post.
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await p.goto(link);
    await p.getByLabel("Full Name").fill("No JS");
    await p.getByLabel("Password", { exact: true }).fill(PASSWORD);
    await p.getByLabel("Confirm Password").fill(PASSWORD);
    await p.locator("main form button[type=submit]").click();
    await expect(p).toHaveURL(new RegExp(`${DASHBOARD}$`));
    await expect(p.locator("nav").getByText("No JS")).toBeVisible();

    // Password reset as a plain form post: success, back on the login page.
    // Clear the inbox first: the registration mail is still there, and the reset mail is
    // sent off the request path, so `waitForLatest` would otherwise return the old one.
    await deleteFor(email);
    await setupPage.goto("/auth/forgot-password");
    await waitForHydration(setupPage);
    await setupPage.getByLabel("Email").fill(email);
    await setupPage.getByRole("button", { name: "Send Reset Link" }).click();
    await expect(setupPage).toHaveURL(/\/auth\/login/);
    const resetMail = await waitForLatest(email);
    const resetLink = firstUrlWithPath(resetMail, "/auth/reset-password/")!;
    expect(resetLink).toBeTruthy();

    const fresh = "brand-new-password-2";
    const q = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
    await q.goto(resetLink);
    await q.getByLabel("New Password", { exact: true }).fill(fresh);
    await q.getByLabel("Confirm New Password").fill(fresh);
    await q.getByRole("button", { name: "Reset Password" }).click();
    await expect(q).toHaveURL(/\/auth\/login/);
    await expect(q.getByText(T.resetDone)).toBeVisible();

    await q.context().close();
    await plain.close();
    await setup.close();
  });

  test("auth pages render and hydrate without errors", async ({ page }) => {
    const errors = collectPageErrors(page);
    for (const path of ["/auth/login", "/auth/register", "/auth/forgot-password"]) {
      const res = await page.goto(path);
      expect(res?.status(), path).toBe(200);
      await waitForHydration(page);
    }
    expect(errors).toEqual([]);
  });
});
