import { expect, type Page } from "@playwright/test";

/**
 * Waits until the wasm bundle has hydrated the page (the frontend sets
 * `<html data-hydrated>` after `hydrate_body`). Interact with Leptos-driven elements
 * only after this, or clicks can land before the event handlers exist.
 */
export async function waitForHydration(page: Page): Promise<void> {
  await expect(page.locator("html[data-hydrated]")).toHaveCount(1);
}

/** Collects console errors and uncaught exceptions (hydration mismatches show up here). */
export function collectPageErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("pageerror", (err) => errors.push(`pageerror: ${err.message}`));
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(`console: ${msg.text()}`);
  });
  return errors;
}

/** The origin the server accepts for POSTs (same as APP_URL). */
export function appOrigin(baseURL: string | undefined): string {
  return new URL(baseURL ?? "http://localhost:3000").origin;
}

/**
 * Navigates inside the app, as a click on an in-app link would (the router handles it,
 * no document load). `noscroll` adds `data-noscroll`: the router then keeps the scroll
 * position instead of scrolling to the top.
 */
export async function clientNavigate(page: Page, href: string, noscroll = false): Promise<void> {
  await page.evaluate(
    ([href, noscroll]) => {
      const a = document.createElement("a");
      a.href = href;
      if (noscroll) a.setAttribute("data-noscroll", "");
      document.body.appendChild(a);
      a.click();
      a.remove();
    },
    [href, noscroll] as const,
  );
}

/**
 * Makes the page scrollable (a spacer below the content) and scrolls it to `y`
 * immediately, so scroll assertions start from a settled, nonzero position.
 */
export async function scrollPageTo(page: Page, y: number): Promise<void> {
  await page.evaluate(async (y) => {
    const spacer = document.createElement("div");
    spacer.style.height = "2000px";
    // The body is a flex column: keep the spacer from shrinking to the viewport.
    spacer.style.flexShrink = "0";
    document.body.appendChild(spacer);
    // Bootstrap enables smooth scrolling.
    document.documentElement.style.scrollBehavior = "auto";
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    window.scrollTo(0, y);
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }, y);
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(y);
}
