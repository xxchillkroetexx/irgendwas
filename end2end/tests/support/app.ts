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
