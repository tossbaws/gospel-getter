// Loads the page's Rust pieces (ui-leptos/, compiled to WebAssembly by
// Trunk, see Trunk.toml) beside its scripts: the wasm-bindgen glue as a
// classic script, then the module itself. Trunk inlines this file at the
// end of <body>, so Tauri hashes it into the Content Security Policy like
// the page's own scripts; the glue and the module are the app's own files
// (script-src and connect-src 'self').
//
// If anything here fails, the page is as it was without the pieces: only
// Export notes… is unavailable, and says so. The frontend test harness
// doesn't run this: it hands the glue the .wasm bytes itself (jsdom has no
// fetch), see tests/frontend/harness.mjs.
(function () {
    var script = document.createElement('script');
    script.src = 'ui-leptos.js';
    script.onload = function () {
        wasm_bindgen({ module_or_path: 'ui-leptos_bg.wasm' }).catch(function (e) {
            console.error('Failed to load the page’s Rust pieces', e);
        });
    };
    script.onerror = function () {
        console.error('Failed to load the page’s Rust pieces: ui-leptos.js');
    };
    document.head.appendChild(script);
})();
