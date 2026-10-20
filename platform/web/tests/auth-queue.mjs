export async function queuedAuthentication({
  origin,
  context,
  step,
  click,
  api,
  assert,
  wait,
  username,
  password,
}) {
  const browserContext = await context();
  const holder = await browserContext.newPage();
  const page = await browserContext.newPage();
  await holder.goto(origin + "/login");
  await page.goto(origin + "/login");
  await page.getByLabel("Password", { exact: true }).waitFor();
  await wait(500);
  let requests = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/api/auth/login") requests++;
  });
  const lock = async () => {
    await holder.evaluate(() => {
      window.cookieLockHeld = false;
      navigator.locks.request("brews-account-cookie", async () => {
        window.cookieLockHeld = true;
        await new Promise((resolve) => {
          window.releaseCookieLock = resolve;
        });
      });
    });
    await holder.waitForFunction(() => window.cookieLockHeld);
  };
  const submit = async () => {
    await page.getByLabel("Username", { exact: true }).fill(username);
    await page.getByLabel("Password", { exact: true }).fill(password);
    await click(page, "Log in");
    await page.waitForFunction(() =>
      document.querySelector('input[type="password"]')?.matches(":disabled"),
    );
  };
  await step(
    "queued-login-cancelled-on-navigation-without-late-cookie-write",
    async () => {
      await lock();
      await submit();
      await page
        .getByRole("banner")
        .getByRole("link", { name: "Brews Bingo", exact: true })
        .click();
      await page.waitForURL(origin + "/");
      await holder.evaluate(() => window.releaseCookieLock());
      await wait(800);
      assert(requests === 0, "cancelled-login-not-dispatched");
      const session = await api(page, "/api/session");
      assert(
        session.status === 200 && session.body.authenticated === false,
        "cancelled-login-anonymous",
      );
    },
  );
  await step("cross-tab-cookie-lock-wait-is-bounded", async () => {
    await page.goto(origin + "/login");
    await page.getByLabel("Password", { exact: true }).waitFor();
    await wait(500);
    await lock();
    await submit();
    await page.waitForFunction(
      () =>
        document
          .querySelector('input[type="password"]')
          ?.matches(":disabled") === false,
      undefined,
      { timeout: 35000 },
    );
    assert(requests === 0, "expired-queued-login-not-dispatched");
    await holder.evaluate(() => window.releaseCookieLock());
    await wait(800);
    assert(requests === 0, "expired-lock-callback-cannot-dispatch-later");
  });
}
