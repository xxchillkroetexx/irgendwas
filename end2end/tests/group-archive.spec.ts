import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * group-archive acceptance tests (docs/specs/group-archive.md). Only the main journeys and
 * what depends on hydration, confirm dialogs, redirects or JavaScript being off; the
 * exhaustive cases (the archive clock, the sweep, read-only rules, secrecy, mails) are
 * integration tests in server/tests/it/group_archive.rs.
 *
 * Not covered here: the admin's warning alert with "Keep active" (AC22) and its hydration
 * check (AC38). A group only enters its 14-day warning window through the clock (the
 * browser cannot move it, and `update_group` / "Keep active" restart the clock), so those
 * are integration tests that render the page for a group drawn six months minus a week ago.
 *
 * Texts come from the catalogs by key. Registration is limited to 20 per hour and IP, so the
 * file registers exactly two users (Alice the admin, Bob a member) and runs the tests in
 * order on top of each other.
 *
 * The page contract (data-testid) is documented in server/tests/it/group_archive.rs.
 */

function load(locale: string): Record<string, string> {
  return JSON.parse(readFileSync(resolve(__dirname, `../../locales/${locale}.json`), "utf8"));
}
const EN = load("en");
const DE = load("de");

function text(catalog: Record<string, string>, key: string, params: Record<string, string> = {}): string {
  const value = catalog[key];
  if (value === undefined) throw new Error(`key ${key} is missing from the catalog`);
  return Object.entries(params).reduce((acc, [k, v]) => acc.replaceAll(`{${k}}`, v), value);
}
const t = (key: string, params: Record<string, string> = {}) => text(EN, key, params);
const tDe = (key: string, params: Record<string, string> = {}) => text(DE, key, params);
/** The part of a catalog text before its `{date}` placeholder. */
const before = (catalog: Record<string, string>, key: string) => catalog[key].split("{date}")[0];

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const GROUP_NAME = "Archive Group";

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

/**
 * Switches the account language with the nav's language switch (the nav offers the other
 * locale) when the page is not in `locale` yet.
 */
async function setLocale(page: Page, locale: "en" | "de"): Promise<void> {
  await page.goto(DASHBOARD);
  await waitForHydration(page);
  if ((await page.locator("html").getAttribute("lang")) === locale) return;
  await page.locator('form[action="/api/set_locale"] button[type=submit]').first().click();
  await expect(page.locator("html")).toHaveAttribute("lang", locale);
}

test.describe.configure({ mode: "serial" });

