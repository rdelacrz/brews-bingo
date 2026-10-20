import { expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject } from "cloudflare:test";
import { enroll, post, resetStorage, origin, inOwner } from "./fixtures.js";

const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

test("overlapping unchanged account broadcasts serialize without retiring a healthy socket", async () => {
  await reset();
  await resetStorage();
  await runInDurableObject(
    env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory")),
    async (_instance, state) => state.storage.deleteAll(),
  );
  const actor = await enroll();
  const created = await post(
    "/api/games",
    {
      configuration: {
        numeric_upper_bound: 9,
        board_side_length: 2,
        free_cells_enabled: false,
        free_cell_positions: [],
        player_capacity: 20,
        spectator_capacity: 50,
      },
    },
    { session: actor.session },
  );
  expect(created.status).toBe(200);
  const id = (await created.json()).game.game_id;
  const owner = env.GAMES.get(env.GAMES.idFromName(id));
  const response = await exports.default.fetch(
    `${origin}/api/games/${id}/stream?view=account`,
    {
      headers: {
        Cookie: actor.session,
        Origin: origin,
        Upgrade: "websocket",
        Connection: "Upgrade",
      },
    },
  );
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  let firstFrame;
  const opened = new Promise((resolve) => (firstFrame = resolve));
  socket.addEventListener("message", (event) => {
    const frame = JSON.parse(event.data);
    socket.send(
      JSON.stringify({
        version: 1,
        kind: "snapshot_ack",
        connection_id: frame.connection_id,
        delivery_id: frame.delivery_id,
        view_revision: frame.view_revision,
      }),
    );
    firstFrame();
  });
  socket.accept();
  await opened;
  await wait(100);
  let closeAttempts = 0;
  await inOwner((instance) => {
    instance.overlapEntered = false;
    // Create and resolve the hold inside its owning DO's I/O context.
    instance.overlapGate = new Promise((resolve) => {
      instance.overlapRelease = resolve;
    });
    instance.overlapOriginal = instance.fetch.bind(instance);
    instance.fetch = async (request) => {
      if (new URL(request.url).pathname === "/game-authority") {
        const body = await request.clone().json();
        if (
          !instance.overlapEntered &&
          body.action === "authorize_connection"
        ) {
          instance.overlapEntered = true;
          await instance.overlapGate;
        }
      }
      return instance.overlapOriginal(request);
    };
  });
  await runInDurableObject(owner, (_instance, state) => {
    for (const ws of state.getWebSockets()) {
      ws.overlapClose = ws.close;
      // No closure should be attempted, even if the SDK would not complete it.
      ws.close = () => {
        closeAttempts++;
      };
    }
  });
  const read = () =>
    exports.default.fetch(`${origin}/api/games/${id}/sync?view=account`, {
      headers: { Cookie: actor.session },
    });
  const first = read();
  let second;
  try {
    let entered = false;
    for (let attempt = 0; attempt < 100; attempt++) {
      entered = await inOwner((instance) => instance.overlapEntered);
      if (entered) break;
      await wait(10);
    }
    expect(entered).toBe(true);
    second = read();
    await wait(200);
    await inOwner((instance) => instance.overlapRelease());
    const results = await Promise.all([first, second]);
    expect(closeAttempts).toBe(0);
    expect(results.map((result) => result.status)).toEqual([200, 200]);
  } finally {
    await inOwner((instance) => instance.overlapRelease());
    await Promise.allSettled([first, second]);
    await inOwner((instance) => {
      instance.fetch = instance.overlapOriginal;
    });
    await runInDurableObject(owner, (_instance, state) => {
      for (const ws of state.getWebSockets())
        if (ws.overlapClose) {
          ws.close = ws.overlapClose;
          ws.close(1000);
        }
    });
    socket.close();
  }
});
