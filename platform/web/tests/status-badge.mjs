const colors = {
  neutral: "rgb(240, 236, 229)",
  info: "rgb(234, 242, 247)",
  warning: "rgb(255, 244, 219)",
  success: "rgb(234, 245, 238)",
  danger: "rgb(255, 240, 237)",
};

function check(value, name) {
  if (!value) {
    const error = new Error("Status layout check failed");
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

export async function statusBadge(page, label, tone) {
  const root = page.locator(`.status-badge.status-${tone}`).filter({ hasText: label });
  await observed("status-badge-visible", () => root.first().waitFor());
  check(await observed("status-badge-text", () => root.first().textContent()) === label,
    "badge-readable-label");
  const geometry = await root.first().evaluate((node) => {
    const style = getComputedStyle(node);
    const range = document.createRange();
    range.selectNodeContents(node);
    return {
      width: node.getBoundingClientRect().width,
      textWidth: range.getBoundingClientRect().width,
      horizontalPadding: parseFloat(style.paddingLeft) + parseFloat(style.paddingRight),
      background: style.backgroundColor,
      display: style.display,
    };
  });
  // Flex items are blockified: inline-flex computes to flex inside a flex row.
  check(["inline-flex", "flex"].includes(geometry.display), "badge-flex-layout");
  check(geometry.background === colors[tone], `badge-${tone}-color`);
  check(geometry.horizontalPadding <= 20, "badge-compact-padding");
  check(geometry.width <= geometry.textWidth + geometry.horizontalPadding + 3,
    "badge-intrinsic-label-width");
}

export async function currentGameLayout(page, label, tone, published, hostAlias) {
  check(await page.getByText("Admin account view: operator access is verified by the server.", { exact: true }).count() === 0,
    "dashboard-no-admin-operator-alert");
  check(await page.getByRole("link", { name: "Create game", exact: true }).count() === 0,
    "dashboard-create-hidden-while-game-exists");
  check(await page.locator('main a[href="/users"]').count() === 0,
    "dashboard-users-only-in-header-menu");
  await observed("dashboard-fonts-ready", () => page.evaluate(() => document.fonts.ready));
  for (const width of [320, 390, 768, 1440]) {
    await observed(`dashboard-viewport-${width}`, () => page.setViewportSize({ width, height: 900 }));
    await statusBadge(page, label, tone);
    const card = page.locator(".current-game");
    const refresh = card.getByRole("button", { name: "Refresh current game", exact: true });
    check((await observed(`dashboard-refresh-button-${width}`, () => refresh.textContent())).trim() === "",
      "dashboard-refresh-icon-only");
    check(await refresh.locator('svg[aria-hidden="true"]').count() === 1,
      "dashboard-refresh-decorative-icon");
    const refreshBox = await observed(`dashboard-refresh-bounds-${width}`, () => refresh.boundingBox());
    const titleBox = await observed(`dashboard-title-bounds-${width}`, () => card.locator(".current-game-heading h2").boundingBox());
    check(refreshBox && titleBox, "dashboard-refresh-and-title-visible");
    check(refreshBox.width >= 44 && refreshBox.width <= 45 && refreshBox.height >= 44,
      `dashboard-refresh-compact-touch-target-${width}`);
    check(refreshBox.x >= titleBox.x + titleBox.width &&
      Math.abs(refreshBox.y + refreshBox.height / 2 - titleBox.y - titleBox.height / 2) < 2,
      `dashboard-refresh-right-of-title-same-row-${width}`);
    const panelBox = await card.boundingBox();
    check(panelBox, "dashboard-panel-visible");
    const rightInset = panelBox.x + panelBox.width - refreshBox.x - refreshBox.width;
    check(rightInset >= 20 && rightInset <= 32,
      `dashboard-refresh-aligns-with-panel-right-padding-${width}`);
    const badge = card.locator(".game-status-row .status-badge");
    await observed("dashboard-designated-host-alias", () => card.locator(".designated-host-alias").getByText(hostAlias, { exact: true }).waitFor());
    const publication = card.locator(".game-publication");
    check(await publication.textContent() === (published ?? "Not published"), "dashboard-publication-label");
    const a = await badge.boundingBox();
    const b = await publication.boundingBox();
    const row = await card.locator(".game-status-row").boundingBox();
    check(a && b && row, "dashboard-status-row-visible");
    check(a.width < row.width / 2, `dashboard-compact-badge-${width}`);
    check(b.x >= a.x + a.width + 10, `dashboard-publication-right-of-badge-${width}`);
    check(Math.abs(a.y + a.height / 2 - b.y - b.height / 2) < 2,
      `dashboard-publication-same-row-${width}`);
    check(Math.abs(b.x + b.width - row.x - row.width) < 2,
      `dashboard-publication-right-aligned-${width}`);
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      `dashboard-no-page-overflow-${width}`);
    const spacing = await card.evaluate((node) => {
      const style = getComputedStyle(node);
      return {
        gap: parseFloat(style.rowGap),
        margins: [...node.children].map((child) => {
          const css = getComputedStyle(child);
          return parseFloat(css.marginTop) + parseFloat(css.marginBottom);
        }),
        actionHeight: node.querySelector("a.button").getBoundingClientRect().height,
      };
    });
    check(spacing.gap <= 20 && spacing.margins.every((margin) => margin === 0),
      `dashboard-consistent-card-spacing-${width}`);
    check(spacing.actionHeight >= 44, "dashboard-action-touch-target");
  }
  await page.setViewportSize({ width: 1280, height: 900 });
  const refresh = page.locator(".current-game").getByRole("button", { name: "Refresh current game", exact: true });
  const readback = page.waitForResponse((response) => new URL(response.url()).pathname === "/api/games/current");
  await observed("dashboard-refresh-focus", () => refresh.focus());
  await observed("dashboard-refresh-keyboard", () => refresh.press("Enter"));
  check((await observed("dashboard-refresh-request", () => readback)).ok(), "dashboard-refresh-keyboard-readback");
  await observed("dashboard-game-row-after-refresh", () => page.locator(".current-game .game-status-row").waitFor());
  check(await page.getByRole("link", { name: "Create game", exact: true }).count() === 0,
    "dashboard-create-stays-hidden-after-refresh");
  const menu = page.getByRole("button", { name: "Account menu", exact: true });
  await menu.click();
  check(await observed("dashboard-header-users-link", () => page.locator("header .account-menu-panel").getByRole("link", { name: "Users", exact: true }).isVisible()),
    "dashboard-users-still-accessible-from-header");
  await menu.click();
}

export async function gameStatusLayout(page, stateLabel, stateTone) {
  const row = page.locator(".game-live-status-row");
  const live = row.locator(".status-badge.status-success").getByText("Live", { exact: true });
  const state = row.locator(`.status-badge.status-${stateTone}`).getByText(stateLabel, { exact: true });
  await observed("game-live-and-state-badges", () => live.waitFor());
  await observed("game-state-badge", () => state.waitFor());
  for (const width of [320, 390, 768, 1440]) {
    await observed(`game-status-viewport-${width}`, () => page.setViewportSize({ width, height: 900 }));
    const liveBox = await observed(`game-live-bounds-${width}`, () => live.boundingBox());
    const stateBox = await observed(`game-state-bounds-${width}`, () => state.boundingBox());
    const rowBox = await observed(`game-status-row-bounds-${width}`, () => row.boundingBox());
    check(liveBox && stateBox && rowBox, "game-status-visible");
    check(Math.abs(liveBox.y + liveBox.height / 2 - stateBox.y - stateBox.height / 2) < 2,
      `game-state-shares-live-row-${width}`);
    check(stateBox.x >= liveBox.x + liveBox.width,
      `game-state-follows-live-badge-${width}`);
    check(stateBox.x + stateBox.width <= rowBox.x + rowBox.width + 1,
      `game-state-fits-status-row-${width}`);
    check(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      `game-page-no-horizontal-overflow-${width}`);
  }
  await observed("game-status-viewport-restore", () => page.setViewportSize({ width: 1280, height: 900 }));
}
