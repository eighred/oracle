import { Capacitor, CapacitorHttp } from "@capacitor/core";
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

export function signOut() {
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
  if (!native) return fetch(path, init);
  const key = authorization;
  const response = await nativeRead(path, init, key);
  if (response.status === 401 && key && key === authorization) signOut();
  return response;
}
