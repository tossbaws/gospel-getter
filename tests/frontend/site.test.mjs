// The website in site/ downloads through GitHub's
// releases/latest/download/<name> links, which always resolve to the newest
// release because the release assets have the same names every time. These
// tests require exactly those five stable links, refuse any versioned
// download link (it would go stale with the next release), and fail until
// the version the site shows matches src-tauri/tauri.conf.json, so a
// version bump can't merge without updating it. They also check that the
// site's own links stay relative (it's served from /gospel-getter/ on
// github.io and later from a domain's root), that in-page anchors and local
// files exist, and that images keep their dimensions and alt text. The
// Support section's tips are plain links to Stripe Payment Links, the
// Stripe customer portal and Ko-fi: nothing from Stripe or Ko-fi loads on
// the site, and it never says "donate" or "donation".

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

test('the feature screenshots are in the default theme, Chi Rho; the theme strip has every theme but System, Chi Rho first', () => {
    const { document } = pages.find((p) => p.name === 'index.html');
    for (const img of document.querySelectorAll('.hero-shot img, .feature .shot img')) {
        assert.match(img.getAttribute('alt'), /in the Chi Rho theme/, img.getAttribute('src'));
        assert.doesNotMatch(img.getAttribute('alt'), /Matrix|Classic Light/);
    }
    // System has no tile: it shows Vellum or Chi Rho, as the computer is set.
    const tiles = [...document.querySelectorAll('.themes figure')];
    assert.deepEqual(
        tiles.map((f) => f.querySelector('figcaption').textContent),
        ['Chi Rho', 'Vellum', 'Classic Light', 'Classic Dark', 'Vaporwave', 'Matrix', 'Beast Slayer', 'Hot Pink'],
    );
    for (const f of tiles) {
        const img = f.querySelector('img');
        const slug = f.querySelector('figcaption').textContent.toLowerCase().replace(/ /g, '-');
        assert.equal(img.getAttribute('src'), `assets/theme-${slug}.webp`);
        assert.deepEqual([img.getAttribute('width'), img.getAttribute('height')], ['720', '435']);
        assert.match(img.getAttribute('alt'), new RegExp(`in the ${f.querySelector('figcaption').textContent} theme`));
    }
});

test('the Support section links to four Payment Links, the customer portal, Ko-fi and the refund email', () => {
    const { document } = pages.find((p) => p.name === 'index.html');
    const support = document.getElementById('support');
    assert.ok(support, 'site/index.html has no #support section');
    assert.equal(support.getAttribute('aria-labelledby'), 'support-title');
    assert.equal(document.getElementById('support-title').textContent, 'Support Gospel Getter');
    const hrefs = [...support.querySelectorAll('a[href]')].map((a) => a.getAttribute('href'));
    // One one-time tip and three monthly tiers, each its own Payment Link.
    const paymentLinks = hrefs.filter((href) => href.startsWith('https://buy.stripe.com/'));
    assert.equal(paymentLinks.length, 4, `#support has ${paymentLinks.length} Payment Links, not 4`);
    assert.equal(new Set(paymentLinks).size, 4, '#support repeats a Payment Link');
    assert.ok(
        hrefs.some((href) => href.startsWith('https://billing.stripe.com/p/login/')),
        '#support has no link to the Stripe customer portal',
    );
    assert.ok(hrefs.includes('https://ko-fi.com/crcllc'), '#support has no link to https://ko-fi.com/crcllc');
    // A tip is refunded on request.
    assert.ok(hrefs.includes('mailto:help@chirhocode.com'), '#support has no link to mailto:help@chirhocode.com');
});

test('no site page links to a Stripe sandbox (test-mode) link', () => {
    for (const page of pages) {
        for (const sandbox of ['buy.stripe.com/test_', 'billing.stripe.com/p/login/test_']) {
            assert.ok(!page.html.includes(sandbox), `${page.name} still has a ${sandbox} link; use the live one`);
        }
    }
});

test('the site runs no scripts and loads nothing from Stripe or Ko-fi', () => {
    const thirdParty = (url) => {
        try {
            const { hostname } = new URL(url);
            return ['stripe.com', 'ko-fi.com'].some((host) => hostname === host || hostname.endsWith(`.${host}`));
        } catch {
            return false; // a relative URL
        }
    };
    for (const page of pages) {
        assert.equal(page.document.querySelectorAll('script').length, 0, `${page.name} has a <script>`);
        const loaded = [...page.document.querySelectorAll('[src], [srcset], link[href]')].flatMap((el) => [
            el.getAttribute('src'),
            el.tagName === 'LINK' ? el.getAttribute('href') : null,
            ...(el.getAttribute('srcset') || '').split(',').map((s) => s.trim().split(/\s+/)[0]),
        ]).filter(Boolean);
        assert.deepEqual(loaded.filter(thirdParty), [], `${page.name} loads something from Stripe or Ko-fi`);
    }
});

test('no site page says "donate" or "donation"', () => {
    for (const page of pages) {
        const found = page.html.match(/donat(e|ion)/gi);
        assert.equal(found, null, `${page.name} says ${found?.join(', ')}; the wording is "Support" or "Tip"`);
    }
});

test('the footer links to Support; the main nav and the hero don\'t', () => {
    const { document } = pages.find((p) => p.name === 'index.html');
    const link = document.querySelector('.site-footer .footer-links a[href="#support"]');
    assert.ok(link, 'the footer has no link to #support');
    assert.equal(link.textContent, 'Support');
    assert.equal(document.querySelectorAll('.site-nav a[href="#support"], .hero a[href="#support"]').length, 0);
});
