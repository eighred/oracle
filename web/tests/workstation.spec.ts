import { expect, test } from "@playwright/test";

const resource = "/api/portfolios/own-paper-01/reports/latest";

test("inspect the stored loss through the settlement and immutable export", async ({
  page,
  request,
}) => {
  const response = await request.get(resource);
  expect(response.ok()).toBeTruthy();
  const report = await response.json();
  const run = report.backtest.runs.find(
    (item: { rule: string }) => item.rule === "momentum",
  );
  const first = run.ledger[0];
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Portfolio review" }),
  ).toBeVisible();
  await expect(page.getByLabel("Opening portfolio and risk")).toContainText(
    report.risk.snapshot.nav + " " + report.risk.snapshot.quote,
  );
  await expect(page.getByLabel("Opening portfolio and risk")).toContainText(
    report.risk.expected_shortfall,
  );
  await expect(page.getByRole("button", { name: /Momentum/ })).toContainText(
    run.net_pnl,
  );
  await expect(page.getByRole("button", { name: /Momentum/ })).toHaveCSS(
    "background-color",
    "rgb(255, 47, 156)",
  );
  await expect(page.locator(".drawdown")).toContainText("6.29%");
  await expect(page.locator(".asof")).toContainText(
    "Archived synthetic snapshot",
  );
  await expect(page.locator(".detail")).toContainText(
    first.posting.fill.price + " USDT",
  );
  await expect(page.locator(".detail")).toContainText(
    first.posting.fill.fee + " / " + first.posting.fill.cash_delta,
  );
  await page
    .getByRole("button", { name: "Inspect settlement 00:03:00" })
    .click();
  await expect(page.locator(".detail")).toContainText(
    run.ledger[1].posting.fill.price + " USDT",
  );
  await page.getByRole("button", { name: /Buy & hold/ }).click();
  await expect(page.getByRole("button", { name: /Buy & hold/ })).toHaveCSS(
    "background-color",
    "rgb(14, 133, 252)",
  );
  await expect(page.locator(".detail")).toContainText("Hold position");
  await page.getByText("Report provenance", { exact: true }).click();
  await expect(page.locator(".provenance")).toContainText(
    report.backtest.ledger_sha256,
  );
  const link = await page.locator("a[download]").getAttribute("href");
  expect(link).toContain(response.headers()["x-oracle-report-id"]);
  expect(await (await request.get(link!)).body()).toEqual(
    await response.body(),
  );
});

test("narrow view fits and the ledger remains keyboard accessible", async ({
  page,
}) => {
  await page.setViewportSize({ width: 375, height: 800 });
  await page.goto("/");
  await expect(page.getByRole("button", { name: /Momentum/ })).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(375);
  const control = page.getByRole("button", {
    name: "Inspect settlement 00:04:00",
  });
  await control.focus();
  await page.keyboard.press("Enter");
  await expect(control).toHaveAttribute("aria-pressed", "true");
});

test("denied operator receives no financial data", async ({
  browser,
  baseURL,
}) => {
  const context = await browser.newContext({
    baseURL,
    httpCredentials: {
      username: "observer",
      password: process.env.ORACLE_DENIED_PASSWORD!,
    },
  });
  const page = await context.newPage();
  await page.goto("/");
  await expect(page.getByRole("status")).toHaveText(
    "Access to this portfolio is denied.",
  );
  await expect(page.locator("table")).toHaveCount(0);
  await context.close();
});

test("pending, empty and unavailable projections have explicit states", async ({
  page,
}) => {
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  await page.route("**" + resource, async (route) => {
    await pending;
    await route.fulfill({ status: 404 });
  });
  await page.goto("/");
  await expect(page.getByRole("status")).toHaveText("Loading portfolio…");
  finish();
  await expect(page.getByRole("status")).toHaveText(
    "No published report for this portfolio.",
  );
  await page.unrouteAll({ behavior: "wait" });
  await page.route("**" + resource, (route) => route.fulfill({ status: 503 }));
  await page.getByRole("button", { name: "Refresh report" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Report service unavailable.",
  );
  await expect(page.locator("table")).toHaveCount(0);
});
