import { CapacitorHttp } from "@capacitor/core";
import { Browser } from "@capacitor/browser";
import { App } from "@capacitor/app";

const redirect = "com.eighred.oracle://identity/callback";
const clientId = "oracle-mobile";
let authority = "";
let pending: { state: string; verifier: string; until: number } | undefined;
let refreshToken = "";

function encoded(bytes: Uint8Array) {
  return btoa(String.fromCharCode(...bytes))
    .replace(/=/g, "")
    .replace(/\+/g, "-")
    .replace(/\//g, "_");
}

async function token(fields: Record<string, string>) {
  const answer = await CapacitorHttp.post({
    url: authority + "/protocol/openid-connect/token",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    data: new URLSearchParams({ client_id: clientId, ...fields }).toString(),
    disableRedirects: true,
    connectTimeout: 10000,
    readTimeout: 10000,
  });
  if (answer.status !== 200 || typeof answer.data?.access_token !== "string")
    throw Error("Sign in again.");
  refreshToken = answer.data.refresh_token ?? "";
  return answer.data.access_token as string;
}

export async function initializeNative(
  issuer: string,
  accept: (bearer: string) => Promise<void>,
  fail: () => void,
) {
  authority = issuer;
  await App.addListener("appUrlOpen", async ({ url }) => {
    const callback = new URL(url);
    if (
      url.split("?")[0] !== redirect ||
      !pending ||
      callback.searchParams.get("state") !== pending.state
    )
      return;
    const request = pending;
    pending = undefined;
    await Browser.close().catch(() => {});
    try {
      const code = callback.searchParams.get("code");
      if (
        !code ||
        callback.searchParams.get("iss") !== authority ||
        request.until < Date.now()
      )
        throw Error();
      await accept(
        await token({
          grant_type: "authorization_code",
          code,
          code_verifier: request.verifier,
          redirect_uri: redirect,
        }),
      );
    } catch {
      refreshToken = "";
      fail();
    }
  });
}

export async function nativeLogin(register: boolean) {
  const verifier = encoded(crypto.getRandomValues(new Uint8Array(32)));
  const state = encoded(crypto.getRandomValues(new Uint8Array(32)));
  const challenge = encoded(
    new Uint8Array(
      await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier)),
    ),
  );
  pending = { state, verifier, until: Date.now() + 15 * 60000 };
  const query = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirect,
    response_type: "code",
    scope: "openid email profile",
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
    prompt: "login",
  });
  await Browser.open({
    url:
      authority +
      "/protocol/openid-connect/" +
      (register ? "registrations" : "auth") +
      "?" +
      query,
  });
}

export function refreshNative() {
  if (!refreshToken) throw Error("Sign in again.");
  return token({ grant_type: "refresh_token", refresh_token: refreshToken });
}

export async function logoutNative() {
  const previous = refreshToken;
  refreshToken = "";
  pending = undefined;
  if (previous) {
    const result = await CapacitorHttp.post({
      url: authority + "/protocol/openid-connect/logout",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      data: new URLSearchParams({
        client_id: clientId,
        refresh_token: previous,
      }).toString(),
      disableRedirects: true,
      connectTimeout: 10000,
      readTimeout: 10000,
    });
    if (result.status !== 204 && result.status !== 200)
      throw Error("Provider logout unavailable.");
  }
}
