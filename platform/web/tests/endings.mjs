export async function endings(h) {
  const {
    admin,
    origin,
    context,
    step,
    click,
    api,
    until,
    assert,
    issueCall,
    resolveOriginalUntil,
  } = h;
  const openAccountMenu = async () => {
    const toggle = admin.locator(".account-menu-toggle");
    await toggle.waitFor();
    if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
    await admin.locator(".account-menu-panel").waitFor();
  };
  const slotFree = () =>
    until(
      async () => (await api(admin, "/api/games/current")).body.slot === "free",
      "terminal-slot-released",
    );
  const create = async (pool, side) => {
    await slotFree();
    await admin.goto(origin + "/games/new");
    await admin
      .getByLabel("Numeric pool upper bound (1–1,000)")
      .fill(String(pool));
    await admin.getByLabel("Board side length (2–10)").fill(String(side));
    await admin.getByLabel("Enable free cells", { exact: true }).uncheck();
    await click(admin, "Create game");
    await admin
      .getByRole("heading", { name: "Game setup", exact: true })
      .waitFor();
    return new URL(admin.url()).pathname.split("/")[2];
  };
  await step("public-home-and-guarded-games-remain-separate", async () => {
    await admin.goto(origin + "/");
    await openAccountMenu();
    await admin
      .getByRole("navigation")
      .getByRole("link", { name: "Games", exact: true })
      .waitFor();
    await admin
      .getByRole("heading", { name: "Good company. Great games.", exact: true })
      .waitFor();
    assert(
      (await admin
        .getByRole("button", { name: "Join a game", exact: true })
        .count()) === 1,
      "signed-in-home-attendee-entry",
    );
    assert(
      (await admin
        .getByRole("heading", { name: "Current game", exact: true })
        .count()) === 0,
      "public-home-has-no-dashboard",
    );
    const anonymous = await (await context()).newPage();
    await anonymous.goto(origin + "/games");
    await anonymous
      .getByRole("link", { name: "Sign in", exact: true })
      .waitFor();
    assert(
      (await anonymous
        .getByRole("button", { name: "Join a game", exact: true })
        .count()) === 0,
      "anonymous-games-has-no-attendee-entry",
    );
    await anonymous.close();
  });
  await step("explicit-prestart-cancellation-has-no-history", async () => {
    const before = (await api(admin, "/api/history")).body.games.length;
    const id = await create(9, 2);
    await click(admin, "Cancel game");
    await admin.getByRole("dialog").getByRole("button").last().click();
    await resolveOriginalUntil(
      admin,
      () =>
        admin
          .getByText(
            "The unstarted game was cancelled. No History entry was created.",
            { exact: true },
          )
          .isVisible(),
      "prestart-cancel-original-resolved",
    );
    await slotFree();
    assert(
      (await api(admin, "/api/history")).body.games.length === before,
      "prestart-no-history",
    );
    assert(
      (await api(admin, `/api/history/${id}`)).status === 404,
      "prestart-history-unavailable",
    );
  });
  let id;
  await step("large-board-start-and-confirmed-no-winner-end", async () => {
    id = await create(100, 10);
    let failedSlotReads = 0;
    await admin.route("**/api/games/current", async (route) => {
      if (route.request().method() === "GET") {
        failedSlotReads++;
        await route.fulfill({status: 503, contentType: "application/json", body: JSON.stringify({error: {code: "service_unavailable", message: "test-only read failure"}})});
      } else {
        await route.continue();
      }
    });
    await admin.goto(origin + "/games");
    await admin.getByRole("button", { name: "Retry current game lookup", exact: true }).waitFor();
    assert(failedSlotReads > 0, "failed-slot-read-is-retryable-from-games");
    await admin.getByRole("link", { name: "Create game", exact: true }).click();
    await admin.getByRole("heading", { name: "Create a game", exact: true }).waitFor();
    await admin.getByRole("button", { name: "Create game", exact: true }).click();
    await admin.getByText("Creation conflicted with server state. Check Games for an existing game before trying again.", { exact: true }).waitFor();
    await admin.getByRole("link", { name: "Check current game", exact: true }).click();
    await admin.getByRole("button", { name: "Retry current game lookup", exact: true }).waitFor();
    await admin.unroute("**/api/games/current");
    await admin.getByRole("button", { name: "Retry current game lookup", exact: true }).click();
    await admin.getByRole("button", { name: "Refresh current game", exact: true }).waitFor();
    await admin.getByRole("button", { name: "Refresh current game", exact: true }).click();
    assert(await admin.getByRole("link", { name: "Create game", exact: true }).isEnabled(), "create-game-route-remains-actionable-while-slot-occupied");
    await admin.getByRole("link", { name: "Create game", exact: true }).click();
    await admin.getByText("The global game slot is occupied or coordinating. A second game cannot be created.", { exact: true }).waitFor();
    assert((await admin.getByText("Check the submitted fields and try again.", { exact: true }).count()) === 0, "occupied-slot-not-reported-as-invalid-form");
    await admin.getByRole("button", { name: "Create game", exact: true }).click();
    await admin.getByText("The game slot is occupied or coordinating. Check the current game before trying again.", { exact: true }).waitFor();
    const resume = admin.getByRole("link", { name: "Open current game", exact: true });
    assert((await resume.getAttribute("href")) === `/games/${id}/account`, "occupied-create-conflict-links-existing-game");
    await resume.click();
    await admin.getByRole("heading", { name: "Game setup", exact: true }).waitFor();
    await click(admin, "Open saved lobby");
    await admin.getByRole("dialog").getByRole("button").last().click();
    await admin
      .getByRole("heading", { name: "Waiting for the host to start" })
      .waitFor();
    const code = (await api(admin, `/api/games/${id}/sync?view=account`)).body
      .snapshot.game_code;
    const seats = [];
    for (const alias of ["LargeOne", "LargeTwo"]) {
      const player = await (await context()).newPage();
      seats.push(player);
      await player.goto(origin + "/join/" + code);
      await player.getByLabel("Your alias", { exact: true }).fill(alias);
      await click(player, "Join as player");
      await player
        .getByRole("heading", { name: "Waiting for the host to start" })
        .waitFor();
    }
    await until(
      async () =>
        (await api(admin, `/api/games/${id}/sync?view=account`)).body.snapshot
          ?.connected_player_count === 2,
      "large-connected-roster",
    );
    await click(admin, "Start game");
    await seats[0]
      .getByRole("table", { name: "Game board", exact: true })
      .waitFor();
    await admin.getByLabel("Manual value", { exact: true }).fill("100");
    await issueCall(admin, "Call value", 1, id);
    await click(admin, "End without winner");
    await admin.getByRole("dialog").getByRole("button").last().click();
    await resolveOriginalUntil(
      admin,
      () =>
        admin
          .getByRole("heading", { name: "Game result", exact: true })
          .isVisible(),
      "no-winner-original-resolved",
    );
    const history = (await api(admin, `/api/history/${id}`)).body.history;
    assert(
      history.outcome === "cancelled" && history.winner === null,
      "cancelled-without-winner",
    );
    assert(
      history.ordered_calls.length === 1 && history.ordered_calls[0] === "100",
      "cancelled-calls-retained",
    );
    assert(
      history.players.length === 2 &&
        history.players.every((player) => player.cells.length === 100),
      "large-history-boards",
    );
    await slotFree();
    await admin.goto(origin + "/games");
    await admin.getByRole("heading", { name: "Games", exact: true }).waitFor();
    assert((await admin.getByRole("button", { name: "Refresh current game", exact: true }).count()) === 0, "refresh-hidden-with-no-nonterminal-game");
  });
  await step(
    "history-outcome-filter-and-mobile-large-board-accessibility",
    async () => {
      await admin.goto(origin + "/history");
      await admin
        .getByLabel("Outcome", { exact: true })
        .selectOption("cancelled");
      await until(
        async () =>
          (await admin
            .getByRole("link", { name: "View history", exact: true })
            .count()) === 1,
        "cancelled-filter",
      );
      await admin
        .getByRole("link", { name: "View history", exact: true })
        .click();
      await admin
        .getByRole("heading", { name: "Final player boards", exact: true })
        .waitFor();
      await admin.setViewportSize({ width: 320, height: 780 });
      await admin
        .getByLabel("Inspect a player's final board", { exact: true })
        .selectOption({ label: "LargeOne" });
      const board = admin.getByRole("table", {
        name: "LargeOne final board",
        exact: true,
      });
      await board.waitFor();
      const region = admin.getByRole("region", {
        name: "LargeOne final board; scroll to inspect every cell",
        exact: true,
      });
      assert(
        (await board.getByRole("row").count()) === 10 &&
          (await board.getByRole("cell").count()) === 100,
        "large-board-semantic-grid",
      );
      const geometry = await board.evaluate((node) => ({
        documentFits: document.documentElement.scrollWidth <= innerWidth + 1,
        scrollable:
          node.parentElement.scrollWidth > node.parentElement.clientWidth,
        focusable: node.parentElement.tabIndex === 0,
        cellWidth: node.querySelector('[role="cell"]').getBoundingClientRect()
          .width,
        markedWithoutColor: [...node.querySelectorAll('[role="cell"]')].some(
          (cell) => cell.textContent.includes("✓"),
        ),
      }));
      assert(
        geometry.documentFits &&
          geometry.scrollable &&
          geometry.focusable &&
          geometry.cellWidth >= 44 &&
          geometry.markedWithoutColor,
        "large-board-accessible-overflow",
      );
      await region.focus();
      assert(
        await region.evaluate((node) => document.activeElement === node),
        "large-board-region-receives-focus",
      );
      const initialScrollLeft = await region.evaluate(
        (node) => node.scrollLeft,
      );
      await admin.keyboard.press("ArrowRight");
      await until(
        async () =>
          (await region.evaluate((node) => node.scrollLeft)) >
          initialScrollLeft,
        "large-board-arrow-right-scrolls-focused-region",
      );
      for (let attempt = 0; attempt < 30; attempt++)
        await admin.keyboard.press("ArrowRight");
      await until(
        async () =>
          await region.evaluate((node) => {
            const last = node
              .querySelector('[role="row"] [role="cell"]:last-child')
              .getBoundingClientRect();
            const left = node.getBoundingClientRect().left + node.clientLeft;
            return (
              document.activeElement === node &&
              last.left >= left - 1 &&
              last.right <= left + node.clientWidth + 1
            );
          }),
        "large-board-keyboard-exposes-last-column",
      );
      await admin.setViewportSize({ width: 1280, height: 900 });
    },
  );
}

