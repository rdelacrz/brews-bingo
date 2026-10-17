// Real local Workerd owner routes; these observations do not certify deployment.
import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { cookie, enroll, origin, password, post, resetStorage } from "./fixtures.js";

const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
beforeEach(async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), async (_instance, state) => state.storage.deleteAll());
});

test("local Slice9 real recovery Unicode proof and owner clock/randomness observations", async () => {
  const before = Date.now();
  const actor = await enroll("LocalReadinessHost");
  const created = await post("/api/games", { configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 4, spectator_capacity: 0,
  } }, { session: actor.session });
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const opened = await post(`/api/games/${id}/lobby`, { expected_revision: 0 }, { session: actor.session });
  expect(opened.status).toBe(200);
  const code = (await opened.json()).game_code;
  const cases = [
    ["nfc_outer_whitespace_sharp_s", "CAFE\u0301 Straße", "café STRASSE", "cafe strasse"],
    ["greek_sigma", "Σςσ", "σσΣ", "σσ"],
    ["non_turkic_i", "İIı", "i\u0307iı", "iii"],
    ["internal_whitespace", "A  B!", "a  b!", "a b!"],
  ];
  const samples = [];
  // Tokens stay in memory; only boolean length/distinctness checks are emitted.
  const sessions = new Set();
  for (const [index, [label, source, equivalent, different]] of cases.entries()) {
    const suffix = password();
    const context = await post(`/api/games/${id}/admission-context`, { game_code: code }, { id: null });
    expect(context.status).toBe(200);
    const alias = `Readiness${index}`;
    const joined = await post(`/api/games/${id}/players`, {
      game_code: code, alias, recovery_answer: `\u2003${source} ${suffix}\u00a0`,
    }, { session: cookie(context) });
    expect(joined.status).toBe(200);
    const player = (await joined.json()).player_id;
    sessions.add(cookie(joined));
    const recovered = await post("/api/player-recovery", { game_code: code, alias, recovery_answer: `${equivalent} ${suffix}` });
    expect(recovered.status).toBe(200);
    const session = cookie(recovered);
    expect(sessions.has(session)).toBe(false);
    sessions.add(session);
    const synced = await exports.default.fetch(`${origin}/api/games/${id}/sync?view=player`, { headers: { Cookie: session } });
    expect(synced.status).toBe(200);
    expect((await synced.json()).snapshot.player_id).toBe(player);
    const denied = await post("/api/player-recovery", { game_code: code, alias, recovery_answer: `${different} ${suffix}` });
    expect(denied.status).toBe(401);
    expect(denied.headers.has("set-cookie")).toBe(false);
    samples.push({ case: label, equivalent_accepted: true, different_denied: true });
  }
  const after = Date.now();
  const clock = await runInDurableObject(owner(id), (_instance, state) => {
    const game = state.storage.sql.exec("SELECT r.created_at,m.last_observed_ms FROM game_record r CROSS JOIN game_metadata m").one();
    return { created_at: game.created_at, last_observed_ms: game.last_observed_ms };
  });
  expect(clock.created_at).toBeGreaterThanOrEqual(before);
  expect(clock.created_at).toBeLessThanOrEqual(after);
  expect(clock.last_observed_ms).toBeGreaterThanOrEqual(clock.created_at);
  expect(clock.last_observed_ms).toBeLessThanOrEqual(after);
  expect(sessions.size).toBe(8);

  // The host runner persists this safe line to its scratch artifact; Workerd has no host filesystem writes.
  console.log(`LOCAL-SLICE9-MEASUREMENTS ${JSON.stringify({ marker: "LOCAL-SLICE9-MEASUREMENTS",
    target: "actual-local-Workerd",
    recovery: samples, clock: { observations: 2, created_within_request_window: true, durable_clock_within_request_window: true },
    csprng: { owner_generated_session_samples: sessions.size, distinct: true, cryptographic_quality_certified: false },
    pending: ["deployed Unicode/clock/CSPRNG", "provider CPU/memory/quotas/headroom", "maximum legal game and owner forecast", "Slice2/6/7 acceptance"] })}`);
}, 120_000);
