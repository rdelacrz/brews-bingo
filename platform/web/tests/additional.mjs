import { randomBytes } from "node:crypto";

async function openAccountMenu(page) {
  const toggle = page.locator(".account-menu-toggle");
  await toggle.waitFor();
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await page.locator(".account-menu-panel").waitFor();
}

export async function accountManagement({
  admin,
  origin,
  context,
  step,
  click,
  api,
  until,
  assert,
  enrollment,
}) {
  const hostContext = await context();
  await hostContext.addInitScript(() => {
    const RealDate = Date;
    window.Date = class extends RealDate {
      constructor(...args) {
        super(...(args.length ? args : [RealDate.now() + 60000]));
      }
      static now() {
        return RealDate.now() + 60000;
      }
    };
  });
  const host = await hostContext.newPage();
  let password = randomBytes(20).toString("hex"),
    hostId;
  await step("host-enrollment-with-fast-device-clock", async () => {
    await host.goto(enrollment);
    await host.getByLabel("New password", { exact: true }).fill(password);
    await host.getByLabel("Confirm password", { exact: true }).fill(password);
    await click(host, "Set password");
    await openAccountMenu(host);
    await host
      .getByRole("navigation")
      .getByRole("link", { name: "Games", exact: true })
      .waitFor();
    const session = (await api(host, "/api/session")).body;
    assert(
      session.authenticated && session.account.role === "Host",
      "host-normal-session",
    );
    hostId = session.account.account_id;
    assert(
      (await host.getByRole("link", { name: "Users", exact: true }).count()) ===
        0,
      "host-users-hidden",
    );
    await host.getByRole("link", { name: "Games", exact: true }).click();
    await host.getByRole("heading", { name: "Games", exact: true }).waitFor();
    assert((await host.locator(".account-menu-panel").count()) === 0, "account-menu-closes-after-navigation");
    assert((await host.getByRole("button", { name: "Refresh current game", exact: true }).count()) === 0, "refresh-hidden-when-slot-free");
    assert(await host.getByRole("link", { name: "Create game", exact: true }).isEnabled(), "host-can-open-create-game");
    await host.getByRole("link", { name: "Create game", exact: true }).click();
    await host.getByRole("heading", { name: "Create a game", exact: true }).waitFor();
    await host.getByRole("button", { name: "Back to Games", exact: true }).click();
  });
  const manage = async () => {
    await admin.goto(origin + "/users");
    await admin
      .getByRole("row")
      .filter({ hasText: "BrowserHost" })
      .getByRole("button", { name: "Manage", exact: true })
      .click();
  };
  const confirm = async (label) => {
    await click(admin, label);
    await admin.getByRole("dialog").getByRole("button").last().click();
  };
  const finishPending = async () => {
    const retry = admin.getByRole("button", {
      name: "Review and retry original action",
      exact: true,
    });
    if (await retry.count()) {
      await retry.click();
      await admin.getByRole("dialog").getByRole("button").last().click();
    }
  };
  await step("admin-disable-enable-preserves-account", async () => {
    await manage();
    await confirm("Disable account");
    await until(async () => {
      await finishPending();
      const account = await api(admin, `/api/users/${hostId}`);
      const enable = admin.getByRole("button", {
        name: "Enable account",
        exact: true,
      });
      return (
        account.body.account?.disabled &&
        (await enable.count()) &&
        (await enable.isEnabled())
      );
    }, "disable-reconciled");
    assert(
      !(await api(host, "/api/session")).body.authenticated,
      "disabled-session-revoked",
    );
    await confirm("Enable account");
    await until(
      async () =>
        !(await api(admin, `/api/users/${hostId}`)).body.account?.disabled,
      "enable-readback",
    );
  });
  await step("issued-password-reset-requires-fresh-login", async () => {
    await confirm("Issue / reissue reset link");
    const field = admin.getByLabel("Private access URL", { exact: true });
    await field.waitFor();
    const resetLink = await field.inputValue();
    assert(
      new URL(resetLink).pathname === "/password-reset",
      "issued-reset-route",
    );
    await click(admin, "Dismiss and clear");
    await host.goto(resetLink);
    password = randomBytes(20).toString("hex");
    await host.getByLabel("New password", { exact: true }).fill(password);
    await host.getByLabel("Confirm password", { exact: true }).fill(password);
    await click(host, "Set new password");
    await host
      .getByRole("link", { name: "Continue to login", exact: true })
      .waitFor();
    assert(
      !(await api(host, "/api/session")).body.authenticated,
      "reset-does-not-login",
    );
    assert(!host.url().includes("#"), "reset-fragment-cleared");
    await host.goto(origin + "/login");
    await host.getByLabel("Username", { exact: true }).fill("BrowserHost");
    await host.getByLabel("Password", { exact: true }).fill(password);
    await click(host, "Log in");
    await openAccountMenu(host);
    await host
      .getByRole("navigation")
      .getByRole("link", { name: "Games", exact: true })
      .waitFor();
  });
  return { host, hostId };
}