export async function deleteAccount({
  admin,
  host,
  hostId,
  origin,
  step,
  click,
  api,
  until,
  assert,
}) {
  await step("admin-delete-revokes-target-and-preserves-history", async () => {
    const count = (await api(admin, "/api/history")).body.games.length;
    await host.goto(origin + "/games");
    await host.getByRole("heading", { name: "Games", exact: true }).waitFor();
    await admin.goto(origin + "/users");
    await admin
      .getByRole("row")
      .filter({ hasText: "BrowserHost" })
      .getByRole("button", { name: "Manage", exact: true })
      .click();
    await click(admin, "Delete account");
    await admin.getByRole("dialog").getByRole("button").last().click();
    await until(async () => {
      const retry = admin.getByRole("button", {
        name: "Review and retry original action",
        exact: true,
      });
      if ((await retry.count()) && (await retry.isEnabled())) {
        await retry.click();
        await admin.getByRole("dialog").getByRole("button").last().click();
      }
      return (await api(admin, `/api/users/${hostId}`)).status === 404;
    }, "deleted-account-readback");
    const session = await api(host, "/api/session");
    assert(
      session.status === 200 && session.body.authenticated === false,
      "deleted-session-denied",
    );
    const refresh = host.getByRole("button", {
      name: "Refresh current game",
      exact: true,
    });
    if (await refresh.count()) await refresh.click();
    await host
      .getByText("Sign in with a normal Host or Admin account to view Games.", {
        exact: true,
      })
      .waitFor();
    await host.getByRole("link", { name: "Sign in", exact: true }).waitFor();
    assert(
      (await host
        .getByRole("button", { name: "Join a game", exact: true })
        .count()) === 0,
      "revoked-games-has-no-attendee-entry",
    );
    assert(
      (await host
        .getByRole("heading", { name: "Current game", exact: true })
        .count()) === 0,
      "revoked-games-clears-dashboard",
    );
    assert(
      (await api(admin, "/api/history")).body.games.length === count,
      "delete-keeps-history",
    );
  });
}
