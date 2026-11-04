import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * wishlists slice acceptance tests (docs/specs/wishlists.md). Only the main
 * journeys and what depends on hydration, confirm dialogs, redirects or JavaScript being
 * off; the exhaustive cases are integration tests in server/tests/it/wishlists_*.rs.
 *
 * Texts come from the catalogs by key. The mail endpoints are limited per IP and hour, so
 * the file registers exactly two users (Alice, the group admin and list owner, and Bob,
 * a member who views) in `beforeAll`; the tests run in order on top of each other.
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
const XSS_TITLE = "<script>alert(1)</script>";

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

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** A button/link whose accessible name contains the label and the item title (in any order). */
function control(page: Page, role: "button" | "link", key: string, title: string) {
  const name = new RegExp(`(?=.*${escapeRe(t(key))})(?=.*${escapeRe(title)})`, "s");
  return page.getByRole(role, { name });
}

/** Item titles in page order (rows are the elements carrying a data-testid-free title text). */
async function order(page: Page, titles: string[]): Promise<string[]> {
  const body = (await page.locator("main").innerText()) ?? "";
  return titles
    .map((title) => ({ title, at: body.indexOf(title) }))
    .filter((x) => x.at >= 0)
    .sort((a, b) => a.at - b.at)
    .map((x) => x.title);
}

test.describe.configure({ mode: "serial" });

