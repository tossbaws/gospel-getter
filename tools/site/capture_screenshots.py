#!/usr/bin/env python3
"""Capture the landing page's screenshots from the real app UI.

Loads the real page as Trunk builds it (dist/: ui/index.html with its Rust
pieces, from `trunk build --release`) in WebKitGTK (the engine the Linux app
uses), offscreen, and answers its Tauri `invoke` calls with the app's real command
code and the real bundled KJV, BSB and WEB text, through the same test-only bridge
the frontend tests use (src-tauri/examples/frontend_bridge.rs, against a
throwaway database). Each scene sets the display preferences the app keeps
in local storage, then drives the page's own controls with DOM events, as
the frontend tests do. Nothing is drawn over or composited in.

Usage, from the repository root:
    trunk build --release
    cargo build --manifest-path src-tauri/Cargo.toml --example frontend_bridge
    PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py --all
    PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py OUT_DIR [--theme NAME] [SCENE ...]
    PYTHONDONTWRITEBYTECODE=1 python3 tools/site/capture_screenshots.py --readme OUT_DIR [--theme NAME] [SCENE ...]

--theme sets the theme of the feature screenshots (the site's reading,
search, compare and select, or the README's); it defaults to chi-rho, the
app's default theme. The theme-* scenes are always in their own themes.
Naming scenes captures only those. The first-run welcome is marked as seen
and no highlights are set, and a capture fails if either is on screen.

--all remakes every image in place: the site's raw captures go to a
temporary folder and through tools/site/build_images.sh into site/assets/,
and the README's go to screenshots/.

Without --readme, writes the site's raw PNGs (at 2x) to OUT_DIR; cropping
and WebP conversion are separate (see site/README.md). With --readme, writes
the README's screenshots at 1x, at their final names and sizes (OUT_DIR
can be screenshots/ itself). Every scene shows the picker as the app does at
its window size: from 1200px wide the book and chapter picker is a sidebar on
the left, and narrower it's a popover from the reference button, closed.
The README's margin-notes scene is in reading mode, which shows no picker.
The verses are one to a line (Verses, the default), except in the README's
flowing scene, which sets Flowing under Aa. Scenes read the KJV or the WEB,
except the README's bsb scene; the compare scenes read the KJV, compared with
the WEB.
Needs PyGObject with WebKit2 4.1 and a display (it never shows a window).
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

# One device pixel per GTK pixel. The site's 2x captures use the page's
# zoom instead (see Capturer): with GDK_SCALE=2, WebKit lays text out at 1x
# and draws it at 2x, which spaces Literata's letters unevenly. Set, not
# defaulted, as sessions often export their own GDK_SCALE.
os.environ["GDK_SCALE"] = "1"
# Capture on XWayland, set rather than defaulted, as the session exports
# GDK_BACKEND=wayland,... The backend doesn't matter for key events: GTK3's
# key-simulation test helpers (Gtk.test_widget_send_key, Gdk.test_simulate_key)
# segfault on any backend here, as offscreen windows leave them unimplemented
# and GDK calls through a NULL pointer. Use send_key below instead.
os.environ["GDK_BACKEND"] = "x11"
# Offscreen windows can't get a GL context; render in software.
os.environ.setdefault("WEBKIT_DISABLE_COMPOSITING_MODE", "1")

import gi  # noqa: E402

gi.require_version("Gdk", "3.0")
gi.require_version("Gtk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
BRIDGE = ROOT / "src-tauri/target/debug/examples/frontend_bridge"


def send_key(widget, keyval, mods=0):
    """Press and release a key on `widget`, which may sit in the offscreen
    window. Builds the events itself, as Gtk.test_widget_send_key segfaults
    on offscreen windows."""
    display = Gdk.Display.get_default()
    keyboard = display.get_default_seat().get_keyboard()
    ok, entries = Gdk.Keymap.get_for_display(display).get_entries_for_keyval(keyval)
    for kind in (Gdk.EventType.KEY_PRESS, Gdk.EventType.KEY_RELEASE):
        event = Gdk.Event.new(kind)
        event.key.window = widget.get_window()
        event.key.send_event = True
        event.key.time = Gdk.CURRENT_TIME
        event.key.keyval = keyval
        event.key.state = mods
        event.key.hardware_keycode = entries[0].keycode if ok else 0
        event.set_device(keyboard)
        widget.event(event)


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
    def __init__(self, out_dir, base_url, bridge, scale=1):
        self.out_dir = Path(out_dir)
        self.base_url = base_url
        self.bridge = bridge
        self.ucm = WebKit2.UserContentManager()
        self.ucm.register_script_message_handler("invoke")
        self.ucm.connect("script-message-received::invoke", self.on_invoke)
        self.view = WebKit2.WebView.new_with_user_content_manager(self.ucm)
        # `scale` device pixels per CSS pixel: a window `scale` times the
        # scene's size, zoomed so the page still sees the scene's size.
        self.scale = scale
        self.view.set_zoom_level(scale)
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

    def scene(self, name, *, size, prefs, position, steps, bookmarks=(), notes=(), full=False):
        """Load the app fresh and run `steps` (an async JS function body).
        `bookmarks` and `notes` (each with its text last) are saved first."""
        book, chapter, translation = position
        self.bridge.call("__reset", {"bookId": book_id(book), "chapter": chapter,
                                     "translationCode": translation})
        for b, c, v1, v2 in bookmarks:
            self.bridge.call("add_bookmark", {"bookId": book_id(b), "chapter": c,
                                              "verseStart": v1, "verseEnd": v2})
        for b, c, v1, v2, body in notes:
            self.bridge.call("add_note", {"bookId": book_id(b), "chapter": c,
                                          "verseStart": v1, "verseEnd": v2, "body": body})
        self.ucm.remove_all_scripts()
        self.ucm.add_script(WebKit2.UserScript.new(
            INIT_JS % {"prefs": json.dumps(prefs)},
            WebKit2.UserContentInjectedFrames.TOP_FRAME,
            WebKit2.UserScriptInjectionTime.START, None, None))
        size = (size[0] * self.scale, size[1] * self.scale)
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

# Select verses `start` through `end` of the chapter being read, as a
# reader does (a click, then a Shift-click), and wait for the card. The
# scenes scroll first: the card is placed under the selection, or above it
# when the window has no room below, as the page stands when it opens.
SELECT = """
                __gg.click(__gg.verse(%d));
                __gg.click(__gg.verse(%d), { shiftKey: true });
                await __gg.until(() => document.querySelector('#selection-card .verse-actions'), 'the card');
