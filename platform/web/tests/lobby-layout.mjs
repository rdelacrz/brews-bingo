function check(value, name) {
  if (!value) {
    const error = new Error("Lobby layout check failed");
    error.check = name;
    throw error;
  }
}

async function observed(name, action) {
  try {
    return await action();
  } catch (error) {
    if (error instanceof Error) error.check ??= name;
    throw error;
  }
}

export async function lobbyLayout(page, code, players, playerCapacity, connectedPlayers, spectators, spectatorCapacity, connectedSpectators, copyCode = false) {
  const codeCard = page.locator(".game-code-card");
  const codeText = codeCard.locator(".game-code");
  const copy = codeCard.locator(".code-copy-button");
  await observed("lobby-code-card-visible", () => codeCard.waitFor());
  await observed("lobby-copy-control-visible", () => copy.waitFor());
  check((await observed("lobby-code-text", () => codeText.textContent())).trim() === code,
    "lobby-code-matches-current-game");
  const metrics = page.locator(".lobby-metrics");
  const playerMetric = metrics.locator(".lobby-metric").nth(0);
  const spectatorMetric = metrics.locator(".lobby-metric").nth(1);
  check((await observed("lobby-player-label", () => playerMetric.locator("dt").textContent())) === "Players",
    "lobby-player-metric-label");
  check((await observed("lobby-spectator-label", () => spectatorMetric.locator("dt").textContent())) === "Spectators",
    "lobby-spectator-metric-label");
  check(await observed("lobby-player-seat-summary", () => playerMetric.locator(".lobby-metric-total").textContent()) === `${players}/ ${playerCapacity} seats`,
    "lobby-player-seat-count");
  check(await observed("lobby-player-presence-summary", () => playerMetric.locator(".lobby-metric-presence").textContent()) === `${connectedPlayers} connected`,
    "lobby-connected-player-count");
  check(await observed("lobby-spectator-seat-summary", () => spectatorMetric.locator(".lobby-metric-total").textContent()) === `${spectators}/ ${spectatorCapacity} seats`,
    "lobby-spectator-seat-count");
  check(await observed("lobby-spectator-presence-summary", () => spectatorMetric.locator(".lobby-metric-presence").textContent()) === `${connectedSpectators} connected`,
    "lobby-connected-spectator-count");
  const roster = page.getByRole("list", { name: "Admitted players", exact: true });
  if (players === 0) {
    check(await observed("lobby-empty-attendance-state", () => page.getByText("No players have joined yet. Share the game code to invite your group.", { exact: true }).isVisible()),
      "lobby-empty-roster-invitation");
  } else {
    check(await observed("lobby-roster-entry-count", () => roster.getByRole("listitem").count()) === players,
      "lobby-attendance-list-count");
  }
  await observed("lobby-fonts-ready", () => page.evaluate(() => document.fonts.ready));
  for (const width of [320, 390, 768, 1440]) {
    await observed(`lobby-viewport-${width}`, () => page.setViewportSize({ width, height: 900 }));
    const cardBox = await observed(`lobby-code-card-${width}`, () => codeCard.boundingBox());
    const contentBox = await observed(`lobby-code-content-${width}`, () => codeCard.locator(".game-code-content").boundingBox());
    const copyBox = await observed(`lobby-copy-button-${width}`, () => copy.boundingBox());
    check(cardBox && contentBox && copyBox, "lobby-code-actions-visible");
    check(copyBox.width >= 82 && copyBox.height >= 44,
      `lobby-copy-button-touch-target-${width}`);
    check(contentBox.x + contentBox.width < copyBox.x &&
      Math.abs(contentBox.y + contentBox.height / 2 - copyBox.y - copyBox.height / 2) < 4,
      `lobby-code-and-copy-align-${width}`);
    check(copyBox.x + copyBox.width <= cardBox.x + cardBox.width - 12,
      `lobby-copy-button-inset-${width}`);
    const playerBox = await observed(`lobby-player-metric-bounds-${width}`, () => playerMetric.boundingBox());
    const spectatorBox = await observed(`lobby-spectator-metric-bounds-${width}`, () => spectatorMetric.boundingBox());
    check(playerBox && spectatorBox, "lobby-metrics-visible");
    check(Math.abs(playerBox.y - spectatorBox.y) < 2 && spectatorBox.x > playerBox.x,
      `lobby-metrics-two-column-row-${width}`);
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      `lobby-no-horizontal-overflow-${width}`);
  }
  await observed("lobby-viewport-restore", () => page.setViewportSize({ width: 1280, height: 900 }));
  if (copyCode) {
    await observed("lobby-grant-clipboard-permission", () => page.context().grantPermissions(["clipboard-read", "clipboard-write"]));
    await observed("lobby-copy-action", () => copy.click());
    await observed("lobby-copy-success-label", () => page.getByRole("button", { name: "Game code copied", exact: true }).waitFor());
    check(await page.evaluate(() => navigator.clipboard.readText()) === code,
      "lobby-game-code-actually-copied");
  }
}
