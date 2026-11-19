import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

/**
 * dashboard acceptance tests (docs/specs/dashboard.md, AC2, AC5, AC7): the group cards on
 * mobile (390 px) and desktop, keyboard navigation to the group links, the German empty
 * state and card, and the assignment disclosure (label, open state across a re-read of the
 * panel, no JavaScript). The exhaustive cases (membership, escaping, payload privacy, reset,
 * a failing groups query) are integration tests in server/tests/it/draw_dashboard.rs and
 * draw_*.rs.
 *
 * Texts come from the catalogs by key. The file registers three users (Alice and Bob in
 * English, Dieter in German) and runs the tests in order on top of each other.
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

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const ALPHA = "Dash Alpha";
const BETA = "Dash Beta";

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

/** Registers from a German browser, so the account language is German. */
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

async function loginPlain(page: Page, email: string): Promise<void> {
  await page.goto("/auth/login");
  await page.locator("input[type=email]").fill(email);
  await page.locator("input[type=password]").fill(PASSWORD);
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp(`${DASHBOARD}$`));
}

/** Creates a group with `name` (form labels from `catalog`) and returns its path. */
async function createGroup(page: Page, name: string, catalog = EN): Promise<string> {
  await page.goto("/groups/new");
  await waitForHydration(page);
  await page.getByLabel(text(catalog, "group.form.name"), { exact: true }).fill(name);
  await page.getByRole("button", { name: text(catalog, "group.form.createButton") }).click();
  await expect(page).toHaveURL(/\/groups\/\d+$/);
  await waitForHydration(page);
  return new URL(page.url()).pathname;
}

test.describe.configure({ mode: "serial" });