"""


def feature_scenes(theme):
    """The site's feature screenshots, in `theme`."""
    look = {**SEEN, "gospel-getter-theme": theme}
    return [
        # Reading in context: the chapter with its neighbours on either side,
        # the picker's sidebar on the left. The page is at its top, so the
        # headings sit where build_images.sh's hero and og crops expect.
        dict(name="reading", size=(1280, 800), prefs=look, position=("John", 3, "web"),
             steps="window.scrollTo(0, 0);"),
        # Searching the words of the text.
        dict(name="search", size=(1280, 800), prefs=look, position=("John", 15, "web"),
             steps="""
                __gg.click(document.getElementById('search-toggle'));
                __gg.type(document.getElementById('search-input'), 'love one another');
                await __gg.until(() => document.querySelector('#search-results .search-result'), 'results');
                window.scrollTo(0, 0);
             """),
        # KJV and WEB side by side: reading the KJV, compared with the WEB.
        dict(name="compare", size=(1280, 800),
             prefs={**look, "gospel-getter-compare": "on", "gospel-getter-compare-with": "web"},
             position=("Psalms", 23, "kjv"),
             steps="""
                await __gg.until(() => document.querySelector('.compare-grid'), 'compare grid');
             """ + SCROLL_TO % ("document.querySelector('.chapter-heading')", 40)),
        # A selected range with its card floating under it: Copy, Bookmark
        # and + Add note, the highlights, the range's note and its
        # cross-references, over the verses after it. From verse 4, so the
        # whole card fits below.
        dict(name="select", size=(1280, 800), prefs=look, position=("1 Corinthians", 13, "web"),
             notes=[("1 Corinthians", 13, 4, 7,
                     "Paul describes love by what it does. Read verses 4\u20137 again with "
                     "\u201cChrist\u201d in place of \u201clove\u201d.")],
             steps=SCROLL_TO % ("__gg.verse(4)", 8) + SELECT % (4, 7)
             + "await __gg.until(() => document.querySelector('#selection-card .verse-notes'),"
               " 'the notes on it');"),
    ]


