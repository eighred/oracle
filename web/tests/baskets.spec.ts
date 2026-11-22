import { test, expect } from "@playwright/test";
import { randomUUID, randomBytes } from "node:crypto";

test("private sharing, independent clones, unique popularity and official pins survive separate user sessions", async ({
  request,
}) => {
  test.setTimeout(100000);
  const observer = {
    Authorization:
      "Basic " +
      Buffer.from("observer:" + process.env.ORACLE_DENIED_PASSWORD).toString(
        "base64",
      ),
  };
  const id = randomUUID();
  const call = (
    op: string,
    fields: Record<string, unknown> = {},
    asGuest = false,
  ) =>
    request.post("/api/baskets", {
      data: { op, id, ...fields },
      headers: asGuest ? observer : {},
    });
  const catalog = async (guest = false) =>
    (
      await (
        await request.get("/api/baskets", { headers: guest ? observer : {} })
      ).json()
    ).baskets as {
      id: string;
      users: number;
      name: string;
      pinned: boolean;
      mine: boolean;
      public: boolean;
      sourceRevision: number;
      items: unknown[];
    }[];
  const visible = async () => (await catalog(true)).some((b) => b.id === id);
  const items = [{ venue: "okx", pair: "SOL/USDT", bps: 10000 }];
  expect(
    (
      await call("create", {
        name: "Pilot mix",
        items: [{ ...items[0], bps: 99 }],
      })
    ).status(),
  ).toBe(422);
  expect(
    (await call("create", { name: "Pilot mix", items })).ok(),
  ).toBeTruthy();
  expect(await visible()).toBe(false);
  expect((await call("join", {}, true)).status()).toBe(404);
  expect((await call("clone", {}, true)).status()).toBe(404);
  expect((await call("invite", { member: "observer" })).ok()).toBeTruthy();
  expect(await visible()).toBe(true);
  await call("uninvite", { member: "observer" });
  expect(await visible()).toBe(false);
  const password = randomBytes(18).toString("hex");
  const { token } = await (await call("link", { password })).json();
  expect(
    (await call("unlock", { token, password: password + "x" }, true)).status(),
  ).toBe(404);
  expect(
    (await call("unlock", { id: "", token, password }, true)).ok(),
  ).toBeTruthy();
  expect(await visible()).toBe(true);
  expect((await call("join", {}, true)).ok()).toBeTruthy();
  expect((await call("join", {}, true)).ok()).toBeTruthy();
  const clone = await (await call("clone", {}, true)).json();
  expect((await (await call("clone", {}, true)).json()).id).toBe(clone.id);
  expect((await catalog()).find((b) => b.id === id)?.users).toBe(1);
  await call("unlink");
  expect(await visible()).toBe(false);
  expect(
    (await call("unlock", { id: "", token, password }, true)).status(),
  ).toBe(404);
  expect(
    (
      await call("edit", {
        name: "Revised mix",
        revision: 1,
        items: [{ ...items[0], pair: "ETH/USDT" }],
      })
    ).ok(),
  ).toBeTruthy();
  expect(
    (await call("edit", { name: "Stale", revision: 1, items })).status(),
  ).toBe(409);
  await call("publish", { enabled: true });
  expect((await call("pin", { enabled: true })).ok()).toBeTruthy();
  const all = await catalog(true);
  const pinned = all.find((b) => b.id === id)!;
  expect(pinned).toMatchObject({ users: 1, pinned: true, public: true });
  expect(all.find((b) => b.id === clone.id)).toMatchObject({
    name: "Pilot mix",
    mine: true,
    public: false,
    sourceRevision: 1,
    items,
  });
  await call("publish", { id: clone.id, enabled: true }, true);
  expect(
    (await call("pin", { id: clone.id, enabled: true }, true)).status(),
  ).toBe(403);
  await call("publish", { enabled: false });
  expect(await visible()).toBe(false);
});

