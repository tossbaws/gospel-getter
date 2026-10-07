// The version line at the foot of the Settings menu: it shows the version
// the app reports about itself, which must be the one tauri.conf.json
// declares, and it stays quietly empty if the version can't be read.

import { after, test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { closeBridge, openApp } from './harness.mjs';

after(closeBridge);

const VERSION = JSON.parse(
    readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'),
).version;

async function withApp(options, body) {
    const app = await openApp(options);
    try {
        await body(app);
    } finally {
        app.close();
    }
}

const versionLine = (app) => app.document.getElementById('app-version');

test(`the Settings menu ends with "Gospel Getter ${VERSION}"`, () =>
    withApp({}, async (app) => {
        await app.idle(0, 'get_app_version');
        app.openSettings();
        const menu = app.settingsMenu();
        assert.equal(menu.lastElementChild, versionLine(app));
        assert.equal(versionLine(app).textContent, `Gospel Getter ${VERSION}`);
    }));

test('if the version can\'t be read, the line stays empty and nothing else breaks', () =>
    withApp({ failing: { get_app_version: 'simulated failure' } }, async (app) => {
        await app.idle(0, 'get_app_version');
        app.openSettings();
        assert.equal(versionLine(app).textContent, '');
        assert.deepEqual(app.consoleErrors, []);
        assert.equal(app.heading(), 'John 3');
    }));