export async function membershipManagement({
  admin,
  players,
  gameId,
  gameCode,
  origin,
  context,
  step,
  click,
  api,
  until,
  assert,
  mark,
  wait,
}) {
  const original = players[0];
  const recoveryAnswer = randomBytes(16).toString("hex");
  const membership = (page) =>
    api(page, `/api/games/${gameId}/membership?view=player`);
  await step("existing-admission-restores-without-a-second-role", async () => {
    await original.goto(origin + "/join/" + gameCode);
    await original
      .getByRole("link", {
        name: "Restore existing player session",
        exact: true,
      })
      .waitFor();
    assert(
      (await original.getByLabel("Your alias", { exact: true }).count()) === 0,
      "no-duplicate-admission-form",
    );
    await original
      .getByRole("link", {
        name: "Restore existing player session",
        exact: true,
      })
      .click();
    await original
      .getByRole("heading", { name: "Waiting for the host to start" })
      .waitFor();
  });
  await step("alias-and-recovery-answer-settings", async () => {
    await click(original, "Alias and recovery settings");
    await original
      .getByLabel("New case-sensitive alias", { exact: true })
      .fill("BrowserRenamed");
    await click(original, "Rename alias");
    await until(
      async () =>
        (await membership(original)).body.membership?.alias ===
        "BrowserRenamed",
      "rename-readback",
    );
    await original
      .getByLabel("New private recovery answer", { exact: true })
      .fill(recoveryAnswer);
    await click(original, "Set or replace answer");
    await until(
      async () =>
        (await membership(original)).body.membership?.recovery_enabled,
      "recovery-enabled",
    );
    await click(original, "Remove recovery answer");
    await original.getByRole("dialog").getByRole("button").last().click();
    await until(
      async () =>
        !(await membership(original)).body.membership?.recovery_enabled,
      "recovery-removed",
    );
    await original
      .getByLabel("New private recovery answer", { exact: true })
      .fill(recoveryAnswer);
    await click(original, "Set or replace answer");
    await until(
      async () =>
        (await membership(original)).body.membership?.recovery_enabled,
      "recovery-restored",
    );
  });
  await step(
    "recovery-refuses-existing-spectator-without-issuing-player-cookie",
    async () => {
      const spectator = players[2];
      let recoveryRequests = 0;
      const count = (request) => {
        if (new URL(request.url()).pathname === "/api/games/player-recovery")
          recoveryRequests++;
      };
      spectator.on("request", count);
      await spectator.goto(origin + "/recover");
      await spectator.getByLabel("Game code", { exact: true }).fill(gameCode);
      await spectator
        .getByLabel("Exact current alias", { exact: true })
        .fill("BrowserRenamed");
      await spectator
        .getByLabel("Your private recovery answer", { exact: true })
        .fill(recoveryAnswer);
      await click(spectator, "Recover access");
      await spectator
        .getByRole("link", {
          name: "Restore spectator participation",
          exact: true,
        })
        .waitFor();
      assert(recoveryRequests === 0, "spectator-recovery-not-issued");
      spectator.off("request", count);
      await spectator
        .getByRole("link", {
          name: "Restore spectator participation",
          exact: true,
        })
        .click();
      await spectator
        .getByRole("heading", { name: "Waiting for the host to start" })
        .waitFor();
    },
  );
  await step("fresh-device-recovery-keeps-player-identity", async () => {
    const before = (await membership(original)).body.membership.player_id;
    const recoveredContext = await context();
    await recoveredContext.addCookies([
      {
        name: "__Secure-brews-spectator",
        value: randomBytes(32).toString("base64url"),
        domain: new URL(origin).hostname,
        path: `/api/games/${gameId}`,
        secure: true,
        httpOnly: true,
        sameSite: "Strict",
      },
    ]);
    const recovered = await recoveredContext.newPage();
    await recovered.goto(origin + "/recover");
    await recovered.getByLabel("Game code", { exact: true }).fill(gameCode);
    await recovered
      .getByLabel("Exact current alias", { exact: true })
      .fill("BrowserRenamed");
    await recovered
      .getByLabel("Your private recovery answer", { exact: true })
      .fill(recoveryAnswer);
    await click(recovered, "Recover access");
    await recovered
      .getByRole("heading", { name: "Waiting for the host to start" })
      .waitFor();
    assert(
      (await membership(recovered)).body.membership.player_id === before,
      "recovery-same-player",
    );
    assert(
      !(await recoveredContext.cookies()).some(
        (cookie) => cookie.name === "__Secure-brews-spectator",
      ),
      "recovery-clears-rejected-opposite-role-cookie",
    );
    assert(
      (await membership(original)).status === 401,
      "recovery-old-session-denied",
    );
    players[0] = recovered;
  });
  await step(
    "prestart-role-switches-navigate-after-cookie-replacement",
    async () => {
      const player = players[0];
      mark("switch-spectator-open");
      await click(player, "Switch to spectator");
      mark("switch-spectator-confirm");
      await player.getByRole("dialog").getByRole("button").last().click();
      mark("switch-spectator-navigation");
      await player.waitForURL(`**/games/${gameId}/spectator`);
      await player
        .getByRole("heading", { name: "Waiting for the host to start" })
        .waitFor();
      assert(
        (await api(player, `/api/games/${gameId}/membership?view=spectator`))
          .body.membership?.role === "spectator",
        "switched-spectator-authority",
      );
      mark("switch-player-open");
      await click(player, "Switch to player");
      await player
        .getByLabel("Player alias", { exact: true })
        .fill("BrowserRenamed");
      mark("switch-player-submit");
      await player
        .locator("form")
        .getByRole("button", { name: "Switch to player", exact: true })
        .click();
      mark("switch-player-navigation");
      await player.waitForURL(`**/games/${gameId}/player`);
      await until(async () => {
        const value = (
          await api(admin, `/api/games/${gameId}/sync?view=account`)
        ).body.snapshot;
        return (
          value?.connected_player_count === 2 &&
          value.connected_spectator_count === 1
        );
      }, "switched-roster-readback");
    },
  );
  await step(
    "replacement-during-held-http-revalidation-stays-paused",
    async () => {
      const previous = players[0];
      const visible = () => {
        Object.defineProperty(document, "hidden", { get: () => false });
        Object.defineProperty(document, "visibilityState", {
          get: () => "visible",
        });
      };
      await previous.evaluate(visible);
      let held = false,
        release;
      const gate = new Promise((resolve) => (release = resolve));
      const path = `${origin}/api/games/${gameId}/membership?view=player`;
      // Explicit fault injection: a stalled read must not hide a real policy close.
      await previous.route(path, async (route) => {
        if (held) return route.continue();
        held = true;
        await gate;
        await route
          .fulfill({
            status: 503,
            contentType: "application/json",
            body: '{"error":{"code":"unavailable"}}',
          })
          .catch(() => {});
      });
      try {
        await until(async () => held, "periodic-membership-read-held");
        const replacement = await previous.context().newPage();
        await replacement.addInitScript(visible);
        await replacement.goto(`${origin}/games/${gameId}/player`);
        await replacement
          .getByRole("heading", { name: "Waiting for the host to start" })
          .waitFor();
        await previous
          .getByRole("button", {
            name: "Restore connection and revalidate",
            exact: true,
          })
          .waitFor();
        const before = await previous.evaluate(
          () => window.__socketProbe.length,
        );
        release();
        await wait(1500);
        assert(
          (await previous.evaluate(() => window.__socketProbe.length)) ===
            before,
          "replaced-window-stays-paused",
        );
        assert(
          (await replacement
            .locator('p.badge[role="status"]')
            .textContent()) === "Live",
          "replacement-remains-live",
        );
        players[0] = replacement;
      } finally {
        release();
        await previous.unroute(path);
      }
      await previous.close();
    },
  );
}

