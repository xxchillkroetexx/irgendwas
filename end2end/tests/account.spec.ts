import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type Locator, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, message, messagesFor, uniqueEmail, waitForLatest, type Mail } from "./support/mailpit";

/**
 * account slice acceptance tests (docs/specs/account.md). Only the main journeys
 * and what depends on hydration or redirects; the exhaustive cases are integration tests
 * in server/tests/account_*.rs.
 *
 * Texts are looked up in the English catalog by key (`t("user.profile.updateProfile")`), so
 * the spec pins keys, not wording. Mail-sending endpoints are limited per IP and hour, so
 * each test registers one user and the file stays at three registrations.
 */

const CATALOG: Record<string, string> = JSON.parse(
  readFileSync(resolve(__dirname, "../../locales/en.json"), "utf8"),
);

/** English text of `key` with `{name}` placeholders replaced. */
function t(key: string, params: Record<string, string> = {}): string {
  const text = CATALOG[key];
  if (text === undefined) throw new Error(`key ${key} is missing from locales/en.json`);
  return Object.entries(params).reduce((acc, [k, v]) => acc.replaceAll(`{${k}}`, v), text);
}

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const PROFILE = "/user/profile";

/**
 * Runs `step`, a deliberately failing request (wrong password, throttle, bad token: any
 * 4xx), and removes from `errors` at most one console line it caused: Chromium's "Failed to
 * load resource ... status of 4xx". Everything else logged during the step, and everything
 * outside it, stays in `errors` and fails the test at its end.
 */
async function expectingFailedRequest(errors: string[], step: () => Promise<void>): Promise<void> {
  const before = errors.length;
  await step();
  const expected = /^console: Failed to load resource: the server responded with a status of 4\d\d\b/;
  const during = errors.splice(before);
  const index = during.findIndex((e) => expected.test(e));
  if (index >= 0) during.splice(index, 1);
  errors.push(...during);
}

