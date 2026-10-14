import { expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";

const endpoint = "https://localhost:8787/api/deployment";

async function checked(url = endpoint, init, status = 200) {
  const response = await exports.default.fetch(url, init);
  expect(response.status).toBe(status);
  expect(response.headers.get("Set-Cookie")).toBeNull();
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.get("Content-Type")).toContain("application/json");
  return response.json();
}

test("serving identity exposes only version protocol and immutable version UUID", async () => {
  const result = await checked();
  expect(result).toEqual({ version: 1, version_id: env.CF_VERSION_METADATA.id });
  expect(result).not.toHaveProperty("tag");
  expect(result).not.toHaveProperty("APP_ORIGIN");
  expect(result).not.toHaveProperty("bindings");
});

test("serving identity denies plaintext transport", async () => {
  expect(await checked(endpoint.replace("https:", "http:"), undefined, 403)).toHaveProperty("error");
});

for (const method of ["POST", "PUT", "PATCH", "DELETE"]) {
  test(`serving identity denies ${method} without side effects`, async () => {
    expect(await checked(endpoint, { method }, 405)).toHaveProperty("error");
    expect(await checked()).toEqual({ version: 1, version_id: env.CF_VERSION_METADATA.id });
  });
}

for (const query of ["?", "?version=x", "?token=x"]) {
  test(`serving identity rejects nonempty query grammar ${query}`, async () => {
    expect(await checked(endpoint + query, undefined, 400)).toHaveProperty("error");
  });
}

test("serving identity transport rejects GET content before routing", async () => {
  await expect(exports.default.fetch(endpoint, { method: "GET", body: "{}" })).rejects.toThrow("GET or HEAD");
});
