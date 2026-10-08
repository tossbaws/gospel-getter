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

test('the Chi Rho Code LLC mark is the favicon and the header icon, and only there and in About', () => {
    const { document } = pages.find((p) => p.name === 'index.html');
    // Favicons: the flat mark at every size, an .ico holding 16–64, and a
    // 180px Apple touch icon. The small sizes use the bolder flat-small mark.
    const icons = [...document.querySelectorAll('link[rel="icon"], link[rel="apple-touch-icon"]')]
        .map((l) => [l.getAttribute('rel'), l.getAttribute('sizes'), l.getAttribute('href')]);
    assert.deepEqual(icons, [
        ['icon', '16x16 32x32 48x48 64x64', 'assets/brand/favicon.ico'],
        ['icon', '16x16', 'assets/brand/chirho-code-flat-small-16.png'],
        ['icon', '32x32', 'assets/brand/chirho-code-flat-small-32.png'],
        ['icon', '48x48', 'assets/brand/chirho-code-flat-48.png'],
        ['icon', '64x64', 'assets/brand/chirho-code-flat-64.png'],
        ['apple-touch-icon', '180x180', 'assets/brand/chirho-code-flat-180.png'],
    ]);
    // The header brand icon, crisp at 2x.
    const brand = document.querySelector('.brand > img');
    assert.equal(brand.getAttribute('src'), 'assets/brand/chirho-code-flat-48.png');
    assert.match(brand.getAttribute('srcset'), /chirho-code-flat-180\.png 2x/);
    assert.equal(brand.getAttribute('alt'), '');
    // The hero's line above the headline is text only.
    const eyebrow = document.querySelector('.eyebrow');
    assert.equal(eyebrow.querySelectorAll('img').length, 0);
    assert.equal(eyebrow.textContent, 'Gospel Getter · Free Bible reader for Windows & Linux');
    // So is the footer: no mark, no Alpha and Omega, just the credit.
    const footer = document.querySelector('.site-footer');
    assert.equal(footer.querySelectorAll('img, .ornament').length, 0);
    assert.ok(
        [...footer.querySelectorAll('p')].some((p) => p.textContent === 'Gospel Getter is made by Chi Rho Code LLC and released under the MIT license.'),
    );
    // The Gospel Getter app icon is no longer used anywhere on the page.
    assert.equal(document.querySelectorAll('[src*="icon.svg"], [href*="icon.svg"], [href*="favicon.svg"]').length, 0);
    // One mark in the header: the byline is text only.
    assert.equal(document.querySelector('.brand-byline').textContent, 'by Chi Rho Code LLC');
    assert.equal(document.querySelectorAll('.brand-byline img').length, 0);
    // The detailed mark, large and described, in About the maker.
    const mark = document.querySelector('#about .maker-mark img');
    assert.match(mark.getAttribute('src'), /^assets\/brand\/chirho-code-mark-240\.webp$/);
    assert.match(mark.getAttribute('srcset'), /chirho-code-mark-480\.webp 2x/);
    assert.match(mark.getAttribute('alt'), /^Chi Rho Code LLC logo: a gold Chi-Rho in a round stained-glass window/);
    // Alpha and Omega flank the About heading only, hidden from assistive
    // technology.
    const ornaments = [...document.querySelectorAll('.ornament')];
    assert.deepEqual(ornaments.map((o) => o.textContent), ['Α', 'Ω']);
    for (const o of ornaments) {
        assert.equal(o.getAttribute('aria-hidden'), 'true');
        assert.ok(o.closest('#about-title'));
    }
    assert.equal(
        document.getElementById('about-title').textContent.replace(/[ΑΩ]/g, ''),
        'About the maker',
    );
});

test('the feature screenshots are in the Matrix theme; the theme strip keeps all six', () => {
    const { document } = pages.find((p) => p.name === 'index.html');
    for (const img of document.querySelectorAll('.hero-shot img, .feature .shot img')) {
        assert.match(img.getAttribute('alt'), /in the Matrix theme/, img.getAttribute('src'));
        assert.doesNotMatch(img.getAttribute('alt'), /Classic Light/);
    }
    assert.deepEqual(
        [...document.querySelectorAll('.themes figcaption')].map((f) => f.textContent),
        ['Classic Light', 'Classic Dark', 'Vaporwave', 'Matrix', 'Beast Slayer', 'Hot Pink'],
    );
});
