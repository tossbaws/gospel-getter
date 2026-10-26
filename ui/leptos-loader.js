// Loads the page's Rust pieces (ui-leptos/, compiled to WebAssembly by
// Trunk, see Trunk.toml) beside its scripts: the wasm-bindgen glue as a
// classic script, and the module itself. Trunk inlines this file in
// <head>, after the pre-paint script, so Tauri hashes it into the Content
// Security Policy like the page's own scripts; the glue and the module are
// the app's own files (script-src and connect-src 'self', and
// 'wasm-unsafe-eval' to compile it).
//
// The module is fetched and compiled while the page is still loading (the
// reader draws the chapter, so the sooner it's there the sooner the
// chapter is), and started once the page has been read and its scripts
// have run (DOMContentLoaded): the pieces find the page's markup, and the
// page's listeners, there.
//
// If anything here fails, it says so on the page, as
// `gospel-getter:pieces-failed` on `document`, once the page is read: the
// page then shows the chapter as plain text, and each piece says it isn't
// available when asked for. The frontend test harness doesn't run this: it
// hands the glue the .wasm bytes itself (jsdom has no fetch), and says the
// same when it loads no pieces; see tests/frontend/harness.mjs.
(function () {
    var MODULE = 'ui-leptos_bg.wasm';
    var read = new Promise(function (resolve) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', resolve, { once: true });
        } else {
            resolve();
        }
    });
    var failed = function (what, e) {
        console.error(what, e);
        read.then(function () {
            document.dispatchEvent(new CustomEvent('gospel-getter:pieces-failed'));
        });
    };
    // Compiled as it arrives where the module is served as application/wasm;
    // otherwise from its bytes.
    var compiled = (WebAssembly.compileStreaming
        ? WebAssembly.compileStreaming(fetch(MODULE))
        : Promise.reject(new Error('no compileStreaming'))
    ).catch(function () {
        return fetch(MODULE).then(function (r) {
            return r.arrayBuffer();
        }).then(function (bytes) {
            return WebAssembly.compile(bytes);
        });
    });
    // Its failure is said below, once the glue is there to say it.
    compiled.catch(function () {});
    var glue = new Promise(function (resolve, reject) {
        var script = document.createElement('script');
        script.src = 'ui-leptos.js';
        script.onload = resolve;
        script.onerror = function () {
            reject(new Error('ui-leptos.js'));
        };
        document.head.appendChild(script);
    });
    glue.then(function () {
        return Promise.all([compiled, read]);
    }).then(function (ready) {
        return wasm_bindgen({ module_or_path: ready[0] });
    }).catch(function (e) {
        failed('Failed to load the page’s Rust pieces', e);
    });
})();
