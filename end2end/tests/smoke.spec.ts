import { expect, test } from "@playwright/test";
import { appOrigin, collectPageErrors, waitForHydration } from "./support/app";
import { firstUrlWithPath, messagesFor, uniqueEmail } from "./support/mailpit";

test.describe("smoke", () => {
  test("home page renders and hydrates", async ({ page }) => {
    const errors = collectPageErrors(page);
    // Narrow viewport: the navbar collapses behind the toggler.
    await page.setViewportSize({ width: 390, height: 844 });

    const res = await page.goto("/");
    expect(res?.status()).toBe(200);
    await expect(page.locator("html")).toHaveAttribute("lang", "en");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Secret Santa");

    await waitForHydration(page);

    // The toggler is driven by Leptos (no Bootstrap collapse JS): it only works hydrated.
    const toggler = page.locator("button.navbar-toggler");
    const menu = page.locator("#navbarNav");
    await expect(menu).not.toHaveClass(/\bshow\b/);
    await expect(toggler).toHaveAttribute("aria-expanded", "false");
    await toggler.click();
    await expect(menu).toHaveClass(/\bshow\b/);
    await expect(toggler).toHaveAttribute("aria-expanded", "true");
    await expect(menu.getByRole("link", { name: "Login" })).toBeVisible();
    await toggler.click();
    await expect(menu).not.toHaveClass(/\bshow\b/);

    expect(errors).toEqual([]);
  });

  test("unknown path gives the 404 page", async ({ page }) => {
    const res = await page.goto("/this/path/does-not-exist");
    expect(res?.status()).toBe(404);
    await expect(page.getByText("Page not found")).toBeVisible();
  });

  test("/health reports ok", async ({ request }) => {
    const res = await request.get("/health");
    expect(res.status()).toBe(200);
    expect(await res.json()).toEqual({ status: "ok", database: "ok" });
  });

  test("Origin check rejects foreign POSTs and accepts the app's own", async ({ request, page, baseURL }) => {
    const foreign = await request.post("/api/session_info", {
      headers: { Origin: "https://evil.example", "Content-Type": "application/x-www-form-urlencoded" },
      data: "",
    });
    expect(foreign.status()).toBe(403);

    const own = await request.post("/api/session_info", {
      headers: { Origin: appOrigin(baseURL), "Content-Type": "application/x-www-form-urlencoded" },
      data: "",
    });
    expect(own.status()).toBe(200);

    // A same-origin fetch from the page: the browser adds Origin itself.
    await page.goto("/");
    const status = await page.evaluate(async () => {
      const r = await fetch("/api/session_info", {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: "",
      });
      return r.status;
    });
    expect(status).toBe(200);
  });

  test("Mailpit helper talks to Mailpit", async () => {
    expect(await messagesFor(uniqueEmail("nobody"))).toEqual([]);
    expect(
      firstUrlWithPath(
        { Text: "Open http://h.example/auth/reset/abc. Thanks", HTML: "" },
        "/auth/reset/",
      ),
    ).toBe("http://h.example/auth/reset/abc");
  });
});
