import { expect, test } from "@playwright/test";
import { collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, uniqueEmail, waitForLatest } from "./support/mailpit";

test("installed-app manifest registers text and URL intake", async ({ request }) => {
  const response = await request.get("/manifest.webmanifest");
  expect(response.ok()).toBeTruthy();
  expect((await response.json()).share_target).toEqual({
    action: "/share-target",
    method: "GET",
    params: { title: "title", text: "text", url: "url" },
  });
});

test("a long shared product link survives login and registration without automatic saving", async ({ page }) => {
  const errors = collectPageErrors(page);
  const product = `https://shop.example/product?tracking=${"x".repeat(900)}`;
  const params = new URLSearchParams({ title: "Birthday present", text: `Look at ${product}` });
  await page.goto(`/share-target?${params}`);
  await expect(page).toHaveURL(/\/auth\/login\?next=/);
  const next = new URL(page.url()).searchParams.get("next")!;
  expect(next).toMatch(/^\/wishes\/shared\?id=[\w-]+$/);
  expect(next.length).toBeLessThan(100);
  await waitForHydration(page);
  await page.getByRole("link", { name: "Register", exact: true }).last().click();
  await waitForHydration(page);
  const email = uniqueEmail("share-target");
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Register", exact: true }).click();
  await expect(page.getByText("Check your inbox")).toBeVisible();
  const confirm = firstUrlWithPath(await waitForLatest(email), "/auth/register/confirm/")!;
  await page.goto(confirm);
  await waitForHydration(page);
  await page.getByLabel("Full Name").fill("Share Recipient");
  await page.getByLabel("Password", { exact: true }).fill("share-password-1");
  await page.getByLabel("Confirm Password").fill("share-password-1");
  await page.locator("main form button[type=submit]").click();
  await expect(page).toHaveURL(new RegExp("/wishes/shared\\?id="));
  await waitForHydration(page);
  await expect(page.getByRole("heading", { name: "Add a shared link", exact: true })).toBeVisible();
  await expect(page.getByText("No wishlist is available yet.", { exact: false })).toBeVisible();
  const id = new URL(page.url()).searchParams.get("id")!;
  const draft = await page.evaluate(async (id) => {
    const response = await fetch("/api/get_shared_draft", {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ id }),
    });
    return response.json();
  }, id);
  expect(draft.title).toBe("Birthday present");
  expect(draft.link).toBe(product);
  await expect(page.getByText("Safari on iPhone and iPad cannot offer this PWA", { exact: false })).toBeVisible();
  expect(errors).toEqual([]);
});

test("iPhone fallback guidance is public and works without JavaScript", async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  await page.goto("/install");
  await expect(page.getByText("Copy the product link", { exact: false })).toBeVisible();
  await page.getByRole("link", { name: "Add a shared link", exact: true }).click();
  await expect(page).toHaveURL(/\/auth\/login\?next=%2Fwishes%2Fshared/);
  await context.close();
});
