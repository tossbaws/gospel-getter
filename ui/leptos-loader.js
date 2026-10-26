// Loads the page's Rust pieces (ui-leptos/, compiled to WebAssembly by
// Trunk, see Trunk.toml) beside its scripts: the wasm-bindgen glue as a
// classic script, then the module itself. Trunk inlines this file at the
// end of <body>, so Tauri hashes it into the Content Security Policy like
// the page's own scripts; the glue and the module are the app's own files
// (script-src and connect-src 'self').
//
// If anything here fails, it says so on the page, as
// `gospel-getter:pieces-failed` on `document`: the page then shows the
// chapter as plain text, and each piece says it isn't available when asked
// for. The frontend test harness doesn't run this: it hands the glue the
// .wasm bytes itself (jsdom has no fetch), and says the same when it loads
// no pieces; see tests/frontend/harness.mjs.
(function () {
    var failed = function () {
        document.dispatchEvent(new CustomEvent('gospel-getter:pieces-failed'));
    };
    var script = document.createElement('script');
    script.src = 'ui-leptos.js';
    script.onload = function () {
        wasm_bindgen({ module_or_path: 'ui-leptos_bg.wasm' }).catch(function (e) {
            console.error('Failed to load the page’s Rust pieces', e);
            failed();
        });
    };
    script.onerror = function () {
        console.error('Failed to load the page’s Rust pieces: ui-leptos.js');
        failed();
    };
    document.head.appendChild(script);
})();
