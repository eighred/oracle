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
  await page.goto("/#discover");
  await expect(page.locator("tbody tr").first()).toContainText("BETA/USDT");
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

test("joining follows owner revisions and leaving removes membership without changing a private copy", async ({
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
      .getByRole("button", { name: "Join basket", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Leave basket" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Copy to my library" }).click();
    await expect(page.getByText(/Copied from revision 1/)).toBeVisible();
    await page.reload();
    await page.getByRole("button", { name: "Joined", exact: true }).click();
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
    await page.getByRole("button", { name: "Leave basket" }).click();
    await expect(page.getByText("No baskets here yet.")).toBeVisible();
    await page.getByRole("button", { name: "Library", exact: true }).click();
    await page.getByRole("button", { name: new RegExp(name) }).click();
    await expect(page.getByRole("cell", { name: "SOL/USDT" })).toBeVisible();
  } finally {
    await context.close();
  }
});
