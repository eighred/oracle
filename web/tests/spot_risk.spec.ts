import { expect, test } from "@playwright/test";

const endpoint = "/api/portfolios/own-paper-01/spot-risk";

test("risk rejects an unauthorized principal and a mixed currency scenario before market reads", async ({
  request,
}) => {
  const data = {
    cash: "0",
    closes: 12,
    confidence_bps: 9500,
    positions: [
      { venue: "binance", pair: "ABC/USDC", quantity: "1", shock_bps: -250 },
      { venue: "okx", pair: "XYZ/USDT", quantity: "3", shock_bps: 500 },
    ],
  };
  const denied = Buffer.from(
    "observer:" + process.env.ORACLE_DENIED_PASSWORD,
  ).toString("base64");
  expect(
    (
      await request.post(endpoint, {
        data,
        headers: { Authorization: "Basic " + denied },
      })
    ).status(),
  ).toBe(403);
  const result = await request.post(endpoint, { data });
  expect(result.status()).toBe(422);
  expect((await result.json()).reason).toContain("shared quote currency");
});

test("operator selects arbitrary spot pairs and inspects joint risk", async ({
  page,
}) => {
  await page.route("**" + endpoint, async (route) => {
    const request = route.request().postDataJSON();
    expect(request.positions.map((p: { pair: string }) => p.pair)).toEqual([
      "ALPHA/USDC",
      "BETA/USDC",
    ]);
    await route.fulfill({
      json: {
        schema_version: 1,
        source: "public_spot_research",
        quote: "USDC",
        first_open_ms: 0,
        as_of_ms: 3600000,
        known_at_ms: 3600010,
        risk: {
          model: "joint_minute_spot_v1",
          nav: "20",
          stress_pnl: "-2",
          sample_size: 59,
          confidence_bps: 9500,
          value_at_risk: "1/3",
          expected_shortfall: "4/3",
        },
        positions: request.positions.map(
          (p: { venue: string; pair: string }) => ({
            ...p,
            exposure: "10",
            mark: "10",
            stress_pnl: "-1",
            tick: "0.01",
            quantity_step: "0.1",
            candles_sha256: "abc",
          }),
        ),
      },
    });
  });
  await page.goto("/#spot-risk");
  await page.getByLabel("Spot pair").fill("ALPHA/USDC");
  await page.getByRole("button", { name: "Add position" }).click();
  await page.getByLabel("Spot pair").nth(1).fill("BETA/USDC");
  await page.getByRole("button", { name: "Calculate risk" }).click();
  await expect(
    page.getByRole("heading", { name: "Portfolio risk · USDC" }),
  ).toBeVisible();
  await expect(page.getByTitle("4/3")).toHaveText("1.33333333 USDC");
  await page.getByText("okx · BETA/USDC").click();
  await expect(page.getByText("Candle receipt abc").last()).toBeVisible();
  await page.getByLabel("Quantity").first().fill("2");
  await expect(
    page.getByRole("heading", { name: "Portfolio risk · USDC" }),
  ).toHaveCount(0);
});
