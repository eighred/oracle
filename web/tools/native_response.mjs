import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { createServer } from "vite";

// Web, Android and iOS ship the same Vite distribution, including lazy chunks.
for (const file of await readdir("dist", { recursive: true })) {
  if (/\.(js|css|html|json|webmanifest)$/i.test(file)) {
    assert.doesNotMatch(await readFile(join("dist", file), "utf8"), /binance|okx/i, file);
  }
}

// The Capacitor bridge returns an empty string for a bodyless HTTP result.
globalThis.androidBridge = {};
globalThis.Capacitor = {
  PluginHeaders: [
    { name: "CapacitorHttp", methods: [{ name: "request", rtype: "promise" }] },
  ],
  nativePromise: async (plugin, method, request) => {
    assert.equal(plugin, "CapacitorHttp");
    assert.equal(method, "request");
    assert.equal(new URL(request.url).pathname, "/api/auth/session");
    assert.equal(request.method, "DELETE");
    return { status: 204, data: "", headers: {} };
  },
};

const server = await createServer({
  appType: "custom",
  optimizeDeps: { noDiscovery: true, include: [] },
  server: { middlewareMode: true, hmr: false, watch: null },
});
try {
  const { native, apiFetch } = await server.ssrLoadModule("/src/api.ts");
  assert.equal(native, true);
  const response = await apiFetch("/api/auth/session", { method: "DELETE" });
  assert.equal(response.status, 204);
  assert.equal(response.body, null);
  console.log("PASS native logout response");
} finally {
  await server.close();
}
