import { Capacitor, CapacitorHttp } from "@capacitor/core";
import {
  initializeNative,
  nativeLogin,
  refreshNative,
  logoutNative,
} from "./native_account";
import Keycloak from "keycloak-js";
import { ref } from "vue";

export const native = Capacitor.isNativePlatform();
export const signedIn = ref(false);
let authorization = "";

function pilotOrigin() {
  const url = new URL(
    import.meta.env.VITE_ORACLE_API_ORIGIN || "http://127.0.0.1:8040",
  );
  if (
    (url.protocol !== "https:" &&
      !(url.protocol === "http:" && url.hostname === "127.0.0.1")) ||
    url.pathname !== "/" ||
    url.search ||
    url.hash ||
    url.username ||
    url.password
  )
    throw Error("Invalid Oracle API origin in this build.");
  return url.origin;
}

export function shareBase() {
  return native ? pilotOrigin() + "/" : location.origin + location.pathname;
}

let provider: Keycloak | undefined;
let expires = 0;
let refresh: Promise<void> | undefined;
export const accountState = ref("");
export const memberHandle = ref("");
export const accountReady = ref(false);
export async function initializeAccount() {
  try {
    const res = native
      ? await nativeRead("/api/auth/config", {}, "")
      : await fetch("/api/auth/config", {
          signal: AbortSignal.timeout(4000),
          cache: "no-store",
        });
    if (!res.ok) return;
    const config = await res.json();
    if (!config.issuer) return;
    if (native) {
      await initializeNative(config.issuer, exchangeBearer, () => {
        signOut();
        accountState.value =
          "Complete email verification and sign in with your authenticator.";
      });
      accountReady.value = true;
      return;
    }
    provider = new Keycloak({
      url: config.issuer.split("/realms/")[0],
      realm: "oracle",
      clientId: config.clientId,
    });
    const logged = await provider.init({
      pkceMethod: "S256",
      checkLoginIframe: false,
      responseMode: "query",
      redirectUri: location.origin + "/",
    });
    accountReady.value = true;
    if (logged) {
      await exchange();
      const destination = sessionStorage.getItem("oracle.return");
      sessionStorage.removeItem("oracle.return");
      if (destination?.startsWith("#")) location.hash = destination;
    }
  } catch {
    accountState.value =
      "Member session unavailable. Complete email verification and sign in with your authenticator.";
  }
}
export async function memberLogin(register = false) {
  if (native) return nativeLogin(register);
  if (location.hash.startsWith("#") && location.hash.length<=512)
    sessionStorage.setItem("oracle.return", location.hash);
  if (!provider) return;
  if (register) await provider.register();
  else await provider.login({ prompt: "login" });
}
async function exchange() {
  if (native) return exchangeBearer(await refreshNative());
  if (!provider) throw Error("Account provider unavailable.");
  await provider.updateToken(30);
  if (!provider.token) throw Error("Sign in again.");
  await exchangeBearer(provider.token);
}
async function exchangeBearer(bearer: string) {
  const auth = "Bearer " + bearer;
  const r = native
    ? await nativeRead("/api/auth/session", { method: "POST" }, auth)
    : await fetch("/api/auth/session", {
        method: "POST",
        headers: { Authorization: auth },
        cache: "no-store",
        redirect: "error",
      });
  if (!r.ok) throw Error("Member verification failed.");
  const result = await r.json();
  authorization = "Basic " + btoa(result.username + ":" + result.token);
  accountState.value = "";
  expires = result.expires;
  memberHandle.value = result.username;
  signedIn.value = true;
}
export async function endSession() {
  if (memberHandle.value && authorization) {
    const r = native
      ? await nativeRead(
          "/api/auth/session",
          { method: "DELETE" },
          authorization,
        )
      : await fetch("/api/auth/session", {
          method: "DELETE",
          headers: { Authorization: authorization },
        });
    if (!r.ok && r.status !== 401) throw Error("Could not end session.");
    signOut();
    if (native) return logoutNative();
    await provider?.logout({ redirectUri: location.origin + "/" });
  } else signOut();
}
export function signOut() {
  memberHandle.value = "";
  expires = 0;
  authorization = "";
  signedIn.value = false;
}

async function nativeRead(path: string, init: RequestInit, auth: string) {
  const url = new URL(path, pilotOrigin());
  if (url.origin !== pilotOrigin() || !url.pathname.startsWith("/api/"))
    throw Error("Invalid API path.");
  init.signal?.throwIfAborted();
  const headers = {} as Record<string, string>;
  new Headers(init.headers).forEach((value, key) => {
    headers[key === "content-type" ? "Content-Type" : key] = value;
  });
  if (auth) headers.Authorization = auth;
  const response = await CapacitorHttp.request({
    url: url.href,
    method: init.method ?? "GET",
    headers,
    data: typeof init.body === "string" ? JSON.parse(init.body) : undefined,
    responseType: "text",
    disableRedirects: true,
    connectTimeout: 10000,
    readTimeout: 30000,
  });
  init.signal?.throwIfAborted();
  return new Response(
    typeof response.data === "string"
      ? response.data
      : JSON.stringify(response.data),
    { status: response.status, headers: response.headers },
  );
}

export async function signIn(user: string, pass: string) {
  if (!user.trim() || user.includes(":"))
    throw Error("Enter a valid username.");
  const key =
    "Basic " +
    btoa(
      String.fromCharCode(
        ...new TextEncoder().encode(user.trim() + ":" + pass),
      ),
    );
  const response = await nativeRead("/api/profile", {}, key);
  if (!response.ok)
    throw Error(
      response.status === 401
        ? "Check your Oracle username and passphrase."
        : "Oracle is unavailable.",
    );
  const profile = await response.json();
  if (profile.principal !== user.trim())
    throw Error("Account verification failed.");
  authorization = key;
  signedIn.value = true;
}

export async function apiFetch(path: string, init: RequestInit = {}) {
  if (
    !path.startsWith("/api/") ||
    new URL(path, "https://oracle.test").origin !== "https://oracle.test"
  )
    throw Error("Invalid API path.");
  if (memberHandle.value && Date.now() / 1000 >= expires - 15) {
    refresh ??= exchange().finally(() => {
      refresh = undefined;
    });
    try {
      await refresh;
    } catch {
      signOut();
      throw Error("Session expired.");
    }
  }
  const key = authorization;
  const headers = new Headers(init.headers);
  if (key) headers.set("Authorization", key);
  const response = native
    ? await nativeRead(path, init, key)
    : await fetch(path, { ...init, headers, redirect: "error" });
  if (response.status === 401 && key && key === authorization) signOut();
  return response;
}