test("direct discovery rankings lead to a saved template and server-sized risk", async ({
  page,
}) => {
  const ticker = (name: string, price: string, volume: string) => ({
    instType: "SPOT",
    instId: name + "-USDT",
    last: price,
    open24h: "10",
    volCcy24h: volume,
    ts: String(Date.now()),
  });
  await page.route(
    "https://www.okx.com/api/v5/market/tickers?instType=SPOT",
    (route) =>
      route.fulfill({
        json: {
          code: "0",
          data: [ticker("ALPHA", "12", "200"), ticker("BETA", "9", "300")],
        },
      }),
  );
  await page.route("**/api/discover/news", (route) =>
    route.fulfill({
      json: {
        articles: [
          {
            title: "Market listing update",
            source: "OKX",
            publishedMs: Date.now(),
            url: "https://www.okx.com/help/listing",
          },
        ],
      },
    }),
  );
  await page.route(
    "https://data-api.binance.vision/api/v3/exchangeInfo",
    (route) =>
      route.fulfill({
        json: {
          symbols: [
            {
              symbol: "ALPHAUSDT",
              baseAsset: "ALPHA",
              quoteAsset: "USDT",
              status: "TRADING",
              isSpotTradingAllowed: true,
            },
          ],
        },
      }),
  );
  await page.route(
    "https://data-api.binance.vision/api/v3/ticker/24hr",
    (route) =>
      route.fulfill({
        json: [
          {
            symbol: "ALPHAUSDT",
            openPrice: "10",
            lastPrice: "13",
            quoteVolume: "200",
            closeTime: Date.now(),
          },
        ],
      }),
  );
  await page.goto("/#discover");
  await expect(page.locator("tbody tr").first()).toContainText("ALPHA/USDT");
  await expect(page.locator("tbody tr")).toHaveCount(2);
  await expect(
    page.locator("tbody tr").first().locator("td").nth(1),
  ).toHaveText("13");
  await page.getByRole("button", { name: "News", exact: true }).click();
  await expect(
    page.getByRole("link", { name: "Market listing update" }),
  ).toHaveAttribute("href", "https://www.okx.com/help/listing");
  await page.getByRole("button", { name: "Markets", exact: true }).click();
  await page.getByRole("button", { name: "Gainers", exact: true }).click();
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("button", { name: "Losers", exact: true }).click();
  await page
    .getByRole("button", { name: "Create basket with BETA/USDT" })
    .click();
  await page.getByLabel("Basket name").fill("Discovered basket");
  await page.getByRole("button", { name: "Save basket" }).click();
  await page.getByRole("button", { name: "Review basket risk" }).click();
  await expect(page.getByLabel("Quote allocation")).toHaveValue("100.00");
  await expect(page.getByLabel("Spot pair")).toHaveValue("BETA/USDT");
});

test("only an administrator stages connections and appends future fee proposals", async ({
  request,
}) => {
  const url = "/api/platform-settings";
  const headers = {
    Authorization:
      "Basic " +
      Buffer.from("observer:" + process.env.ORACLE_DENIED_PASSWORD).toString(
        "base64",
      ),
  };
  expect(
    (await request.post(url, { headers, data: { op: "list" } })).status(),
  ).toBe(403);
  const data = {
    op: "connection",
    request: randomUUID(),
    venue: "okx",
    enabled: true,
  };
  const send = async (d: object) => request.post(url, { data: d });
  const first = await (await send(data)).json();
  const retry = await (await send(data)).json();
  expect(retry.connections).toEqual(first.connections);
  expect(first.connections[0]).toMatchObject({
    venue: "okx",
    enabled: true,
    secret_ref: "vault:oracle/company/okx",
  });
  expect(
    (
      await (
        await send({ ...data, request: randomUUID(), enabled: false })
      ).json()
    ).connections[0].enabled,
  ).toBe(false);
  const fee = {
    op: "fee",
    request: randomUUID(),
    bps: 1200,
    effective: new Date(Date.now() + 86400000).toISOString(),
  };
  const proposal = await (await send(fee)).json();
  expect(proposal.fees[0].bps).toBe(1200);
  expect(
    (
      await send({
        ...fee,
        request: randomUUID(),
        effective: "2000-01-01T00:00:00Z",
      })
    ).status(),
  ).toBe(422);
});

