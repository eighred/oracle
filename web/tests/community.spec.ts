import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";

test("member profiles, reviewed posts, consent and follows use the scoped server account", async ({
  request,
  page,
}) => {
  const guest = {
    Authorization:
      "Basic " +
      Buffer.from("observer:" + process.env.ORACLE_DENIED_PASSWORD).toString(
        "base64",
      ),
  };
  const write = (data: Record<string, unknown>, other = false) =>
    request.post("/api/community", { data, headers: other ? guest : {} });
  const read = async (query: string, other = false) =>
    (
      await request.get("/api/community?" + query, {
        headers: other ? guest : {},
      })
    ).json();
  const profile = {
    op: "profile",
    public: true,
    ranking: true,
    bio: "Research notes",
    avatar: "",
  };
  expect((await write(profile, true)).ok()).toBeTruthy();
  const body = "Paper observation " + randomUUID();
  const post = { op: "post", request: randomUUID(), pair: "ETH/USDT", body };
  expect((await write(post, true)).ok()).toBeTruthy();
  expect((await write(post, true)).ok()).toBeTruthy();
  expect(
    (await read("view=global")).posts.map((p: { body: string }) => p.body),
  ).not.toContain(body);
  const pending = (await read("view=review")).posts.filter(
    (p: { body: string }) => p.body === body,
  );
  expect(pending).toHaveLength(1);
  const moderate = { op: "moderate", id: pending[0].id, enabled: true };
  expect((await write(moderate, true)).status()).toBe(403);
  expect((await write(moderate)).ok()).toBeTruthy();
  expect(
    (await write({ op: "follow", handle: "observer", enabled: true })).ok(),
  ).toBeTruthy();
  expect(
    (await read("view=following")).posts.map((p: { body: string }) => p.body),
  ).toContain(body);
  expect(
    (await write({ op: "mute", handle: "observer", enabled: true })).ok(),
  ).toBeTruthy();
  expect(
    (await read("view=following")).posts.map((p: { body: string }) => p.body),
  ).not.toContain(body);
  expect(
    (await write({ op: "mute", handle: "observer", enabled: false })).ok(),
  ).toBeTruthy();
  expect((await write({ ...profile, public: false }, true)).ok()).toBeTruthy();
  expect(
    (await request.get("/api/community?view=profile&handle=observer")).status(),
  ).toBe(404);
  expect(
    (await read("view=global")).posts.map((p: { body: string }) => p.body),
  ).not.toContain(body);
  expect((await read("view=rankings&currency=USD")).rankings).toBeInstanceOf(
    Array,
  );
  await page.goto("/#profile");
  await page.getByRole("button", { name: "Funding methods" }).click();
  await page.getByRole("button", { name: "Bank transfer (IBAN)" }).click();
  await expect(page.getByText(/banking partner supports TRY/)).toBeVisible();
  await page
    .getByRole("button", { name: "Identity verification", exact: true })
    .click();
  await expect(page.getByText(/Verification provider pending/)).toBeVisible();
});
