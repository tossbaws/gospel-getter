// The cross references' credit links (OpenBible.info and CC BY 4.0) at the
// end of each chapter. They read as links, with their real href, but the
// page never navigates or opens a window: a click asks the backend to open
// the link's fixed page, by key, in the system browser.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { closeBridge, openApp } from './harness.mjs';

after(closeBridge);

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const credits = (app) => [...app.document.querySelectorAll('.xref-credit a')];

test('each credit link reads as a link and opens through the backend, by key', () =>
    withApp({ book: 'Psalms', chapter: 23 }, async (app) => {
        const links = credits(app);
        assert.deepEqual(
            links.map((a) => [a.textContent, a.dataset.link, a.getAttribute('href')]),
            [
                ['OpenBible.info', 'openbible', 'https://www.openbible.info/labs/cross-references/'],
                ['CC BY 4.0', 'cc-by', 'https://creativecommons.org/licenses/by/4.0/'],
            ],
        );
        for (const link of links) {
            assert.equal(link.getAttribute('target'), null, 'no new window');
            const before = app.calls.length;
            const click = new app.window.MouseEvent('click', { bubbles: true, cancelable: true });
            link.dispatchEvent(click);
            assert.ok(click.defaultPrevented, 'the page doesn’t navigate itself');
            await app.idle(before, 'open_credit_link');
            const opened = app.calls.slice(before).filter((c) => c.cmd === 'open_credit_link');
            // The key only: the backend has the URL.
            assert.deepEqual(JSON.parse(JSON.stringify(opened.map((c) => c.args))), [{ which: link.dataset.link }]);
            assert.equal(app.window.location.href, 'http://tauri.localhost/');
            assert.equal(app.toast().textContent, '');
        }
    }));

test('a credit link that can’t be opened says so, and one the backend doesn’t know is refused', () =>
    withApp({ book: 'Psalms', chapter: 23, failing: { open_credit_link: 'Couldn’t open the link in your browser.' } }, async (app) => {
        const [openbible] = credits(app);
        let before = app.calls.length;
        app.click(openbible);
        await app.idle(before, 'open_credit_link');
        assert.equal(app.toast().textContent, 'Couldn’t open the link in your browser.');

        app.failCommand('open_credit_link', null);
        openbible.dataset.link = 'https://example.com/';
        before = app.calls.length;
        app.click(openbible);
        await app.idle(before, 'open_credit_link');
        assert.equal(app.toast().textContent, 'That isn’t a link Gospel Getter opens.');
    }));
