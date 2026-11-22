import { expect, test } from "@playwright/test";

const resource = "/api/portfolios/own-paper-01/reports/latest";

test("follow a costed strategy to its settlements and immutable export", async ({
  page,
  request,
}) => {
  const response = await request.get(resource);
  expect(response.ok()).toBeTruthy();
  const report = await response.json();
  const run = report.backtest.runs.find(
    (item: { rule: string }) => item.rule === "momentum",
  );
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Portfolio review" }),
  ).toBeVisible();
  const metrics = page.getByLabel("Opening portfolio and risk");
  await expect(metrics).toContainText(
    report.risk.snapshot.nav + " " + report.risk.snapshot.quote,
  );
  await expect(metrics).toContainText(report.risk.expected_shortfall);
  await expect(page.locator(".asof")).toContainText(
    "Archived synthetic snapshot",
  );
  await expect(page.locator("table")).toHaveCount(0);
  const toggle = page.getByRole("button", { name: "Toggle navigation" });
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  const nav = page.getByRole("navigation", { name: "Workspace" });
  await nav.getByRole("link", { name: "Strategy comparison" }).click();
  await expect(
    nav.getByRole("link", { name: "Strategy comparison" }),
  ).toHaveAttribute("aria-current", "page");
  const strategy = page.locator(".strategy").filter({ hasText: "Momentum" });
  await expect(strategy).toContainText(run.net_pnl);
  await expect(strategy.locator("i")).toHaveCSS(
    "background-color",
    "rgb(255, 47, 156)",
  );
  await expect(page.locator(".drawdown")).toContainText("6.29%");
  await page.getByRole("link", { name: "View settlements" }).click();
  await page
    .getByRole("button", { name: "Inspect settlement 00:02:00" })
    .click();
  const drawer = page.getByRole("dialog", { name: "Posting detail" });
  await expect(drawer).toContainText(
    run.ledger[0].posting.fill.price + " USDT",
  );
  await expect(drawer).toContainText(
    run.ledger[0].posting.fill.fee +
      " / " +
      run.ledger[0].posting.fill.cash_delta,
  );
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", { name: "Inspect settlement 00:03:00" })
    .click();
  await expect(drawer).toContainText(
    run.ledger[1].posting.fill.price + " USDT",
  );
  await drawer.getByRole("button", { name: "Close details" }).click();
  await page.getByLabel("Strategy", { exact: true }).selectOption("hold");
  await page
    .getByRole("button", { name: "Inspect settlement 00:02:00" })
    .click();
  await expect(drawer).toContainText("Hold position");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Report provenance" }).click();
  const provenance = page.getByRole("dialog", { name: "Report provenance" });
  await expect(provenance).toContainText(report.backtest.ledger_sha256);
  const link = await provenance.locator("a[download]").getAttribute("href");
  expect(link).toContain(response.headers()["x-oracle-report-id"]);
  expect(await (await request.get(link!)).body()).toEqual(
    await response.body(),
  );
  await page.keyboard.press("Escape");
  await page.goBack();
  await expect(
    page.getByRole("heading", { name: "Strategy comparison" }),
  ).toBeVisible();
  await expect(
    page.locator(".strategy").filter({ hasText: "Buy & hold" }).locator("i"),
  ).toHaveCSS("background-color", "rgb(14, 133, 252)");
});

test("mobile navigation and settlement inspector support keyboard return", async ({
  page,
}) => {
  await page.setViewportSize({ width: 375, height: 800 });
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Momentum" })).toBeVisible();
  const icon = page
    .getByRole("button", { name: "Toggle navigation" })
    .locator("svg");
  await expect
    .poll(() =>
      icon.evaluate((el) => (el as SVGGraphicsElement).getBBox().width),
    )
    .toBeGreaterThan(0);
  await page.getByRole("button", { name: "Toggle navigation" }).click();
  const menu = page.getByRole("dialog", { name: "Workstation navigation" });
  await menu.getByRole("link", { name: "Settlement ledger" }).click();
  await expect(menu).toBeHidden();
  const control = page.getByRole("button", {
    name: "Inspect settlement 00:04:00",
  });
  await control.focus();
  await page.keyboard.press("Enter");
  await expect(
    page.getByRole("dialog", { name: "Posting detail" }),
  ).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(375);
  await page.keyboard.press("Escape");
  await expect(control).toBeFocused();
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