test("following tracks owner revisions and unfollowing removes membership without changing a private copy", async ({
  browser,
  request,
}) => {
  test.setTimeout(90000);
  const id = randomUUID(),
    name = "Follow " + id.slice(0, 8);
  const items = [{ venue: "okx", pair: "SOL/USDT", bps: 10000 }];
  const act = (op: string, extra: Record<string, unknown> = {}) =>
    request.post("/api/baskets", { data: { id, op, ...extra } });
  expect((await act("create", { name, items })).ok()).toBeTruthy();
  expect((await act("publish", { enabled: true })).ok()).toBeTruthy();
  const context = await browser.newContext({
    baseURL: process.env.ORACLE_TEST_ORIGIN,
    httpCredentials: {
      username: "observer",
      password: process.env.ORACLE_DENIED_PASSWORD!,
    },
  });
  try {
    const page = await context.newPage();
    await page.goto("/#baskets");
    await page.getByRole("button", { name: "Popular", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(name) }).click();
    await page
      .getByRole("button", { name: "Follow basket", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Unfollow basket" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Copy to my library" }).click();
    await expect(page.getByText(/Copied from revision 1/)).toBeVisible();
    await page.reload();
    await page.getByRole("button", { name: "Following", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(name) }).click();
    await expect(page.getByRole("cell", { name: "SOL/USDT" })).toBeVisible();
    await act("edit", {
      name,
      revision: 1,
      items: [{ ...items[0], pair: "ETH/USDT" }],
    });
    await expect(page.getByRole("cell", { name: "ETH/USDT" })).toBeVisible({
      timeout: 25000,
    });
    await page.getByRole("button", { name: "Unfollow basket" }).click();
    await expect(page.getByText("No baskets here yet.")).toBeVisible();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(name) }).click();
    await expect(page.getByRole("cell", { name: "SOL/USDT" })).toBeVisible();
  } finally {
    await context.close();
  }
});
test("paper cash buys weighted lots, rebalances offline and redeems exactly once", async ({
  browser,
  request,
}) => {
  test.setTimeout(120000);
  const id = randomUUID();
  const name = "Weighted " + id.slice(0, 8);
  const items = [
    { venue: "okx", pair: "SOL/USDT", bps: 6000 },
    { venue: "binance", pair: "ETH/USDT", bps: 4000 },
  ];
  const owner = (op: string, extra: Record<string, unknown> = {}) =>
    request.post("/api/baskets", { data: { op, id, ...extra } });
  const guest = await browser.newContext({
    baseURL: process.env.ORACLE_TEST_ORIGIN,
    httpCredentials: {
      username: "observer",
      password: process.env.ORACLE_DENIED_PASSWORD!,
    },
  });
  const statement = async () =>
    (await (await guest.request.get("/api/baskets")).json()).paper;
  const account = async () =>
    (await statement()).accounts.find(
      (a: { basket: string }) => a.basket === id,
    );
  const guestAction = (op: string, extra: Record<string, unknown>) =>
    guest.request.post("/api/baskets", { data: { op, id, ...extra } });
  try {
    expect((await owner("create", { name, items })).ok()).toBeTruthy();
    expect((await owner("publish", { enabled: true })).ok()).toBeTruthy();
    expect(
      (
        await guestAction("fund", {
          amount: "10001",
          request: randomUUID(),
          revision: 1,
        })
      ).status(),
    ).toBe(422);
    const page = await guest.newPage();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/#profile");
    await page.getByRole("button", { name: "Add paper funds" }).click();
    await page.getByLabel("Deposit currency").fill("USDT");
    await page.getByLabel("Deposit amount").fill("10000");
    await page.getByRole("button", { name: "Confirm paper deposit" }).click();
    await expect(page.locator(".total")).toHaveText("10,000.00 USDT");
    const otherDeposit = {
      request: randomUUID(),
      currency: "TRY",
      amount: "2500.25",
    };
    expect((await guestAction("deposit", otherDeposit)).ok()).toBeTruthy();
    expect((await guestAction("deposit", otherDeposit)).ok()).toBeTruthy();
    expect(
      (
        await guestAction("deposit", { ...otherDeposit, amount: "2500" })
      ).status(),
    ).toBe(409);
    expect((await statement()).wallets.TRY).toBe("2500.25");
    await page.goto("/#baskets");
    await page.getByRole("button", { name: "Popular", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(name) }).click();
    await page.getByLabel("Paper amount (USDT)").fill("1000");
    await page.getByRole("button", { name: "Join with paper funds" }).click();
    await expect(
      page.getByRole("heading", { name: "Paper positions" }),
    ).toBeVisible();
    await expect(page.getByText(/Committed 1000 USDT/)).toBeVisible();
    await page.close();
    await expect
      .poll(async () => (await account())?.revision, { timeout: 15000 })
      .toBe(1);
    const first = await account();
    const profilePage = await guest.newPage();
    await profilePage.goto("/#profile");
    await profilePage
      .getByLabel("Currency", { exact: true })
      .selectOption("USDT");
    await expect(profilePage.locator(".total")).toHaveText("9,998.46 USDT");
    await expect(
      profilePage.getByRole("region", { name: "Coin holdings" }),
    ).toContainText("5.99 SOL");
    await profilePage
      .getByRole("button", { name: "Baskets", exact: true })
      .click();
    await expect(
      profilePage.getByRole("region", { name: "Basket returns" }),
    ).toContainText("-0.1537%");
    await profilePage
      .getByRole("button", { name: "History", exact: true })
      .click();
    const history = profilePage.getByRole("region", {
      name: "Transaction history",
    });
    await expect(history).toContainText("Basket entry");
    await history.locator("summary").first().click();
    await expect(history).toContainText("buy 5.99 SOL/USDT");
    await profilePage.close();
    expect(first.cash).toBe("0.4625611");
    expect(first.receipt).toMatchObject({
      fees: "0.9985389",
      nav: "998.4625611",
    });
    expect(first.holdings).toEqual([
      { venue: "binance", pair: "ETH/USDT", quantity: "7.98" },
      { venue: "okx", pair: "SOL/USDT", quantity: "5.99" },
    ]);
    expect(
      (
        await guestAction("fund", {
          amount: "1000",
          request: first.id,
          revision: 1,
        })
      ).ok(),
    ).toBeTruthy();
    expect(
      (
        await guestAction("fund", {
          amount: "1001",
          request: first.id,
          revision: 1,
        })
      ).status(),
    ).toBe(409);
    expect((await statement()).wallets.USDT).toBe("9000");
    expect((await owner("redeem", { request: first.id })).status()).toBe(404);
    const catalog = await (await request.get("/api/baskets")).json();
    expect(catalog.paper.accounts).toHaveLength(0);
    expect(catalog.baskets.find((b: { id: string }) => b.id === id).users).toBe(
      1,
    );
    expect(
      (
        await owner("edit", {
          revision: 1,
          name,
          items: [{ ...items[1], pair: "ETH/BTC", bps: 10000 }],
        })
      ).status(),
    ).toBe(409);
    expect(
      (
        await owner("edit", {
          revision: 1,
          name,
          items: [{ ...items[1], bps: 10000 }],
        })
      ).ok(),
    ).toBeTruthy();
    await expect
      .poll(async () => (await account()).revision, { timeout: 15000 })
      .toBe(2);
    const revised = await account();
    expect(revised.holdings).toEqual([
      { venue: "binance", pair: "ETH/USDT", quantity: "19.93" },
    ]);
    expect(revised.cash).toBe("0.1080021");
    await owner("publish", { enabled: false });
    await owner("edit", { revision: 2, name: "Hidden name", items });
    expect(await account()).toMatchObject({
      revision: 2,
      name,
      status: "Access paused",
    });
    const redemption = await guest.newPage();
    await redemption.goto("/#baskets");
    await redemption
      .getByRole("button", { name: "Joined", exact: true })
      .click();
    await redemption
      .getByRole("button", { name: "Redeem to paper wallet" })
      .click();
    expect(
      (await guestAction("redeem", { request: first.id })).ok(),
    ).toBeTruthy();
    await expect
      .poll(async () => (await account()).status, { timeout: 15000 })
      .toBe("Closed");
    expect((await account()).holdings).toEqual([]);
    expect((await statement()).wallets.USDT).toBe("9995.0142");
    expect(
      (
        await guestAction("fund", {
          amount: "1000",
          request: first.id,
          revision: 1,
        })
      ).ok(),
    ).toBeTruthy();
    expect(
      (await statement()).accounts.filter(
        (a: { basket: string }) => a.basket === id,
      ),
    ).toHaveLength(1);
  } finally {
    await guest.close();
  }
});
test("app entry supports mobile market search, public basket discovery and wallet navigation", async ({
  page,
  request,
}) => {
  const id = randomUUID(),
    hidden = randomUUID();
  for (const [basket, name] of [
    [id, "Community " + id],
    [hidden, "Private " + hidden],
  ]) {
    expect(
      (
        await request.post("/api/baskets", {
          data: {
            op: "create",
            id: basket,
            name,
            items: [{ venue: "okx", pair: "ETH/USDT", bps: 10000 }],
          },
        })
      ).ok(),
    ).toBeTruthy();
  }
  expect(
    (
      await request.post("/api/baskets", {
        data: { op: "publish", id, enabled: true },
      })
    ).ok(),
  ).toBeTruthy();
  expect(
    (
      await request.post("/api/baskets", {
        data: { op: "pin", id, enabled: true },
      })
    ).ok(),
  ).toBeTruthy();
  const feed = await (await request.get("/api/discover/baskets")).json();
  expect(feed.baskets.map((b: { id: string }) => b.id)).toContain(id);
  expect(feed.baskets.map((b: { id: string }) => b.id)).not.toContain(hidden);
  await page.route("https://data-api.binance.vision/**", (route) =>
    route.abort(),
  );
  await page.route(
    "https://www.okx.com/api/v5/market/tickers?instType=SPOT",
    (r) =>
      r.fulfill({
        json: {
          code: "0",
          data: Array.from({ length: 30 }, (_, i) => ({
            instId: "COIN" + i + "-USDT",
            instType: "SPOT",
            last: "12",
            open24h: "10",
            volCcy24h: String(100 - i),
            ts: String(Date.now()),
          })),
        },
      }),
  );
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Discover markets" }),
  ).toBeVisible();
  await page.getByRole("searchbox").fill("COIN29");
  await expect(page.locator("tbody tr")).toHaveCount(1);
  await expect(page.locator("tbody tr")).toContainText("COIN29/USDT");
  await page.getByRole("button", { name: "Community", exact: true }).click();
  await page.getByRole("link", { name: new RegExp("Community " + id) }).click();
  await expect(
    page.getByRole("heading", { name: "Community " + id }),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: "App navigation" })
    .getByRole("link", { name: "Wallet" })
    .click();
  await expect(
    page.getByRole("heading", { name: "Balance overview" }),
  ).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(375);
  const manifest = await (await request.get("/manifest.webmanifest")).json();
  expect(manifest.start_url).toBe("/#discover");
  for (const icon of manifest.icons)
    expect((await request.get(icon.src)).ok()).toBeTruthy();
});