export async function hostTransfer({
  admin,
  host,
  hostId,
  gameId,
  origin,
  step,
  click,
  api,
  until,
  assert,
}) {
  await step("read-only-host-and-explicit-host-transfer", async () => {
    await host.goto(`${origin}/games/${gameId}/account`);
    await host.getByText(/Read-only account view/).waitFor();
    assert(
      (await host
        .getByRole("button", { name: "Start game", exact: true })
        .count()) === 0,
      "other-host-read-only",
    );
    await click(admin, "Transfer host");
    await click(admin, "BrowserHost");
    await admin.getByRole("dialog").getByRole("button").last().click();
    await until(
      async () =>
        (await api(admin, `/api/games/${gameId}/sync?view=account`)).body
          .snapshot?.game.designated_host_id === hostId,
      "host-transfer-readback",
    );
    await host.reload();
    await host
      .getByRole("button", { name: "Start game", exact: true })
      .waitFor();
  });
  await step(
    "hosted-account-guard-explains-the-required-next-step",
    async () => {
      await admin.goto(origin + "/users");
      await admin
        .getByRole("row")
        .filter({ hasText: "BrowserHost" })
        .getByRole("button", { name: "Manage", exact: true })
        .click();
      await click(admin, "Disable account");
      await admin.getByRole("dialog").getByRole("button").last().click();
      await admin
        .getByText(
          "This account hosts a current game. Transfer or explicitly end/cancel the game before disabling or deleting the account.",
          { exact: true },
        )
        .waitFor();
      const target = await api(admin, `/api/users/${hostId}`);
      assert(
        target.status === 200 &&
          target.body.account.account_id === hostId &&
          target.body.account.disabled === false,
        "hosted-account-remains-enabled",
      );
      await admin.goto(`${origin}/games/${gameId}/account`);
      await admin
        .getByRole("heading", { name: "Waiting for the host to start" })
        .waitFor();
    },
  );
}