async function register(page: Page, email: string, name: string): Promise<void> {
  await page.goto("/auth/register");
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Register" }).click();
  await expect(page.getByText("Check your inbox")).toBeVisible();
  const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
  expect(link, "registration mail has a link").toBeTruthy();
  await page.goto(link);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill(name);
  await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
  await page.getByLabel("Confirm Password").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

async function login(page: Page, email: string, password: string, url = "/auth/login") {
  await page.goto(url);
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Login" }).click();
}

async function logout(page: Page): Promise<void> {
  await page.locator("nav button", { hasText: "Logout" }).dispatchEvent("click");
  await expect(page).toHaveURL(/\/$/);
}

async function newUser(browser: Browser, prefix: string, name = "E2E User") {
  const context = await browser.newContext();
  const page = await context.newPage();
  const email = uniqueEmail(prefix);
  await register(page, email, name);
  return { context, page, email };
}

/** The profile card whose title is `titleKey`. */
function card(page: Page, titleKey: string): Locator {
  return page.locator(".card").filter({ has: page.getByText(t(titleKey), { exact: true }) });
}

/** Polls until a mail with `subject` has arrived at `address`. */
async function waitForSubject(address: string, subject: string, timeoutMs = 10_000): Promise<Mail> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const hit = (await messagesFor(address)).find((m) => m.Subject === subject);
    if (hit) return message(hit.ID);
    if (Date.now() > deadline) throw new Error(`no mail "${subject}" to ${address} within ${timeoutMs} ms`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

test.describe("account", () => {
  test("AC1 AC2 AC6 AC8 AC10 AC20 AC21 AC23 AC25 AC28: dashboard, profile, name, pending email change", async ({
    browser,
  }) => {
    const { context, page, email } = await newUser(browser, "ann", "Ann <b>Bold</b>");
    const errors = collectPageErrors(page);

    // AC6: the name is text, never markup; subtitle and tips are there.
    const welcome = t("user.dashboard.welcome", { name: "Ann <b>Bold</b>" });
    await expect(page.locator("main h1")).toHaveText(welcome);
    await expect(page.locator("main b")).toHaveCount(0);
    await expect(page.getByText(t("user.dashboard.subtitle"))).toBeVisible();
    for (const key of ["createWishlist", "inviteFriends", "setExclusions"]) {
      await expect(page.getByText(t(`user.dashboard.tips.${key}`))).toBeVisible();
    }
    await expect(page.getByText("Share your group's invitation code with friends and family")).toBeVisible();

    // AC2: logged out, the profile sends to login and back.
    await logout(page);
    await page.goto(PROFILE);
    await expect(page).toHaveURL(/\/auth\/login\?next=%2Fuser%2Fprofile$/);
    await waitForHydration(page);
    await page.getByLabel("Email").fill(email);
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Login" }).click();
    await expect(page).toHaveURL(new RegExp(`${PROFILE}$`));
    await waitForHydration(page);

    // AC8: current name and email, no pending box.
    await expect(page.getByLabel(t("user.profile.name"))).toHaveValue("Ann <b>Bold</b>");
    await expect(page.getByText(email).first()).toBeVisible();
    await expect(page.getByRole("button", { name: t("user.profile.changeEmail.cancel") })).toHaveCount(0);

    // AC10: trimmed name, success message, nav updates without a reload.
    await page.evaluate(() => ((window as unknown as { __marker: number }).__marker = 1));
    await page.getByLabel(t("user.profile.name")).fill("  Alice B  ");
    await page.getByRole("button", { name: t("user.profile.updateProfile") }).click();
    await expect(page.getByText(t("flash.success.profile_updated"))).toBeVisible();
    await expect(page.locator("nav").getByText("Alice B", { exact: true })).toBeVisible();
    expect(await page.evaluate(() => (window as unknown as { __marker?: number }).__marker)).toBe(1);

    // AC23 / AC21: wrong password and invalid address are refused inline.
    const emailCard = card(page, "user.profile.changeEmail.title");
    const newEmail = uniqueEmail("ann-new");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill("not the password");
    await expectingFailedRequest(errors, async () => {
      await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
      await expect(page.getByText(t("flash.error.current_password_incorrect"))).toBeVisible();
    });
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill("not-an-email");
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await expectingFailedRequest(errors, async () => {
      await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
      await expect(page.getByText(t("flash.error.invalid_email"))).toBeVisible();
    });

    // AC20: the request succeeds, the pending box appears, the mail has the link.
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(page.getByText(t("flash.success.email_verification_sent"))).toBeVisible();
    await expect(page.getByText(t("user.profile.emailPendingVerification", { email: newEmail }))).toBeVisible();
    const mail = await waitForLatest(newEmail);
    expect(firstUrlWithPath(mail, "/user/verify-email/"), "verification link").toBeTruthy();
    // The pending box is persistent: it is still there after a reload.
    await page.reload();
    await waitForHydration(page);
    await expect(page.getByText(t("user.profile.emailPendingVerification", { email: newEmail }))).toBeVisible();

    // AC25: a second request within a minute is throttled.
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(uniqueEmail("ann-other"));
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await expectingFailedRequest(errors, async () => {
      await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
      await expect(page.getByText(t("flash.error.email_change_too_soon"))).toBeVisible();
    });

    // AC28: cancel removes the pending box.
    await page.getByRole("button", { name: t("user.profile.changeEmail.cancel") }).click();
    await expect(page.getByText(t("flash.success.email_change_cancelled"))).toBeVisible();
    await expect(page.getByText(t("user.profile.emailPendingVerification", { email: newEmail }))).toHaveCount(0);

    expect(errors).toEqual([]);
    await context.close();
  });

  test("AC14 AC15 AC18: changing the password logs out the other browser", async ({ browser }) => {
    const { context: aContext, page: a, email } = await newUser(browser, "pw");
    const errors = collectPageErrors(a);
    const bContext = await browser.newContext();
    const b = await bContext.newPage();
    await login(b, email, PASSWORD);
    await expect(b).toHaveURL(new RegExp(`${DASHBOARD}$`));

    await a.goto(PROFILE);
    await waitForHydration(a);
    const pwCard = card(a, "user.profile.changePassword.title");
    const fill = async (current: string, next: string, confirm: string) => {
      await pwCard.getByLabel(t("user.profile.changePassword.current"), { exact: true }).fill(current);
      await pwCard.getByLabel(t("user.profile.changePassword.new"), { exact: true }).fill(next);
      await pwCard.getByLabel(t("user.profile.changePassword.confirm"), { exact: true }).fill(confirm);
      await pwCard.getByRole("button", { name: t("user.profile.changePassword.submit") }).click();
    };
    const fresh = "brand-new-password-2";

    // AC15: wrong current password; fields are cleared after an error.
    await expectingFailedRequest(errors, async () => {
      await fill("not the password", fresh, fresh);
      await expect(a.getByText(t("flash.error.current_password_incorrect"))).toBeVisible();
    });
    await expect(pwCard.getByLabel(t("user.profile.changePassword.new"), { exact: true })).toHaveValue("");

    // AC14: success message, fields cleared, still logged in here.
    await fill(PASSWORD, fresh, fresh);
    await expect(a.getByText(t("flash.success.password_changed"), { exact: true })).toBeVisible();
    await expect(pwCard.getByLabel(t("user.profile.changePassword.current"), { exact: true })).toHaveValue("");
    await a.goto(PROFILE);
    await expect(a).toHaveURL(new RegExp(`${PROFILE}$`));

    // AC18: the other browser is logged out.
    await b.goto(PROFILE);
    await expect(b).toHaveURL(/\/auth\/login\?next=/);

    // The new password works, the old one does not.
    await logout(a);
    await expectingFailedRequest(errors, async () => {
      await login(a, email, PASSWORD);
      await expect(a.getByText("Invalid email or password")).toBeVisible();
    });
    await login(a, email, fresh);
    await expect(a).toHaveURL(new RegExp(`${DASHBOARD}$`));

    // #71: a password change cancels a pending email change. The card says so and the
    // pending box disappears without a reload.
    await a.goto(PROFILE);
    await waitForHydration(a);
    const emailCard = card(a, "user.profile.changeEmail.title");
    const newEmail = uniqueEmail("pw-new");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(fresh);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    const pendingBox = a.getByText(t("user.profile.emailPendingVerification", { email: newEmail }));
    await expect(pendingBox).toBeVisible();
    const fresher = "brand-new-password-3";
    await fill(fresh, fresher, fresher);
    await expect(a.getByText(t("flash.success.password_changed_email_change_cancelled"))).toBeVisible();
    await expect(pendingBox).toHaveCount(0);
    await expect(a.getByRole("button", { name: t("user.profile.changeEmail.cancel") })).toHaveCount(0);

    expect(errors).toEqual([]);
    await aContext.close();
    await bContext.close();
  });

  test("AC29 AC30 AC31 AC34: confirm an email change from the mail link", async ({ browser }) => {
    const { context, page, email } = await newUser(browser, "chg");
    const errors = collectPageErrors(page);
    const newEmail = uniqueEmail("chg-new");

    await page.goto(PROFILE);
    await waitForHydration(page);
    const emailCard = card(page, "user.profile.changeEmail.title");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(page.getByText(t("flash.success.email_verification_sent"))).toBeVisible();
    const link = firstUrlWithPath(await waitForLatest(newEmail), "/user/verify-email/")!;
    expect(link).toBeTruthy();

    // AC29: another browser without a session opens the link: a confirmation page, no change.
    const other = await browser.newContext();
    const otherPage = await other.newPage();
    const otherErrors = collectPageErrors(otherPage);
    await otherPage.goto(link);
    await waitForHydration(otherPage);
    await expect(otherPage.getByText(t("user.verifyEmail.title"), { exact: true })).toBeVisible();
    await expect(otherPage.getByText(t("user.verifyEmail.description"))).toBeVisible();
    await page.reload();
    await waitForHydration(page);
    await expect(page.getByText(t("user.profile.emailPendingVerification", { email: newEmail }))).toBeVisible();

    // AC30: confirm; success message with a login link.
    await otherPage.getByRole("button", { name: t("user.verifyEmail.submit") }).click();
    await expect(otherPage.getByText(t("flash.success.email_verified"))).toBeVisible();
    await otherPage.getByRole("link", { name: t("user.verifyEmail.goToLogin") }).click();
    await expect(otherPage).toHaveURL(/\/auth\/login/);

    // Logging in works with the new address and fails with the old one.
    await expectingFailedRequest(otherErrors, async () => {
      await login(otherPage, email, PASSWORD);
      await expect(otherPage.getByText("Invalid email or password")).toBeVisible();
    });
    await login(otherPage, newEmail, PASSWORD);
    await expect(otherPage).toHaveURL(new RegExp(`${DASHBOARD}$`));

    // The old address got the notice (and it names the new one).
    const notice = await waitForSubject(email, t("email.emailChanged.subject"));
    expect(notice.HTML).toContain(newEmail);

    // The original session is still valid (no session invalidation on email change) and
    // now shows the new address.
    await page.goto(PROFILE);
    await expect(page).toHaveURL(new RegExp(`${PROFILE}$`));
    await expect(page.getByText(newEmail).first()).toBeVisible();

    // AC31: the link is single-use.
    await otherPage.goto(link);
    await waitForHydration(otherPage);
    await expectingFailedRequest(otherErrors, async () => {
      await otherPage.getByRole("button", { name: t("user.verifyEmail.submit") }).click();
      await expect(otherPage.getByText(t("flash.error.email_verification_failed"))).toBeVisible();
    });

    expect(errors).toEqual([]);
    expect(otherErrors).toEqual([]);
    await other.close();
    await context.close();
  });

  test("AC41: confirming works without JavaScript and ends on the profile with a flash", async ({ browser }) => {
    const { context, page, email } = await newUser(browser, "nojs");
    const newEmail = uniqueEmail("nojs-new");
    await page.goto(PROFILE);
    await waitForHydration(page);
    const emailCard = card(page, "user.profile.changeEmail.title");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(page.getByText(t("flash.success.email_verification_sent"))).toBeVisible();
    const link = firstUrlWithPath(await waitForLatest(newEmail), "/user/verify-email/")!;
    expect(link).toBeTruthy();

    // A browser without JavaScript: plain form posts only (no hydration, so no waiting for it).
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await p.goto("/auth/login");
    await p.getByLabel("Email").fill(email);
    await p.getByLabel("Password").fill(PASSWORD);
    await p.getByRole("button", { name: "Login" }).click();
    await expect(p).toHaveURL(new RegExp(`${DASHBOARD}$`));

    // Success: the change is applied (no 403 from the Origin check), back on the profile.
    await p.goto(link);
    await p.getByRole("button", { name: t("user.verifyEmail.submit") }).click();
    await expect(p).toHaveURL(new RegExp(`${PROFILE}$`));
    await expect(p.getByText(t("flash.success.email_verified"))).toBeVisible();
    await expect(p.getByText(newEmail).first()).toBeVisible();

    // Error: the used-up link ends on the profile as well, with the error's flash.
    await p.goto(link);
    await p.getByRole("button", { name: t("user.verifyEmail.submit") }).click();
    await expect(p).toHaveURL(new RegExp(`${PROFILE}$`));
    await expect(p.getByText(t("flash.error.email_verification_failed"))).toBeVisible();

    await plain.close();
    await context.close();
  });

  test("AC44: without JavaScript every profile form ends on the profile with a server-rendered flash", async ({
    browser,
  }) => {
    const { context, page, email } = await newUser(browser, "nojsform");
    await page.close();

    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await p.goto("/auth/login");
    await p.getByLabel("Email").fill(email);
    await p.getByLabel("Password").fill(PASSWORD);
    await p.getByRole("button", { name: "Login" }).click();
    await expect(p).toHaveURL(new RegExp(`${DASHBOARD}$`));
    const profileUrl = new RegExp(`${PROFILE}$`);
    const openProfile = async () => {
      await p.goto(PROFILE);
      await expect(p).toHaveURL(profileUrl);
    };
    const flash = (key: string, params: Record<string, string> = {}) => p.getByText(t(key, params));

    // Name form: error (too long), then success.
    await openProfile();
    await p.getByLabel(t("user.profile.name")).fill("x".repeat(101));
    await p.getByRole("button", { name: t("user.profile.updateProfile") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.error.name_too_long", { max: "100" })).toBeVisible();
    await p.getByLabel(t("user.profile.name")).fill("  Nojs Renamed  ");
    await p.getByRole("button", { name: t("user.profile.updateProfile") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.success.profile_updated")).toBeVisible();
    await expect(p.locator("nav").getByText("Nojs Renamed", { exact: true })).toBeVisible();

    // Email form: wrong password (error), then success, then cancel.
    const emailCard = card(p, "user.profile.changeEmail.title");
    const newEmail = uniqueEmail("nojsform-new");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill("not the password");
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.error.current_password_incorrect")).toBeVisible();
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(newEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(PASSWORD);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.success.email_verification_sent")).toBeVisible();
    await expect(flash("user.profile.emailPendingVerification", { email: newEmail })).toBeVisible();
    await p.getByRole("button", { name: t("user.profile.changeEmail.cancel") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.success.email_change_cancelled")).toBeVisible();
    await expect(flash("user.profile.emailPendingVerification", { email: newEmail })).toHaveCount(0);

    // Password form: wrong current password (error), then success.
    const pwCard = card(p, "user.profile.changePassword.title");
    const fresh = "brand-new-password-2";
    const fillPasswords = async (current: string, next: string, confirm: string) => {
      await pwCard.getByLabel(t("user.profile.changePassword.current"), { exact: true }).fill(current);
      await pwCard.getByLabel(t("user.profile.changePassword.new"), { exact: true }).fill(next);
      await pwCard.getByLabel(t("user.profile.changePassword.confirm"), { exact: true }).fill(confirm);
      await pwCard.getByRole("button", { name: t("user.profile.changePassword.submit") }).click();
    };
    await fillPasswords("not the password", fresh, fresh);
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.error.current_password_incorrect")).toBeVisible();
    await fillPasswords(PASSWORD, fresh, fresh);
    await expect(p).toHaveURL(profileUrl);
    await expect(p.getByText(t("flash.success.password_changed"), { exact: true })).toBeVisible();

    // #71: with a pending email change, the password flash says that it was cancelled.
    const laterEmail = uniqueEmail("nojsform-later");
    await emailCard.getByLabel(t("user.profile.changeEmail.new")).fill(laterEmail);
    await emailCard.getByLabel(t("user.profile.changeEmail.currentPassword")).fill(fresh);
    await emailCard.getByRole("button", { name: t("user.profile.changeEmail.submit") }).click();
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("user.profile.emailPendingVerification", { email: laterEmail })).toBeVisible();
    const fresher = "brand-new-password-3";
    await fillPasswords(fresh, fresher, fresher);
    await expect(p).toHaveURL(profileUrl);
    await expect(flash("flash.success.password_changed_email_change_cancelled")).toBeVisible();
    await expect(flash("user.profile.emailPendingVerification", { email: laterEmail })).toHaveCount(0);

    await plain.close();
    await context.close();
  });

  test("account pages render and hydrate without errors", async ({ browser }) => {
    const { context, page } = await newUser(browser, "pages");
    const errors = collectPageErrors(page);
    for (const path of [DASHBOARD, PROFILE]) {
      const res = await page.goto(path);
      expect(res?.status(), path).toBe(200);
      await waitForHydration(page);
    }
    // The removed JSON endpoint is a plain 404 page.
    const gone = await page.request.get("/api/user/1");
    expect(gone.status()).toBe(404);
    expect(gone.headers()["content-type"] ?? "").not.toContain("json");
    expect(errors).toEqual([]);
    await context.close();
  });
});
