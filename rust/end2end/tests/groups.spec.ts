import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * groups slice acceptance tests (docs/rewrite/specs/groups.md). Only the main journeys and
 * what depends on hydration, confirm dialogs, redirects or JavaScript being off; the
 * exhaustive cases are integration tests in server/tests/it/groups_*.rs.
 *
 * Texts come from the catalogs by key, so the spec pins keys, not wording. The mail
 * endpoints are limited per IP and hour, so the file registers exactly three users (Alice,
 * Bob, Carol) in `beforeAll` and the tests run in order on top of each other.
 */

function load(locale: string): Record<string, string> {
  return JSON.parse(readFileSync(resolve(__dirname, `../../locales/${locale}.json`), "utf8"));
}
const EN = load("en");
const DE = load("de");

function text(catalog: Record<string, string>, key: string, params: Record<string, string>): string {
  const value = catalog[key];
  if (value === undefined) throw new Error(`key ${key} is missing from the catalog`);
  return Object.entries(params).reduce((acc, [k, v]) => acc.replaceAll(`{${k}}`, v), value);
}
const t = (key: string, params: Record<string, string> = {}) => text(EN, key, params);
const tDe = (key: string, params: Record<string, string> = {}) => text(DE, key, params);

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";

/** Runs a deliberately failing step and removes the one Chromium "status of 4xx" line it logged. */
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