test.describe("wishlists", () => {
  let browserRef: Browser;
  const users = {
    alice: { email: uniqueEmail("w-alice"), name: "Alice Owner", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("w-bob"), name: "Bob Viewer", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  let gid = "";
  let editUrl = "";

  test.beforeAll(async ({ browser }) => {
    browserRef = browser;
    for (const user of Object.values(users)) {
      user.context = await browser.newContext();
      user.page = await user.context.newPage();
      await register(user.page, user.email, user.name);
    }
    // Alice creates the group, Bob joins through the invitation link.
    const alice = users.alice.page;
    await alice.goto("/groups/new");
    await waitForHydration(alice);
    await alice.getByLabel(t("group.form.name"), { exact: true }).fill("Wish Group");
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    gid = /\/groups\/(\d+)$/.exec(alice.url())![1];
    editUrl = `/groups/${gid}/wishlist`;
    await waitForHydration(alice);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);
    const bob = users.bob.page;
    await bob.goto(`/join/${code}`);
    await waitForHydration(bob);
    await bob.getByRole("button", { name: t("group.join.submit") }).click();
    await expect(bob).toHaveURL(new RegExp(`/groups/${gid}$`));
  });

  test.afterAll(async () => {
    for (const user of Object.values(users)) await user.context?.close();
  });

  test("AC8 AC10 AC9 AC11 AC12 AC13 AC28: the panel leads to an empty list; adding validates inline", async () => {
    const page = users.alice.page;
    const errors = collectPageErrors(page);
    await page.goto(`/groups/${gid}`);
    await waitForHydration(page);
    await expect(page.getByText(t("group.show.yourWishlist"), { exact: true })).toBeVisible();
    await expect(page.getByText(t("wishlist.panel.membersTitle"))).toBeVisible();
    await page.getByRole("link", { name: t("group.show.editWishlist") }).click();
    await expect(page).toHaveURL(new RegExp(`${editUrl}$`));
    await waitForHydration(page);
    await expect(page.getByText(t("wishlist.edit.noItems"))).toBeVisible();
    // AC10: no clear section for an empty list.
    await expect(page.getByText(t("wishlist.edit.deleteWishlistConfirm"))).toHaveCount(0);

    const title = page.getByLabel(t("wishlist.edit.itemTitle"), { exact: true });
    const description = page.getByLabel(t("wishlist.edit.itemDescription"), { exact: true });
    const link = page.getByLabel(t("wishlist.edit.link"), { exact: true });
    const add = page.getByRole("button", { name: t("wishlist.edit.addItem") });

    // AC11: a blank title is refused inline.
    await title.fill("   ");
    await expectingFailedRequest(errors, async () => {
      await add.click();
      await expect(page.getByText(t("validation.wishlist.title_required"))).toBeVisible();
    });
    // AC12: a script link is refused inline.
    await title.fill("Item A");
    await link.fill("javascript:alert(1)");
    await expectingFailedRequest(errors, async () => {
      await add.click();
      await expect(page.getByText(t("validation.wishlist.link_invalid"))).toBeVisible();
    });

    // AC9 + AC13: scheme-less link is repaired, flash is shown, page stays on the edit URL.
    await description.fill("Desc A");
    await link.fill("www.example.com/x");
    await add.click();
    await expect(page.getByText(t("flash.success.item_added"))).toBeVisible();
    await expect(page).toHaveURL(new RegExp(`${editUrl}$`));
    const anchor = page.locator("main a[href='https://www.example.com/x']").first();
    await expect(anchor).toHaveAttribute("target", "_blank");
    await expect(anchor).toHaveAttribute("rel", "noopener noreferrer nofollow");
    await expect(page.getByText("www.example.com").first()).toBeVisible();

    // AC28: markup in a title is shown as text and nothing runs.
    let dialog = false;
    const onDialog = (d: import("@playwright/test").Dialog) => {
      dialog = true;
      void d.dismiss();
    };
    page.on("dialog", onDialog);
    await waitForHydration(page);
    await title.fill(XSS_TITLE);
    await description.fill("<img src=x onerror=alert(1)>\nline 2");
    await add.click();
    await expect(page.getByText(XSS_TITLE, { exact: true }).first()).toBeVisible();
    await expect(page.locator("main img[src='x']")).toHaveCount(0);
    expect(dialog, "no script ran").toBe(false);
    page.off("dialog", onDialog);

    await title.fill("Item C");
    await add.click();
    await expect(page.getByText("Item C", { exact: true }).first()).toBeVisible();
    await expect(page.getByText(t("wishlist.edit.itemCount", { count: "3", max: "50" }))).toBeVisible();
    expect(errors).toEqual([]);
  });

  test("AC22 AC16 AC18 AC20: reorder, inline edit with its own errors, remove with confirmation", async () => {
    const page = users.alice.page;
    const errors = collectPageErrors(page);
    await page.goto(editUrl);
    await waitForHydration(page);
    const titles = ["Item A", XSS_TITLE, "Item C"];
    expect(await order(page, titles)).toEqual(titles);

    // AC22: the first row has no Move up, the last no Move down.
    await expect(control(page, "button", "wishlist.edit.moveUp", "Item A")).toHaveCount(0);
    await expect(control(page, "button", "wishlist.edit.moveDown", "Item C")).toHaveCount(0);
    await control(page, "button", "wishlist.edit.moveUp", "Item C").click();
    await expect.poll(() => order(page, titles)).toEqual(["Item A", "Item C", XSS_TITLE]);

    // AC16 + AC18: the inline edit form shows its own errors, not the add form's.
    await control(page, "link", "wishlist.edit.edit", "Item C").click();
    await expect(page).toHaveURL(/\?edit=\d+$/);
    await waitForHydration(page);
    const inline = page.locator("main form").filter({ has: page.getByRole("button", { name: t("wishlist.edit.save") }) });
    const addForm = page.locator("main form").filter({ has: page.getByRole("button", { name: t("wishlist.edit.addItem") }) });
    await inline.getByLabel(t("wishlist.edit.link"), { exact: true }).fill("ftp://example.com/x");
    await expectingFailedRequest(errors, async () => {
      await inline.getByRole("button", { name: t("wishlist.edit.save") }).click();
      await expect(inline.getByText(t("validation.wishlist.link_invalid"))).toBeVisible();
    });
    await expect(addForm.getByText(t("validation.wishlist.link_invalid"))).toHaveCount(0);
    await inline.getByLabel(t("wishlist.edit.itemTitle"), { exact: true }).fill("Item C2");
    await inline.getByLabel(t("wishlist.edit.link"), { exact: true }).fill("");
    await inline.getByRole("button", { name: t("wishlist.edit.save") }).click();
    await expect(page.getByText(t("flash.success.item_updated"))).toBeVisible();
    await expect(page.getByText("Item C2", { exact: true }).first()).toBeVisible();
    // Cancel link back to the page without ?edit.
    await expect(page).toHaveURL(new RegExp(`${editUrl}$`));

    // AC20: remove asks for confirmation (cancel keeps, accept removes).
    await waitForHydration(page);
    page.once("dialog", (d) => void d.dismiss());
    await control(page, "button", "wishlist.edit.removeItem", XSS_TITLE).click();
    await expect(page.getByText(XSS_TITLE, { exact: true }).first()).toBeVisible();
    page.once("dialog", (d) => void d.accept());
    await control(page, "button", "wishlist.edit.removeItem", XSS_TITLE).click();
    await expect(page.getByText(t("flash.success.item_deleted"))).toBeVisible();
    await expect(page.getByText(XSS_TITLE, { exact: true })).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("AC1 AC6 AC8 AC27: a member views Alice's list from the panel, read-only, in order", async () => {
    const page = users.bob.page;
    const errors = collectPageErrors(page);
    await page.goto(`/groups/${gid}`);
    await waitForHydration(page);
    await expect(page.getByText("Alice Owner").first()).toBeVisible();
    await expect(page.getByText(users.alice.email)).toHaveCount(0);
    await expect(page.getByText("2 items")).toBeVisible();
    await page.getByRole("link", { name: t("wishlist.panel.view") }).first().click();
    await expect(page).toHaveURL(new RegExp(`/groups/${gid}/wishlist/\\d+$`));
    await waitForHydration(page);

    const titles = ["Item A", "Item C2"];
    expect(await order(page, titles)).toEqual(titles);
    await expect(page.getByText(t("wishlist.view.orderHint"))).toBeVisible();
    await expect(page.getByText(/Last updated/)).toBeVisible();
    await expect(page.getByRole("link", { name: t("wishlist.edit.title") })).toHaveCount(0);
    await expect(page.getByRole("button", { name: t("wishlist.edit.moveUp") })).toHaveCount(0);
    const anchor = page.locator("main a[href='https://www.example.com/x']").first();
    await expect(anchor).toHaveAttribute("rel", "noopener noreferrer nofollow");
    await expect(anchor).toHaveAttribute("target", "_blank");

    // The edit page of his own list is his; Alice's is not reachable by id.
    await page.goto(editUrl);
    await expect(page.getByText("Item A")).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test("AC3 AC7: unknown groups are 404, logged-out visitors go through login and come back", async () => {
    const page = users.bob.page;
    const response = await page.goto("/groups/999999/wishlist/1");
    expect(response?.status()).toBe(404);

    const anon = await browserRef.newContext();
    const p = await anon.newPage();
    await p.goto(editUrl);
    await expect(p).toHaveURL(new RegExp(`/auth/login\\?next=${encodeURIComponent(editUrl)}`));
    await waitForHydration(p);
    await p.getByLabel("Email").fill(users.bob.email);
    await p.getByLabel("Password").fill(PASSWORD);
    await p.getByRole("button", { name: "Login" }).click();
    await expect(p).toHaveURL(new RegExp(`${editUrl}$`));
    await anon.close();
  });

  test("AC23 AC9 AC16 AC20 AC21: without JavaScript the forms work (add, move, edit, remove, clear)", async ({ browser }) => {
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await loginPlain(p, users.alice.email);
    await p.goto(editUrl);

    // Add (a plain form post ends on the edit page with the flash).
    await p.getByLabel(t("wishlist.edit.itemTitle"), { exact: true }).fill("Plain B");
    await p.getByRole("button", { name: t("wishlist.edit.addItem") }).click();
    await expect(p).toHaveURL(new RegExp(`${editUrl}$`));
    await expect(p.getByText(t("flash.success.item_added"))).toBeVisible();
    const titles = ["Item A", "Item C2", "Plain B"];
    expect(await order(p, titles)).toEqual(titles);

    // Move down on the first item: the page reloads in the new order.
    await control(p, "button", "wishlist.edit.moveDown", "Item A").click();
    await expect(p).toHaveURL(new RegExp(`${editUrl}$`));
    expect(await order(p, titles)).toEqual(["Item C2", "Item A", "Plain B"]);

    // Edit through ?edit=ID and the inline form.
    await control(p, "link", "wishlist.edit.edit", "Plain B").click();
    await expect(p).toHaveURL(/\?edit=\d+$/);
    await p.getByLabel(t("wishlist.edit.itemTitle"), { exact: true }).last().fill("Plain B2");
    await p.getByRole("button", { name: t("wishlist.edit.save") }).click();
    await expect(p).toHaveURL(new RegExp(`${editUrl}$`));
    await expect(p.getByText(t("flash.success.item_updated"))).toBeVisible();
    await expect(p.getByText("Plain B2", { exact: true }).first()).toBeVisible();

    // Remove (no confirm without JS).
    await control(p, "button", "wishlist.edit.removeItem", "Plain B2").click();
    await expect(p.getByText(t("flash.success.item_deleted"))).toBeVisible();
    await expect(p.getByText("Plain B2")).toHaveCount(0);

    // Clear the list: the ticked checkbox is the confirmation.
    await p.getByRole("button", { name: t("wishlist.edit.deleteWishlist") }).click();
    await expect(p.getByText(t("wishlist.edit.deleteWishlistConfirm"))).toBeVisible(); // required: nothing happened
    await p.getByLabel(t("wishlist.edit.deleteWishlistConfirm")).check();
    await p.getByRole("button", { name: t("wishlist.edit.deleteWishlist") }).click();
    await expect(p.getByText(t("flash.success.wishlist_deleted"))).toBeVisible();
    await expect(p.getByText(t("wishlist.edit.noItems"))).toBeVisible();
    await plain.close();
    void browserRef;
  });

  test("#57 copies selected own wishes with and without JavaScript", async ({ browser }) => {
    const alice = users.alice.page;
    await alice.goto(editUrl);
    await waitForHydration(alice);
    for (const title of ["Copy A", "Copy B"]) {
      await alice.getByLabel(t("wishlist.edit.itemTitle"), { exact: true }).fill(title);
      await alice.getByRole("button", { name: t("wishlist.edit.addItem") }).click();
      await expect(alice.locator(".list-group-item .fw-semibold").getByText(title, { exact: true })).toBeVisible();
    }
    await alice.goto("/groups/new");
    await waitForHydration(alice);
    await alice.getByLabel(t("group.form.name"), { exact: true }).fill("Copy destination");
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    const target = /\/groups\/(\d+)$/.exec(alice.url())![1];
    for (const js of [true, false]) {
      const context = await browser.newContext({ javaScriptEnabled: js, storageState: await users.alice.context.storageState() });
      const page = await context.newPage();
      const errors = collectPageErrors(page);
      await page.goto(`/groups/${target}/wishlist`);
      if (js) await waitForHydration(page);
      await page.getByLabel(t("wishlist.copy.source")).selectOption(gid);
      await page.getByRole("button", { name: t("wishlist.copy.load") }).click();
      await expect(page.getByRole("checkbox", { name: "Copy A", exact: true })).toBeVisible();
      if (js) await waitForHydration(page);
      await page.getByRole("checkbox", { name: "Copy A", exact: true }).check();
      await page.getByRole("checkbox", { name: "Copy B", exact: true }).check();
      await page.getByRole("button", { name: t("wishlist.copy.submit") }).click();
      await expect(page.getByText(t("flash.success.wishlist_copied"))).toBeVisible();
      await expect(page.locator(".list-group-item .fw-semibold")).toHaveText(js ? ["Copy A", "Copy B"] : ["Copy A", "Copy B", "Copy A", "Copy B"]);
      await page.goto(editUrl);
      await expect(page.locator(".list-group-item .fw-semibold")).toHaveText(["Copy A", "Copy B"]);
      expect(errors).toEqual([]);
      await context.close();
    }
  });

});
