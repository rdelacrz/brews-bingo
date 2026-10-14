import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";
import { randomBytes } from "node:crypto";

export default defineConfig({
  plugins: [cloudflareTest({
    wrangler: { configPath: "./wrangler.toml" },
    miniflare: {
      bindings: { APP_ORIGIN: "https://localhost:8787", RATE_LIMIT_KEY: randomBytes(32).toString("base64url"), DEV_CLI_KEY: randomBytes(32).toString("base64url"), CF_VERSION_METADATA: { id: "11111111-1111-4111-8111-111111111111", tag: "local-unit-fixture" } },
    },
  })],
  test: { include: ["worker-tests/**/*.test.js"], testTimeout: 30000, hookTimeout: 30000, fileParallelism: false },
});
