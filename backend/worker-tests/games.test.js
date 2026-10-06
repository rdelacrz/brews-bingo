import { beforeEach, expect, test } from "vitest";
import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage } from "./fixtures.js";

beforeEach(async () => {
  await resetStorage();
  const directory = env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
  await runInDurableObject(directory, async (_instance, state) => state.storage.deleteAll());
});

test("an enrolled host creates a New game through the Worker", async () => {
  const actor = await enroll();
  const response = await post("/api/games", {}, { session: actor.session });
  expect(response.status).toBe(200);
  const body = await response.json();
  expect(body.result).toBe("created");
  expect(body.game.state).toBe("new");
  expect(body.game.designated_host_id).toBe(actor.accountId);
  expect(body.configuration.numeric_upper_bound).toBe(75);
  expect(body.configuration.board_side_length).toBe(5);
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  expect(response.headers.has("Set-Cookie")).toBe(false);
});