def readme_scenes(theme):
    """The README's screenshots, in `theme`, at their final sizes."""
    look = {**SEEN, "gospel-getter-theme": theme}
    return [
        # John 3 in the KJV with John 2 and 4, the reference in the bar and
        # John's chapters open in the sidebar.
        dict(name="reading-neighboring-chapters", size=(1280, 850), prefs=look,
             position=("John", 3, "kjv"), steps="window.scrollTo(0, 0);"),
        # The same, in the Flowing layout: each chapter as prose.
        dict(name="flowing", size=(1280, 850), prefs={**look, "gospel-getter-layout": "flowing"},
             position=("John", 3, "kjv"), steps="window.scrollTo(0, 0);"),
        # The same in reading mode, with three notes in the margin beside
        # their verses (Aa's "Notes in the margin" is on by default): two
        # on single verses, stacked, and one on 5-8 with its rule. 1320
        # wide, not 1280: the margin needs 17.5rem beside the text (14rem,
        # its gap and 2rem to the edge), and at 1280 there's 274px.
        dict(name="margin-notes", size=(1320, 850), prefs={**look, "gospel-getter-reader-mode": "on"},
             position=("John", 3, "kjv"),
             notes=[
                 ("John", 3, 2, 2,
                  "He comes by night, a ruler of the Jews. By 19:39 he brings myrrh and "
                  "aloes to bury Jesus."),
                 ("John", 3, 3, 3,
                  "\u201cAgain\u201d is anothen, which also means \u201cfrom above\u201d (as in v. 31). "
                  "Nicodemus hears only \u201ca second time\u201d."),
                 ("John", 3, 5, 8,
                  "Water and Spirit together recall Ezekiel 36:25\u201327: clean water, a new heart, "
                  "God's Spirit within. A teacher of Israel should have known it (v. 10)."),
             ],
             steps="""
                window.scrollTo(0, 0);
                await __gg.until(() => window.gospelGetterMargin && window.gospelGetterMargin.shown() === 'margin'
                    && document.querySelectorAll('.chapter-main .margin-notes .margin-note').length === 3, 'the margin');
             """),
        # The same chapter in the Berean Standard Bible, with its footnote
        # markers (John 3:3's is in view).
        dict(name="bsb", size=(1280, 850), prefs=look,
             position=("John", 3, "bsb"), steps="window.scrollTo(0, 0);"),
        # The translation button's popover open over John 3 in the KJV: the
        # translations, a line each, and Compare with its "with" choices.
        dict(name="translation-button", size=(1280, 850), prefs=look,
             position=("John", 3, "kjv"),
             steps="""
                window.scrollTo(0, 0);
                __gg.click(document.getElementById('translation-toggle'));
                await __gg.until(() => !document.getElementById('translation-popover').hidden
                    && document.querySelector('#translation-choices [aria-checked="true"]'), 'the translations');
             """),
        # Searching the words of the text.
        dict(name="search-results", size=(1280, 1000), prefs=look, position=("John", 3, "web"),
             steps="""
                __gg.click(document.getElementById('search-toggle'));
                __gg.type(document.getElementById('search-input'), 'love one another');
                await __gg.until(() => document.querySelector('#search-results .search-result'), 'results');
                window.scrollTo(0, 0);
             """),
        # The same box as a command palette: ">theme" lists the theme
        # commands only, over John 3 in the KJV.
        dict(name="palette", size=(1280, 850), prefs=look, position=("John", 3, "kjv"),
             steps="""
                __gg.click(document.getElementById('search-toggle'));
                __gg.type(document.getElementById('search-input'), '>theme');
                await __gg.until(() => document.querySelectorAll('#search-results [data-command^="theme"]').length >= 9, 'the theme commands');
                window.scrollTo(0, 0);
             """),
        # Romans 14 from verse 21 in compare mode, reading the KJV compared
        # with the WEB: the WEB numbers 24-26, the KJV doesn't.
        dict(name="compare-translations", size=(1280, 750),
             prefs={**look, "gospel-getter-compare": "on", "gospel-getter-compare-with": "web"},
             position=("Romans", 14, "kjv"),
             steps="""
                await __gg.until(() => document.querySelector('.compare-grid'), 'compare grid');
             """ + SCROLL_TO % ("__gg.verse(21, 'kjv')", 16)),
        # John 3:16-18 selected, its card under it with the actions,
        # citation and cross-references.
        dict(name="copy-verse-range", size=(1280, 850), prefs=look, position=("John", 3, "web"),
             steps=SCROLL_TO % ("__gg.verse(12)", 8) + SELECT % (16, 18)),
        # John 3:16-21 selected, its card showing the range's three notes;
        # the one on 3:16 has bold, italics and two references, as links.
        dict(name="notes", size=(1280, 850), prefs=look, position=("John", 3, "web"),
             notes=[
                 ("John", 3, 16, 16,
                  "**Love's measure:** he *gave*. See Rom 5:8 and 1 John 4:9-10."),
                 ("John", 3, 16, 18,
                  "Verse 17 matters as much as 16: he came to save, not to condemn. "
                  "The judgment in 18 is what unbelief already chooses."),
                 ("John", 3, 21, 21,
                  "Coming to the light is something you do, not only something you believe."),
             ],
             steps=SCROLL_TO % ("__gg.verse(16)", 4) + SELECT % (16, 21) + """
                await __gg.until(() => document.querySelectorAll('#selection-card .note-card').length === 3
                    && document.querySelectorAll('#selection-card .note-card strong, #selection-card .note-card em').length === 2
                    && document.querySelectorAll('#selection-card .note-card .note-ref-link').length === 2, 'notes');
             """),
        # Psalm 23 in the KJV, verse 6 selected by its footnote marker, the
        # card under it showing its footnote.
        dict(name="footnotes", size=(1280, 850), prefs=look, position=("Psalms", 23, "kjv"),
             steps=SCROLL_TO % ("__gg.verse(1)", 24) + """
                __gg.click(__gg.verse(6).querySelector('.fn-marker'));
                await __gg.until(() => document.querySelector('#selection-card .selection-footnotes:not([hidden])'), 'the footnotes');
             """),
        # Psalm 23 with verses 1-4 bookmarked, and the Library's Bookmarks tab.
        dict(name="bookmarks", size=(1280, 850), prefs=look, position=("Psalms", 23, "kjv"),
             bookmarks=[("John", 3, 16, 16), ("Romans", 8, 28, 28), ("Psalms", 23, 1, 4)],
             steps="""
                __gg.click(document.getElementById('library-toggle'));
                __gg.click(document.getElementById('library-tab-bookmarks'));
                await __gg.until(() => document.querySelectorAll('#bookmark-list .bookmark-preview').length === 3, 'previews');
             """ + SCROLL_TO % ("document.querySelector('.chapter-main .chapter-heading')", 36)),
    ]


