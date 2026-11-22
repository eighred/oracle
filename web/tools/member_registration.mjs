import assert from "node:assert/strict";
import { createHmac, randomBytes } from "node:crypto";
import { chromium } from "playwright";

// This integration creates a synthetic member only on the loopback pilot.
const origin = "http://127.0.0.1:8040";
const mail = "http://127.0.0.1:8125";
const issuer = "http://127.0.0.1:8180";
const handle = "member_" + randomBytes(6).toString("hex");
const email = handle + "@example.test";
const password = randomBytes(20).toString("hex");

function code(key) {
  const bits = [...key.replace(/\s/g, "")]
    .map((c) => {
      const index = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c.toUpperCase());
      assert.ok(index >= 0);
      return index.toString(2).padStart(5, "0");
    })
    .join("");
  const secret = Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(Date.now() / 30000)));
  const mac = createHmac("sha1", secret).update(counter).digest();
  return ((mac.readUInt32BE(mac[19] & 15) & 0x7fffffff) % 1000000)
    .toString()
    .padStart(6, "0");
}

async function verificationLink(subject) {
  for (let attempt = 0; attempt < 35; attempt++) {
    const list = await (await fetch(mail + "/api/v1/messages")).json();
    const match = list.messages.find(
      (m) => m.To.some((t) => t.Address === email) && subject.test(m.Subject),
    );
    if (match) {
      const message = await (
        await fetch(mail + "/api/v1/message/" + match.ID)
      ).json();
      const link = new URL(
        message.Text.match(
          /http:\/\/[^\s<>]+\/login-actions\/action-token\?[^\s<>]+/,
        )[0],
      );
      assert.equal(link.origin, issuer);
      return link.href;
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
  throw Error("Verification email did not arrive");
}

const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  page.setDefaultTimeout(10000);
  const submit = () =>
    page.locator("button[type=submit],input[type=submit]").first().click();
  await page.goto(origin);
  await page.getByRole("button", { name: "Register", exact: true }).click();
  await page.locator("#username").fill(handle);
  await page.locator("#email").fill(email);
  await submit();
  const verify = await verificationLink(/verify/i);
  assert.equal((await page.request.get(origin + "/api/profile")).status(), 401);
  await page.goto(verify);
  await page.locator("#password-new").fill(password);
  await page.locator("#password-confirm").fill(password);
  await submit();
  await page.getByRole("link", { name: "Unable to scan?" }).click();
  const secret = (await page.locator("#kc-totp-secret-key").innerText()).trim();
  await page.locator("#totp").fill(code(secret));
  await page.locator("#userLabel").fill("Integration authenticator");
  await submit();
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.locator("#username").fill(email);
  await page.locator("#password").fill(password);
  await submit();
  const valid = code(secret);
  await page.locator("#otp").fill(valid === "111111" ? "222222" : "111111");
  await submit();
  await page.locator("#otp").waitFor();
  await page.waitForTimeout(31000 - (Date.now() % 30000));
  await page.locator("#otp").fill(code(secret));
  const exchange = page.waitForResponse(
    (r) =>
      r.url() === origin + "/api/auth/session" &&
      r.request().method() === "POST",
  );
  await submit();
  const response = await exchange;
  assert.equal(response.status(), 200);
  const session = await response.json();
  assert.equal(session.username, "@" + handle);
  const headers = {
    Authorization:
      "Basic " +
      Buffer.from(session.username + ":" + session.token).toString("base64"),
  };
  await page.evaluate(() => {
    location.hash = "profile";
  });
  await page.getByRole("heading", { name: "Balance overview" }).waitFor();
  const profile = await (
    await page.request.get(origin + "/api/profile", { headers })
  ).json();
  assert.equal(profile.principal, session.username);
  assert.equal(JSON.stringify(profile).includes(email), false);
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page.getByRole("button", { name: "Sign in", exact: true }).waitFor();
  assert.equal(
    (await page.request.get(origin + "/api/profile", { headers })).status(),
    401,
  );

  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByRole("link", { name: /forgot password/i }).click();
  await page.locator("#username").fill(email);
  await submit();
  const recovery = await verificationLink(/reset/i);
  await page.goto(recovery);
  const replacement = randomBytes(24).toString("hex");
  await page.locator("#password-new").fill(replacement);
  await page.locator("#password-confirm").fill(replacement);
  await submit();
  await page.goto(origin);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.locator("#username").fill(handle);
  await page.locator("#password").fill(password);
  await submit();
  await page.locator("#password").waitFor();
  await page.locator("#username").fill(handle);
  await page.locator("#password").fill(replacement);
  await submit();
  await page.locator("#otp").waitFor();
  await page.waitForTimeout(31000 - (Date.now() % 30000));
  await page.locator("#otp").fill(code(secret));
  const recovered = page.waitForResponse(
    (r) =>
      r.url() === origin + "/api/auth/session" &&
      r.request().method() === "POST",
  );
  await submit();
  const result = await recovered;
  assert.equal(result.status(), 200);
  assert.equal((await result.json()).username, session.username);
  console.log(
    "PASS password recovery with the original authenticator and wallet identity",
  );
  console.log(
    "PASS verified email registration, password, required TOTP, scoped wallet and revoked session",
  );
} finally {
  await browser.close();
}
