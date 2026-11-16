import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { appOrigin, collectPageErrors, waitForHydration } from "./support/app";
import { deleteFor, firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * copy-wishes acceptance tests (docs/specs/copy-wishes.md, #76). Main journeys and what
 * depends on hydration, redirects or JavaScript being off; the exhaustive cases (AC4, AC6
 * variants, AC7, AC8, AC10, AC15, AC16, AC17) are integration tests in
 * server/tests/it/wishlist_copy.rs.
 *
 * One user is registered for the whole file (registrations are limited per IP and hour).
 * The tests run in order; each builds its own groups ("Family 2025 [tag]", ...) through the
 * UI and the item server fns, so the lists of earlier tests only show up as older, closed
 * sources. The last test ends the user's session.
 *
 * The AC numbers a test covers are in its name.
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

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const ORIGIN = appOrigin(process.env.BASE_URL);
const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

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
  const link = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
  await page.goto(link);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill(name);
  await page.getByLabel("Password", { exact: true }).fill(PASSWORD);
  await page.getByLabel("Confirm Password").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

/** Creates a group through the UI and returns its id. */
async function createGroup(page: Page, name: string): Promise<string> {
  await page.goto("/groups/new");
  await waitForHydration(page);
  await page.getByLabel(t("group.form.name"), { exact: true }).fill(name);
  await page.getByRole("button", { name: t("group.form.createButton") }).click();
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

/** Runs `step` after the session ended and drops the "status of 401" lines Chromium logs. */
async function withoutSession(errors: string[], step: () => Promise<void>): Promise<void> {
  const before = errors.length;
  await step();
  const rejected = /^console: Failed to load resource: the server responded with a status of 401\b/;
  errors.push(...errors.splice(before).filter((e) => !rejected.test(e)));
}

/** Calls a server fn as the context's user (a "second tab"), returns the JSON body. */
async function api(context: BrowserContext, name: string, fields: Record<string, string>): Promise<unknown> {
  const res = await context.request.post(`/api/${name}`, { form: fields, headers: { Origin: ORIGIN } });
  expect(res.ok(), `${name}: HTTP ${res.status()}`).toBeTruthy();
  return res.json();
}

async function addItem(
  context: BrowserContext,
  group: string,
  title: string,
  description = "",
  link = "",
): Promise<string> {
  const id = await api(context, "add_item", { group_id: group, title, description, link });
  return String(id);
}

/** The spec's fixture, built through the UI: T (target, "Socks"), S1 (Book, socks, Tea), S2 (Mug). */
interface World {
  t: string;
  s1: string;
  s2: string;
  tName: string;
  s1Name: string;
  s2Name: string;
  book: string;
  socks: string;
  tea: string;
  mug: string;
  tEdit: string;
}

let counter = 0;

async function seedWorld(page: Page, context: BrowserContext): Promise<World> {
  counter += 1;
  const tag = `${Date.now().toString(36)}.${counter}`;
  const tName = `Family 2026 [${tag}]`;
  const s2Name = `Office [${tag}]`;
  const s1Name = `Family 2025 [${tag}]`;
  const t = await createGroup(page, tName);
  const s2 = await createGroup(page, s2Name);
  const s1 = await createGroup(page, s1Name);
  const mug = await addItem(context, s2, "Mug");
  const book = await addItem(context, s1, "Book", "Paperback", "https://shop.example/book");
  const socks = await addItem(context, s1, "socks", "", "https://shop.example/socks");
  const tea = await addItem(context, s1, "Tea");
  await addItem(context, t, "Socks");
  return { t, s1, s2, tName, s1Name, s2Name, book, socks, tea, mug, tEdit: `/groups/${t}/wishlist` };
}

/**
 * Seeds a world on a page of its own. The sources are listed newest first and only the
 * first is open (spec), so a test that needs two worlds seeds the second one only when it
 * gets to it: then each world's S1 is the newest source of its own T when it is used.
 */
async function seedNow(context: BrowserContext): Promise<World> {
  const seed = await context.newPage();
  const world = await seedWorld(seed, context);
  await seed.close();
  return world;
}

/** The nested `<details>` of one source list. */
function source(page: Page, groupName: string): Locator {
  return page.locator("#copy details").filter({ has: page.locator("summary", { hasText: groupName }) });
}

function box(scope: Locator | Page, id: string): Locator {
  return scope.locator(`input[name^="item_ids"][value="${id}"]`);
}

function submit(scope: Locator): Locator {
  return scope.getByRole("button", { name: new RegExp(`^${escapeRe(t("wishlist.copy.submit"))}`) });
}

const selectAll = (scope: Locator) => scope.locator("input[type=checkbox]:not([name])");
const copySummary = (page: Page) => page.locator("#copy > summary");
const isOpen = (locator: Locator) => locator.evaluate((el) => (el as HTMLDetailsElement).open);
const listTitles = (page: Page) => page.locator(".list-group-item .fw-semibold");

async function openSection(page: Page): Promise<void> {
  if (!(await isOpen(page.locator("#copy")))) await copySummary(page).click();
  await expect(page.locator("#copy")).toHaveJSProperty("open", true);
}

async function ys(...locators: Locator[]): Promise<number[]> {
  const out: number[] = [];
  for (const locator of locators) out.push((await locator.boundingBox())!.y);
  return out;
}

/** A page of the user without JavaScript. */
async function noJsPage(browser: Browser, context: BrowserContext): Promise<{ page: Page; close: () => Promise<void> }> {
  const noJs = await browser.newContext({ javaScriptEnabled: false, storageState: await context.storageState() });
  return { page: await noJs.newPage(), close: () => noJs.close() };
}

test.describe.configure({ mode: "serial" });

test.describe("copy wishes (#76)", () => {
  let context: BrowserContext;
  const email = uniqueEmail("copy76");

  test.beforeAll(async ({ browser }) => {
    context = await browser.newContext();
    const page = await context.newPage();
    await register(page, email, "Copy Owner");
    await page.close();
  });

  test.afterAll(async () => {
    await context?.close();
    await deleteFor(email);
  });

  test("AC1 AC5 AC6 AC24: the section sits below the add form, closed, lists every source with details, greys out duplicates", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.goto(w.tEdit);
    await waitForHydration(page);

    // AC1: placement, closed, nothing of the old chooser.
    const copy = page.locator("details#copy");
    await expect(copy).toHaveCount(1);
    await expect(page.getByText(t("wishlist.copy.title"), { exact: true })).toHaveCount(1);
    expect(await isOpen(copy)).toBe(false);
    const [add, section, items, clear] = await ys(
      page.locator("#add-title"),
      copy,
      page.getByRole("heading", { name: t("wishlist.edit.itemsHeading") }),
      page.getByText(t("wishlist.edit.deleteWishlist"), { exact: true }).first(),
    );
    expect(add).toBeLessThan(section);
    expect(section).toBeLessThan(items);
    expect(section).toBeLessThan(clear);
    await expect(page.locator("select[name=source]")).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Show wishes" })).toHaveCount(0);

    // AC5: two sources, newest first; S1 open, S2 closed; nothing ticked.
    await copySummary(page).click();
    await expect(page.getByText(t("wishlist.copy.instructions"))).toBeVisible();
    const summaries = page.locator("#copy details > summary");
    await expect(summaries.nth(0)).toContainText(w.s1Name);
    await expect(summaries.nth(0)).toContainText(t("wishlist.panel.itemCount.other", { count: "3" }));
    await expect(summaries.nth(0)).toContainText(/Last updated \d{1,2} \w{3} \d{4}/);
    await expect(summaries.nth(1)).toContainText(w.s2Name);
    await expect(summaries.nth(1)).toContainText(t("wishlist.panel.itemCount.one", { count: "1" }));
    const s1 = source(page, w.s1Name);
    const s2 = source(page, w.s2Name);
    expect(await isOpen(s1)).toBe(true);
    expect(await isOpen(s2)).toBe(false);
    const book = box(s1, w.book);
    const socks = box(s1, w.socks);
    const tea = box(s1, w.tea);
    for (const item of [book, socks, tea]) await expect(item).toBeVisible();
    const [bookY, socksY, teaY] = await ys(book, socks, tea);
    expect(bookY).toBeLessThan(socksY);
    expect(socksY).toBeLessThan(teaY);
    await expect(s1.getByText("Paperback", { exact: true })).toBeVisible();
    await expect(s1.getByText("shop.example").first()).toBeVisible();
    await expect(box(s2, w.mug)).toBeHidden();
    await s2.locator("summary").click();
    await expect(box(s2, w.mug)).toBeVisible();
    await expect(page.locator("#copy input[type=checkbox]:checked")).toHaveCount(0);

    // AC6: "socks" equals the "Socks" on the list; the link does not matter.
    await expect(socks).toBeDisabled();
    await expect(book).toBeEnabled();
    await expect(tea).toBeEnabled();
    await expect(s1.getByText(t("wishlist.copy.alreadyOnList"))).toHaveCount(1);
    expect(errors).toEqual([]);
    await page.close();
  });

  test("AC3 AC24: the section is open for an empty list and for ?copy=1; ?source= is ignored with and without JavaScript", async ({ browser }) => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    const empty = await createGroup(page, `Empty [${w.tName}]`);
    await page.goto(`/groups/${empty}/wishlist`);
    await waitForHydration(page);
    await expect(page.locator("#copy")).toHaveJSProperty("open", true);
    await page.goto(`${w.tEdit}?copy=1`);
    await waitForHydration(page);
    await expect(page.locator("#copy")).toHaveJSProperty("open", true);
    await expect(source(page, w.s1Name)).toHaveJSProperty("open", true);
    // The old parameter: a normal, closed page.
    const response = await page.goto(`${w.tEdit}?source=${w.s1}`);
    expect(response!.status()).toBe(200);
    await waitForHydration(page);
    await expect(page.locator("#copy")).toHaveJSProperty("open", false);
    await expect(page.locator("select[name=source]")).toHaveCount(0);
    expect(errors).toEqual([]);
    await page.close();

    const plain = await noJsPage(browser, context);
    const old = await plain.page.goto(`${w.tEdit}?source=${w.s1}`);
    expect(old!.status()).toBe(200);
    await expect(plain.page.locator("#copy")).toHaveJSProperty("open", false);
    await expect(plain.page.locator("select[name=source]")).toHaveCount(0);
    await plain.close();
  });

  test("AC9 AC20 AC24: copying two wishes with JavaScript closes the section, counts them once and greys them out afterwards", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.goto(w.tEdit);
    await waitForHydration(page);
    await openSection(page);
    const s1 = source(page, w.s1Name);
    await box(s1, w.book).check();
    await box(s1, w.tea).check();
    await submit(s1).click();
    await expect(page.getByText(t("flash.success.wishes_copied.other", { count: "2" }))).toHaveCount(1);
    await expect(page).toHaveURL(new RegExp(`/groups/${w.t}/wishlist$`));
    await expect(page.locator("#copy")).toHaveJSProperty("open", false);
    await expect(listTitles(page)).toHaveText(["Socks", "Book", "Tea"]);
    const book = page.locator(".list-group-item", { hasText: "Book" });
    await expect(book.getByText("Paperback")).toBeVisible();
    await expect(book.locator('a[href="https://shop.example/book"]')).toHaveCount(1);
    // AC20: opened again, what was copied is greyed out.
    await openSection(page);
    await expect(box(s1, w.book)).toBeDisabled();
    await expect(box(s1, w.tea)).toBeDisabled();
    await expect(s1.getByText(t("wishlist.copy.alreadyOnList"))).toHaveCount(3);
    // The source is unchanged.
    await page.goto(`/groups/${w.s1}/wishlist`);
    await expect(listTitles(page)).toHaveText(["Book", "socks", "Tea"]);
    expect(errors).toEqual([]);
    await page.close();
  });

  test("AC9 AC19: without JavaScript the select-all control is hidden and copying by hand works", async ({ browser }) => {
    const seed = await context.newPage();
    const w = await seedWorld(seed, context);
    await seed.close();
    const plain = await noJsPage(browser, context);
    const page = plain.page;
    await page.goto(w.tEdit);
    await copySummary(page).click();
    const s1 = source(page, w.s1Name);
    await expect(selectAll(s1)).toBeHidden();
    await box(s1, w.book).check();
    await box(s1, w.tea).check();
    await submit(s1).click();
    await expect(page).toHaveURL(new RegExp(`/groups/${w.t}/wishlist$`));
    await expect(page.getByText(t("flash.success.wishes_copied.other", { count: "2" }))).toHaveCount(1);
    await expect(page.locator("#copy")).toHaveJSProperty("open", false);
    await expect(listTitles(page)).toHaveText(["Socks", "Book", "Tea"]);
    await plain.close();
  });

  for (const js of [true, false]) {
    test(`AC11 AC12 AC24: failed copies keep the section open with the message (${js ? "with" : "without"} JavaScript)`, async ({ browser }) => {
      test.setTimeout(90_000);
      const w = await seedNow(context);
      const plain = js ? undefined : await noJsPage(browser, context);
      const page = plain ? plain.page : await context.newPage();
      const errors = collectPageErrors(page);

      // AC11: nothing ticked.
      await page.goto(`${w.tEdit}?copy=1`);
      if (js) await waitForHydration(page);
      const s1 = source(page, w.s1Name);
      const before = page.url();
      await expectingFailedRequest(errors, async () => {
        await submit(s1).click();
        await expect(page.getByText(t("validation.wishlist.selection_required"))).toBeVisible();
      });
      await expect(page.locator("#copy")).toHaveJSProperty("open", true);
      await expect(s1).toHaveJSProperty("open", true);
      if (js) {
        await expect(page).toHaveURL(before);
        await expect(s1.getByText(t("validation.wishlist.selection_required"))).toBeVisible();
      } else {
        await expect(page).toHaveURL(new RegExp(`/groups/${w.t}/wishlist\\?copy=1$`));
      }
      await expect(listTitles(page)).toHaveText(["Socks"]);

      // AC12: 49 items on the list, two selected.
      const full = await seedNow(context);
      for (let i = 0; i < 48; i++) await addItem(context, full.t, `Filler ${i}`);
      await page.goto(`${full.tEdit}?copy=1`);
      if (js) await waitForHydration(page);
      const fullS1 = source(page, full.s1Name);
      await box(fullS1, full.book).check();
      await box(fullS1, full.tea).check();
      const tooMany = t("validation.wishlist.too_many_items", { max: "50" });
      await expectingFailedRequest(errors, async () => {
        await submit(fullS1).click();
        await expect(page.getByText(tooMany)).toBeVisible();
      });
      await expect(page.locator("#copy")).toHaveJSProperty("open", true);
      if (js) {
        await expect(fullS1.getByText(tooMany)).toBeVisible();
        await expect(box(fullS1, full.book)).toBeChecked();
        await expect(box(fullS1, full.tea)).toBeChecked();
      } else {
        await expect(page).toHaveURL(new RegExp(`/groups/${full.t}/wishlist\\?copy=1$`));
      }
      await expect(listTitles(page).getByText("Book", { exact: true })).toHaveCount(0);
      expect(errors).toEqual([]);
      if (plain) await plain.close();
      else await page.close();
    });
  }

  for (const js of [true, false]) {
    test(`AC13 AC14 AC24: a stale selection is refused and the section is read again (${js ? "with" : "without"} JavaScript)`, async ({ browser }) => {
      test.setTimeout(90_000);
      const gone = await seedNow(context);
      const plain = js ? undefined : await noJsPage(browser, context);
      const page = plain ? plain.page : await context.newPage();
      const errors = collectPageErrors(page);

      // AC13: "Book" is deleted in another tab after the page was loaded.
      await page.goto(`${gone.tEdit}?copy=1`);
      if (js) await waitForHydration(page);
      const goneS1 = source(page, gone.s1Name);
      await box(goneS1, gone.book).check();
      await box(goneS1, gone.tea).check();
      await api(context, "delete_item", { item_id: gone.book });
      await expectingFailedRequest(errors, async () => {
        await submit(goneS1).click();
        await expect(page.getByText(t("wishlist.copy.changed"))).toBeVisible();
      });
      if (js) await expect(goneS1.getByText(t("wishlist.copy.changed"))).toBeVisible();
      else await expect(page).toHaveURL(new RegExp(`/groups/${gone.t}/wishlist\\?copy=1$`));
      await expect(page.locator("#copy")).toHaveJSProperty("open", true);
      await expect(box(page, gone.book)).toHaveCount(0);
      await expect(box(page, gone.tea)).toHaveCount(1);
      await expect(listTitles(page)).toHaveText(["Socks"]);

      // AC14: "tea" is added to the list in another tab.
      const dup = await seedNow(context);
      await page.goto(`${dup.tEdit}?copy=1`);
      if (js) await waitForHydration(page);
      const dupS1 = source(page, dup.s1Name);
      await box(dupS1, dup.book).check();
      await box(dupS1, dup.tea).check();
      await addItem(context, dup.t, "tea");
      const already = t("validation.wishlist.already_on_list");
      await expectingFailedRequest(errors, async () => {
        await submit(dupS1).click();
        await expect(page.getByText(already)).toBeVisible();
      });
      if (js) await expect(dupS1.getByText(already)).toBeVisible();
      else await expect(page).toHaveURL(new RegExp(`/groups/${dup.t}/wishlist\\?copy=1$`));
      await expect(box(page, dup.tea)).toBeDisabled();
      await expect(box(page, dup.book)).toBeEnabled();
      await expect(listTitles(page).getByText("Book", { exact: true })).toHaveCount(0);
      expect(errors).toEqual([]);
      if (plain) await plain.close();
      else await page.close();
    });
  }

  test("AC19 AC24: select all ticks the enabled boxes of its source only", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.goto(w.tEdit);
    await waitForHydration(page);
    await openSection(page);
    const s1 = source(page, w.s1Name);
    const s2 = source(page, w.s2Name);
    await s2.locator("summary").click();
    await expect(selectAll(s1)).toBeVisible();
    await expect(s1.getByText(t("wishlist.copy.selectAll"))).toBeVisible();
    await selectAll(s1).check();
    await expect(box(s1, w.book)).toBeChecked();
    await expect(box(s1, w.tea)).toBeChecked();
    await expect(box(s1, w.socks)).not.toBeChecked();
    await expect(box(s1, w.socks)).toBeDisabled();
    await expect(box(s2, w.mug)).not.toBeChecked();
    await selectAll(s1).uncheck();
    await expect(box(s1, w.book)).not.toBeChecked();
    await expect(box(s1, w.tea)).not.toBeChecked();
    // And it copies what it ticked.
    await selectAll(s1).check();
    await submit(s1).click();
    await expect(page.getByText(t("flash.success.wishes_copied.other", { count: "2" }))).toBeVisible();
    await expect(listTitles(page)).toHaveText(["Socks", "Book", "Tea"]);
    expect(errors).toEqual([]);
    await page.close();
  });

  test("AC20: the submit button is disabled while the copy runs and the wish is added once", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.route("**/api/copy_wishlist_items", async (route) => {
      await new Promise((r) => setTimeout(r, 1500));
      await route.continue();
    });
    await page.goto(w.tEdit);
    await waitForHydration(page);
    await openSection(page);
    const s1 = source(page, w.s1Name);
    await box(s1, w.book).check();
    await submit(s1).click();
    await expect(submit(s1)).toBeDisabled();
    await expect(page.getByText(t("flash.success.wishes_copied.one", { count: "1" }))).toBeVisible();
    await expect(listTitles(page)).toHaveText(["Socks", "Book"]);
    await openSection(page);
    await expect(box(s1, w.book)).toBeDisabled();
    await expect(s1.getByText(t("wishlist.copy.alreadyOnList"))).toHaveCount(2);
    expect(errors).toEqual([]);
    await page.close();
  });

  test("AC21: adding, moving and inline editing do not close the section or clear the ticks", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.goto(w.tEdit);
    await waitForHydration(page);
    await openSection(page);
    const s1 = source(page, w.s1Name);
    await box(s1, w.book).check();
    const stable = async () => {
      await expect(page.locator("#copy")).toHaveJSProperty("open", true);
      await expect(s1).toHaveJSProperty("open", true);
      await expect(box(s1, w.book)).toBeChecked();
      await expect(box(s1, w.tea)).not.toBeChecked();
    };

    await page.locator("#add-title").fill("Added item");
    await page.getByRole("button", { name: t("wishlist.edit.addItem") }).click();
    await expect(listTitles(page)).toHaveText(["Socks", "Added item"]);
    await stable();

    await page.getByRole("button", { name: `${t("wishlist.edit.moveUp")}: Added item` }).click();
    await expect(listTitles(page)).toHaveText(["Added item", "Socks"]);
    await stable();

    await page.getByRole("link", { name: `${t("wishlist.edit.edit")}: Socks` }).click();
    await expect(page.getByRole("link", { name: t("wishlist.edit.cancel") })).toBeVisible();
    await stable();
    await page.getByRole("link", { name: t("wishlist.edit.cancel") }).click();
    await expect(page.getByRole("link", { name: t("wishlist.edit.cancel") })).toHaveCount(0);
    await stable();
    expect(errors).toEqual([]);
    await page.close();
  });

  test("AC22: on a phone a 255 character title wraps without horizontal scrolling and tapping it ticks the box", async ({ browser }) => {
    const phone = await browser.newContext({
      viewport: { width: 390, height: 844 },
      storageState: await context.storageState(),
    });
    const page = await phone.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    const long = "W".repeat(255);
    const longId = await addItem(context, w.s1, long);
    await page.goto(`${w.tEdit}?copy=1`);
    await waitForHydration(page);
    const s1 = source(page, w.s1Name);
    await expect(s1.getByText(long, { exact: true })).toBeVisible();
    const fits = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
    expect(fits, "no horizontal scrolling").toBe(true);
    const right = await s1.getByText(long, { exact: true }).evaluate((el) => el.getBoundingClientRect().right);
    expect(right).toBeLessThanOrEqual(390);
    await expect(box(s1, longId)).not.toBeChecked();
    await s1.getByText(long, { exact: true }).click();
    await expect(box(s1, longId)).toBeChecked();
    await s1.getByText(long, { exact: true }).click();
    await expect(box(s1, longId)).not.toBeChecked();
    expect(errors).toEqual([]);
    await phone.close();
  });

  test("AC23: markup in titles and descriptions is shown as text and no script runs", async ({ browser }) => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const dialogs: string[] = [];
    page.on("dialog", (dialog) => {
      dialogs.push(dialog.message());
      void dialog.dismiss();
    });
    const w = await seedWorld(page, context);
    const title = "<script>alert(1)</script>";
    const description = "<img src=x onerror=alert(1)>";
    await addItem(context, w.s1, title, description);
    await page.goto(`${w.tEdit}?copy=1`);
    await waitForHydration(page);
    const s1 = source(page, w.s1Name);
    await expect(s1.getByText(title, { exact: true })).toBeVisible();
    await expect(s1.getByText(description, { exact: true })).toBeVisible();
    await page.waitForTimeout(500);
    expect(dialogs).toEqual([]);
    expect(errors).toEqual([]);
    await page.close();

    const plain = await noJsPage(browser, context);
    plain.page.on("dialog", (dialog) => {
      dialogs.push(dialog.message());
      void dialog.dismiss();
    });
    await plain.page.goto(`${w.tEdit}?copy=1`);
    await expect(source(plain.page, w.s1Name).getByText(title, { exact: true })).toBeVisible();
    expect(dialogs).toEqual([]);
    await plain.close();
  });

  test("AC24: the open section reads in German without raw keys", async () => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await context.addCookies([{ name: "locale", value: "de", url: ORIGIN }]);
    try {
      await page.goto(`${w.tEdit}?copy=1`);
      await waitForHydration(page);
      await expect(page.locator("html")).toHaveAttribute("lang", "de");
      await expect(page.getByText(DE["wishlist.copy.title"], { exact: true })).toBeVisible();
      const s1 = source(page, w.s1Name);
      await expect(s1.getByText(DE["wishlist.copy.selectAll"])).toBeVisible();
      await expect(s1.getByText(DE["wishlist.copy.alreadyOnList"])).toHaveCount(1);
      await expect(page.locator("main")).not.toContainText(/wishlist\.copy\.|validation\.wishlist\.|flash\.success\./);
      await box(s1, w.book).check();
      await s1.getByRole("button", { name: new RegExp(`^${escapeRe(DE["wishlist.copy.submit"])}`) }).click();
      const copied = page.getByText(text(DE, "flash.success.wishes_copied.one", { count: "1" }));
      await expect(copied).toBeVisible();
      // Started on ?copy=1: the URL loses it, and the flash is read (and shown) once.
      await expect(page).toHaveURL(new RegExp(`/groups/${w.t}/wishlist$`));
      await expect(copied).toHaveCount(1);
      expect(errors).toEqual([]);
    } finally {
      await context.addCookies([{ name: "locale", value: "en", url: ORIGIN }]);
      await page.close();
    }
  });

  test("AC13: with JavaScript the message stays when the failing source vanishes, also when it was the only one", async ({ browser }) => {
    // A user of its own: its only sources are the two lists below.
    const own = await browser.newContext();
    const ownEmail = uniqueEmail("copy76gone");
    try {
      const page = await own.newPage();
      await register(page, ownEmail, "Gone Owner");
      const errors = collectPageErrors(page);
      const target = await createGroup(page, "Target [gone]");
      const older = await createGroup(page, "Older [gone]");
      const newer = await createGroup(page, "Newer [gone]");
      const lamp = await addItem(own, older, "Lamp");
      const vase = await addItem(own, newer, "Vase");
      await addItem(own, target, "Socks");
      const changed = t("wishlist.copy.changed");

      // Two sources; the open (newer) one is emptied in another tab.
      await page.goto(`/groups/${target}/wishlist?copy=1`);
      await waitForHydration(page);
      const newerSource = source(page, "Newer [gone]");
      await box(newerSource, vase).check();
      await api(own, "clear_wishlist", { group_id: newer, confirm: "true" });
      await expectingFailedRequest(errors, async () => {
        await submit(newerSource).click();
        await expect(page.getByText(changed)).toBeVisible();
      });
      await expect(newerSource).toHaveCount(0);
      await expect(page.getByText(changed)).toBeVisible();
      await expect(source(page, "Older [gone]")).toHaveCount(1);

      // The only source is emptied: the section stays for the message.
      await page.goto(`/groups/${target}/wishlist?copy=1`);
      await waitForHydration(page);
      const olderSource = source(page, "Older [gone]");
      await expect(olderSource).toHaveJSProperty("open", true);
      await box(olderSource, lamp).check();
      await api(own, "clear_wishlist", { group_id: older, confirm: "true" });
      await expectingFailedRequest(errors, async () => {
        await submit(olderSource).click();
        await expect(page.getByText(changed)).toBeVisible();
      });
      await expect(olderSource).toHaveCount(0);
      await expect(page.locator("#copy")).toHaveCount(1);
      await expect(page.getByText(changed)).toBeVisible();
      await expect(listTitles(page)).toHaveText(["Socks"]);
      expect(errors).toEqual([]);
    } finally {
      await own.close();
      await deleteFor(ownEmail);
    }
  });

  // Last: it ends the user's session.
  test("AC18: after the session ended, submitting goes to the login page and back to the open section", async ({ browser }) => {
    const page = await context.newPage();
    const errors = collectPageErrors(page);
    const w = await seedWorld(page, context);
    await page.goto(w.tEdit);
    await waitForHydration(page);
    await openSection(page);
    const s1 = source(page, w.s1Name);
    await box(s1, w.book).check();
    await logoutElsewhere(context);
    await withoutSession(errors, async () => {
      await submit(s1).click();
      await expect(page).toHaveURL(
        new RegExp(`/auth/login\\?next=${escapeRe(encodeURIComponent(`/groups/${w.t}/wishlist?copy=1`))}$`),
      );
    });
    expect(errors).toEqual([]);

    // A plain post without a session: 302 to the same login URL; nothing is written.
    const anonymous = await browser.newContext();
    const res = await anonymous.request.post("/api/copy_wishlist_items", {
      form: { group_id: w.t, source_group_id: w.s1, "item_ids[0]": w.book },
      headers: { Origin: ORIGIN, Accept: "text/html" },
      maxRedirects: 0,
    });
    expect(res.status()).toBe(302);
    expect(res.headers()["location"]).toBe(`/auth/login?next=${encodeURIComponent(`/groups/${w.t}/wishlist?copy=1`)}`);
    await anonymous.close();
  });
});