/** Registers on a German page (account language de, auth AC48); selectors are language neutral. */
async function registerDe(page: Page, email: string, name: string): Promise<void> {
  await page.goto("/auth/register");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await waitForHydration(page);
  await page.locator("input[type=email]").fill(email);
  await page.locator("main form button[type=submit]").click();
  const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
  expect(link, "registration mail has a link").toBeTruthy();
  await page.goto(link);
  await waitForHydration(page);
  await page.locator("main form input[type=text]").fill(name);
  const passwords = page.locator("main form input[type=password]");
  await passwords.nth(0).fill(PASSWORD);
  await passwords.nth(1).fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

/** Logs in on the login page, optionally through a `?next=` link. */
async function login(page: Page, email: string, url = "/auth/login"): Promise<void> {
  await page.goto(url);
  await waitForHydration(page);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Login" }).click();
}

/** Login without JavaScript (plain form post). */
async function loginPlain(page: Page, email: string): Promise<void> {
  await page.goto("/auth/login");
  await page.locator("input[type=email]").fill(email);
  await page.locator("input[type=password]").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

function groupId(page: Page): string {
  const match = /\/groups\/(\d+)(?:$|\?)/.exec(page.url());
  if (!match) throw new Error(`not on a group page: ${page.url()}`);
  return match[1];
}

/** Accepts the confirm dialog the next click opens. */
function acceptNextDialog(page: Page): void {
  page.once("dialog", (dialog) => void dialog.accept());
}

async function fillGroupForm(
  page: Page,
  values: { name?: string; description?: string; deadline?: string; draw?: string },
): Promise<void> {
  if (values.name !== undefined) await page.getByLabel(t("group.form.name"), { exact: true }).fill(values.name);
  if (values.description !== undefined)
    await page.getByLabel(t("group.form.description"), { exact: true }).fill(values.description);
  if (values.deadline !== undefined)
    await page.getByLabel(t("group.form.registrationDeadline"), { exact: true }).fill(values.deadline);
  if (values.draw !== undefined) await page.getByLabel(t("group.form.drawDate"), { exact: true }).fill(values.draw);
}

test.describe.configure({ mode: "serial" });

test.describe("groups", () => {
  let browserRef: Browser;
  const users = {
    alice: { email: uniqueEmail("g-alice"), name: "Alice Admin", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("g-bob"), name: "Bob Member", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    carol: { email: uniqueEmail("g-carol"), name: "Carol Newcomer", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  let gid = "";
  let inviteLink = "";
  let inviteCode = "";

  test.beforeAll(async ({ browser }) => {
    browserRef = browser;
    // Carol is the German user: registering on a German page makes a German account, and
    // the session language (from the account) beats cookie and browser language.
    for (const [key, user] of Object.entries(users)) {
      const german = key === "carol";
      user.context = await browser.newContext(german ? { locale: "de-DE" } : {});
      user.page = await user.context.newPage();
      if (german) await registerDe(user.page, user.email, user.name);
      else await register(user.page, user.email, user.name);
    }
  });

  test.afterAll(async () => {
    for (const user of Object.values(users)) await user.context?.close();
  });

  test("AC1 AC5 AC10 AC47: creating a group shows the flash, the dates and no hydration error", async () => {
    const page = users.alice.page;
    const errors = collectPageErrors(page);

    // AC10: no groups yet.
    await page.goto("/groups");
    await waitForHydration(page);
    await expect(page.getByText(t("group.emptyMessage"))).toBeVisible();
    await page.getByRole("link", { name: t("group.createButton") }).first().click();
    await expect(page).toHaveURL(/\/groups\/new$/);
    await waitForHydration(page);

    // AC2: a blank name is refused inline.
    await fillGroupForm(page, { name: "   " });
    await expectingFailedRequest(errors, async () => {
      await page.getByRole("button", { name: t("group.form.createButton") }).click();
      await expect(page.getByText(t("validation.group.name_required"))).toBeVisible();
    });
    // AC6: a draw date before the deadline is refused inline.
    await fillGroupForm(page, { name: "Family 2026", deadline: "2026-12-12", draw: "2026-12-10" });
    await expectingFailedRequest(errors, async () => {
      await page.getByRole("button", { name: t("group.form.createButton") }).click();
      await expect(page.getByText(t("validation.group.draw_before_deadline"))).toBeVisible();
    });

    // AC1 + AC5: trimmed name, dates, flash, landing on the group.
    await fillGroupForm(page, {
      name: "  Family 2026  ",
      description: "Line one\nLine two",
      deadline: "2026-12-10",
      draw: "2026-12-12",
    });
    await page.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(page).toHaveURL(/\/groups\/\d+$/);
    gid = groupId(page);
    await expect(page.getByText(t("flash.success.group_created"))).toBeVisible();
    await expect(page.getByRole("heading", { name: "Family 2026", exact: true })).toBeVisible();
    await expect(page.getByText("10 Dec 2026")).toBeVisible();
    await expect(page.getByText("12 Dec 2026")).toBeVisible();
    await expect(page.getByText(t("group.show.drawDateInfo"))).toBeVisible();

    // The admin sees the code and the link, built from APP_URL.
    await waitForHydration(page);
    const link = page.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/);
    await expect(link.first()).toBeVisible();
    inviteLink = ((await link.first().textContent()) ?? "").trim();
    inviteCode = inviteLink.slice(-8);
    expect(inviteLink).toMatch(/^https?:\/\/[^/]+\/join\/[A-HJ-NP-Z2-9]{8}$/);
    await expect(page.getByRole("button", { name: t("group.show.deleteGroup") })).toBeVisible();
    await expect(page.getByRole("button", { name: t("group.show.leaveGroup") })).toHaveCount(0);

    // The list shows the group with the singular member count.
    await page.goto("/groups");
    await expect(page.getByText("Family 2026")).toBeVisible();
    await expect(page.getByText("1 member", { exact: true })).toBeVisible();

    expect(errors).toEqual([]);
  });

  test("AC17 AC18 AC19: editing prefills the form, saves, and an empty date clears it", async () => {
    const page = users.alice.page;
    const errors = collectPageErrors(page);
    await page.goto(`/groups/${gid}/edit`);
    await waitForHydration(page);

    await expect(page.getByLabel(t("group.form.name"), { exact: true })).toHaveValue("Family 2026");
    await expect(page.getByLabel(t("group.form.registrationDeadline"), { exact: true })).toHaveValue("2026-12-10");
    await expect(page.getByLabel(t("group.form.drawDate"), { exact: true })).toHaveValue("2026-12-12");

    await fillGroupForm(page, { name: "Family", draw: "" });
    await page.getByRole("button", { name: t("group.form.updateButton") }).click();
    await expect(page).toHaveURL(new RegExp(`/groups/${gid}$`));
    await expect(page.getByText(t("flash.success.group_updated"))).toBeVisible();
    await expect(page.getByRole("heading", { name: "Family", exact: true })).toBeVisible();
    await expect(page.getByText("10 Dec 2026")).toBeVisible();
    await expect(page.getByText("12 Dec 2026")).toHaveCount(0);

    // Put the draw date back for the German check below.
    await page.goto(`/groups/${gid}/edit`);
    await waitForHydration(page);
    await expect(page.getByLabel(t("group.form.drawDate"), { exact: true })).toHaveValue("");
    await fillGroupForm(page, { draw: "2026-12-12" });
    await page.getByRole("button", { name: t("group.form.updateButton") }).click();
    await expect(page).toHaveURL(new RegExp(`/groups/${gid}$`));
    expect(errors).toEqual([]);
  });

  test("AC34 AC27 AC11: the invitation link goes through login to a preview, and Join makes a member", async () => {
    const page = users.bob.page;
    const errors = collectPageErrors(page);
    // Bob logs out (the registration left him logged in) and opens the link.
    await page.locator("nav button", { hasText: "Logout" }).dispatchEvent("click");
    await expect(page).toHaveURL(/\/$/);

    await page.goto(`/join/${inviteCode}`);
    await expect(page).toHaveURL(new RegExp(`/auth/login\\?next=%2Fjoin%2F${inviteCode}$`));
    await waitForHydration(page);
    await page.getByLabel("Email").fill(users.bob.email);
    await page.getByLabel("Password").fill(PASSWORD);
    await page.getByRole("button", { name: "Login" }).click();

    // The preview, not an automatic join.
    await expect(page).toHaveURL(new RegExp(`/join/${inviteCode}$`));
    await waitForHydration(page);
    await expect(page.getByText(t("group.join.previewTitle", { name: "Family" }))).toBeVisible();
    await expect(page.getByText("Alice Admin")).toBeVisible();
    await expect(page.getByText("1 member", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: t("group.join.submit") }).click();

    await expect(page).toHaveURL(new RegExp(`/groups/${gid}$`));
    await expect(page.getByText(t("flash.success.group_joined"))).toBeVisible();
    await waitForHydration(page);

    // AC11: names, "You", no emails, no code, no admin actions, a Leave button.
    await expect(page.getByText("Alice Admin").first()).toBeVisible();
    await expect(page.getByText(t("group.show.you"), { exact: true })).toBeVisible();
    await expect(page.getByText(users.alice.email)).toHaveCount(0);
    await expect(page.getByText(inviteCode)).toHaveCount(0);
    await expect(page.getByRole("button", { name: t("group.show.deleteGroup") })).toHaveCount(0);
    await expect(page.getByRole("button", { name: t("group.show.leaveGroup") })).toBeVisible();

    // AC30: opening the link again says so.
    await page.goto(`/join/${inviteCode}`);
    await expect(page.getByText(t("flash.info.already_member"))).toBeVisible();
    expect(errors).toEqual([]);
  });

  test("AC25 AC29: regenerating (after the confirm) kills the old link at once", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(`/groups/${gid}`);
    await waitForHydration(alice);
    // Members card: Alice sees Bob's email (AC12).
    await expect(alice.getByText(users.bob.email)).toBeVisible();

    // Dismissing the confirm changes nothing.
    alice.once("dialog", (dialog) => void dialog.dismiss());
    await alice.getByRole("button", { name: t("group.show.regenerateCode") }).click();
    await expect(alice.getByText(inviteCode)).toBeVisible();

    acceptNextDialog(alice);
    await alice.getByRole("button", { name: t("group.show.regenerateCode") }).click();
    await expect(alice.getByText(t("flash.success.invitation_generated"))).toBeVisible();
    await expect(alice.getByText(inviteCode)).toHaveCount(0);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const newLink = ((await link.textContent()) ?? "").trim();
    expect(newLink).not.toBe(inviteLink);
    const oldCode = inviteCode;
    inviteLink = newLink;
    inviteCode = newLink.slice(-8);

    // Carol (German account) opens the old link: invalid code and the enter-code form.
    const carol = users.carol.page;
    const carolErrors = collectPageErrors(carol);
    await carol.goto(`/join/${oldCode}`);
    await waitForHydration(carol);
    await expect(carol.getByText(tDe("flash.error.invalid_invitation_code"))).toBeVisible();
    await expect(carol.getByLabel(tDe("group.join.code"))).toBeVisible();

    // AC28: she types the new code lowercase, with spaces and a dash, on /join.
    await carol.goto("/join");
    await waitForHydration(carol);
    const typed = ` ${inviteCode.slice(0, 4).toLowerCase()}-${inviteCode.slice(4).toLowerCase()} `;
    await carol.getByLabel(tDe("group.join.code")).fill(typed);
    await carol.getByRole("button", { name: tDe("group.join.submit") }).click();
    await expect(carol).toHaveURL(new RegExp(`/join/${inviteCode}$`));
    await expect(carol.getByText(tDe("group.join.previewTitle", { name: "Family" }))).toBeVisible();
    await expect(carol.getByText(tDe("group.memberCount.other", { count: "2" }), { exact: true })).toBeVisible();

    // She joins (needed as the German viewer of the next test).
    await carol.getByRole("button", { name: tDe("group.join.submit") }).click();
    await expect(carol).toHaveURL(new RegExp(`/groups/${gid}$`));
    await expect(carol.getByText(tDe("flash.success.group_joined"))).toBeVisible();
    expect(errors).toEqual([]);
    expect(carolErrors.filter((e) => !/status of 4\d\d/.test(e))).toEqual([]);
  });

  test("AC47: the details show the dates in the page language without a hydration error", async () => {
    const page = users.carol.page;
    const errors = collectPageErrors(page);

    await page.goto(`/groups/${gid}`);
    await waitForHydration(page);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    await expect(page.getByText("10.12.2026")).toBeVisible();
    await expect(page.getByText("12.12.2026")).toBeVisible();
    await expect(page.getByText(tDe("group.show.drawDateInfo"))).toBeVisible();
    await expect(page.getByText("10 Dec 2026")).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("AC41 AC35: Alice hands over the admin role and then leaves", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(`/groups/${gid}`);
    await waitForHydration(alice);

    acceptNextDialog(alice);
    const bobRow = alice.locator("li, tr, .list-group-item").filter({ hasText: "Bob Member" }).first();
    await bobRow.getByRole("button", { name: t("group.show.makeAdmin") }).click();
    await expect(alice.getByText(t("flash.success.admin_transferred"))).toBeVisible();
    await expect(alice.getByRole("button", { name: t("group.show.deleteGroup") })).toHaveCount(0);
    await expect(alice.getByText(users.bob.email)).toHaveCount(0);
    await expect(alice.getByRole("button", { name: t("group.show.leaveGroup") })).toBeVisible();

    // Bob (reload) is the admin now.
    const bob = users.bob.page;
    await bob.goto(`/groups/${gid}`);
    await waitForHydration(bob);
    await expect(bob.getByRole("button", { name: t("group.show.deleteGroup") })).toBeVisible();
    await expect(bob.getByText(users.alice.email)).toBeVisible();

    // AC35: Alice leaves, lands on her list without the group.
    acceptNextDialog(alice);
    await alice.getByRole("button", { name: t("group.show.leaveGroup") }).click();
    await expect(alice).toHaveURL(/\/groups$/);
    await expect(alice.getByText(t("flash.success.left_group"))).toBeVisible();
    await expect(alice.getByText(t("group.emptyMessage"))).toBeVisible();
    expect(errors).toEqual([]);
  });

  test("AC13 AC39: Bob removes Carol, who then gets the 404 page", async () => {
    const carol = users.carol.page;

    const bob = users.bob.page;
    const errors = collectPageErrors(bob);
    await bob.goto(`/groups/${gid}`);
    await waitForHydration(bob);
    const row = bob.locator("li, tr, .list-group-item").filter({ hasText: "Carol Newcomer" }).first();
    acceptNextDialog(bob);
    await row.getByRole("button", { name: t("group.show.removeMember") }).click();
    await expect(bob.getByText(t("flash.success.member_removed"))).toBeVisible();
    await expect(bob.getByText("Carol Newcomer")).toHaveCount(0);

    const response = await carol.goto(`/groups/${gid}`);
    expect(response?.status()).toBe(404);
    await expect(carol.getByText("Family")).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("AC22 AC24: the admin deletes the group after the confirm", async () => {
    const bob = users.bob.page;
    const errors = collectPageErrors(bob);
    await bob.goto(`/groups/${gid}`);
    await waitForHydration(bob);
    bob.once("dialog", (dialog) => void dialog.dismiss());
    await bob.getByRole("button", { name: t("group.show.deleteGroup") }).click();
    await expect(bob.getByRole("heading", { name: "Family", exact: true })).toBeVisible();

    acceptNextDialog(bob);
    await bob.getByRole("button", { name: t("group.show.deleteGroup") }).click();
    await expect(bob).toHaveURL(/\/groups$/);
    await expect(bob.getByText(t("flash.success.group_deleted"))).toBeVisible();
    const gone = await bob.goto(`/groups/${gid}`);
    expect(gone?.status()).toBe(404);

    // AC24: a plain GET to the old-style delete URL does nothing (and there is nothing left).
    const get = await bob.request.get(`/groups/${gid}/delete`);
    expect(get.status()).toBeGreaterThanOrEqual(400);
    expect(errors.filter((e) => !/status of 4\d\d/.test(e))).toEqual([]);
  });

  test("without JavaScript the create, join and leave forms end on a page with the flash", async ({ browser }) => {
    // Alice creates a fresh group with JavaScript to have a code to join.
    const alice = users.alice.page;
    await alice.goto("/groups/new");
    await waitForHydration(alice);
    await fillGroupForm(alice, { name: "Plain Group" });
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    const plainGid = groupId(alice);
    await waitForHydration(alice);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);

    const plainContext = await browser.newContext({ javaScriptEnabled: false, locale: "de-DE" });
    const p = await plainContext.newPage();
    await loginPlain(p, users.carol.email);

    // Create: a plain form post ends on the new group with the flash.
    await p.goto("/groups/new");
    await p.getByLabel(tDe("group.form.name"), { exact: true }).fill("Carols Plain Group");
    await p.getByRole("button", { name: tDe("group.form.createButton") }).click();
    await expect(p).toHaveURL(/\/groups\/\d+$/);
    await expect(p.getByText(tDe("flash.success.group_created"))).toBeVisible();
    await expect(p.getByText("Carols Plain Group").first()).toBeVisible();

    // Join: the preview's button is a plain form post.
    await p.goto(`/join/${code}`);
    await expect(p.getByText(tDe("group.join.previewTitle", { name: "Plain Group" }))).toBeVisible();
    await p.getByRole("button", { name: tDe("group.join.submit") }).click();
    await expect(p).toHaveURL(new RegExp(`/groups/${plainGid}$`));
    await expect(p.getByText(tDe("flash.success.group_joined"))).toBeVisible();

    // Leave: ends on the list with the flash; the group is gone from it.
    await p.getByRole("button", { name: tDe("group.show.leaveGroup") }).click();
    await expect(p).toHaveURL(/\/groups$/);
    await expect(p.getByText(tDe("flash.success.left_group"))).toBeVisible();
    await expect(p.getByText("Plain Group", { exact: true })).toHaveCount(0);
    await expect(p.getByText("Carols Plain Group")).toBeVisible();

    // A wrong code without JavaScript shows the invalid-code message.
    await p.goto("/join/ZZZZZZZZ");
    await expect(p.getByText(tDe("flash.error.invalid_invitation_code"))).toBeVisible();
    await plainContext.close();
    void browserRef;
  });
});
