import { env, exports } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";

export const origin = "https://localhost:8787";
export function commandId() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  let time = BigInt(Date.now());
  for (let i = 5; i >= 0; i--) { bytes[i] = Number(time & 255n); time >>= 8n; }
  bytes[6] = (bytes[6] & 15) | 112;
  bytes[8] = (bytes[8] & 63) | 128;
  const h = Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;
}


export function password() {
  return String.fromCharCode(...crypto.getRandomValues(new Uint8Array(24)).map(b => 33 + b % 94));
}
export function cookie(response) {
  const raw = response.headers.get("Set-Cookie");
  if (!raw) throw new Error("expected a session cookie");
  return raw.split(";", 1)[0];
}
export function stub() { return env.ACCOUNTS.get(env.ACCOUNTS.idFromName("accounts")); }
export async function inOwner(operation) { return runInDurableObject(stub(), operation); }
export async function resetStorage() {
  await inOwner(async (_instance, state) => { await state.storage.deleteAll(); });
  const response = await exports.default.fetch(`${origin}/api/session`);
  if (response.status !== 200) throw new Error("account storage initialization failed");
}
export async function post(path, body, { session, id = commandId(), caller = "192.0.2.10" } = {}) {
  const headers = { Origin: origin, "CF-Connecting-IP": caller };
  if (session) headers.Cookie = session;
  if (id) headers["Idempotency-Key"] = id;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  return exports.default.fetch(`${origin}${path}`, { method: "POST", headers, body: body === undefined ? undefined : JSON.stringify(body) });
}
export async function current(session) {
  return exports.default.fetch(`${origin}/api/session`, { headers: session ? { Cookie: session } : {} });
}
export async function seedPending(username = "ExactCaseUser") {
  const accountId = commandId();
  await inOwner((_instance, state) => {
    state.storage.sql.exec("INSERT INTO accounts(account_id,username,role,status,verifier,credential_epoch,created_at,password_set_at,disabled_at) VALUES(?,?,'host','pending_enrollment',NULL,0,?,NULL,NULL)", accountId, username, Date.now());
  });
  return { accountId, username, token: await seedLink(accountId, "enrollment", 0) };
}
export async function seedLink(accountId, purpose, epoch) {
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  const token = btoa(String.fromCharCode(...bytes)).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const linkId = commandId();
  await inOwner((_instance, state) => {
    const now = Date.now();
    state.storage.sql.exec("INSERT INTO access_links(link_id,account_id,purpose,token_verifier,credential_epoch,issued_at,expires_at,consumed_at,revoked_at) VALUES(?,?,?,?,?,?,?,NULL,NULL)", linkId, accountId, purpose, digest, epoch, now, now + 86400000);
  });
  return token;
}
export async function enroll() {
  const fixture = await seedPending();
  const redemption = await post("/api/auth/enrollment/redeem", { enrollment_token: fixture.token });
  if (redemption.status !== 200) throw new Error("fixture redemption failed");
  const secret = password();
  const completion = await post("/api/auth/enrollment/complete", { new_password: secret }, { session: cookie(redemption) });
  if (completion.status !== 200) throw new Error("fixture enrollment failed");
  return { ...fixture, password: secret, session: cookie(completion) };
}
