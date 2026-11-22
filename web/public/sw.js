// Only provide a navigation fallback. Private API responses and mutations are always network-only.
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET" || event.request.mode !== "navigate" || new URL(event.request.url).pathname !== "/") return;
  event.respondWith(fetch(event.request).catch(() => new Response(
    '<!doctype html><html lang="en"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Oracle offline</title><body style="background:#080809;color:white;font:16px system-ui;padding:2rem"><h1>Oracle is offline</h1><p>Reconnect to see current markets and your paper wallet.</p><a style="color:#83c3ff" href="/">Try again</a></body></html>',
    {headers:{"Content-Type":"text/html; charset=utf-8","Cache-Control":"no-store"}}
  )));
});
