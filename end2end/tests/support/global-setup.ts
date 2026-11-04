import { request, type FullConfig } from "@playwright/test";

/** Waits until the server answers GET /health (cargo-leptos does not wait for it). */
export default async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use.baseURL ?? "http://localhost:3000";
  const ctx = await request.newContext({ baseURL });
  const deadline = Date.now() + 120_000;
  let last = "no response";
  try {
    while (Date.now() < deadline) {
      try {
        const res = await ctx.get("/health", { timeout: 2_000 });
        if (res.ok()) return;
        last = `HTTP ${res.status()}`;
      } catch (err) {
        last = String(err);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    throw new Error(`server at ${baseURL} not healthy: ${last}`);
  } finally {
    await ctx.dispose();
  }
}
