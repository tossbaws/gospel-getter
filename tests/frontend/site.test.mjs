// The website in site/ downloads through GitHub's
// releases/latest/download/<name> links, which always resolve to the newest
// release because the release assets have the same names every time. These
// tests require exactly those five stable links, refuse any versioned
// download link (it would go stale with the next release), and fail until
// the version the site shows matches src-tauri/tauri.conf.json, so a
// version bump can't merge without updating it. They also check that the
// site's own links stay relative (it's served from /gospel-getter/ on
// github.io and later from a domain's root), that in-page anchors and local
// files exist, and that images keep their dimensions and alt text.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { JSDOM } from 'jsdom';

const root = new URL('../../', import.meta.url);
const site = new URL('site/', root);
const VERSION = JSON.parse(readFileSync(new URL('src-tauri/tauri.conf.json', root), 'utf8')).version;
const REPO = 'https://github.com/tossbaws/gospel-getter';
const DOWNLOAD = `${REPO}/releases/latest/download/`;

// The release assets' names, the same in every release from v2.4.0 on.
const ASSETS = [
    'Gospel-Getter_x64-setup.exe',
    'Gospel-Getter_x64_en-US.msi',
    'Gospel-Getter_amd64.AppImage',
    'Gospel-Getter_amd64.deb',
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

test('the homepage links to all five release files by their stable latest-release links', () => {
    const index = pages.find((p) => p.name === 'index.html');
    const hrefs = new Set(links(index));
    const missing = ASSETS.filter((file) => !hrefs.has(DOWNLOAD + file));
    assert.deepEqual(missing, [], `site/index.html has no link to ${missing.map((f) => DOWNLOAD + f).join(', ')}`);
});

test('no download or release link names a version', () => {
    const versioned = allLinks().filter(({ href }) =>
        /\/releases\/(download|tag)\//.test(href)
        || (href.includes('/releases/latest/download/') && /\d+\.\d+\.\d+/.test(href)));
    assert.deepEqual(versioned, [], 'use releases/latest/download/<stable name> and releases/latest instead');
    const unknown = allLinks().filter(({ href }) =>
        href.startsWith(DOWNLOAD) && !ASSETS.includes(href.slice(DOWNLOAD.length)));
    assert.deepEqual(unknown, [], 'links to a release file the release doesn\'t have');
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
