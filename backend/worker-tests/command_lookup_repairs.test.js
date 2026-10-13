import { beforeEach, expect, test } from "vitest";
import { env, exports } from "cloudflare:workers";
import { reset, runInDurableObject, runDurableObjectAlarm } from "cloudflare:test";
import { createHash } from "node:crypto";
import { enroll, post, resetStorage, origin, commandId, inOwner } from "./fixtures.js";

const owner = id => env.GAMES.get(env.GAMES.idFromName(id));
const directory = () => env.GAME_DIRECTORY.get(env.GAME_DIRECTORY.idFromName("directory"));
const clock = Date.now;
beforeEach(async () => {
  Date.now = clock;
  await reset();
  await resetStorage();
  await runInDurableObject(directory(), (_, state) => state.storage.deleteAll());
});

async function lookup(game, command, session) {
  const response = await exports.default.fetch(`${origin}/api/games/${game}/commands/${command}`, {
    headers: {Cookie: session},
  });
  expect(response.headers.get("Set-Cookie")).toBeNull();
  expect(response.headers.get("Cache-Control")).toBe("no-store");
  const raw = await response.text();
  expect(new TextEncoder().encode(raw).length).toBeLessThanOrEqual(4096);
  return {status: response.status, body: JSON.parse(raw)};
}

for (const fault of ["absent", "directory-unavailable"]) {
  for (const auditFault of [null, "IGNORE", "ABORT"]) {
    test(`command lookup creation Admin ${fault} audit ${auditFault ?? "captured"}`, async () => {
      const admin = await enroll("LookupRepairAdmin");
      await inOwner((_, state) => state.storage.sql.exec(
        "UPDATE accounts SET role='admin' WHERE account_id=?", admin.accountId,
      ));
      const game = commandId();
      const command = commandId();
      if (auditFault) {
        // Initialize only the owner schema/target, then fault the required audit write.
        expect((await lookup(game, command, admin.session)).status).toBe(403);
        await runInDurableObject(owner(game), (_, state) => {
          state.storage.sql.exec("DELETE FROM game_admin_audit");
          const action = auditFault === "IGNORE" ? "IGNORE" : "ABORT,'required audit fault'";
          state.storage.sql.exec(`CREATE TRIGGER reject_lookup_audit BEFORE INSERT ON game_admin_audit BEGIN SELECT RAISE(${action}); END`);
        });
      }
      if (fault === "directory-unavailable") {
        await runInDurableObject(directory(), instance => {
          instance.lookupRepairFetch = instance.fetch.bind(instance);
          instance.fetch = request => new URL(request.url).pathname === "/command-creation"
            ? new Response("unavailable", {status: 503}) : instance.lookupRepairFetch(request);
        });
      }
      try {
        const response = await lookup(game, command, admin.session);
        expect(response.status).toBe(auditFault || fault === "directory-unavailable" ? 503 : 403);
        expect(response.body.result_status).toBeUndefined();
        const rows = await runInDurableObject(owner(game), async (_, state) => ({
          audit: state.storage.sql.exec("SELECT actor_account_id,target_game_id,operation,outcome FROM game_admin_audit").toArray(),
          games: state.storage.sql.exec("SELECT count(*) AS count FROM game_record").one().count,
          alarm: await state.storage.getAlarm(),
          deadlines: state.storage.sql.exec("SELECT expires_at FROM game_admin_audit").toArray(),
        }));
        expect(rows.games).toBe(0);
        expect(rows.audit).toEqual(auditFault ? [] : [{
          actor_account_id: admin.accountId, target_game_id: game,
          operation: "command_lookup", outcome: fault === "absent" ? "rejected" : "failed",
        }]);
        if (!auditFault) {
          expect(rows.alarm).not.toBeNull();
          expect(rows.alarm).toBeLessThanOrEqual(rows.deadlines[0].expires_at);
        }
      } finally {
        if (fault === "directory-unavailable") {
          await runInDurableObject(directory(), instance => {
            instance.fetch = instance.lookupRepairFetch;
            delete instance.lookupRepairFetch;
          });
        }
      }
    });
  }
}