export async function runningLeave({
  admin,
  players,
  gameId,
  gameCode,
  origin,
  step,
  click,
  api,
  snapshot,
  until,
  assert,
  wait,
  mark,
}) {
  await step(
    "running-leave-requires-explicit-restore-and-preserves-board",
    async () => {
      const player = players[1];
      const before = (await snapshot(player, "player", gameId)).body;
      mark("leave-confirm");
      await click(player, "Leave game");
      await player.getByRole("dialog").getByRole("button").last().click();
      mark("leave-navigation");
      await player.waitForURL(origin + "/");
      const absent = async () => {
        const view = (await snapshot(admin, "account", gameId)).body;
        return view?.players.some(
          (value) => value.player_id === before.player_id && !value.connected,
        );
      };
      await until(absent, "leave-presence-readback");
      await wait(1000);
      assert(await absent(), "no-automatic-leave-reconnect");
      await player.goto(origin + "/join/" + gameCode);
      await player
        .getByRole("link", {
          name: "Restore existing player session",
          exact: true,
        })
        .click();
      await player
        .getByRole("table", { name: "Game board", exact: true })
        .waitFor();
      const after = (await snapshot(player, "player", gameId)).body;
      assert(
        after.player_id === before.player_id &&
          JSON.stringify(after.board) === JSON.stringify(before.board),
        "restore-same-board",
      );
      await until(
        async () =>
          (await api(admin, `/api/games/${gameId}/sync?view=account`)).body
            .snapshot?.connected_player_count === 2,
        "restored-connected-roster",
      );
    },
  );
}

export async function finalExits({
  admin,
  players,
  gameId,
  origin,
  step,
  click,
  api,
  until,
  assert,
}) {
  await step("final-exits-retire-access-and-navigate", async () => {
    for (const player of players.slice(0, 2)) {
      await click(player, "Exit to Home");
      await player.waitForURL(origin + "/");
      await until(
        async () =>
          [401, 403, 404, 410].includes(
            (await api(player, `/api/games/${gameId}/sync?view=player`)).status,
          ),
        "player-final-grant-retired",
      );
    }
    await click(players[2], "Exit to Home");
    await players[2].waitForURL(origin + "/");
    await admin.goto(`${origin}/games/${gameId}/account`);
    await click(admin, "Exit and view History");
    await admin.waitForURL("**/history/*");
    await admin
      .getByRole("heading", { name: "Final player boards", exact: true })
      .waitFor();
    assert(
      (await admin
        .getByRole("heading", { name: "Final player boards", exact: true })
        .count()) === 1,
      "exit-history-destination",
    );
    await until(
      async () =>
        [401, 403, 404, 410].includes(
          (await api(admin, `/api/games/${gameId}/sync?view=account`)).status,
        ),
      "account-final-grant-retired",
    );
  });
}
