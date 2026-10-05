import { exports } from "cloudflare:workers";
import { expect, it } from "vitest";

import { commandId, origin, resetStorage } from "./fixtures.js";
import { beforeEach } from "vitest";
beforeEach(resetStorage);
it("returns an anonymous no-store session through actual Wasm and SQLite", async () => {
  const response = await exports.default.fetch(`${origin}/api/session`);
  expect(response.status).toBe(200);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  const body = await response.json();
  expect(body.authenticated).toBe(false);
  expect(body.session).toBe(null);
  expect(response.headers.has("Set-Cookie")).toBe(false);
});

it("does not expose private Durable Object paths as public routes", async () => {
  const response = await exports.default.fetch(`${origin}/internal/auth`);
  expect(response.status).toBe(404);
  expect(response.headers.get("Content-Type")).toContain("application/json");
});

it("rejects an absent Origin before any credential operation", async () => {
  const response = await exports.default.fetch(`${origin}/api/auth/login`, {
    method: "POST", headers: { "Content-Type": "application/json", "Idempotency-Key": commandId() },
    body: JSON.stringify({ username: "ExactCaseUser", password: String.fromCharCode(...new Uint8Array(10)) }),
  });
  expect(response.status).toBe(403);
  expect(response.headers.has("Set-Cookie")).toBe(false);
});

it("logout with no cookie is idempotent and clears only the host session cookie", async () => {
  for (let i = 0; i < 2; i++) {
    const response = await exports.default.fetch(`${origin}/api/auth/logout`, { method: "POST", headers: { Origin: origin } });
    expect(response.status).toBe(200);
    expect((await response.json()).logged_out).toBe(true);
    const cookie = response.headers.get("Set-Cookie");
    expect(cookie).toContain("__Host-brews_session=");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("Secure");
    expect(cookie).toContain("SameSite=Lax");
    expect(cookie).toContain("Max-Age=0");
    expect(cookie).not.toContain("Domain=");
  }
});
