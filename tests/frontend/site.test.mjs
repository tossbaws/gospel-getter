// The website in site/ links straight to one release's installers, so it
// has to change with every version bump. These tests read the app version
// from src-tauri/tauri.conf.json and fail until every download link and
// every version the site shows match it. They also check that the site's
// own links stay relative (it's served from /gospel-getter/ on github.io
// and later from a domain's root), that in-page anchors and local files
// exist, and that images keep their dimensions and alt text.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { JSDOM } from 'jsdom';

const root = new URL('../../', import.meta.url);
const site = new URL('site/', root);
const VERSION = JSON.parse(readFileSync(new URL('src-tauri/tauri.conf.json', root), 'utf8')).version;
const REPO = 'https://github.com/tossbaws/gospel-getter';
const DOWNLOAD = `${REPO}/releases/download/v${VERSION}/`;

const ASSETS = [
    `Gospel-Getter_${VERSION}_x64-setup.exe`,
    `Gospel-Getter_${VERSION}_x64_en-US.msi`,
    `Gospel-Getter_${VERSION}_amd64.AppImage`,
    `Gospel-Getter_${VERSION}_amd64.deb`,
    'SHA256SUMS',
];

const pages = readdirSync(site)
    .filter((name) => name.endsWith('.html'))
    .map((name) => {
        const html = readFileSync(new URL(name, site), 'utf8');
        return { name, html, document: new JSDOM(html).window.document };
    });

const links = (page) => [...page.document.querySelectorAll('a[href]')].map((a) => a.getAttribute('href'));
const allLinks = () => pages.flatMap((page) => links(page).map((href) => ({ page: page.name, href })));

test('the site has a homepage', () => {
    assert.ok(pages.some((p) => p.name === 'index.html'), 'site/index.html is missing');
});

test(`the homepage links to all five v${VERSION} release files`, () => {
    const index = pages.find((p) => p.name === 'index.html');
    const hrefs = new Set(links(index));
    const missing = ASSETS.filter((file) => !hrefs.has(DOWNLOAD + file));
    assert.deepEqual(missing, [], `site/index.html has no link to ${missing.map((f) => DOWNLOAD + f).join(', ')}`);
});

test(`every release link points at v${VERSION}`, () => {
    const wrong = allLinks().filter(({ href }) => {
        const download = href.match(/\/releases\/download\/([^/]+)\/(.*)$/);
        if (download) {
            const fileVersion = download[2].match(/_(\d+\.\d+\.\d+)_/);
            return download[1] !== `v${VERSION}` || (fileVersion && fileVersion[1] !== VERSION);
        }
        const tag = href.match(/\/releases\/tag\/([^/?#]+)/);
        return tag ? tag[1] !== `v${VERSION}` : false;
    });
    assert.deepEqual(wrong, [], `links to another version (the app is ${VERSION}); update site/ for this release`);
});

test(`every version number the site shows is ${VERSION}`, () => {
    for (const page of pages) {
        const text = page.document.body.textContent;
        const versions = [...text.matchAll(/\b\d+\.\d+\.\d+\b/g)].map((m) => m[0]);
        const stale = [...new Set(versions.filter((v) => v !== VERSION))];
        assert.deepEqual(stale, [], `${page.name} shows version ${stale.join(', ')}, but the app is ${VERSION}`);
        if (page.name === 'index.html') {
            assert.ok(versions.includes(VERSION), `${page.name} doesn't show the version ${VERSION}`);
        }
    }
});

test('internal links and files are relative, and exist', () => {
    for (const page of pages) {
        const refs = [...page.document.querySelectorAll('[href], [src], [srcset]')].flatMap((el) => [
            el.getAttribute('href'),
            el.getAttribute('src'),
            ...(el.getAttribute('srcset') || '').split(',').map((s) => s.trim().split(/\s+/)[0]),
        ]).filter(Boolean);
        for (const ref of refs) {
            if (/^(https?:|mailto:)/.test(ref) || ref.startsWith('#')) continue;
            assert.ok(!ref.startsWith('/'), `${page.name}: ${ref} is root-relative and would break under /gospel-getter/`);
            const path = ref.split(/[?#]/)[0];
            assert.ok(existsSync(new URL(path, site)), `${page.name}: ${ref} doesn't exist in site/`);
        }
    }
});

test('every in-page anchor has a target', () => {
    for (const page of pages) {
        const anchors = links(page).filter((href) => href.startsWith('#') && href.length > 1);
        const missing = anchors.filter((href) => !page.document.getElementById(href.slice(1)));
        assert.deepEqual([...new Set(missing)], [], `${page.name} links to anchors that don't exist`);
    }
});

test('images have alt text and dimensions', () => {
    for (const page of pages) {
        for (const img of page.document.querySelectorAll('img')) {
            const src = img.getAttribute('src');
            assert.ok(img.hasAttribute('alt'), `${page.name}: ${src} has no alt attribute`);
            assert.ok(img.getAttribute('width') && img.getAttribute('height'), `${page.name}: ${src} has no width/height`);
        }
    }
});