test("command lookup orphan Admin audit alarm purges at fixed expiry and stays purged through rollback", async () => {
  const admin = await enroll("LookupRepairAuditExpiry");
  await inOwner((_, state) => state.storage.sql.exec("UPDATE accounts SET role='admin' WHERE account_id=?", admin.accountId));
  const game = commandId();
  expect((await lookup(game, commandId(), admin.session)).status).toBe(403);
  const expiry = await runInDurableObject(owner(game), (_, state) => state.storage.sql.exec("SELECT expires_at FROM game_admin_audit").one().expires_at);
  try {
    Date.now = () => expiry;
    expect(await runDurableObjectAlarm(owner(game))).toBe(true);
    expect(await runInDurableObject(owner(game), (_, state) => state.storage.sql.exec("SELECT count(*) AS count FROM game_admin_audit").one().count)).toBe(0);
    Date.now = () => expiry - 1;
    await expect(runInDurableObject(owner(game), instance => instance.alarm())).rejects.toThrow("game recovery unavailable");
    expect(await runInDurableObject(owner(game), (_, state) => state.storage.sql.exec("SELECT count(*) AS count FROM game_admin_audit").one().count)).toBe(0);
  } finally {
    Date.now = clock;
  }
});

function transferFingerprint(target, assignment) {
  const revision = Buffer.alloc(8);
  revision.writeBigInt64BE(BigInt(assignment));
  return createHash("sha256").update("brews-host-transfer-v1\0").update(target).update(revision).digest();
}

for (const alteration of ["target", "assignment", "target-with-canonical-fingerprint", "assignment-with-canonical-fingerprint", "receipt-target"]) {
  test(`command lookup committed transfer rejects semantic ${alteration} contradiction`, async () => {
    const host = await enroll("LookupRepairHost");
    const target = await enroll("LookupRepairTarget");
    const other = await enroll("LookupRepairOther");
    const created = await post("/api/games", {configuration: {
      numeric_upper_bound: 4, board_side_length: 2, free_cells_enabled: false,
      free_cell_positions: [], player_capacity: 3, spectator_capacity: 3,
    }}, {session: host.session});
    expect(created.status).toBe(200);
    const {game: {game_id: game}} = await created.json();
    const command = commandId();
    await runInDurableObject(directory(), instance => {
      instance.lookupRepairFetch = instance.fetch.bind(instance);
      instance.fetch = async request => {
        if (new URL(request.url).pathname === "/host-transfer" && (await request.clone().json()).action === "settle") {
          return new Response("unavailable", {status: 503});
        }
        return instance.lookupRepairFetch(request);
      };
    });
    try {
      const transfer = await post(`/api/games/${game}/host-transfer`, {
        target_account_id: target.accountId, confirmed: true, expected_host_assignment_revision: 0,
      }, {session: host.session, id: command});
      expect(transfer.status).toBe(200);
      await transfer.text();
    } finally {
      await runInDurableObject(directory(), instance => {
        instance.fetch = instance.lookupRepairFetch;
        delete instance.lookupRepairFetch;
      });
    }
    const before = await lookup(game, command, host.session);
    expect(before.status).toBe(200);
    expect(before.body.result_status.status).toBe("committed");
    expect(before.body.result_status.follow_up_pending).toBe(true);
    expect(before.body.result_status.receipt.outcome.designated_host_id).toBe(target.accountId);
    const semanticTarget = alteration.startsWith("target") ? other.accountId : target.accountId;
    const assignment = alteration.startsWith("assignment") ? 1 : 0;
    const canonical = transferFingerprint(semanticTarget, assignment);
    const snapshot = await runInDurableObject(owner(game), (_, state) => {
      const receipt = state.storage.sql.exec("SELECT outcome FROM game_receipts WHERE command_id=?", command).one().outcome;
      if (alteration === "receipt-target") {
        const decoded = JSON.parse(receipt);
        decoded.outcome.designated_host_id = other.accountId;
        state.storage.sql.exec("UPDATE game_receipts SET outcome=? WHERE command_id=?", JSON.stringify(decoded), command);
      } else {
        state.storage.sql.exec("UPDATE game_host_transfers SET target_account_id=?,expected_assignment_revision=? WHERE command_id=?", semanticTarget, assignment, command);
        if (alteration.includes("canonical-fingerprint")) {
          state.storage.sql.exec("UPDATE game_host_transfers SET fingerprint=? WHERE command_id=?", canonical, command);
          state.storage.sql.exec("UPDATE game_receipts SET fingerprint=? WHERE command_id=?", canonical, command);
        }
      }
      return {
        host: state.storage.sql.exec("SELECT host_id FROM game_record").one().host_id,
        work: state.storage.sql.exec("SELECT target_account_id,expected_assignment_revision,phase FROM game_host_transfers WHERE command_id=?", command).one(),
      };
    });
    expect(snapshot.host).toBe(target.accountId);
    expect(snapshot.work.phase).toBe("committed");
    const denied = await lookup(game, command, host.session);
    expect(denied.status).toBe(503);
    expect(denied.body.result_status).toBeUndefined();
  });
}