# Every theme, in the app's order (Aa).
THEMES = ["chi-rho", "vellum", "system", "vaporwave", "classic-dark", "classic-light", "matrix",
          "beast-slayer", "hot-pink"]
# The "Make it yours" strip: the same passage in each theme. System has no
# tile of its own: it shows Chi Rho or Vellum, following the computer's
# dark or light setting.
THEME_SCENES = [
    dict(name=f"theme-{theme}", size=(1280, 800), prefs={**SEEN, "gospel-getter-theme": theme},
         position=("Psalms", 23, "web"),
         steps=SCROLL_TO % ("document.querySelector('.chapter-heading')", 40))
    for theme in THEMES if theme != "system"
]


def serve(directory):
    class Quiet(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

    handler = functools.partial(Quiet, directory=directory)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def capture(out_dir, *, readme, theme, only=()):
    """Capture the README's scenes or the site's (named in `only`, or all)
    into `out_dir`."""
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    server = serve(str(ROOT / "dist"))
    with tempfile.TemporaryDirectory() as tmp:
        bridge = Bridge(Path(tmp) / "gospel_getter.db")
        capturer = Capturer(out_dir, f"http://127.0.0.1:{server.server_port}/index.html", bridge,
                            scale=1 if readme else 2)
        scenes = readme_scenes(theme) if readme else feature_scenes(theme) + THEME_SCENES
        for scene in scenes:
            if not only or scene["name"] in only:
                capturer.scene(**scene)
        bridge.proc.stdin.close()
        bridge.proc.wait()
    server.shutdown()


def main():
    args = sys.argv[1:]
    remake = "--all" in args
    if remake:
        args.remove("--all")
    readme = "--readme" in args
    if readme:
        args.remove("--readme")
    theme = "chi-rho"
    if "--theme" in args:
        i = args.index("--theme")
        theme = args[i + 1]
        del args[i:i + 2]
    if theme not in THEMES:
        sys.exit(f"unknown theme {theme!r}; one of {', '.join(THEMES)}")
    if remake:
        if args or readme:
            sys.exit("--all takes no folder or scenes, and makes the README's too")
        with tempfile.TemporaryDirectory() as raw:
            capture(raw, readme=False, theme=theme)
            subprocess.run([str(ROOT / "tools/site/build_images.sh"), raw], check=True)
        capture(ROOT / "screenshots", readme=True, theme=theme)
        return
    capture(args[0] if args else "screenshots-raw", readme=readme, theme=theme,
            only=set(args[1:]))


if __name__ == "__main__":
    main()
