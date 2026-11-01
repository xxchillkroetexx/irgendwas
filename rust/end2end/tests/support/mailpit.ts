/**
 * Mailpit API client for e2e tests (mirrors `app::test_support::mailpit`).
 * MAILPIT_URL defaults to the dev compose's API at http://127.0.0.1:8026.
 *
 * Tests run in parallel against one Mailpit: send to a unique address (`uniqueEmail`)
 * and query by it. `deleteAll` affects every test.
 *
 *   const to = uniqueEmail("reset");
 *   // ... trigger the mail ...
 *   const mail = await waitForLatest(to);
 *   const link = firstUrlWithPath(mail, "/auth/reset/");
 */

const API = (process.env.MAILPIT_URL ?? "http://127.0.0.1:8026").replace(/\/$/, "");

export interface Address {
  Name: string;
  Address: string;
}

export interface MessageSummary {
  ID: string;
  Subject: string;
  To: Address[];
}

export interface Mail extends MessageSummary {
  From: Address;
  Text: string;
  HTML: string;
}

let counter = 0;

/** An address no other test uses. */
export function uniqueEmail(prefix: string): string {
  counter += 1;
  const rand = Math.random().toString(36).slice(2, 8);
  return `${prefix}-${process.pid}-${Date.now()}-${counter}-${rand}@mail.test`;
}

async function api(path: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(`${API}${path}`, init);
  if (!res.ok) throw new Error(`mailpit ${init?.method ?? "GET"} ${path}: HTTP ${res.status}`);
  return res;
}

/** Messages sent to exactly `address` (case-insensitive), newest first. */
export async function messagesFor(address: string): Promise<MessageSummary[]> {
  const query = encodeURIComponent(`to:"${address}"`);
  const res = await api(`/api/v1/search?query=${query}&limit=100`);
  const body = (await res.json()) as { messages: MessageSummary[] };
  const wanted = address.toLowerCase();
  return body.messages.filter((m) => (m.To ?? []).some((a) => a.Address.toLowerCase() === wanted));
}

export async function message(id: string): Promise<Mail> {
  return (await (await api(`/api/v1/message/${id}`)).json()) as Mail;
}

/** The newest message to `address`, or `undefined`. */
export async function latestFor(address: string): Promise<Mail | undefined> {
  const [first] = await messagesFor(address);
  return first ? message(first.ID) : undefined;
}

/** Polls until a message to `address` arrives (mails are sent off the request path). */
export async function waitForLatest(address: string, timeoutMs = 10_000): Promise<Mail> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const mail = await latestFor(address);
    if (mail) return mail;
    if (Date.now() > deadline) throw new Error(`no mail to ${address} within ${timeoutMs} ms`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

/** First absolute URL whose path starts with `pathPrefix`, from the text body, else HTML. */
export function firstUrlWithPath(mail: Pick<Mail, "Text" | "HTML">, pathPrefix: string): string | undefined {
  for (const body of [mail.Text ?? "", mail.HTML ?? ""]) {
    for (const match of body.matchAll(/https?:\/\/[^\s"'<>]+/g)) {
      const url = match[0].replace(/[.,;:!?)]+$/, "").replaceAll("&amp;", "&");
      try {
        if (new URL(url).pathname.startsWith(pathPrefix)) return url;
      } catch {
        // not a URL after all
      }
    }
  }
  return undefined;
}

export async function deleteFor(address: string): Promise<void> {
  const ids = (await messagesFor(address)).map((m) => m.ID);
  if (ids.length === 0) return;
  await api("/api/v1/messages", {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ IDs: ids }),
  });
}

/** Deletes every message in Mailpit (affects parallel tests). */
export async function deleteAll(): Promise<void> {
  await api("/api/v1/messages", { method: "DELETE" });
}
