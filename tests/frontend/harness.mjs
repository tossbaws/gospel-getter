// Loads the real ui/index.html into jsdom, runs its scripts, and answers
// its Tauri `invoke` calls with the app's real command logic and a real
// SQLite database, through `src-tauri/examples/frontend_bridge.rs` (built
// by `npm test` before the tests run). Each test file gets one bridge
// process and its own disposable database; each test resets the bookmarks
// and reading position first. The clipboard is a stub that can succeed,
// fail, or be missing; get_reading can be made to fail, or (for escaping
// tests only) have a clearly synthetic DTO swapped in.

import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { JSDOM } from 'jsdom';

const root = new URL('../../', import.meta.url);
const read = (path) => readFileSync(new URL(path, root), 'utf8');

const HTML = read('ui/index.html');

// The bundled translation files: the exact strings the seed step stores.
// Tests compare what the page shows and copies against these, so they're
// independent of the database the bridge serves from.
export const BIBLES = {
    kjv: JSON.parse(read('src-tauri/data/kjv.json')),
    web: JSON.parse(read('src-tauri/data/web.json')),
};
const BOOK_NAMES = BIBLES.kjv.map((b) => b.name);

export const bookId = (name) => {
    const index = BOOK_NAMES.indexOf(name);
    if (index === -1) throw new Error(`no book ${name}`);
    return index + 1;
};

/** A verse's stored text: exactly the string in the bundled data file. */
export function storedVerse(code, book, chapter, verse) {
    const text = BIBLES[code][bookId(book) - 1].chapters[chapter - 1][verse - 1];
    if (typeof text !== 'string') throw new Error(`no ${code} ${book} ${chapter}:${verse}`);
    return text;
}

export function verseCount(code, book, chapter) {
    return BIBLES[code][bookId(book) - 1].chapters[chapter - 1].length;
}

// ---- The bridge process

const BRIDGE = process.env.GOSPEL_GETTER_BRIDGE
    || fileURLToPath(new URL(`src-tauri/target/debug/examples/frontend_bridge${process.platform === 'win32' ? '.exe' : ''}`, root));

let bridge = null;

function startBridge() {
    const dir = mkdtempSync(join(tmpdir(), 'gospel-getter-frontend-'));
    const child = spawn(BRIDGE, [join(dir, 'gospel_getter.db')], { stdio: ['pipe', 'pipe', 'pipe'] });
    const pending = new Map();
    let stderr = '';
    let nextId = 1;
    child.stderr.on('data', (chunk) => {
        stderr = (stderr + chunk).slice(-4000);
    });
    let markReady;
    let markFailed;
    const ready = new Promise((resolve, reject) => {
        markReady = resolve;
        markFailed = reject;
    });
    createInterface({ input: child.stdout }).on('line', (line) => {
        const message = JSON.parse(line);
        if (message.ready) {
            markReady();
            return;
        }
        const request = pending.get(message.id);
        pending.delete(message.id);
        if (!request) return;
        if ('err' in message) {
            // Tauri's invoke rejects with the command's error value itself.
            request.reject(message.err);
        } else {
            request.resolve(message.ok);
        }
    });
    const exited = new Promise((resolve) => child.on('exit', resolve));
    child.on('error', (e) => markFailed(new Error(`couldn't start ${BRIDGE}: ${e.message}`)));
    exited.then((code) => {
        const error = new Error(`frontend bridge exited (${code}):\n${stderr}`);
        markFailed(error);
        for (const request of pending.values()) request.reject(error);
        pending.clear();
    });
    return {
        ready,
        call(cmd, args) {
            const id = nextId++;
            return new Promise((resolve, reject) => {
                pending.set(id, { resolve, reject });
                child.stdin.write(`${JSON.stringify({ id, cmd, args })}\n`);
            });
        },
        async close() {
            child.stdin.end();
            await exited;
            rmSync(dir, { recursive: true, force: true });
        },
    };
}

/** Calls a command on the real backend. */
export async function backend(cmd, args = {}) {
    if (!bridge) bridge = startBridge();
    await bridge.ready;
    return bridge.call(cmd, args);
}

/** Stops this test file's bridge and deletes its database. */
export async function closeBridge() {
    if (bridge) {
        const b = bridge;
        bridge = null;
        await b.close();
    }
}

// ---- The page

