import { expect, test } from "vitest";
import { exports } from "cloudflare:workers";
import { origin } from "./fixtures.js";

test("session exposes a standard server clock without authentication or a new cookie", async () => {
  const before = Date.now();
  const response = await exports.default.fetch(`${origin}/api/session`);
  const after = Date.now();
  expect(response.status).toBe(200);
  const sampled = Date.parse(response.headers.get("Date"));
  expect(Number.isFinite(sampled)).toBe(true);
  expect(sampled).toBeGreaterThanOrEqual(Math.floor(before / 1000) * 1000);
  expect(sampled).toBeLessThanOrEqual(after);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.has("Set-Cookie")).toBe(false);
  expect(await response.json()).toEqual({ authenticated: false, session: null });
});
