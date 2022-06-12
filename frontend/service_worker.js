// Code mostly taken from MDN:
// https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Offline_Service_workers

const cacheName = "readtomyshow-v1";

const appShellFiles = [
    "/",
    "/index.html",
    "/readtomyshoe-frontend.js",
    "/readtomyshoe-frontend_bg.wasm"
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

// Respond to fetches with the cached content
self.addEventListener('fetch', (e) => {
    e.respondWith((async () => {
        // If there's a cache hit, return it
        const r = await caches.match(e.request);
        console.log(`[Service Worker] Fetching resource: ${e.request.url}`);
        if (r) {
            return r;
        }

        // Otherwise, fetch the resource normally
        const response = await fetch(e.request);
        const cache = await caches.open(cacheName);
        return response;

        // Do not cache other things like API calls.
        /*
        // Finally, cache the newly fetched resource
        console.log(`[Service Worker] Caching new resource: ${e.request.url}`);
        cache.put(e.request, response.clone());
        return response;
        */
    })());
});
