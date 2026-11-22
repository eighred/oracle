import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";

test("wallet risk uses the signed-in ledger, the selected currency and the wallet entry point", async ({
  request,
  page,
}) => {
  const path = "/api/v1/me/wallet-risk";
  const currency = "AUD";
  const data = { currency, closes: 60, confidence_bps: 9500, shock_bps: -1000 };
  const headers = {
    Authorization:
      "Basic " +
      Buffer.from("observer:" + process.env.ORACLE_DENIED_PASSWORD).toString(
        "base64",
      ),
  };
  const deposit = await request.post("/api/baskets", {
    data: { op: "deposit", currency, amount: "125.25", request: randomUUID() },
  });
  expect(deposit.ok()).toBeTruthy();
  const result = await request.post(path, { data });
  expect(result.status()).toBe(200);
  expect(await result.json()).toMatchObject({
    schema_version: 1,
    state: "cash_only",
    currency,
    cash: "125.25",
    nav: "125.25",
    positions: [],
  });
  expect((await request.post(path, { headers, data })).status()).toBe(404);
  expect(
    (await request.post(path, { data: { ...data, positions: [] } })).status(),
  ).toBe(422);
  await page.goto("/#profile");
  await page
    .getByRole("combobox", { name: "Currency", exact: true })
    .selectOption(currency);
  await page.getByText("Wallet risk · AUD", { exact: true }).click();
  await page.getByRole("button", { name: "Calculate wallet risk" }).click();
  await expect(
    page.getByRole("region", { name: "Wallet risk result" }),
  ).toContainText("125.25 AUD");
  await expect(page.getByText(/entirely cash in AUD/)).toBeVisible();
});
