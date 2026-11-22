import { expect, test } from "@playwright/test";

test("watchlists retain market order, isolate viewers and reconcile concurrent edits", async ({
  request,
  page,
}) => {
  const path = "/api/v1/me/watchlist";
  const observer = {
    Authorization:
      "Basic " +
      Buffer.from("observer:" + process.env.ORACLE_DENIED_PASSWORD).toString(
        "base64",
      ),
  };
  const initial = { pairs: [], revision: 0 };
  expect(await (await request.get(path)).json()).toEqual(initial);
  const replace = (revision: number, pairs: string[]) =>
    request.put(path, { data: { revision, pairs } });
  const updates = await Promise.all([
    replace(0, ["ETH/USDT"]),
    replace(0, ["BTC/USDT"]),
  ]);
  expect(updates.map((r) => r.status()).sort()).toEqual([200, 409]);
  const latest = await updates.find((r) => r.ok())!.json();
  expect(await updates.find((r) => r.status() === 409)!.json()).toEqual(latest);
  expect(await (await replace(0, latest.pairs)).json()).toEqual(latest);
  expect((await replace(1, ["ETH/USDT", "ETH/USDT"])).status()).toBe(422);
  expect((await replace(1, ["BTC"])).status()).toBe(422);
  expect((await replace(1, ["BTC/USDT", "BTC/USD"])).status()).toBe(200);
  expect(await (await request.get(path, { headers: observer })).json()).toEqual(
    initial,
  );

  const pair = "ETH/USDT";
  await page.route("**/api/discover/markets/*", (route) =>
    route.fulfill({
      json: [
        {
          pair,
          quote: "USDT",
          price: "12",
          open: "10",
          volume: "400",
          ts: Date.now(),
        },
      ],
    }),
  );
  await page.setViewportSize({ width: 360, height: 800 });
  await page.goto("/#discover");
  await page
    .getByRole("button", { name: "Watch " + pair, exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Watch " + pair }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Watchlist", exact: true }).click();
  const list = page.getByRole("region", { name: "My watchlist" });
  await expect(list.getByRole("listitem")).toHaveCount(3);
  await expect(list.getByText("Price unavailable")).toHaveCount(2);
  const moveUp = list.getByRole("button", { name: "Move ETH/USDT up" });
  await expect(moveUp).toBeEnabled();
  await moveUp.press("Enter");
  await expect(list.getByRole("listitem").nth(1)).toContainText(pair);
  const edited = await (await request.get(path)).json();
  expect(edited.pairs).toEqual(["BTC/USDT", pair, "BTC/USD"]);
  expect((await replace(edited.revision, ["BTC/USD", pair])).status()).toBe(
    200,
  );
  await list
    .getByRole("button", { name: "Remove ETH/USDT from watchlist" })
    .click();
  await expect(page.getByText(/changed on another device/)).toBeVisible();
  await expect(list.getByRole("listitem")).toHaveCount(2);
  await list
    .getByRole("button", { name: "Remove ETH/USDT from watchlist" })
    .click();
  await expect(list.getByRole("listitem")).toHaveCount(1);
  await page.reload();
  await expect(page).toHaveURL(/#discover\?tab=watchlist$/);
  await expect(list.getByRole("listitem")).toContainText("BTC/USD");
  expect(await (await request.get(path, { headers: observer })).json()).toEqual(
    initial,
  );
});
