#!/usr/bin/env python3
"""Capture the landing page's screenshots from the real app UI.

Loads the real ui/index.html in WebKitGTK (the engine the Linux app uses),
offscreen, and answers its Tauri `invoke` calls with the app's real command
code and the real bundled KJV/WEB text, through the same test-only bridge
the frontend tests use (src-tauri/examples/frontend_bridge.rs, against a
throwaway database). Each scene sets the display preferences the app keeps
in local storage, then drives the page's own controls with DOM events, as
the frontend tests do. Nothing is drawn over or composited in.

Usage, from the repository root:
    cargo build --manifest-path src-tauri/Cargo.toml --example frontend_bridge
    python3 tools/site/capture_screenshots.py OUT_DIR [--theme NAME] [SCENE ...]

--theme sets the theme of the feature screenshots (reading, search, compare,
select); it defaults to matrix, the site's. The theme-* scenes are always in
their own themes. Naming scenes captures only those. The first-run welcome is
marked as seen and no highlights are set, and a capture fails if either is on
screen.

Writes raw PNGs (at 2x) to OUT_DIR; cropping and WebP conversion are
separate (see site/README.md). Needs PyGObject with WebKit2 4.1 and a
display (it never shows a window).
"""

import functools
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

os.environ.setdefault("GDK_SCALE", "2")
os.environ.setdefault("GDK_BACKEND", "x11")
# Offscreen windows can't get a GL context; render in software.
os.environ.setdefault("WEBKIT_DISABLE_COMPOSITING_MODE", "1")

import gi  # noqa: E402

gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
BRIDGE = ROOT / "src-tauri/target/debug/examples/frontend_bridge"


