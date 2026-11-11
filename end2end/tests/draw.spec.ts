import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test, type Browser, type BrowserContext, type Page } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, message, messagesFor, uniqueEmail, waitForLatest, type Mail } from "./support/mailpit";

/**
 * draw slice acceptance tests (docs/specs/draw.md). Only the main journeys and what
 * depends on hydration, confirm dialogs, redirects or JavaScript being off; the exhaustive
 * cases (algorithm, authorization, secrecy, mails, races) are integration tests in
 * server/tests/it/draw_*.rs.
 *
 * Texts come from the catalogs by key. The file registers exactly three users in `beforeAll`
 * (Alice the admin, Bob and Carol members) and runs the tests in order on top of each other.
 *
 * The page contract (data-testid) is documented in server/tests/it/common/draw.rs.
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
/** `<base>.one` for 1, else `<base>.other`. */
const tp = (base: string, n: number, params: Record<string, string> = {}) =>
  t(`${base}.${n === 1 ? "one" : "other"}`, params);

const PASSWORD = "e2e-password-1";
const DASHBOARD = "/user/dashboard";
const GROUP_NAME = "Draw Group";

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

/** Draw-result mails to `address`: waits until `count` have arrived. */
async function drawMails(address: string, count: number, subject: string): Promise<Mail[]> {
  const deadline = Date.now() + 15_000;
  for (;;) {
    const wanted = (await messagesFor(address)).filter((m) => m.Subject === subject);
    if (wanted.length >= count) return Promise.all(wanted.map((m) => message(m.ID)));
    if (Date.now() > deadline) throw new Error(`expected ${count} mails "${subject}" to ${address}, got ${wanted.length}`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

// Looked up lazily: the catalogs get these keys together with the implementation.
const drawSubject = () => t("email.drawResult.subject", { group: GROUP_NAME });
const resetSubject = () => t("email.drawReset.subject", { group: GROUP_NAME });

test.describe.configure({ mode: "serial" });

test.describe("draw", () => {
  let browserRef: Browser;
  const users = {
    alice: { email: uniqueEmail("d-alice"), name: "Alice Santa", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    bob: { email: uniqueEmail("d-bob"), name: "Bob Elf", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
    carol: { email: uniqueEmail("d-carol"), name: "Carol Reindeer", context: undefined as unknown as BrowserContext, page: undefined as unknown as Page },
  };
  type Key = keyof typeof users;
  let gid = "";
  let groupUrl = "";
  const receivers: Partial<Record<Key, string>> = {};

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
    // A registration deadline in the future: the confirm dialog warns that drawing closes it.
    await alice.getByLabel(t("group.form.registrationDeadline"), { exact: true }).fill("2099-12-31");
    await alice.getByRole("button", { name: t("group.form.createButton") }).click();
    await expect(alice).toHaveURL(/\/groups\/\d+$/);
    gid = /\/groups\/(\d+)$/.exec(alice.url())![1];
    groupUrl = `/groups/${gid}`;
    await waitForHydration(alice);
    const link = alice.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);
    for (const user of [users.bob, users.carol]) {
      await user.page.goto(`/join/${code}`);
      await waitForHydration(user.page);
      await user.page.getByRole("button", { name: t("group.join.submit") }).click();
      await expect(user.page).toHaveURL(new RegExp(`${groupUrl}$`));
    }
  });

  test.afterAll(async () => {
    for (const user of Object.values(users)) await user.context?.close();
  });

  test("AC46 AC15: before the draw Bob waits and Alice has the one button, with the small-group hint", async () => {
    const bob = users.bob.page;
    const errors = collectPageErrors(bob);
    await bob.goto(groupUrl);
    await waitForHydration(bob);
    await expect(bob.getByText(t("group.draw.pending"))).toBeVisible();
    await expect(bob.getByTestId("draw-perform")).toHaveCount(0);
    await expect(bob.getByTestId("draw-reset")).toHaveCount(0);

    const alice = users.alice.page;
    const aliceErrors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);
    await expect(alice.getByRole("button", { name: t("group.show.performDraw") })).toHaveCount(1);
    await expect(alice.getByTestId("draw-perform")).toBeEnabled();
    await expect(alice.getByText(tp("group.draw.ready", 3, { count: "3" }))).toBeVisible();
    await expect(alice.getByText(t("group.draw.smallGroupHint"))).toBeVisible();
    expect(errors, errors.join("\n")).toEqual([]);
    expect(aliceErrors, aliceErrors.join("\n")).toEqual([]);
  });

  test("AC14 AC20 AC46: the confirm dialog guards the draw; afterwards Alice sees only her own assignment", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);

    // Dismissing the confirm changes nothing. It warns that drawing closes the registration.
    alice.once("dialog", (dialog) => {
      expect(dialog.message()).toContain(t("group.show.performDrawConfirm"));
      expect(dialog.message()).toContain(t("group.draw.confirmRegistrationOpen", { date: "31 Dec 2099" }));
      void dialog.dismiss();
    });
    await alice.getByTestId("draw-perform").click();
    await expect(alice.getByTestId("draw-perform")).toHaveCount(1);
    await expect(alice.getByTestId("my-assignment")).toHaveCount(0);

    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.getByTestId("draw-perform").click();
    await expect(alice.getByText(t("flash.success.draw_success"))).toBeVisible();
    await expect(alice).toHaveURL(new RegExp(`${groupUrl}$`));

    await expect(alice.getByTestId("my-assignment")).toHaveCount(1);
    const receiver = ((await alice.getByTestId("assignment-receiver").textContent()) ?? "").trim();
    expect([users.bob.name, users.carol.name]).toContain(receiver);
    receivers.alice = receiver;
    await expect(alice.getByTestId("assignment-wishlist")).toHaveAttribute("href", new RegExp(`^/groups/${gid}/wishlist/\\d+$`));
    await expect(alice.getByTestId("draw-perform")).toHaveCount(0);
    await expect(alice.getByTestId("draw-reset")).toBeVisible();
    const invitation = alice.getByTestId("group-invitation");
    await expect(invitation).toContainText(t("group.show.registrationClosedHint"));
    await expect(invitation.locator("input, button, code")).toHaveCount(0);
    await expect(alice.getByRole("button", { name: t("group.show.regenerateCode") })).toHaveCount(0);
    // The admin's page names no other pairing: the third member's name appears only in the
    // member lists, never inside the draw panel.
    const other = receiver === users.bob.name ? users.carol.name : users.bob.name;
    await expect(alice.getByTestId("draw-panel")).not.toContainText(other);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("the mobile group page puts the assignment and wishlist before administration", async () => {
    const page = users.alice.page;
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(groupUrl);
    const draw = await page.getByTestId("draw-panel").boundingBox();
    const wishlist = await page.getByTestId("group-wishlist").boundingBox();
    const details = await page.getByTestId("group-details").boundingBox();
    const invitation = await page.getByTestId("group-invitation").boundingBox();
    expect(draw && wishlist && details && invitation).toBeTruthy();
    expect(draw!.y).toBeLessThan(wishlist!.y);
    expect(wishlist!.y).toBeLessThan(details!.y);
    expect(details!.y).toBeLessThan(invitation!.y);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.setViewportSize({ width: 1280, height: 720 });
  });

  test("AC14 AC31: every member sees their own receiver, gets one mail naming them, and finds them on the dashboard", async () => {
    for (const key of ["bob", "carol"] as const) {
      const page = users[key].page;
      const errors = collectPageErrors(page);
      await page.goto(groupUrl);
      await waitForHydration(page);
      await expect(page.getByTestId("my-assignment")).toHaveCount(1);
      const receiver = ((await page.getByTestId("assignment-receiver").textContent()) ?? "").trim();
      expect(receiver).not.toBe(users[key].name);
      receivers[key] = receiver;
      for (const id of ["draw-perform", "draw-reset", "draw-resend", "draw-notifications"]) {
        await expect(page.getByTestId(id)).toHaveCount(0);
      }

      await page.goto(DASHBOARD);
      await waitForHydration(page);
      const row = page.getByTestId("dashboard-assignment");
      await expect(row).toHaveCount(1);
      await expect(row).toContainText(GROUP_NAME);
      await expect(row).toContainText(receiver);
      await expect(row.getByRole("link", { name: GROUP_NAME })).toHaveAttribute("href", groupUrl);
      await expect(row.locator(`a[href^="/groups/${gid}/wishlist/"]`)).toHaveCount(1);
      expect(errors, errors.join("\n")).toEqual([]);
    }

    // The draw is a permutation: three different receivers, nobody draws themselves.
    expect(new Set(Object.values(receivers)).size).toBe(3);

    // One mail per member, naming their own receiver and nobody else's.
    const names = Object.values(users).map((u) => u.name);
    for (const key of ["alice", "bob", "carol"] as const) {
      const [mail] = await drawMails(users[key].email, 1, drawSubject());
      const content = `${mail.Text}\n${mail.HTML}`;
      expect(content).toContain(receivers[key]!);
      const third = names.find((n) => n !== users[key].name && n !== receivers[key])!;
      expect(content, `mail of ${key} must not name ${third}`).not.toContain(third);
      expect(firstUrlWithPath(mail, `/groups/${gid}`), "link to the group").toBeTruthy();
    }
  });

  test("AC34 AC39: the admin sees how many mails went out", async () => {
    const alice = users.alice.page;
    await expect(async () => {
      await alice.goto(groupUrl);
      await expect(alice.getByText(tp("group.draw.notifications.summary", 3, { sent: "3", total: "3" }))).toBeVisible({ timeout: 1000 });
    }).toPass({ timeout: 15_000 });
    await expect(alice.getByTestId("draw-resend")).toHaveCount(0);
  });

  test("AC40: the admin resets after a confirm; members are told and see the pending state again", async () => {
    const alice = users.alice.page;
    const errors = collectPageErrors(alice);
    await alice.goto(groupUrl);
    await waitForHydration(alice);

    alice.once("dialog", (dialog) => {
      expect(dialog.message()).toBe(t("group.draw.resetConfirm"));
      void dialog.dismiss();
    });
    await alice.getByTestId("draw-reset").click();
    await expect(alice.getByTestId("my-assignment")).toHaveCount(1);

    alice.once("dialog", (dialog) => void dialog.accept());
    await alice.getByTestId("draw-reset").click();
    await expect(alice.getByText(t("flash.success.draw_reset"))).toBeVisible();
    await expect(alice.getByTestId("my-assignment")).toHaveCount(0);
    await expect(alice.getByTestId("draw-perform")).toBeEnabled();

    for (const key of ["bob", "carol"] as const) {
      const [mail] = await drawMails(users[key].email, 1, resetSubject());
      const content = `${mail.Text}\n${mail.HTML}`;
      expect(content).toContain(users.alice.name);
      const third = key === "bob" ? users.carol.name : users.bob.name;
      expect(content, "a reset mail names no receiver").not.toContain(third);
      const page = users[key].page;
      await page.goto(groupUrl);
      await waitForHydration(page);
      await expect(page.getByText(t("group.draw.pending"))).toBeVisible();
      await page.goto(DASHBOARD);
      await waitForHydration(page);
      await expect(page.getByText(t("user.dashboard.assignments.empty"))).toBeVisible();
    }
    // Alice reset it herself: she gets no reset mail.
    expect((await messagesFor(users.alice.email)).filter((m) => m.Subject === resetSubject())).toHaveLength(0);
    expect(errors, errors.join("\n")).toEqual([]);
  });

  test("AC14 AC40 (no JavaScript): the admin draws and resets with plain forms, members see their card", async ({ browser }) => {
    const plain = await browser.newContext({ javaScriptEnabled: false });
    const p = await plain.newPage();
    await loginPlain(p, users.alice.email);
    await p.goto(groupUrl);

    await p.getByRole("button", { name: t("group.show.performDraw") }).click();
    await expect(p).toHaveURL(new RegExp(`${groupUrl}$`));
    await expect(p.getByText(t("flash.success.draw_success"))).toBeVisible();
    await expect(p.getByTestId("my-assignment")).toHaveCount(1);
    await expect(p.getByTestId("assignment-receiver")).not.toBeEmpty();

    // Bob (also without JavaScript) sees his card.
    const bobPlain = await browser.newContext({ javaScriptEnabled: false });
    const b = await bobPlain.newPage();
    await loginPlain(b, users.bob.email);
    await b.goto(groupUrl);
    await expect(b.getByTestId("my-assignment")).toHaveCount(1);
    await expect(b.getByTestId("draw-reset")).toHaveCount(0);
    await bobPlain.close();

    // A second draw is not offered; reset works as a plain form too.
    await expect(p.getByTestId("draw-perform")).toHaveCount(0);
    await p.getByRole("button", { name: t("group.draw.reset") }).click();
    await expect(p).toHaveURL(new RegExp(`${groupUrl}$`));
    await expect(p.getByText(t("flash.success.draw_reset"))).toBeVisible();
    await expect(p.getByTestId("my-assignment")).toHaveCount(0);
    await expect(p.getByTestId("draw-perform")).toBeVisible();
    await plain.close();
    void browserRef;
  });

  test("AC47: with a German account the panel and the confirm dialog are German", async ({ browser }) => {
    // The account language beats the browser language after login, so the German viewer
    // registers from a German browser. Dieter admins a second group that Bob joins.
    const de = await browser.newContext({ locale: "de-DE" });
    const p = await de.newPage();
    const errors = collectPageErrors(p);
    await registerDe(p, uniqueEmail("d-dieter"), "Dieter Wichtel");
    await p.goto("/groups/new");
    await waitForHydration(p);
    await p.getByLabel(tDe("group.form.name"), { exact: true }).fill("Gruppe Zwei");
    await p.getByRole("button", { name: tDe("group.form.createButton") }).click();
    await expect(p).toHaveURL(/\/groups\/\d+$/);
    const url = new URL(p.url()).pathname;
    await waitForHydration(p);
    await expect(p.locator("html")).toHaveAttribute("lang", "de");

    // Alone in the group: singular "ready" line and the blocker, both German.
    await expect(p.getByText(text(DE, "group.draw.ready.one", { count: "1" }))).toBeVisible();
    await expect(p.getByText(tDe("group.draw.blocked.tooFewMembers").replace("{min}", "2").replace("{count}", "1"))).toBeVisible();
    await expect(p.getByTestId("draw-perform")).toBeDisabled();

    const link = p.locator("main").getByText(/\/join\/[A-Z2-9]{8}$/).first();
    const code = ((await link.textContent()) ?? "").trim().slice(-8);
    const bob = users.bob.page;
    await bob.goto(`/join/${code}`);
    await waitForHydration(bob);
    await bob.getByRole("button", { name: t("group.join.submit") }).click();
    await expect(bob).toHaveURL(new RegExp(`${url}$`));

    await p.goto(url);
    await waitForHydration(p);
    await expect(p.getByText(tDe("group.draw.title"))).toBeVisible();
    await expect(p.getByText(text(DE, "group.draw.ready.other", { count: "2" }))).toBeVisible();
    await expect(p.getByText(tDe("group.draw.smallGroupHint"))).toBeVisible();
    await expect(p.getByText(tDe("group.show.performDraw"))).toBeVisible();
    p.once("dialog", (dialog) => {
      expect(dialog.message()).toContain(tDe("group.show.performDrawConfirm"));
      void dialog.dismiss();
    });
    await p.getByTestId("draw-perform").click();
    await expect(p.getByTestId("draw-perform")).toHaveCount(1);
    expect(errors, errors.join("\n")).toEqual([]);
    await de.close();
  });
});
