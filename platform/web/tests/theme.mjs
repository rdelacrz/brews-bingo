// Public-page visual regressions; no account data or mocked API responses.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";

const [origin, output] = process.argv.slice(2);
assert(origin && output, "Usage: node tests/theme.mjs https://preview-origin output-directory");
await mkdir(output, { recursive: true });
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH || "/usr/bin/chromium",
  headless: true,
  args: ["--no-sandbox"],
});
const errors = [];
const external = new Set();
const fontResponses = new Set();
const checks = [];
try {
  const context = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await context.newPage();
  page.on("pageerror", () => errors.push("page-error"));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push("console-error");
  });
  page.on("request", (request) => {
    if (new URL(request.url()).origin !== origin) external.add(new URL(request.url()).origin);
  });
  page.on("response", (response) => {
    if (response.url().endsWith(".woff2") && response.ok()) fontResponses.add(response.url());
  });
  for (const width of [320, 390, 768, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(origin, { waitUntil: "networkidle" });
    await page.getByRole("heading", { name: "Good company. Great games." }).waitFor();
    await page.evaluate(() => document.fonts.ready);
    const styles = await page.evaluate(() => {
      const css = (selector) => getComputedStyle(document.querySelector(selector));
      return {
        overflow: document.documentElement.scrollWidth > innerWidth,
        header: css(".app-header").backgroundColor,
        paper: css("html").backgroundColor,
        heading: css("h1").fontFamily,
        body: css("body").fontFamily,
        button: css("button[type=submit]").backgroundColor,
        buttonText: css("button[type=submit]").color,
        displayLoaded: document.fonts.check('500 48px "Cormorant Garamond"'),
        bodyLoaded: document.fonts.check('400 16px "DM Sans"'),
      };
    });
    assert.equal(styles.overflow, false, `page overflow at ${width}`);
    assert.equal(styles.header, "rgb(33, 12, 3)");
    assert.equal(styles.paper, "rgb(246, 244, 239)");
    assert.equal(styles.button, "rgb(237, 173, 56)");
    assert.equal(styles.buttonText, "rgb(33, 12, 3)");
    assert(styles.heading.startsWith('"Cormorant Garamond"'));
    assert(styles.body.startsWith('"DM Sans"'));
    assert(styles.displayLoaded && styles.bodyLoaded);
    const header = page.getByRole("banner");
    const brand = header.getByRole("link", { name: "Brews Bingo", exact: true });
    const login = header.getByRole("link", { name: "Login", exact: true });
    assert.equal(await brand.getAttribute("href"), "/");
    assert.equal(await login.getAttribute("href"), "/login");
    assert.equal(await page.getByRole("link", { name: "Home", exact: true }).count(), 0);
    assert.equal(await page.locator('a[href="/login"]').count(), 1, "only the header login remains");
    assert.equal(await header.getByRole("navigation").getByRole("link").count(), 1);
    const brandBox = await brand.boundingBox();
    const loginBox = await login.boundingBox();
    assert(brandBox && loginBox);
    assert(loginBox.x >= brandBox.x + brandBox.width, `login stays to the right at ${width}`);
    assert(Math.abs(loginBox.y + loginBox.height / 2 - brandBox.y - brandBox.height / 2) < 2,
      `logo and login share the top row at ${width}`);
    assert.equal(await page.locator("input.code-box").count(), 8);
    await page.getByRole("button", { name: "Join a game", exact: true }).click();
    await page.getByRole("dialog").waitFor();
    assert(await page.evaluate(() => !!document.activeElement?.closest("dialog")));
    await page.getByRole("button", { name: "Go back", exact: true }).click();
    if (width === 390 || width === 1440) {
      await page.screenshot({ path: join(output, `home-${width}.png`), fullPage: true });
    }
    await login.click();
    await page.waitForURL(origin + "/login");
    assert.equal(await page.locator("p.eyebrow").textContent(), "Login");
    const passwordInput = page.getByLabel("Password", { exact: true });
    const togglePassword = page.getByRole("button", { name: "Show password", exact: true });
    await passwordInput.waitFor();
    await passwordInput.fill("temporary-fixture");
    const passwordBox = await passwordInput.boundingBox();
    const toggleBox = await togglePassword.boundingBox();
    assert(passwordBox && toggleBox);
    assert(toggleBox.x > passwordBox.x + passwordBox.width * 0.7);
    assert(toggleBox.x + toggleBox.width <= passwordBox.x + passwordBox.width);
    assert.equal(await passwordInput.getAttribute("type"), "password");
    await togglePassword.click();
    assert.equal(await passwordInput.getAttribute("type"), "text");
    assert.equal(await page.getByRole("button", { name: "Hide password", exact: true }).count(), 1);
    await page.getByRole("button", { name: "Hide password", exact: true }).click();
    assert.equal(await passwordInput.getAttribute("type"), "password");
    assert.equal(await passwordInput.inputValue(), "temporary-fixture");
    assert.equal(await page.getByText(/Interpret ASCII escapes|Optional for control characters/).count(), 0);
    assert.equal(await page.getByRole("link", { name: "Back to Home", exact: true }).count(), 0);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.keyboard.press("Tab");
    assert(await page.evaluate(() => {
      const style = getComputedStyle(document.activeElement);
      return style.outlineStyle !== "none" && parseFloat(style.outlineWidth) >= 3;
    }), "keyboard focus stays visible");
    await brand.click();
    await page.waitForURL(origin + "/");
    await page.getByRole("heading", { name: "Good company. Great games." }).waitFor();
    checks.push(`brand-home-dialog-login-${width}`);
  }
  await page.emulateMedia({ reducedMotion: "reduce", colorScheme: "dark" });
  await page.goto(origin, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Join a game", exact: true }).waitFor();
  assert.equal(await page.locator("button[type=submit]").evaluate(e => getComputedStyle(e).transitionDuration), "0s");
  assert.equal(await page.locator("html").evaluate(e => getComputedStyle(e).colorScheme), "light");
  checks.push("reduced-motion-and-explicit-color-scheme");
  assert.equal(fontResponses.size, 2, "both self-hosted fonts actually loaded");
  assert.equal(external.size, 0, "no external font or other runtime requests");
  assert.equal(errors.length, 0, "no browser script/console errors");
  checks.push("local-fonts-no-external-requests-no-browser-errors");
  const report = { status: "passed", checks, check_count: checks.length, browser_errors: errors.length };
  await writeFile(join(output, "report.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(JSON.stringify(report));
} finally {
  await browser.close();
}