test.describe("group archive", () => {
  let browserRef: Browser;
  const users = {
    alice: { email: uniqueEmail("a-alice"), name: "Alice Archivist", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("a-bob"), name: "Bob Browser", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  let gid = "";
  let groupUrl = "";
  let code = "";

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
    await alice.getByLabel(t("group.form.name"), { exact: true }).fill(GROUP_NAME);
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    gid = /\/groups\/(\d+)$/.exec(alice.url())![1];
    groupUrl = `/groups/${gid}`;
    await waitForHydration(alice);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    code = ((await link.textContent()) ?? "").trim().slice(-8);
    const bob = users.bob.page;
    await bob.goto(`/join/${code}`);
    await waitForHydration(bob);
    await bob.getByRole("button", { name: t("group.join.submit") }).click();
    await expect(bob).toHaveURL(new RegExp(`${groupUrl}$`));
  });

  test.afterAll(async () => {
    for (const user of Object.values(users)) await user.context?.close();
    void browserRef;
  });

  test("AC22 AC36 AC37: the admin sees when the group is archived, members never do", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);
    const line = alice.getByTestId("auto-archive-on");
    await expect(line).toBeVisible();
    await expect(line).toContainText(before(EN, "group.show.autoArchiveOn").trim());
    await expect(line).toContainText(/20\d\d/);
    // A new group is nowhere near its warning window.
    await expect(alice.getByTestId("archive-soon")).toHaveCount(0);
    await expect(alice.getByRole("button", { name: t("group.show.keepActive") })).toHaveCount(0);
    expect(errors, errors.join("\n")).toEqual([]);

    const bob = users.bob.page;
    const bobErrors = collectPageErrors(bob);
    await bob.goto(groupUrl);
    await waitForHydration(bob);
    await expect(bob.getByTestId("auto-archive-on")).toHaveCount(0);
    await expect(bob.getByText(before(EN, "group.show.autoArchiveOn").trim())).toHaveCount(0);
    await expect(bob.getByRole("button", { name: t("group.show.archiveGroup") })).toHaveCount(0);
    expect(bobErrors, bobErrors.join("\n")).toEqual([]);
  });

  test("AC18 AC30 AC31: the confirm dialog guards 'Archive group'; the archived view is read-only", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);

    // Dismissing the confirm changes nothing.
    alice.once("dialog", (dialog) => {
      expect(dialog.message()).toContain(t("group.show.archiveGroupConfirm"));
      void dialog.dismiss();
    });
    await alice.getByRole("button", { name: t("group.show.archiveGroup") }).click();
    await expect(alice.getByRole("button", { name: t("group.show.archiveGroup") })).toBeVisible();
    await expect(alice.getByTestId("archived-notice")).toHaveCount(0);

    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.getByRole("button", { name: t("group.show.archiveGroup") }).click();
    await expect(alice.getByText(t("flash.success.group_archived"))).toBeVisible();
    await expect(alice).toHaveURL(new RegExp(`${groupUrl}$`));
    const notice = alice.getByTestId("archived-notice");
    await expect(notice).toBeVisible();
    await expect(notice).toContainText(before(EN, "group.show.archivedNotice").trim());
    await expect(alice.getByText(t("group.status.archived"), { exact: true })).toBeVisible();

    // Read-only: Restore and Delete only.
    await expect(alice.getByRole("button", { name: t("group.show.restoreGroup") })).toBeVisible();
    await expect(alice.getByRole("button", { name: t("group.show.deleteGroup") })).toBeVisible();
    for (const name of [
      t("group.show.editButton"),
      t("group.show.regenerateCode"),
      t("group.show.archiveGroup"),
      t("group.show.keepActive"),
      t("group.show.editWishlist"),
    ]) {
      await expect(alice.getByText(name, { exact: true })).toHaveCount(0);
    }
    await expect(alice.getByTestId("group-invitation").getByText(code)).toHaveCount(0);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC33 AC34: the group moved from My groups to the archive page", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto("/groups");
    await waitForHydration(alice);
    await expect(alice.getByText(GROUP_NAME)).toHaveCount(0);
    await alice.getByRole("link", { name: t("group.archive.title") }).click();
    await expect(alice).toHaveURL(/\/groups\/archive$/);
    await waitForHydration(alice);
    const card = alice.getByTestId("archived-group");
    await expect(card).toHaveCount(1);
    await expect(card).toContainText(GROUP_NAME);
    await expect(card).toContainText(before(EN, "group.archive.archivedOn").trim());
    await expect(card.getByRole("link", { name: t("group.viewDetails") })).toHaveAttribute("href", groupUrl);
    await expect(alice.getByRole("link", { name: t("nav.groups") }).first()).toBeVisible();
    expect(errors, errors.join("\n")).toEqual([]);

    // Open it again from the archive.
    await card.getByRole("link", { name: t("group.viewDetails") }).click();
    await expect(alice).toHaveURL(new RegExp(`${groupUrl}$`));
  });

  test("AC30 AC35 AC37: a member sees the archived group read-only and finds it in the archive", async () => {
    const bob = users.bob.page;
    const errors = collectPageErrors(bob);
    await bob.goto(groupUrl);
    await waitForHydration(bob);
    await expect(bob.getByTestId("archived-notice")).toBeVisible();
    await expect(bob.getByText(t("group.status.archived"), { exact: true })).toBeVisible();
    await expect(bob.getByRole("button", { name: t("group.show.leaveGroup") })).toHaveCount(0);
    await expect(bob.getByText(t("group.show.cannotLeaveAfterDraw"))).toHaveCount(0);
    await expect(bob.getByText(t("group.show.editWishlist"))).toHaveCount(0);
    await expect(bob.getByTestId("auto-archive-on")).toHaveCount(0);
    await expect(bob.locator(`a[href="${groupUrl}/exclusions"]`)).toHaveCount(0);
    await expect(bob.getByTestId("group-invitation").getByText(code)).toHaveCount(0);

    // The wishlist page: the notice and no way to add anything.
    await bob.goto(`${groupUrl}/wishlist`);
    await waitForHydration(bob);
    await expect(bob.getByText(t("flash.error.group_archived"))).toBeVisible();
    await expect(bob.locator('form[action="/api/add_item"]')).toHaveCount(0);

    // The exclusions page: the notice and no add form.
    await bob.goto(`${groupUrl}/exclusions`);
    await waitForHydration(bob);
    await expect(bob.getByText(t("flash.error.group_archived"))).toBeVisible();
    await expect(bob.locator('form[action="/api/add_exclusion"]')).toHaveCount(0);

    // The archive page lists it; the invitation link no longer joins.
    await bob.goto("/groups/archive");
    await waitForHydration(bob);
    await expect(bob.getByTestId("archived-group")).toHaveCount(1);
    await expect(bob.getByTestId("archived-group")).toContainText(GROUP_NAME);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC21 AC22: restoring brings the group back; the schedule is the admin's only", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);

    alice.once("dialog", (dialog) => {
      expect(dialog.message()).toContain(t("group.show.restoreGroupConfirm"));
      void dialog.accept();
    });
    await alice.getByRole("button", { name: t("group.show.restoreGroup") }).click();
    await expect(alice.getByText(t("flash.success.group_restored"))).toBeVisible();
    await expect(alice.getByTestId("archived-notice")).toHaveCount(0);
    await expect(alice.getByRole("button", { name: t("group.show.archiveGroup") })).toBeVisible();
    await expect(alice.getByTestId("auto-archive-on")).toBeVisible();

    await alice.goto("/groups");
    await waitForHydration(alice);
    await expect(alice.getByText(GROUP_NAME)).toBeVisible();
    await alice.goto("/groups/archive");
    await waitForHydration(alice);
    await expect(alice.getByText(t("group.archive.empty"))).toBeVisible();

    // Bob: writable again, still no date.
    const bob = users.bob.page;
    await bob.goto(groupUrl);
    await waitForHydration(bob);
    await expect(bob.getByTestId("archived-notice")).toHaveCount(0);
    await expect(bob.getByTestId("auto-archive-on")).toHaveCount(0);
    await expect(bob.getByRole("link", { name: t("group.show.editWishlist") })).toBeVisible();
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC23 (no JavaScript): archive and restore are plain forms that redirect back with a flash", async ({ browser }) => {
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await loginPlain(p, users.alice.email);
    await p.goto(groupUrl);

    await p.getByRole("button", { name: t("group.show.archiveGroup") }).click();
    await expect(p).toHaveURL(new RegExp(`${groupUrl}$`));
    await expect(p.getByText(t("flash.success.group_archived"))).toBeVisible();
    await expect(p.getByTestId("archived-notice")).toBeVisible();

    await p.getByRole("button", { name: t("group.show.restoreGroup") }).click();
    await expect(p).toHaveURL(new RegExp(`${groupUrl}$`));
    await expect(p.getByText(t("flash.success.group_restored"))).toBeVisible();
    await expect(p.getByTestId("archived-notice")).toHaveCount(0);
    await plain.close();
  });

  test("AC34: a logged-out visitor is sent to login and lands on the archive page afterwards", async ({ browser }) => {
    const visitor = await browser.newContext();
    const p = await visitor.newPage();
    await p.goto("/groups/archive");
    await expect(p).toHaveURL(/\/auth\/login\?next=/);
    await waitForHydration(p);
    await p.locator("input[type=email]").fill(users.bob.email);
    await p.locator("input[type=password]").fill(PASSWORD);
    await p.locator("main form button[type=submit]").click();
    await expect(p).toHaveURL(/\/groups\/archive$/);
    await visitor.close();
  });

  test("AC38: the archive page and the archived group render in German without hydration errors", async () => {
    const alice = users.alice.page;
    // Archive again (no dialog handler needed: accept any).
    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.goto(groupUrl);
    await waitForHydration(alice);
    await alice.getByRole("button", { name: t("group.show.archiveGroup") }).click();
    await expect(alice.getByTestId("archived-notice")).toBeVisible();

    await setLocale(alice, "de");
    const errors = collectPageErrors(alice);

    await alice.goto("/groups/archive");
    await expect(alice.locator("html")).toHaveAttribute("lang", "de");
    await waitForHydration(alice);
    await expect(alice.getByRole("heading", { name: tDe("group.archive.title") })).toBeVisible();
    await expect(alice.getByText(tDe("group.archive.intro"))).toBeVisible();
    await expect(alice.getByTestId("archived-group")).toContainText(tDe("group.status.archived"));

    await alice.goto(groupUrl);
    await waitForHydration(alice);
    await expect(alice.getByTestId("archived-notice")).toContainText(before(DE, "group.show.archivedNotice").trim());
    await expect(alice.getByRole("button", { name: tDe("group.show.restoreGroup") })).toBeVisible();

    // A member's German archived view has no archive date and no hydration error.
    const bob = users.bob.page;
    await setLocale(bob, "de");
    const bobErrors = collectPageErrors(bob);
    await bob.goto(groupUrl);
    await expect(bob.locator("html")).toHaveAttribute("lang", "de");
    await waitForHydration(bob);
    await expect(bob.getByTestId("archived-notice")).toContainText(before(DE, "group.show.archivedNotice").trim());
    await expect(bob.getByTestId("auto-archive-on")).toHaveCount(0);
    expect(bobErrors, bobErrors.join("\n")).toEqual([]);

    // Restore (German confirm), then the active page in German, with the admin's date line.
    alice.once("dialog", (dialog) => {
      expect(dialog.message()).toContain(tDe("group.show.restoreGroupConfirm"));
      void dialog.accept();
    });
    await alice.getByRole("button", { name: tDe("group.show.restoreGroup") }).click();
    await expect(alice.getByText(tDe("flash.success.group_restored"))).toBeVisible();
    await expect(alice.getByTestId("auto-archive-on")).toContainText(before(DE, "group.show.autoArchiveOn").trim());
    expect(errors, errors.join("\n")).toEqual([]);

    await setLocale(alice, "en");
    await setLocale(bob, "en");
  });
});