test.describe("dashboard", () => {
  const users = {
    alice: { email: uniqueEmail("db-alice"), name: "Alice Dash", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("db-bob"), name: "Bob Dash", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  const dieter = { email: uniqueEmail("db-dieter"), name: "Dieter Dash", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page };
  let alphaUrl = "";
  let betaUrl = "";

  test.beforeAll(async ({ browser }) => {
    for (const user of Object.values(users)) {
      user.context = await browser.newContext();
      user.page = await user.context.newPage();
      await register(user.page, user.email, user.name);
    }
    dieter.context = await browser.newContext({ locale: "de-DE" });
    dieter.page = await dieter.context.newPage();
    await registerDe(dieter.page, dieter.email, dieter.name);

    const alice = users.alice.page;
    alphaUrl = await createGroup(alice, ALPHA);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);
    betaUrl = await createGroup(alice, BETA);
    const bob = users.bob.page;
    await bob.goto(`/join/${code}`);
    await waitForHydration(bob);
    await bob.getByRole("button", { name: t("group.join.submit") }).click();
    await expect(bob).toHaveURL(new RegExp(`${alphaUrl}$`));
  });

  test.afterAll(async () => {
    for (const user of [...Object.values(users), dieter]) await user.context?.close();
  });

  for (const [label, width, height] of [["desktop", 1280, 800], ["mobile", 390, 844]] as const) {
    test(`AC2 AC7: the group cards fit the ${label} viewport (${width} px)`, async () => {
      const page = users.alice.page;
      const errors = collectPageErrors(page);
      await page.setViewportSize({ width, height });
      await page.goto(DASHBOARD);
      await waitForHydration(page);

      const cards = page.getByTestId("dashboard-group");
      await expect(cards).toHaveCount(2);
      // Newest first; each card links to its group and shows the Admin and status badges.
      for (const [i, name, url] of [[0, BETA, betaUrl], [1, ALPHA, alphaUrl]] as const) {
        const card = cards.nth(i);
        await expect(card.getByRole("link", { name })).toHaveAttribute("href", url);
        await expect(card.getByText(t("group.admin"), { exact: true })).toBeVisible();
        await expect(card.getByTestId("group-status")).toHaveText(t("group.status.open"));
      }

      // Nothing sticks out sideways; the cards sit side by side on desktop, stacked on mobile.
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      const first = (await cards.nth(0).boundingBox())!;
      const second = (await cards.nth(1).boundingBox())!;
      for (const box of [first, second]) expect(box.x + box.width).toBeLessThanOrEqual(width);
      if (label === "desktop") {
        expect(second.y).toBe(first.y);
        expect(second.x).toBeGreaterThan(first.x);
      } else {
        expect(second.y).toBeGreaterThanOrEqual(first.y + first.height);
      }
      expect(errors, errors.join("\n")).toEqual([]);
      await page.setViewportSize({ width: 1280, height: 720 });
    });
  }

  test("AC2 AC7: keyboard users reach every group link and open the group with Enter", async () => {
    const page = users.alice.page;
    const errors = collectPageErrors(page);
    await page.goto(DASHBOARD);
    await waitForHydration(page);

    const reached: string[] = [];
    for (let i = 0; i < 40 && reached.length < 2; i++) {
      await page.keyboard.press("Tab");
      const href = await page.evaluate(() => {
        const el = document.activeElement;
        return el?.closest("[data-testid=dashboard-group]") ? el.getAttribute("href") : null;
      });
      if (href !== null && !reached.includes(href)) reached.push(href);
    }
    expect(reached).toEqual([betaUrl, alphaUrl]);

    // Focus is on Alpha's link now: Enter opens the group.
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(new RegExp(`${alphaUrl}$`));
    await expect(page.getByRole("heading", { level: 1, name: ALPHA })).toBeVisible();
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC2 AC7: the German dashboard shows the German empty state, then a German card", async () => {
    const page = dieter.page;
    const errors = collectPageErrors(page);
    await page.goto(DASHBOARD);
    await waitForHydration(page);
    await expect(page.locator("html")).toHaveAttribute("lang", "de");
    const section = page.getByTestId("dashboard-groups");
    await expect(section.getByRole("heading", { name: tDe("group.title") })).toBeVisible();
    await expect(page.getByTestId("dashboard-groups-empty")).toContainText(tDe("group.emptyMessage"));
    await expect(page.getByTestId("dashboard-groups-empty")).toContainText(tDe("group.emptyMessageAction"));
    await expect(page.getByTestId("dashboard-group")).toHaveCount(0);

    const url = await createGroup(page, "Wichtelrunde", DE);
    await page.goto(DASHBOARD);
    await waitForHydration(page);
    await expect(page.getByTestId("dashboard-groups-empty")).toHaveCount(0);
    const card = page.getByTestId("dashboard-group");
    await expect(card).toHaveCount(1);
    await expect(card.getByRole("link", { name: "Wichtelrunde" })).toHaveAttribute("href", url);
    await expect(card.getByText(tDe("group.admin"), { exact: true })).toBeVisible();
    await expect(card.getByTestId("group-status")).toHaveText(tDe("group.status.open"));
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC4 AC5: the disclosure says Reveal/Hide, works by keyboard and stays open while the panel re-reads", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(alphaUrl);
    await waitForHydration(alice);
    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.getByTestId("draw-perform").click();
    await expect(alice.getByText(t("flash.success.draw_success"))).toBeVisible();

    const disclosure = alice.getByTestId("assignment-disclosure");
    const summary = disclosure.locator("summary");
    await expect(alice.getByTestId("assignment-receiver")).toBeHidden();
    await expect(summary).toHaveAccessibleName(t("group.draw.revealAssignment"));

    // Keyboard: Enter opens, the label switches to "Hide"; Space closes again.
    await summary.focus();
    await alice.keyboard.press("Enter");
    await expect(alice.getByTestId("assignment-receiver")).toHaveText(users.bob.name);
    await expect(summary).toHaveAccessibleName(t("group.draw.hideAssignment"));
    await alice.keyboard.press("Space");
    await expect(alice.getByTestId("assignment-receiver")).toBeHidden();
    await expect(summary).toHaveAccessibleName(t("group.draw.revealAssignment"));

    // Open it, then make Bob the admin: the page stays and re-reads the group and the draw
    // panel (the card is rebuilt). The disclosure is still open.
    await summary.click();
    await expect(alice.getByTestId("assignment-receiver")).toBeVisible();
    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.getByRole("button", { name: t("group.show.makeAdminFor", { name: users.bob.name }) }).click();
    await expect(alice.getByRole("button", { name: t("group.show.makeAdminFor", { name: users.bob.name }) })).toHaveCount(0);
    await expect(alice.getByTestId("draw-reset")).toHaveCount(0);
    await expect(alice.getByTestId("assignment-receiver")).toBeVisible();
    await expect(summary).toHaveAccessibleName(t("group.draw.hideAssignment"));

    // A new page load starts closed.
    await alice.reload();
    await waitForHydration(alice);
    await expect(alice.getByTestId("assignment-receiver")).toBeHidden();
    await expect(summary).toHaveAccessibleName(t("group.draw.revealAssignment"));
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC5 (no JavaScript): the label switches between Reveal and Hide", async ({ browser }) => {
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const page = await plain.newPage();
    await loginPlain(page, users.bob.email);
    await page.goto(alphaUrl);
    const summary = page.getByTestId("assignment-disclosure").locator("summary");
    await expect(page.getByTestId("assignment-receiver")).toBeHidden();
    await expect(summary).toHaveAccessibleName(t("group.draw.revealAssignment"));
    await summary.click();
    await expect(page.getByTestId("assignment-receiver")).toHaveText(users.alice.name);
    await expect(summary).toHaveAccessibleName(t("group.draw.hideAssignment"));
    await summary.click();
    await expect(page.getByTestId("assignment-receiver")).toBeHidden();
    await expect(summary).toHaveAccessibleName(t("group.draw.revealAssignment"));
    await plain.close();
  });
});
