import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * exclusions slice acceptance tests (docs/specs/exclusions.md). Only the main
 * journeys and what depends on hydration, confirm dialogs, redirects or JavaScript being
 * off; the exhaustive cases are integration tests in server/tests/it/exclusions_*.rs.
 *
 * Texts come from the catalogs by key. The file registers exactly three users in
 * `beforeAll` (Alice the group admin, Bob and Carol members); the tests run in order on top
 * of each other. Group "Excl Group" therefore has the members Alice, Bob and Carol: any rule
 * that leaves one of them without a receiver or a giver is refused (AC11).
 */

function load(locale: string): Record<string, string> {
  return JSON.parse(readFileSync(resolve(__dirname, `../../locales/${locale}.json`), "utf8"));
}
const EN = load("en");

function text(catalog: Record<string, string>, key: string, params: Record<string, string>): string {
  const value = catalog[key];
  if (value === undefined) throw new Error(`key ${key} is missing from the catalog`);
  return Object.entries(params).reduce((acc, [k, v]) => acc.replaceAll(`{${k}}`, v), value);
}
const t = (key: string, params: Record<string, string> = {}) => text(EN, key, params);

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

async function loginPlain(page: Page, email: string): Promise<void> {
  await page.goto("/auth/login");
  await page.locator("input[type=email]").fill(email);
  await page.locator("input[type=password]").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

/** The options of the "add" select (without the placeholder), as visible labels. */
async function candidates(page: Page): Promise<string[]> {
  const labels = await page.locator("main select option").allTextContents();
  return labels.map((l) => l.trim()).filter((l) => l !== "" && l !== t("exclusion.addNew.selectPlaceholder"));
}

/** The row of the "Current" list that shows `name` (the one with a remove control). */
function currentRow(page: Page, name: string) {
  return page.locator("main li, main tr").filter({ hasText: name }).filter({
    has: page.getByRole("button", { name: t("exclusion.current.remove") }),
  });
}

test.describe.configure({ mode: "serial" });

test.describe("exclusions", () => {
  let browserRef: Browser;
  const users = {
    alice: { email: uniqueEmail("x-alice"), name: "Alice Admin", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("x-bob"), name: "Bob Member", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    carol: { email: uniqueEmail("x-carol"), name: "Carol Member", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  let gid = "";
  let pageUrl = "";

  test.beforeAll(async ({ browser }) => {
    browserRef = browser;
    for (const user of Object.values(users)) {
      user.context = await browser.newContext();
      user.page = await user.context.newPage();
      await register(user.page, user.email, user.name);
    }
    const alice = users.alice.page;
    await alice.goto("/groups/new");
    await waitForHydration(alice);
    await alice.getByLabel(t("group.form.name"), { exact: true }).fill("Excl Group");
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    gid = /\/groups\/(\d+)$/.exec(alice.url())![1];
    pageUrl = `/groups/${gid}/exclusions`;
    await waitForHydration(alice);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);
    for (const user of [users.bob, users.carol]) {
      await user.page.goto(`/join/${code}`);
      await waitForHydration(user.page);
      await user.page.getByRole("button", { name: t("group.join.submit") }).click();
      await expect(user.page).toHaveURL(new RegExp(`/groups/${gid}$`));
    }
  });

  test.afterAll(async () => {
    for (const user of Object.values(users)) await user.context?.close();
  });

  test("AC2 AC1: the group page links to the page, which starts empty with the other members to choose from", async () => {
    const page = users.bob.page;
    const errors = collectPageErrors(page);
    await page.goto(`/groups/${gid}`);
    await waitForHydration(page);
    await page.getByRole("link", { name: t("group.show.manageExclusions") }).click();
    await expect(page).toHaveURL(new RegExp(`${pageUrl}$`));
    await waitForHydration(page);
    await expect(page.getByText(t("exclusion.current.empty"))).toBeVisible();
    await expect(page.getByText(t("exclusion.privacy"))).toBeVisible();
    await expect(page.getByText(t("exclusion.addNew.limit", { max: "5", count: "0" }))).toBeVisible();
    expect(await candidates(page)).toEqual(["Alice Admin", "Carol Member"]);
    // The submit button waits for a choice.
    await expect(page.getByRole("button", { name: t("exclusion.addNew.submit") })).toBeDisabled();
    // The note carries its prefix once (it is part of the key text).
    await expect(page.getByText(t("exclusion.why.note"))).toHaveCount(1);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC3 AC13: adding a rule updates the lists in place; removing asks for a confirmation", async () => {
    const page = users.bob.page;
    const errors = collectPageErrors(page);
    await page.goto(pageUrl);
    await waitForHydration(page);

    await page.getByLabel(t("exclusion.addNew.selectPerson")).selectOption({ label: "Carol Member" });
    await page.getByRole("button", { name: t("exclusion.addNew.submit") }).click();
    await expect(page.getByText(t("flash.success.exclusion_added"))).toBeVisible();
    await expect(currentRow(page, "Carol Member")).toHaveCount(1);
    expect(await candidates(page)).toEqual(["Alice Admin"]);
    await expect(page.getByText(t("exclusion.addNew.limit", { max: "5", count: "1" }))).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`${pageUrl}$`));

    // Cancelling the confirmation keeps the rule.
    page.once("dialog", (dialog) => {
      expect(dialog.message()).toBe(t("exclusion.current.removeConfirm"));
      void dialog.dismiss();
    });
    await currentRow(page, "Carol Member").getByRole("button", { name: t("exclusion.current.remove") }).click();
    await expect(currentRow(page, "Carol Member")).toHaveCount(1);

    // Accepting it removes the rule.
    page.once("dialog", (dialog) => void dialog.accept());
    await currentRow(page, "Carol Member").getByRole("button", { name: t("exclusion.current.remove") }).click();
    await expect(page.getByText(t("flash.success.exclusion_removed"))).toBeVisible();
    await expect(page.getByText(t("exclusion.current.empty"))).toBeVisible();
    expect(await candidates(page)).toEqual(["Alice Admin", "Carol Member"]);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC4 AC11: a rule is invisible to the excluded person; a rule that would block the draw is refused", async () => {
    const bob = users.bob.page;
    const errors = collectPageErrors(bob);
    await bob.goto(pageUrl);
    await waitForHydration(bob);
    await bob.getByLabel(t("exclusion.addNew.selectPerson")).selectOption({ label: "Carol Member" });
    await bob.getByRole("button", { name: t("exclusion.addNew.submit") }).click();
    await expect(currentRow(bob, "Carol Member")).toHaveCount(1);

    // Carol sees nothing of it, and Bob is a candidate for her.
    const carol = users.carol.page;
    await carol.goto(pageUrl);
    await waitForHydration(carol);
    await expect(carol.getByText(t("exclusion.current.empty"))).toBeVisible();
    expect(await candidates(carol)).toContain("Bob Member");
    // Alice (the admin) sees nothing of it either.
    const alice = users.alice.page;
    await alice.goto(pageUrl);
    await waitForHydration(alice);
    await expect(alice.getByText(t("exclusion.current.empty"))).toBeVisible();

    // With Bob -> Carol, Bob -> Alice would leave Bob nobody to draw.
    await bob.getByLabel(t("exclusion.addNew.selectPerson")).selectOption({ label: "Alice Admin" });
    await expectingFailedRequest(errors, async () => {
      await bob.getByRole("button", { name: t("exclusion.addNew.submit") }).click();
      await expect(bob.getByText(t("flash.error.exclusion_would_block_draw"))).toBeVisible();
    });
    await expect(currentRow(bob, "Alice Admin")).toHaveCount(0);
    expect(await candidates(bob)).toEqual(["Alice Admin"]);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC3 AC13: without JavaScript the forms work (add, remove)", async ({ browser }) => {
    // Start from a clean slate for Carol (Bob's rule stays untouched).
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await loginPlain(p, users.carol.email);
    await p.goto(pageUrl);

    await p.getByLabel(t("exclusion.addNew.selectPerson")).selectOption({ label: "Alice Admin" });
    await p.getByRole("button", { name: t("exclusion.addNew.submit") }).click();
    await expect(p).toHaveURL(new RegExp(`${pageUrl}$`));
    await expect(p.getByText(t("flash.success.exclusion_added"))).toBeVisible();
    await expect(currentRow(p, "Alice Admin")).toHaveCount(1);
    expect(await candidates(p)).toEqual(["Bob Member"]);

    // A refused rule (it would leave Carol nobody to draw) comes back as a flash.
    await p.getByLabel(t("exclusion.addNew.selectPerson")).selectOption({ label: "Bob Member" });
    await p.getByRole("button", { name: t("exclusion.addNew.submit") }).click();
    await expect(p).toHaveURL(new RegExp(`${pageUrl}$`));
    await expect(p.getByText(t("flash.error.exclusion_would_block_draw"))).toBeVisible();
    await expect(currentRow(p, "Bob Member")).toHaveCount(0);

    // Remove (no confirm without JS).
    await currentRow(p, "Alice Admin").getByRole("button", { name: t("exclusion.current.remove") }).click();
    await expect(p).toHaveURL(new RegExp(`${pageUrl}$`));
    await expect(p.getByText(t("flash.success.exclusion_removed"))).toBeVisible();
    await expect(p.getByText(t("exclusion.current.empty"))).toBeVisible();
    await plain.close();
    void browserRef;
  });

  test("AC20: a logged-out visitor is sent to login and lands on the page afterwards", async ({ browser }) => {
    const anon = await browser.newContext();
    const p = await anon.newPage();
    await p.goto(pageUrl);
    await expect(p).toHaveURL(/\/auth\/login\?next=/);
    await waitForHydration(p);
    await p.locator("input[type=email]").fill(users.bob.email);
    await p.locator("input[type=password]").fill(PASSWORD);
    await p.getByRole("button", { name: "Login" }).click();
    await expect(p).toHaveURL(new RegExp(`${pageUrl}$`));
    await expect(p.getByText(t("exclusion.privacy"))).toBeVisible();
    await anon.close();
  });
});
