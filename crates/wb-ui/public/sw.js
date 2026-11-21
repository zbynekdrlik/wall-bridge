// wall-bridge PWA service worker: makes the dashboard installable. Network only: the page
// shows live state, so nothing is cached.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));
self.addEventListener("fetch", () => {});
