import { chromium } from "playwright";
import {
  accountManagement,
  membershipManagement,
  hostTransfer,
  runningLeave,
  finalExits,
} from "./additional.mjs";
import { endings, deleteAccount } from "./endings.mjs";
import { queuedAuthentication } from "./auth-queue.mjs";
import { currentGameLayout, gameStatusLayout, statusBadge } from "./status-badge.mjs";
import { lobbyLayout } from "./lobby-layout.mjs";
import { randomBytes } from "node:crypto";
import { join } from "node:path";

let input = "";
for await (const chunk of process.stdin) input += chunk;
const { origin, username, enrollment, output } = JSON.parse(input);
input = "";
let browser, admin, adminContext, gameId, gameCode;
const completed = [],
  errors = [],
  httpStatuses = [],
  gameReplies = [],
  contexts = [],
  players = [];
let current = "browser-start",
  phase = "start";
const assert = (value, check) => {
  if (!value) {
    const error = new Error("Acceptance check failed");
    error.check = check;
    throw error;
  }
};
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function openAccountMenu(page) {
  const toggle = page.locator(".account-menu-toggle");
  await toggle.waitFor();
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await page.locator(".account-menu-panel").waitFor();
}
async function openGameOptions(page) {
  const toggle = page.getByRole("button", { name: "Options", exact: true });
  await toggle.waitFor();
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await page.locator("#game-options-panel").waitFor();
}
async function step(name, fn) {
  current = name;
  phase = "start";
  await fn();
  completed.push(name);
}
async function context(options = {}) {
  const c = await browser.newContext({ ignoreHTTPSErrors: true, ...options });
  contexts.push(c);
  c.setDefaultTimeout(40000);
  await c.addInitScript(() => {
    const original = window.fetch.bind(window);
    window.__mutationProbe = [];
    window.__socketProbe = [];
    const OriginalSocket = window.WebSocket;
    window.WebSocket = class extends OriginalSocket {
      constructor(...args) {
        super(...args);
        const item = { opened: false, received: 0, sent: 0 };
        window.__socketProbe.push(item);
        this.addEventListener("open", () => {
          item.opened = true;
        });
        this.addEventListener("message", () => {
          item.received++;
        });
        this.addEventListener("close", (event) => {
          item.code = event.code;
          item.clean = event.wasClean;
        });
        this.addEventListener("error", () => {
          item.error = true;
        });
        const send = this.send.bind(this);
        this.send = (data) => {
          item.sent++;
          return send(data);
        };
      }
    };
    window.fetch = async (...args) => {
      const request = args[0];
      if (
        !(request instanceof Request) ||
        request.method === "GET" ||
        !new URL(request.url).pathname.startsWith("/api/games")
      )
        return original(...args);
      const suffix = new URL(request.url).pathname.split("/").at(-1);
      const item = {
        aborted: false,
        finished: false,
        operation: [
          "manual",
          "random",
          "cancel",
          "winner",
          "start",
          "lobby",
        ].includes(suffix)
          ? suffix
          : "other",
      };
      const started = performance.now();
      window.__mutationProbe.push(item);
      request.signal.addEventListener("abort", () => {
        item.aborted = true;
        item.abortMs = Math.round(performance.now() - started);
      });
      const response = await original(...args);
      item.status = response.status;
      response
        .clone()
        .json()
        .then((body) => {
          item.finished = true;
          item.result = [
            "started",
            "created",
            "lobby_opened",
            "pending",
            "committed",
            "call_accepted",
          ].includes(body.result)
            ? body.result
            : "other";
        })
        .catch((error) => {
          item.error = ["AbortError", "SyntaxError", "TypeError"].includes(
            error.name,
          )
            ? error.name
            : "other";
        });
      return response;
    };
  });
  c.on("page", (page) => {
    page.on("pageerror", () => errors.push("page-error"));
    page.on("response", async (response) => {
      const path = new URL(response.url()).pathname;
      const index = path.includes("/commands/")
        ? 4
        : [
            "/api/session",
            "/api/auth/enrollment/redeem",
            "/api/auth/enrollment/complete",
            "/api/auth/login",
          ].indexOf(path);
      if (
        index === 0 &&
        httpStatuses.filter((item) => item.endpoint === 0).length >= 10
      )
        return;
      if (index >= 0 && httpStatuses.length < 80) {
        const item = {
          endpoint: index,
          status: response.status(),
          hasDate: !!response.headers().date,
        };
        httpStatuses.push(item);
        if (index === 4) {
          const body = await response.json().catch(() => null);
          const status = body?.result_status?.status;
          item.commandStatus = [
            "committed",
            "pending",
            "unknown",
            "expired",
            "inadmissible",
          ].includes(status)
            ? status
            : "other";
        }
      }
      if (path.endsWith("/start")) {
        const body = await response.json().catch(() => null);
        const allowed = ["started", "pending", "committed"];
        gameReplies.push({
          status: response.status(),
          jsonReadable: body !== null,
          jsonMedia: response
            .headers()
            ["content-type"]?.startsWith("application/json"),
          result: allowed.includes(body?.result) ? body.result : "other",
          fields:
            body && typeof body === "object"
              ? Object.keys(body).filter((key) =>
                  [
                    "result",
                    "game",
                    "game_code",
                    "players",
                    "receipt",
                    "error",
                    "snapshot",
                    "response",
                  ].includes(key),
                )
              : [],
        });
      }
    });
  });
  return c;
}
async function api(page, path) {
  return page.evaluate(async (path) => {
    const response = await fetch(path, {
      credentials: "same-origin",
      cache: "no-store",
    });
    return { status: response.status, body: await response.json() };
  }, path);
}
async function snapshot(page, view, id = gameId) {
  for (let attempt = 0; attempt < 20; attempt++) {
    const response = await api(page, `/api/games/${id}/sync?view=${view}`);
    if (response.status === 200 && response.body.snapshot)
      return { status: response.status, body: response.body.snapshot };
    assert(
      response.status === 503 || response.status === 429,
      "snapshot-authority",
    );
    await wait(100);
  }
  assert(false, "snapshot-available");
}
async function until(fn, check) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (await fn()) return;
    await wait(200);
  }
  assert(false, check);
}
async function click(page, name) {
  await page.getByRole("button", { name, exact: true }).click();
}
async function resolveOriginalUntil(page, complete, check) {
  await until(async () => {
    for (const name of ["Retry original request", "Check original result"]) {
      const button = page.getByRole("button", { name, exact: true });
      if (await button.count()) {
        if (await button.isEnabled()) await button.click();
        return false;
      }
    }
    return complete();
  }, check);
}
async function issueCall(page, button, expected, id = gameId) {
  await until(async () => {
    const action = page.getByRole("button", { name: button, exact: true });
    if ((await action.count()) && (await action.isEnabled())) return true;
    for (const name of ["Refresh remaining pool", "Check original result"]) {
      const control = page.getByRole("button", { name, exact: true });
      if ((await control.count()) && (await control.isEnabled()))
        await control.click();
    }
    return false;
  }, "call-controls-ready");
  await click(page, button);
  let freshRetries = 0;
  await until(async () => {
    const count = (await snapshot(page, "account", id)).body.calls.length;
    assert(count <= expected, "no-duplicate-call");
    if (count === expected) return true;
    const check = page.getByRole("button", {
      name: "Check original result",
      exact: true,
    });
    const retry = page.getByRole("button", {
      name: "Retry original request",
      exact: true,
    });
    if ((await retry.count()) && (await retry.isEnabled())) {
      await wait(500);
      if ((await retry.count()) && (await retry.isEnabled()))
        await retry.click();
    } else if ((await check.count()) && (await check.isEnabled()))
      await check.click();
    else if (
      !(await retry.count()) &&
      freshRetries < 4 &&
      (await page
        .getByText(
          /The server state changed|Enter an exact value from the current remaining pool/,
        )
        .count()) &&
      (await page
        .getByRole("button", { name: button, exact: true })
        .isEnabled())
    ) {
      freshRetries++;
      await click(page, button);
    }
    return false;
  }, "committed-call-readback");
}
const password = randomBytes(20).toString("hex");
try {
  browser = await chromium.launch({
    executablePath: "/usr/bin/chromium",
    headless: true,
    args: ["--no-sandbox"],
  });
  const publicContext = await context();
  const home = await publicContext.newPage();
  await step("public-home-desktop", async () => {
    const response = await home.goto(origin);
    const headers = response.headers();
    assert(
      headers["cache-control"] === "no-store" &&
        headers["referrer-policy"] === "no-referrer" &&
        headers["x-content-type-options"] === "nosniff",
      "served-private-cache-headers",
    );
    assert(
      headers["content-security-policy"]?.includes("frame-ancestors 'none'") &&
        headers["content-security-policy"]?.includes("connect-src 'self'"),
      "served-csp",
    );
    const unknownApi = await home.request.get(origin + "/api/not-a-route");
    assert(
      unknownApi.status() === 404 &&
        unknownApi.headers()["content-type"]?.startsWith("application/json"),
      "api-not-spa-fallback",
    );
    await home
      .getByRole("heading", { name: "Good company. Great games." })
      .waitFor();
    assert(
      (await home
        .getByRole("textbox", { name: /Game code character/ })
        .count()) === 8,
      "code-box-count",
    );
    assert(
      (await home.getByRole("link", { name: "Users", exact: true }).count()) ===
        0,
      "users-hidden",
    );
    await home.screenshot({
      path: join(output, "home-desktop.png"),
      fullPage: true,
    });
  });
  await step("public-home-mobile", async () => {
    await home.setViewportSize({ width: 390, height: 844 });
    assert(
      await home.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      "mobile-overflow",
    );
    await home.screenshot({
      path: join(output, "home-mobile.png"),
      fullPage: true,
    });
  });
  await step("code-format-error-modal", async () => {
    phase = "open";
    await click(home, "Join a game");
    phase = "visible";
    await home.getByRole("dialog").waitFor();
    assert(
      await home.evaluate(() => !!document.activeElement?.closest("dialog")),
      "dialog-receives-focus",
    );
    await home.keyboard.press("Tab");
    assert(
      await home.evaluate(() => !!document.activeElement?.closest("dialog")),
      "dialog-traps-tab",
    );
    phase = "dismiss";
    await click(home, "Go back");
    assert((await home.getByRole("dialog").count()) === 0, "modal-dismissed");
  });
  await step("private-users-denied", async () => {
    await home.goto(origin + "/users");
    await home
      .getByText(
        "A normal, Verified admin session is required to manage users.",
        { exact: true },
      )
      .waitFor();
    assert(
      (await home.locator("table").count()) === 0,
      "private-table-not-rendered",
    );
  });
  await step("enrollment", async () => {
    adminContext = await context();
    admin = await adminContext.newPage();
    await admin.goto(enrollment);
    phase = "password-form";
    const newPassword = admin.getByLabel("New password", { exact: true });
    await newPassword.waitFor();
    assert(!admin.url().includes("#"), "enrollment-fragment-cleared");
    const passwordBanner = admin.getByRole("banner");
    assert((await passwordBanner.getByRole("link", { name: "Login", exact: true }).count()) === 1, "restricted-session-header-login");
    assert((await passwordBanner.getByRole("link", { name: "Finish setup", exact: true }).count()) === 0, "restricted-header-no-finish-setup");
    assert((await passwordBanner.getByRole("button", { name: "Log out", exact: true }).count()) === 0, "restricted-header-no-logout");
    assert((await admin.getByText(/fixed deadline|Completing setup does not extend it/).count()) === 0, "enrollment-deadline-copy-removed");
    assert((await admin.getByText("10-50 characters", { exact: true }).count()) === 1, "password-helper-copy");
    const revealNewPassword = admin.getByRole("button", { name: "Show password", exact: true }).first();
    await revealNewPassword.click();
    assert(await newPassword.getAttribute("type") === "text", "new-password-reveal");
    await admin.getByRole("button", { name: "Hide password", exact: true }).click();
    assert(await newPassword.getAttribute("type") === "password", "new-password-hide");
    const confirmation = admin.getByLabel("Confirm password", { exact: true });
    await newPassword.fill(password);
    await confirmation.fill(password);
    assert((await admin.getByRole("button", { name: "Show password", exact: true }).count()) === 2, "shared-password-toggle-fields");
    assert((await admin.getByRole("button", { name: /Clear .*password/ }).count()) === 0, "password-clear-buttons-removed");
    assert((await admin.getByText(/Interpret ASCII escapes|For control characters/).count()) === 0, "password-escape-help-removed");
    assert((await admin.getByRole("link", { name: "Back to Home", exact: true }).count()) === 0, "bottom-home-link-removed");
    phase = "submit";
    await click(admin, "Set password");
    phase = "normal-session";
    await openAccountMenu(admin);
    assert(await admin.locator(".account-menu-toggle").textContent() === username, "account-toggle-shows-username-only");
    const menuText = await admin.locator(".account-menu-panel").innerText();
    assert(menuText.includes("Games") && menuText.includes("History") && menuText.includes("Users"), "account-options-in-popup");
    assert(!menuText.includes("Admin"), "account-type-hidden-from-menu");
    assert(await admin.locator(".account-menu-panel").evaluate((node) => [...node.children].map((item) => item.textContent.trim()).filter(Boolean).join(",")) === "Games,History,Users,Log out", "logout-below-menu-options");
    await admin.getByRole("link", { name: "Users", exact: true }).waitFor();
    const result = await api(admin, "/api/session");
    assert(
      result.status === 200 &&
        result.body.authenticated &&
        result.body.session.scope === "Normal",
      "normal-session-readback",
    );
  });
  await step("logout-and-login", async () => {
    await admin.goto(origin + "/enroll");
    const enrollmentNavigation = admin.getByRole("navigation", { name: "Main navigation" });
    await enrollmentNavigation.waitFor();
    await enrollmentNavigation.getByRole("link", { name: "Login", exact: true }).waitFor();
    const enrollmentHeaderState = await admin.evaluate(() => {
      const navigation = document.querySelector('nav[aria-label="Main navigation"]');
      return {
        path: location.pathname,
        loginLinks: [...(navigation?.querySelectorAll("a") ?? [])].filter((link) => link.textContent.trim() === "Login").length,
        logoutButtons: [...(navigation?.querySelectorAll("button") ?? [])].filter((button) => button.textContent.includes("Log out")).length,
        identityBadges: navigation?.querySelectorAll(".badge.identity").length ?? 0,
      };
    });
    assert(enrollmentHeaderState.path === "/enroll" && enrollmentHeaderState.loginLinks === 1 && enrollmentHeaderState.logoutButtons === 0 && enrollmentHeaderState.identityBadges === 0, `enroll-header-login-with-existing-session ${JSON.stringify(enrollmentHeaderState)}`);
    assert((await api(admin, "/api/session")).body.authenticated, "missing-link-preserves-existing-session");
    await admin.goto(origin + "/games");
    await openAccountMenu(admin);
    await admin.getByRole("link", { name: "Users", exact: true }).waitFor();
    phase = "logout";
    const loggedOut = admin.waitForResponse(
      (response) => new URL(response.url()).pathname === "/api/auth/logout",
    );
    await click(admin, "Log out");
    await loggedOut;
    await admin
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { name: "Login", exact: true })
      .waitFor();
    phase = "logout-readback";
    assert(
      !(await api(admin, "/api/session")).body.authenticated,
      "logout-readback",
    );
    await admin.goto(origin + "/login");
    await admin.getByLabel("Username", { exact: true }).fill(username);
    await admin.getByLabel("Password", { exact: true }).fill(password);
    phase = "login";
    await click(admin, "Log in");
    await openAccountMenu(admin);
    await admin.getByRole("link", { name: "Users", exact: true }).waitFor();
  });
  let hostEnrollment;
  await step("long-account-identity-reflows-on-mobile", async () => {
    const identity = admin.locator(".account-menu-toggle");
    const original = await identity.textContent();
    await identity.evaluate((node) => {
      node.textContent = "W".repeat(50);
    });
    await admin.setViewportSize({ width: 320, height: 780 });
    assert(
      await admin.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth + 1,
      ),
      "long-identity-no-page-overflow",
    );
    await identity.evaluate((node, text) => {
      node.textContent = text;
    }, original);
    await admin.setViewportSize({ width: 1280, height: 900 });
  });
  await step("admin-users-create-private-handoff", async () => {
    await admin.goto(origin + "/users");
    await admin.getByRole("heading", { name: "Users", exact: true }).waitFor();
    phase = "create-form";
    await click(admin, "Create account");
    await admin.getByLabel("Username", { exact: true }).fill("BrowserHost");
    phase = "create-submit";
    await admin
      .locator("form")
      .getByRole("button", { name: "Create account", exact: true })
      .click();
    phase = "handoff";
    await admin.getByRole("heading", { name: /Private.*link/i }).waitFor();
    const users = await api(admin, "/api/users");
    assert(
      users.status === 200 &&
        users.body.users.some((a) => a.username === "BrowserHost"),
      "created-account-readback",
    );
    phase = "dismiss-link";
    hostEnrollment = await admin
      .getByLabel("Private access URL", { exact: true })
      .inputValue();
    await click(admin, "Dismiss and clear");
    assert(
      (await admin.locator("input[readonly],textarea[readonly]").count()) === 0,
      "handoff-cleared",
    );
  });
  const harness = {
    admin,
    origin,
    context,
    step,
    click,
    api,
    snapshot,
    until,
    assert,
    wait,
    issueCall,
    resolveOriginalUntil,
    mark: (value) => {
      phase = value;
    },
  };
  const { host, hostId } = await accountManagement({
    ...harness,
    enrollment: hostEnrollment,
  });
  hostEnrollment = undefined;
  await step("create-and-open-lobby", async () => {
    await admin.goto(origin + "/games");
    await admin.getByRole("heading", { name: "Games", exact: true }).waitFor();
    const adminAccount = (await api(admin, "/api/session")).body.account;
    assert(adminAccount.role === "Admin", "game-create-actor-is-admin");
    let slotReadIntercepts = 0;
    await admin.route("**/api/games/current", async (route) => {
      if (route.request().method() === "GET") {
        slotReadIntercepts += 1;
        await route.fulfill({
          status: 503,
          contentType: "application/json",
          body: JSON.stringify({ error: { code: "service_unavailable", message: "test-only slot read failure" } }),
        });
      } else {
        await route.continue();
      }
    });
    await admin.getByRole("link", { name: "Create game", exact: true }).click();
    await admin.getByRole("heading", { name: "Create a game", exact: true }).waitFor();
    await until(async () => slotReadIntercepts > 0, "admin-slot-read-failure-was-injected");
    assert(slotReadIntercepts > 0, "admin-slot-read-failure-was-injected");
    const createButton = admin.getByRole("button", { name: "Create game", exact: true });
    assert(await createButton.isEnabled(), "admin-create-not-disabled-by-slot-read-failure");
    const poolInput = admin.getByLabel("Numeric pool upper bound (1–1,000)");
    assert((await admin.getByText("Check the submitted fields and try again.", { exact: true }).count()) === 0, "no-submission-alert-on-create-load");
    assert((await admin.getByText("The current game slot could not be verified. The server will check availability when you submit.", { exact: true }).count()) === 0, "slot-read-failure-has-no-load-message");
    await poolInput.fill("9");
    await admin.getByLabel("Board side length (2–10)").fill("2");
    await admin.getByLabel("Enable free cells", { exact: true }).uncheck();
    await admin.unroute("**/api/games/current");
    phase = "create";
    await createButton.click();
    phase = "setup";
    await admin
      .getByRole("heading", { name: "Game setup", exact: true })
      .waitFor();
    gameId = new URL(admin.url()).pathname.split("/")[2];
    let privacyReads = 0;
    await admin.route("**/api/games/current", async (route) => {
      privacyReads++;
      await route.continue({ headers: { ...route.request().headers(), "Sec-GPC": "1", DNT: "1", "Sec-Fetch-Storage-Access": "none" } });
    });
    await admin.goto(origin + "/games");
    await admin.getByRole("link", { name: "Resume setup", exact: true }).waitFor();
    assert(privacyReads > 0, "privacy-enabled-game-lookup-was-sent");
    await admin.unroute("**/api/games/current");
    await currentGameLayout(admin, "New", "neutral", undefined, adminAccount.username);
    await admin.getByRole("link", { name: "Resume setup", exact: true }).click();
    await admin.getByRole("heading", { name: "Game setup", exact: true }).waitFor();
    phase = "unsaved-draft-save";
    await admin.getByLabel("Numeric pool upper bound (1–1,000)").fill("10");
    await until(
      async () =>
        await admin
          .getByRole("button", { name: "Save changes", exact: true })
          .isEnabled(),
      "draft-save-enabled",
    );
    assert(
      (await admin
        .getByLabel("Numeric pool upper bound (1–1,000)")
        .inputValue()) === "10",
      "draft-value-preserved",
    );
    await click(admin, "Save changes");
    await until(
      async () =>
        (await snapshot(admin, "account")).body.configuration
          .numeric_upper_bound === 10,
      "configuration-save-readback",
    );
    await admin.getByLabel("Numeric pool upper bound (1–1,000)").fill("9");
    await click(admin, "Save changes");
    await until(
      async () =>
        (await snapshot(admin, "account")).body.configuration
          .numeric_upper_bound === 9,
      "configuration-update-readback",
    );
    phase = "open-lobby";
    await click(admin, "Open saved lobby");
    await admin
      .getByRole("dialog")
      .getByRole("button", { name: "Publish the saved setup?", exact: true })
      .click();
    await admin
      .getByRole("heading", { name: "Waiting for the host to start" })
      .waitFor();
    await gameStatusLayout(admin, "Awaiting players", "warning");
    await admin.locator(".designated-host-line .designated-host-alias").getByText(adminAccount.username, { exact: true }).waitFor();
    await openGameOptions(admin);
    assert((await admin.locator("#game-options-panel").getByRole("button", { name: "Resume game access", exact: true }).count()) === 0, "host-options-menu-no-resume-action");
    assert((await admin.locator("#game-options-panel").getByText(/Resume is an explicit action/, { exact: false }).count()) === 0, "host-options-menu-no-resume-copy");
    assert((await admin.locator("#game-options-panel").getByRole("button", { name: "Transfer host", exact: true }).count()) === 1, "host-options-menu-transfer");
    for (const width of [320, 390, 768, 1440]) {
      await admin.setViewportSize({ width, height: 900 });
      const options = admin.getByRole("button", { name: "Options", exact: true });
      const buttonBox = await options.boundingBox();
      const panelBox = await admin.locator("#game-options-panel").boundingBox();
      assert(buttonBox && panelBox && buttonBox.width >= 44 && buttonBox.height >= 44, `options-touch-target-${width}`);
      assert(panelBox.x >= 0 && panelBox.x + panelBox.width <= width, `options-panel-fits-${width}`);
      assert(panelBox.y >= 0 && panelBox.y + panelBox.height <= 900, `options-panel-visible-${width}`);
      assert(await admin.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `options-no-overflow-${width}`);
      assert(await admin.locator(".game-options-label").isVisible() === (width > 540), `options-responsive-label-${width}`);
    }
    await admin.setViewportSize({ width: 1280, height: 900 });
    await admin.keyboard.press("Escape");
    assert((await admin.getByRole("button", { name: "Options", exact: true }).getAttribute("aria-expanded")) === "false", "options-escape-dismiss");
    await openGameOptions(admin);
    await admin.locator(".game-options-dismiss").click({ position: { x: 10, y: 10 } });
    assert((await admin.getByRole("button", { name: "Options", exact: true }).getAttribute("aria-expanded")) === "false", "options-outside-dismiss");
    const view = await snapshot(admin, "account");
    assert(
      view.status === 200 && view.body.game.state === "awaiting_players",
      "lobby-readback",
    );
    gameCode = view.body.game_code;
    await lobbyLayout(admin, gameCode, 0, 20, 0, 0, 50, 0, true);
    await statusBadge(admin, "Awaiting players", "warning");
    await statusBadge(admin, "Live", "success");
    await admin.goto(origin + "/games");
    await admin.getByRole("link", { name: "Open lobby", exact: true }).waitFor();
    await currentGameLayout(admin, "Awaiting players", "warning", gameCode, adminAccount.username);
    await admin.getByRole("link", { name: "Open lobby", exact: true }).click();
    await admin.getByRole("heading", { name: "Waiting for the host to start" }).waitFor();
  });
  await step("player-and-spectator-admission", async () => {
    for (const alias of ["BrowserOne", "BrowserTwo"]) {
      const c = await context();
      const p = await c.newPage();
      players.push(p);
      await p.goto(origin + "/join/" + gameCode);
      await p.getByLabel("Your alias", { exact: true }).fill(alias);
      await p
        .getByLabel(
          "What do you like most about Brews? (optional recovery answer)",
        )
        .fill(randomBytes(16).toString("hex"));
      await click(p, "Join as player");
      await p
        .getByRole("heading", { name: "Waiting for the host to start" })
        .waitFor();
      assert(
        (await p
          .getByRole("table", { name: "Game board", exact: true })
          .count()) === 0,
        "no-prestart-board",
      );
    }
    const c = await context();
    const spectator = await c.newPage();
    players.push(spectator);
    await spectator.goto(origin + "/join/" + gameCode);
    await spectator.getByLabel("Spectate", { exact: true }).check();
    await click(spectator, "Join as spectator");
    await spectator
      .getByRole("heading", { name: "Waiting for the host to start" })
      .waitFor();
    phase = "connected-roster";
    await until(async () => {
      const v = await snapshot(admin, "account");
      return (
        v.body.connected_player_count === 2 &&
        v.body.connected_spectator_count === 1
      );
    }, "connected-roster");
    for (const page of [admin, ...players]) {
      await until(() => page.evaluate(() => window.__socketProbe.some((socket) =>
        socket.opened && socket.received > 0 && socket.sent > 0
      )), "websocket-open-snapshot-ack");
    }
    await lobbyLayout(admin, gameCode, 2, 20, 2, 1, 50, 1);
  });
  await membershipManagement({ ...harness, players, gameId, gameCode });
  await hostTransfer({ ...harness, host, hostId, gameId });
  await step("start-board-privacy", async () => {
    phase = "start";
    await until(async () => {
      const check = admin.getByRole("button", {
        name: "Check original result",
        exact: true,
      });
      if ((await check.count()) && (await check.isEnabled()))
        await check.click();
      return await admin
        .getByRole("button", { name: "Start game", exact: true })
        .isEnabled();
    }, "lobby-command-reconciled");
    await click(admin, "Start game");
    await admin
      .getByRole("heading", { name: "Call a value", exact: true })
      .waitFor();
    assert(await admin.getByRole("heading", { name: "Call history", exact: true }).count() === 1, "active-game-single-call-history-heading");
    assert(await admin.getByText("Speak new calls on this device", { exact: true }).count() === 0, "active-game-no-device-speech-control");
    for (const p of players.slice(0, 2))
      await p.getByRole("table", { name: "Game board", exact: true }).waitFor();
    assert(
      (await players[2]
        .getByRole("table", { name: "Game board", exact: true })
        .count()) === 0,
      "spectator-has-no-board",
    );
    assert(
      (await players[0]
        .getByRole("button", { name: "Draw random value", exact: true })
        .count()) === 0,
      "player-no-operator-controls",
    );
    phase = "start-reconciliation";
    await until(async () => {
      const check = admin.getByRole("button", {
        name: "Check original result",
        exact: true,
      });
      if ((await check.count()) && (await check.isEnabled()))
        await check.click();
      return await admin
        .getByRole("button", { name: "Draw random value", exact: true })
        .isEnabled();
    }, "start-command-reconciled");
  });
  await runningLeave({ ...harness, players, gameId, gameCode });
  await step("manual-and-random-live-calls", async () => {
    phase = "manual-input";
    await admin.getByLabel("Manual value", { exact: true }).fill("1");
    phase = "manual-submit";
    await issueCall(admin, "Call value", 1);
    phase = "random-call";
    await issueCall(admin, "Draw random value", 2);
    await until(
      async () =>
        (await snapshot(players[0], "player")).body.calls.length === 2,
      "player-snapshot-readback",
    );
    await click(players[2], "Display mode");
    await click(players[2], "Exit display mode");
  });
  await step("automatic-qualification-and-one-winner", async () => {
    for (let i = 0; i < 9; i++) {
      const v = (await snapshot(admin, "account")).body;
      if (v.players.some((p) => p.board?.qualified)) break;
      const count = v.calls.length;
      await issueCall(admin, "Draw random value", count + 1);
    }
    phase = "winner-dialog";
    await admin
      .getByRole("button", { name: "Select as winner", exact: true })
      .first()
      .click();
    await admin.getByRole("dialog").waitFor();
    phase = "winner-confirm";
    await admin.getByRole("dialog").getByRole("button").last().click();
    await admin
      .getByRole("heading", { name: "Game result", exact: true })
      .waitFor();
    const result = await api(admin, `/api/games/${gameId}/result?view=account`);
    await statusBadge(admin, "Resolved", "success");
    assert(
      result.status === 200 &&
        result.body.result?.state === "resolved" &&
        typeof result.body.result?.winner?.player_id === "string",
      "terminal-result-readback",
    );
    const ended =
      new Date(result.body.result.ended_at)
        .toISOString()
        .slice(0, 19)
        .replace("T", " ") + " UTC";
    await admin.getByText(`Ended: ${ended}`, { exact: true }).waitFor();
    for (const p of players.slice(0, 2)) {
      await p
        .getByRole("heading", { name: "Game result", exact: true })
        .waitFor();
      await p.getByText(`Ended: ${ended}`, { exact: true }).waitFor();
    }
    await players[2].getByText(`Ended: ${ended}`, { exact: true }).waitFor();
  });
  await step("history-list-and-detail", async () => {
    await admin.goto(origin + "/history");
    await admin
      .getByRole("link", { name: "View history", exact: true })
      .first()
      .waitFor();
    await statusBadge(admin, "Resolved", "success");
    await admin
      .getByRole("link", { name: "View history", exact: true })
      .first()
      .click();
    await admin
      .getByRole("heading", { name: "A game to remember", exact: true })
      .waitFor();
    await statusBadge(admin, "Resolved", "success");
    await admin
      .getByLabel("Inspect a player's final board", { exact: true })
      .selectOption({ index: 1 });
    const historyBoard = admin.getByRole("table", { name: /final board$/ });
    assert(
      (await historyBoard.getByRole("row").count()) === 2,
      "history-semantic-rows",
    );
    assert(
      (
        await historyBoard.getByRole("cell").first().getAttribute("aria-label")
      ).startsWith("Row 1, column 1:"),
      "history-one-based-coordinates",
    );
    assert(
      (await admin
        .getByRole("button", { name: "Draw random value", exact: true })
        .count()) === 0,
      "history-read-only",
    );
    assert(
      (await api(admin, "/api/history")).body.games.some(
        (g) => g.game_id === gameId,
      ),
      "history-readback",
    );
  });
  await finalExits({ ...harness, players, gameId });
  await endings(harness);
  await deleteAccount({ ...harness, host, hostId });
  await queuedAuthentication({ ...harness, username, password });
  await step("no-browser-script-errors", async () =>
    assert(errors.length === 0, "browser-errors"),
  );
  process.stdout.write(
    JSON.stringify({
      status: "passed",
      checks: completed,
      check_count: completed.length,
      browser_errors: errors.length,
    }),
  );
} catch (error) {
  const participants = await Promise.all(
    players.map((page) =>
      page.evaluate(() => ({
        view: ["player", "spectator"].includes(
          location.pathname.split("/").pop(),
        )
          ? location.pathname.split("/").pop()
          : "other",
        connection: [
          ...document.querySelectorAll('.connection-status .status-badge'),
        ].map((node) => node.textContent),
        controls: [
          "Switch to spectator",
          "Switch to player",
          "Check original result",
          "Retry original request",
        ].map((name) => {
          const button = [...document.querySelectorAll("button")].find(
            (value) => value.textContent === name,
          );
          return { present: !!button, disabled: button?.disabled };
        }),
        mutations: window.__mutationProbe,
      })),
    ),
  );
  const gameState =
    gameId && admin
      ? await admin.evaluate(() => ({
          signals: [
            "Live",
            "Synchronizing",
            "Reconnecting",
            "original command",
            "Check its result",
            "could not be verified",
            "state changed",
            "Access expired",
            "temporarily unavailable",
          ].map((text) => document.body.textContent.includes(text)),
          controls: [
            "Draw random value",
            "Call value",
            "Check original result",
            "Retry original request",
          ].map((name) => {
            const button = [...document.querySelectorAll("button")].find(
              (button) => button.textContent === name,
            );
            return { present: !!button, disabled: button?.disabled };
          }),
          exactConnection: [
            ...document.querySelectorAll('.connection-status .status-badge'),
          ].map((node) => node.textContent),
          invalidFrame: document.body.textContent.includes(
            "invalid or out of order",
          ),
          socketProbe: window.__socketProbe,
        }))
      : undefined;
  const authState =
    current === "enrollment"
      ? await admin.evaluate(() => ({
          hasFragment: !!location.hash,
          passwordFields: document.querySelectorAll('input[type="password"]')
            .length,
          notices: [
            "service is temporarily unavailable",
            "supported browser",
            "No eligible restricted session",
            "usable restricted session",
            "link is unavailable",
            "Loading",
            "Make yourself at home",
            "Page unavailable",
          ].map((text) => document.body.textContent.includes(text)),
        }))
      : undefined;
  const publicModal =
    current === "code-format-error-modal"
      ? await contexts[0].pages()[0].evaluate(() => ({
          count: document.querySelectorAll("dialog").length,
          open: !!document.querySelector("dialog[open]"),
          onHome: location.pathname === "/",
        }))
      : undefined;
  process.stdout.write(
    JSON.stringify({
      status: "failed",
      failed_step: current,
      failed_phase: phase,
      completed_checks: completed,
      browser_errors: errors.length,
      error_class: error?.name ?? "Error",
      failed_at: error?.stack?.match(
        /(?:browser|additional|endings|auth-queue)\.mjs:\d+:\d+/g,
      ),
      failed_assertion: error?.check,
      publicModal,
      httpStatuses,
      authState,
      gameState,
      gameReplies,
      participants,
      mutationProbe: admin
        ? await admin.evaluate(() => window.__mutationProbe)
        : undefined,
    }),
  );
  process.exitCode = 1;
} finally {
  for (const c of contexts) await c.close();
  if (browser) await browser.close();
}