class Bridge:
    """The app's real command logic over stdio, one JSON line each way."""

    def __init__(self, db_path):
        self.proc = subprocess.Popen(
            [str(BRIDGE), str(db_path)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        if not json.loads(self.proc.stdout.readline()).get("ready"):
            raise RuntimeError("bridge didn't start")
        self.next_id = 0

    def raw(self, cmd, args):
        self.next_id += 1
        line = json.dumps({"id": self.next_id, "cmd": cmd, "args": args})
        self.proc.stdin.write(line + "\n")
        self.proc.stdin.flush()
        return self.proc.stdout.readline()

    def call(self, cmd, args):
        reply = json.loads(self.raw(cmd, args))
        if "err" in reply:
            raise RuntimeError(f"{cmd}: {reply['err']}")
        return reply["ok"]


# Runs at document start, before the page's own scripts: the preferences
# for this scene, the Tauri `invoke` the page calls (forwarded to Python),
# and small helpers the scenes use to drive the page.
INIT_JS = """
(function () {
    var prefs = %(prefs)s;
    try {
        localStorage.clear();
        Object.keys(prefs).forEach(function (k) { localStorage.setItem(k, prefs[k]); });
    } catch (e) {}
    var pending = {}, next = 0;
    window.__ggReply = function (id, msg) {
        var p = pending[id];
        delete pending[id];
        if ('err' in msg) p.reject(msg.err); else p.resolve(msg.ok);
    };
    window.__TAURI__ = { core: { invoke: function (cmd, args) {
        return new Promise(function (resolve, reject) {
            var id = ++next;
            pending[id] = { resolve: resolve, reject: reject };
            window.webkit.messageHandlers.invoke.postMessage(
                JSON.stringify({ id: id, cmd: cmd, args: args || {} }));
        });
    } } };
    window.__gg = {
        sleep: function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); },
        until: async function (check, what) {
            for (var i = 0; i < 500; i++) {
                if (check()) return;
                await window.__gg.sleep(10);
            }
            throw new Error('timed out waiting for ' + what);
        },
        click: function (el, opts) {
            var init = Object.assign({ bubbles: true, cancelable: true, button: 0 }, opts || {});
            el.dispatchEvent(new MouseEvent('mousedown', init));
            el.dispatchEvent(new MouseEvent('mouseup', init));
            el.dispatchEvent(new MouseEvent('click', init));
        },
        key: function (key, opts) {
            document.body.dispatchEvent(new KeyboardEvent('keydown',
                Object.assign({ key: key, bubbles: true, cancelable: true }, opts || {})));
        },
        type: function (input, text) {
            input.value = text;
            input.dispatchEvent(new Event('input', { bubbles: true }));
        },
        verse: function (n, code) {
            var sel = '.chapter-main .verse[data-translation' + (code ? '="' + code + '"' : '') +
                '][data-verse="' + n + '"]';
            return document.querySelector(sel);
        },
        booted: function () {
            return window.__gg.until(function () {
                return document.querySelector('.chapter-heading') &&
                    document.querySelector('#bookmark-list li');
            }, 'the reading pane');
        },
    };
})();
"""

BOOKS = [b["name"] for b in json.loads((ROOT / "src-tauri/data/kjv.json").read_text("utf-8-sig"))]


def book_id(name):
    return BOOKS.index(name) + 1


class Capturer:
    def __init__(self, out_dir, base_url, bridge):
        self.out_dir = Path(out_dir)
        self.base_url = base_url
        self.bridge = bridge
        self.ucm = WebKit2.UserContentManager()
        self.ucm.register_script_message_handler("invoke")
        self.ucm.connect("script-message-received::invoke", self.on_invoke)
        self.view = WebKit2.WebView.new_with_user_content_manager(self.ucm)
        self.window = Gtk.OffscreenWindow()
        self.window.add(self.view)
        self.window.show_all()
        self.loop = GLib.MainLoop()
        self.error = None

    def on_invoke(self, _ucm, result):
        value = result.get_js_value() if hasattr(result, "get_js_value") else result
        request = json.loads(value.to_string())
        reply = json.loads(self.bridge.raw(request["cmd"], request["args"]))
        script = f"window.__ggReply({request['id']}, {json.dumps(reply)})"
        self.view.evaluate_javascript(script, -1, None, None, None, None, None)

    def wait(self):
        self.loop.run()
        if self.error:
            raise self.error

    def done(self, error=None):
        self.error = error
        self.loop.quit()

    def scene(self, name, *, size, prefs, position, steps, bookmarks=(), full=False):
        """Load the app fresh and run `steps` (an async JS function body)."""
        book, chapter, translation = position
        self.bridge.call("__reset", {"bookId": book_id(book), "chapter": chapter,
                                     "translationCode": translation})
        for b, c, v1, v2 in bookmarks:
            self.bridge.call("add_bookmark", {"bookId": book_id(b), "chapter": c,
                                              "verseStart": v1, "verseEnd": v2})
        self.ucm.remove_all_scripts()
        self.ucm.add_script(WebKit2.UserScript.new(
            INIT_JS % {"prefs": json.dumps(prefs)},
            WebKit2.UserContentInjectedFrames.TOP_FRAME,
            WebKit2.UserScriptInjectionTime.START, None, None))
        self.window.resize(*size)
        self.view.set_size_request(*size)

        def loaded(view, event):
            if event != WebKit2.LoadEvent.FINISHED:
                return
            view.disconnect(handler)
            body = "await __gg.booted();\n" + steps + "\nawait __gg.sleep(400);\n" + CLEAN_CHECK
            view.call_async_javascript_function(body, -1, None, None, None, None, ran)

        def ran(view, res):
            try:
                view.call_async_javascript_function_finish(res)
            except GLib.Error as e:
                self.done(RuntimeError(f"{name}: {e.message}"))
                return
            region = (WebKit2.SnapshotRegion.FULL_DOCUMENT if full
                      else WebKit2.SnapshotRegion.VISIBLE)
            view.get_snapshot(region, WebKit2.SnapshotOptions.NONE, None, shot)

        def shot(view, res):
            surface = view.get_snapshot_finish(res)
            surface.write_to_png(str(self.out_dir / f"{name}.png"))
            print(f"{name}.png  {surface.get_width()}x{surface.get_height()}")
            self.done()

        handler = self.view.connect("load-changed", loaded)
        self.view.load_uri(self.base_url)
        self.wait()


# Every scene: the first-run welcome already seen (so it never opens).
SEEN = {"gospel-getter-welcome": "seen"}

# Run after each scene's steps: nothing that isn't meant to be in a shot.
CLEAN_CHECK = """
const welcome = document.getElementById('welcome-overlay');
if (welcome && !welcome.hidden) throw new Error('the welcome is on screen');
if (document.querySelector('[data-highlight]')) throw new Error('a highlight is on screen');
"""

# Scroll so `el` sits `top` CSS pixels below the app's top bar, which the
# page scrolls beneath (so nothing a scene shows starts hidden under it).
SCROLL_TO = ("window.scrollTo(0, window.scrollY + %s.getBoundingClientRect().top"
             " - document.getElementById('top-bar').getBoundingClientRect().bottom - %d);")

def feature_scenes(theme):
    """The site's feature screenshots, in `theme`."""
    look = {**SEEN, "gospel-getter-theme": theme}
    return [
        # Reading in context: the chapter with its neighbours on either side.
        # The headings sit where build_images.sh's hero and og crops expect.
        dict(name="reading", size=(1280, 800), prefs=look, position=("John", 3, "web"),
             steps=SCROLL_TO % ("document.querySelector('.chapter-heading')", 86)),
        # Searching the words of the text.
        dict(name="search", size=(1280, 800), prefs=look, position=("John", 15, "web"),
             steps="""
                __gg.click(document.getElementById('search-toggle'));
                __gg.type(document.getElementById('search-input'), 'love one another');
                await __gg.until(() => document.querySelector('#search-results .search-result'), 'results');
                window.scrollTo(0, 0);
             """),
        # KJV and WEB side by side.
        dict(name="compare", size=(1280, 800),
             prefs={**look, "gospel-getter-compare": "on"}, position=("Psalms", 23, "web"),
             steps="""
                await __gg.until(() => document.querySelector('.compare-grid'), 'compare grid');
             """ + SCROLL_TO % ("document.querySelector('.chapter-heading')", 40)),
        # A selected range with Copy and Bookmark, earlier bookmarks starred.
        dict(name="select", size=(1280, 800), prefs=look, position=("1 Corinthians", 13, "web"),
             bookmarks=[("1 Corinthians", 13, 1, 3)],
             steps="""
                __gg.click(__gg.verse(4));
                __gg.click(__gg.verse(7), { shiftKey: true });
                await __gg.until(() => document.querySelector('.verse-actions'), 'verse actions');
             """ + SCROLL_TO % ("document.querySelector('.chapter-heading')", 40)),
    ]


THEMES = ["vaporwave", "classic-dark", "classic-light", "matrix", "beast-slayer", "hot-pink"]
# The "Make it yours" strip: the same passage in each theme.
THEME_SCENES = [
    dict(name=f"theme-{theme}", size=(1280, 800), prefs={**SEEN, "gospel-getter-theme": theme},
         position=("Psalms", 23, "web"),
         steps=SCROLL_TO % ("document.querySelector('.chapter-heading')", 40))
    for theme in THEMES
]


def serve(directory):
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=directory)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def main():
    args = sys.argv[1:]
    theme = "matrix"
    if "--theme" in args:
        i = args.index("--theme")
        theme = args[i + 1]
        del args[i:i + 2]
    if theme not in THEMES:
        sys.exit(f"unknown theme {theme!r}; one of {', '.join(THEMES)}")
    out_dir = Path(args[0] if args else "screenshots-raw")
    out_dir.mkdir(parents=True, exist_ok=True)
    only = set(args[1:])
    server = serve(str(ROOT / "ui"))
    with tempfile.TemporaryDirectory() as tmp:
        bridge = Bridge(Path(tmp) / "gospel_getter.db")
        capturer = Capturer(out_dir, f"http://127.0.0.1:{server.server_port}/index.html", bridge)
        for scene in feature_scenes(theme) + THEME_SCENES:
            if not only or scene["name"] in only:
                capturer.scene(**scene)
        bridge.proc.stdin.close()
        bridge.proc.wait()
    server.shutdown()


if __name__ == "__main__":
    main()
