// Code mostly taken from MDN:
// https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Offline_Service_workers

const cacheName = "readtomyshow-v1";

const appShellFiles = [
    "/",
    "/index.html",
    "/assets/rtms-color-32x32.png",
    "/assets/rtms-color-180x180",
    "/assets/readtomyshoe-frontend.js",
    "/assets/readtomyshoe-frontend_bg.wasm"
];

// Add to caches on installation
self.addEventListener('install', (e) => {
    e.waitUntil((async () => {
        const cache = await caches.open(cacheName);
        await cache.addAll(appShellFiles);
    })());
});

// Clear old caches
self.addEventListener('activate', (e) => {
    e.waitUntil(caches.keys().then((keyList) => {
        // Delete all the keys that don't belong to the current `cacheName`
        return Promise.all(keyList.map((key) => {
            if (key === cacheName) { return; }
            return caches.delete(key);
        }))
    }));
});

// Try to fetch content from the network. On failure, serve from the cache.
self.addEventListener('fetch', (e) => {
    // We don't cache API calls or internal pages
    const reqUrl = new URL(e.request.url);
    if (reqUrl.pathname.startsWith("/api") || reqUrl.pathname.startsWith("/add")) {
        return;
    }

    // Respond to asset fetches as follows
    e.respondWith((async () => {
        // Try to fetch the resource normally
        var cache = await caches.open(cacheName);
        try {
            const response = await fetch(e.request);
            // If fetch succeeded, cache the result and return it
            cache.put(e.request, response.clone());
            return response;
        } catch {
            // If fetching fails, try to hit the cache
            const c = await caches.match(e.request);
            return c;
        }
    })());
});