/**
 * Boots the page at `book` `chapter` in `translation` (as a saved reading
 * position) and waits for the reading pane. `clipboard` is 'ok', 'reject'
 * or 'missing'; `compare` and `readerMode` preset those display settings;
 * `failing` maps commands to the error they reject with from the start.
 * `fresh` starts with no saved reading position, as a new install does;
 * `storage` presets the page's local storage (see `app.storage()`).
 */
export async function openApp({
    book = 'John',
    chapter = 3,
    translation = 'kjv',
    clipboard = 'ok',
    readerMode = false,
    compare = false,
    keepBookmarks = false,
    failing = {},
    fresh = false,
    storage = {},
    alterReading = (dto) => dto,
} = {}) {
    await backend('__reset', fresh
        ? { translationCode: translation, keepBookmarks }
        : { bookId: bookId(book), chapter, translationCode: translation, keepBookmarks });

    const calls = [];
    const clipboardWrites = [];
    const state = { failReading: false, clipboard, failing: new Map(Object.entries(failing)), held: new Map() };

    const invoke = async (cmd, args = {}) => {
        const call = { cmd, args, done: false };
        calls.push(call);
        try {
            return await answer(cmd, args);
        } finally {
            call.done = true;
        }
    };
    const answer = async (cmd, args) => {
        if (cmd === 'get_reading' && state.failReading) {
            throw 'simulated get_reading failure';
        }
        if (state.failing.has(cmd)) {
            throw state.failing.get(cmd);
        }
        if (state.held.has(cmd)) {
            await state.held.get(cmd);
        }
        const result = await backend(cmd, JSON.parse(JSON.stringify(args)));
        return cmd === 'get_reading' && result ? alterReading(result) : result;
    };

    const errors = [];
    const dom = new JSDOM(HTML, {
        url: 'http://tauri.localhost/',
        runScripts: 'dangerously',
        pretendToBeVisual: true,
        beforeParse(window) {
            for (const [key, value] of Object.entries(storage)) window.localStorage.setItem(key, value);
            if (readerMode) window.localStorage.setItem('gospel-getter-reader-mode', 'on');
            if (compare) window.localStorage.setItem('gospel-getter-compare', 'on');
            window.__TAURI__ = { core: { invoke } };
            const stub = {
                writeText(text) {
                    if (state.clipboard === 'reject') {
                        return Promise.reject(
                            new window.DOMException('The request is not allowed', 'NotAllowedError'),
                        );
                    }
                    clipboardWrites.push(text);
                    return Promise.resolve();
                },
            };
            Object.defineProperty(window.navigator, 'clipboard', {
                configurable: true,
                get: () => (state.clipboard === 'missing' ? undefined : stub),
            });
            window.console.error = (...args) => errors.push(args);
        },
    });
    const { window } = dom;
    const document = window.document;
    const SELECTABLE = '.chapter-main .verse[data-translation]';

    const app = {
        window,
        document,
        calls,
        clipboardWrites,
        consoleErrors: errors,
        /** The page's local storage, to carry into the next openApp (a restart). */
        storage() {
            const out = {};
            for (let i = 0; i < window.localStorage.length; i++) {
                const key = window.localStorage.key(i);
                out[key] = window.localStorage.getItem(key);
            }
            return out;
        },
        setClipboard(mode) {
            state.clipboard = mode;
        },
        failReading(on = true) {
            state.failReading = on;
        },
        /** Makes `cmd` reject with `message` (as a Tauri command error would), or stop. */
        failCommand(cmd, message = `simulated ${cmd} failure`) {
            if (message === null) state.failing.delete(cmd);
            else state.failing.set(cmd, message);
        },
        /** Holds every `cmd` call until the returned function is called. */
        holdCommand(cmd) {
            let release;
            state.held.set(cmd, new Promise((resolve) => { release = resolve; }));
            return () => {
                state.held.delete(cmd);
                release();
            };
        },
        /** The answer the next native file dialog gives: a path, or null (cancelled). */
        async pick(path) {
            await backend('__set_picker', { path });
        },
        settingsMenu() {
            return document.getElementById('settings-menu');
        },
        openSettings() {
            if (app.settingsMenu().hidden) app.click(document.getElementById('settings-toggle'));
        },
        bookmarkItems() {
            return [...document.querySelectorAll('#bookmark-list .bookmark-item')].map((li) => ({
                reference: li.querySelector('.bookmark-ref').textContent,
                preview: li.querySelector('.bookmark-preview').textContent,
                missing: li.querySelector('.bookmark-preview').classList.contains('is-missing'),
                partial: !!li.querySelector('.bookmark-partial'),
                open: li.querySelector('.bookmark-open'),
                remove: li.querySelector('.bookmark-remove'),
            }));
        },
        bookmarkButton() {
            return document.querySelector('.verse-actions .verse-bookmark-btn');
        },
        searchInput() {
            return document.getElementById('search-input');
        },
        searchPanel() {
            return document.getElementById('search-panel');
        },
        searchStatus() {
            return document.getElementById('search-status').textContent;
        },
        searchResults() {
            return [...document.querySelectorAll('#search-results .search-result')];
        },
        /** Types `query` into the search box and waits for its results. */
        async search(query) {
            if (app.searchPanel().hidden) app.key('/');
            const before = app.calls.length;
            app.type(app.searchInput(), query);
            await app.idle(before, 'search');
        },
        /** Waits until a `cmd` call made after call number `since` has finished, and every call since. */
        async idle(since = 0, cmd = null) {
            // Calls can lead to more calls (adding a bookmark reloads the
            // list), so wait until things stay quiet, not just until the
            // first batch finishes.
            for (;;) {
                await until(() => {
                    const recent = calls.slice(since);
                    return (!cmd || recent.some((c) => c.cmd === cmd)) && recent.every((c) => c.done);
                }, `${cmd || 'backend'} calls`);
                const count = calls.length;
                await settle();
                await new Promise((r) => setTimeout(r, 10));
                if (calls.length === count && calls.slice(since).every((c) => c.done)) return;
            }
        },
        isComparing() {
            return !!document.querySelector('.compare-grid');
        },
        compareColumns() {
            return [...document.querySelectorAll('.compare-head')].map((h) => h.textContent.trim());
        },
        /** Verse `n` of the chapter being read (in column `code` if given). */
        verse(n, code) {
            const column = code ? `[data-translation="${code}"]` : '';
            const els = document.querySelectorAll(`${SELECTABLE}${column}[data-verse="${n}"]`);
            if (els.length !== 1) throw new Error(`expected one verse ${n}${code ? ` (${code})` : ''}, found ${els.length}`);
            return els[0];
        },
        sideVerse(which, n) {
            return document.querySelectorAll(`.chapter-side-${which} .verse`)[n - 1];
        },
        selectedVerseNumbers(code) {
            return [...document.querySelectorAll(`${SELECTABLE}.verse-selected`)]
                .filter((el) => !code || el.dataset.translation === code)
                .map((el) => Number(el.dataset.verse));
        },
        bookmarkedVerseNumbers(code) {
            return [...document.querySelectorAll(`${SELECTABLE}.is-bookmarked`)]
                .filter((el) => !code || el.dataset.translation === code)
                .map((el) => Number(el.dataset.verse));
        },
        actions() {
            return document.querySelector('.verse-actions');
        },
        status() {
            return document.querySelector('.verse-actions-status');
        },
        notice() {
            const el = document.getElementById('reader-notice');
            return el.hidden ? '' : el.textContent;
        },
        heading() {
            return document.querySelector('.chapter-heading').textContent;
        },
        click(el, opts = {}) {
            const init = { bubbles: true, cancelable: true, button: 0, ...opts };
            const down = new window.MouseEvent('mousedown', init);
            el.dispatchEvent(down);
            el.dispatchEvent(new window.MouseEvent('mouseup', init));
            el.dispatchEvent(new window.MouseEvent('click', init));
            return down;
        },
        key(key, opts = {}, target = document.body) {
            const event = new window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...opts });
            target.dispatchEvent(event);
            return event;
        },
        type(input, text) {
            input.value = text;
            input.dispatchEvent(new window.Event('input', { bubbles: true }));
        },
        async copyButton() {
            const btn = document.querySelector('.verse-actions .verse-copy-btn');
            if (!btn) throw new Error('no Copy button shown');
            app.click(btn);
            await settle();
        },
        async waitForHeading(text) {
            await until(() => document.querySelector('.chapter-heading')?.textContent === text, `heading ${text}`);
        },
        until,
        close() {
            window.close();
        },
    };

    await until(() => document.getElementById('reading-pane-inner'), 'reading pane');
    // Boot loads the bookmark list, then the highlights; wait for all of
    // it to finish.
    await until(() => document.querySelector('#bookmark-list li'), 'bookmark list');
    await app.idle();
    return app;
}

export async function settle() {
    for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0));
}

export async function until(check, what = 'condition') {
    for (let i = 0; i < 400; i++) {
        if (check()) return;
        await new Promise((r) => setTimeout(r, 5));
    }
    throw new Error(`timed out waiting for ${what}`);
}
