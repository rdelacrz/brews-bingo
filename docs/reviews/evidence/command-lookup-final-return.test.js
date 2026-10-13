// Diagnostic acceptance probe, intentionally failing while RUNTIME-FINAL-01 is open.
import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, cookie, commandId } from "../../../backend/worker-tests/fixtures.js";

const clock = Date.now;
const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
beforeEach(async () => {
  Date.now = clock;
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), (_, state) => state.storage.deleteAll());
});

async function fixture() {
  const host = await enroll("LookupHost");
  const created = await post("/api/games", {configuration: {
    numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
    free_cell_positions: [], player_capacity: 3, spectator_capacity: 3,
  }}, {session: host.session, id: commandId()});
  expect(created.status).toBe(200);
  const {game: {game_id: id}} = await created.json();
  const opened = await post(`/api/games/${id}/lobby`, {expected_revision: 0}, {session: host.session});
  expect(opened.status).toBe(200);
  const {game_code: code} = await opened.json();
  const context = await post(`/api/games/${id}/admission-context`, {game_code: code}, {id: null});
  expect(context.status).toBe(200);
  const answer = crypto.randomUUID();
  const alias = "LookupPlayer";
  const command = commandId();
  const joined = await post(`/api/games/${id}/players`, {
    game_code: code, alias, recovery_answer: answer,
  }, {session: cookie(context), id: command});
  expect(joined.status).toBe(200);
  const session = cookie(joined);
  await joined.text();
  return {id, code, answer, alias, command, session};
}

async function lookup(fixture, session) {
  return exports.default.fetch(`${origin}/api/games/${fixture.id}/commands/${fixture.command}`, {
    headers: {Cookie: session},
  });
}

test("G4 shared RUNTIME-FINAL-01: actual recovery during final return must deny the old session", async () => {
  const f = await fixture();
  let releases = 0;
  let changed = false;
  await runInDurableObject(owner(f.id), instance => {
    instance.commandOriginalFetch = instance.fetch.bind(instance);
    instance.fetch = async request => {
      const actual = await instance.commandOriginalFetch(request);
      if (new URL(request.url).pathname === "/read-release" && ++releases === 2) {
        // Capture genuine current-invocation bytes; never synthesize an authorization ACK.
        const bytes = await actual.clone().arrayBuffer();
        const recovered = await post("/api/player-recovery", {
          game_code: f.code, alias: f.alias, recovery_answer: f.answer,
        });
        expect(recovered.status).toBe(200);
        const current = cookie(recovered);
        await recovered.text();
        const own = await lookup(f, current);
        expect(own.status).toBe(200);
        expect((await own.json()).result_status.status).toBe("committed");
        changed = true;
        return new Response(bytes, {status: actual.status, headers: actual.headers});
      }
      return actual;
    };
  });
  try {
    const returned = await lookup(f, f.session);
    const body = await returned.json();
    const old = await lookup(f, f.session);
    expect(old.status).toBe(401);
    await old.text();
    console.log("G4_SHARED_LAST_HOP", JSON.stringify({
      changed, releases, old_session_now: old.status, returned_status: returned.status,
      returned_receipt: body.result_status?.status, cookie: returned.headers.has("Set-Cookie"),
    }));
    expect(changed).toBe(true);
    expect([401, 503]).toContain(returned.status);
    expect(body.result_status?.receipt).toBeUndefined();
  } finally {
    await runInDurableObject(owner(f.id), instance => {
      instance.fetch = instance.commandOriginalFetch;
      delete instance.commandOriginalFetch;
    });
  }
});
