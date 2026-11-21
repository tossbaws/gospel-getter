// Guards the app icon against drift: every shipped raster (the packaging
// PNG, the Tauri PNGs, every ICO frame, every PNG in the ICNS, the favicon)
// must be what packaging/generate_icons.py made from the one source,
// packaging/icon-source.png, in the style its size calls for (see the
// generator's header).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { decodePng, icnsEntries, icoFrames } from './png.mjs';

const root = new URL('../../', import.meta.url);
const bytes = (path) => readFileSync(new URL(path, root));
const text = (path) => bytes(path).toString('utf8');
const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

// The Captain's C2 Chi-Rho, byte for byte:
// Brand_Attempts/2026-10-05_brand_chi-rho_C2_circuit-reliquary_polish.png.
const SOURCE_SHA256 = 'a38236d8300c2df06a0be3a95b84b0e7f7f58365a9c8bc34ad48346dbfc997c7';
const SMALLEST_DETAILED = 48;
const PNGS = {
    'packaging/icon.png': 512,
    'src-tauri/icons/32x32.png': 32,
    'src-tauri/icons/64x64.png': 64,
    'src-tauri/icons/128x128.png': 128,
    'src-tauri/icons/128x128@2x.png': 256,
    'src-tauri/icons/icon.png': 512,
    'ui/favicon.png': 128,
};
const ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 256];
const ICNS_SIZES = { ic07: 128, ic08: 256, ic09: 512, ic10: 1024, ic11: 32, ic12: 64, ic13: 256, ic14: 512 };

const source = decodePng(bytes('packaging/icon-source.png'));
const master = decodePng(bytes('packaging/icon.png'));

/**
 * Box-averages an image down to n x n, composited over mid grey, reading
 * the source `dy` rows lower (rows past the edge count as grey).
 */
function downsample({ width, height, rgba }, n, dy = 0) {
    const out = new Float64Array(n * n * 3);
    for (let ty = 0; ty < n; ty++) {
        for (let tx = 0; tx < n; tx++) {
            const [x0, x1] = [Math.floor((tx * width) / n), Math.floor(((tx + 1) * width) / n)];
            const [y0, y1] = [Math.floor((ty * height) / n), Math.floor(((ty + 1) * height) / n)];
            const sum = [0, 0, 0];
            for (let y = y0 + dy; y < y1 + dy; y++) {
                for (let x = x0; x < x1; x++) {
                    const i = (y * width + x) * 4;
                    const alpha = y < height ? rgba[i + 3] / 255 : 0;
                    for (let ch = 0; ch < 3; ch++) sum[ch] += (alpha ? rgba[i + ch] * alpha : 0) + 128 * (1 - alpha);
                }
            }
            const count = (x1 - x0) * (y1 - y0);
            for (let ch = 0; ch < 3; ch++) out[(ty * n + tx) * 3 + ch] = sum[ch] / count;
        }
    }
    return out;
}

/** Mean difference (0 to 1) of two n x n downsamples, inside 90% of the circle. */
function difference(a, b, n) {
    const centre = (n - 1) / 2;
    let diff = 0;
    let count = 0;
    for (let y = 0; y < n; y++) {
        for (let x = 0; x < n; x++) {
            if (Math.hypot(x - centre, y - centre) > 0.45 * n) continue;
            for (let ch = 0; ch < 3; ch++) diff += Math.abs(a[(y * n + x) * 3 + ch] - b[(y * n + x) * 3 + ch]);
            count += 3;
        }
    }
    return diff / count / 255;
}

/**
 * Checks one raster: its size; the round window with transparent corners;
 * and its style. From 48 px it is the detailed mark (the 512px master,
 * stained glass and all); below, the flat-small glyph (gold on two dark
 * panes, no cyan glass), which looks nothing like the master.
 */
function assertIconRaster(name, image, size) {
    assert.equal(image.width, size, `${name} width`);
    assert.equal(image.height, size, `${name} height`);
    // Tauri's codegen refuses any PNG icon that isn't RGBA.
    assert.equal(image.colourType, 6, `${name} is RGBA`);

    const centre = (size - 1) / 2;
    let opaque = 0;
    let cyan = 0;
    let gold = 0;
    let bluest = 0;
    for (let y = 0; y < size; y++) {
        for (let x = 0; x < size; x++) {
            const i = (y * size + x) * 4;
            const [r, g, b, a] = image.rgba.subarray(i, i + 4);
            // Past the window's edge, only the downsampling's faint ringing
            // (alpha 3 at most, invisible) reaches; the corners are clear.
            const distance = Math.hypot(x - centre, y - centre);
            if (distance > 0.6 * size) assert.equal(a, 0, `${name}: (${x}, ${y}) is in a corner, so transparent`);
            else if (distance > 0.52 * size) assert.ok(a <= 4, `${name}: (${x}, ${y}) is outside the window (alpha ${a})`);
            if (a < 255) continue;
            opaque++;
            bluest = Math.max(bluest, b);
            if (g > 170 && b > 170 && r < 120) cyan++;
            if (r > 230 && g > 180 && b < 140) gold++;
        }
    }
    const middle = (Math.floor(size / 2) * size + Math.floor(size / 2)) * 4;
    assert.equal(image.rgba[middle + 3], 255, `${name}: the middle is opaque`);
    assert.ok(gold > 0, `${name}: the gold glyph`);

    const off = difference(downsample(image, Math.min(size, 512)), downsample(master, Math.min(size, 512)), Math.min(size, 512));
    if (size >= SMALLEST_DETAILED) {
        assert.ok(off < 0.04, `${name} is the detailed mark (differs from packaging/icon.png by ${off.toFixed(3)})`);
        assert.ok(cyan > 0.04 * opaque, `${name} has the cyan glass (${cyan} of ${opaque} pixels)`);
    } else {
        assert.ok(off > 0.1, `${name} is the flat glyph, not the detailed mark (${off.toFixed(3)})`);
        assert.equal(cyan, 0, `${name}: no cyan glass in the flat glyph`);
        assert.ok(bluest < 160, `${name}: the flat glyph's panes are dark (bluest ${bluest})`);
    }
}

test('the icon source is the C2 Chi-Rho brand mark, as approved', () => {
    assert.equal(sha256(bytes('packaging/icon-source.png')), SOURCE_SHA256);
    assert.deepEqual([source.width, source.height, source.colourType], [2048, 2048, 2], '2048x2048 RGB');
});

test('the detailed mark is the source art, its window centred and nothing else changed', () => {
    // generate_icons.py moves the window up 10 px at 2048 to centre it.
    const off = difference(downsample(source, 64, 10), downsample(master, 64), 64);
    assert.ok(off < 0.005, `packaging/icon.png differs from the source by ${off.toFixed(4)}`);
});

test('the generated icons are up to date with the source', () => {
    const manifest = text('packaging/icons.sha256')
        .trim()
        .split('\n')
        .map((line) => line.split(/ {2}/));
    assert.deepEqual(manifest[0], [sha256(bytes('packaging/icon-source.png')), 'packaging/icon-source.png'],
        'icon-source.png changed since packaging/generate_icons.py last ran');
    assert.deepEqual(
        manifest.slice(1).map(([, path]) => path),
        [...Object.keys(PNGS), 'src-tauri/icons/icon.icns', 'src-tauri/icons/icon.ico'].sort(),
        'the generator writes every shipped icon, and nothing else',
    );
    for (const [hash, path] of manifest.slice(1)) {
        assert.equal(sha256(bytes(path)), hash, `${path} isn't what packaging/generate_icons.py wrote`);
    }
});

test('every PNG icon is the mark at its size', () => {
    for (const [path, size] of Object.entries(PNGS)) assertIconRaster(path, decodePng(bytes(path)), size);
});

test('the Windows ICO has every size Windows asks for, flat below 48 px and detailed from 48', () => {
    const frames = icoFrames(bytes('src-tauri/icons/icon.ico'));
    assert.deepEqual(frames.map((f) => f.size), ICO_SIZES);
    for (const { size, png } of frames) assertIconRaster(`icon.ico ${size}px`, decodePng(png), size);
});

test('the macOS ICNS PNG entries are the mark at each size', () => {
    const entries = icnsEntries(bytes('src-tauri/icons/icon.icns'));
    const pngs = entries.filter((e) => e.data.subarray(1, 4).toString('latin1') === 'PNG');
    assert.deepEqual(pngs.map((e) => e.type).sort(), Object.keys(ICNS_SIZES).sort());
    for (const { type, data } of pngs) assertIconRaster(`icon.icns ${type}`, decodePng(data), ICNS_SIZES[type]);
});

test('a size is the same image in every file that has it', () => {
    const images = new Map();
    const add = (name, image) => {
        const seen = images.get(image.width);
        if (seen) assert.ok(seen.image.rgba.equals(image.rgba), `${name} isn't ${seen.name}'s image`);
        else images.set(image.width, { name, image });
    };
    for (const path of Object.keys(PNGS)) add(path, decodePng(bytes(path)));
    for (const { size, png } of icoFrames(bytes('src-tauri/icons/icon.ico'))) add(`icon.ico ${size}px`, decodePng(png));
    for (const { type, data } of icnsEntries(bytes('src-tauri/icons/icon.icns'))) {
        if (data.subarray(1, 4).toString('latin1') === 'PNG') add(`icon.icns ${type}`, decodePng(data));
    }
});

test('Tauri bundles exactly the generated icon files', () => {
    const conf = JSON.parse(text('src-tauri/tauri.conf.json'));
    assert.deepEqual(conf.bundle.icon, [
        'icons/32x32.png',
        'icons/128x128.png',
        'icons/128x128@2x.png',
        'icons/icon.icns',
        'icons/icon.ico',
    ]);
});

test('the webview favicon is the 128px icon, served from the frontend', () => {
    const html = text('ui/index.html');
    const head = html.slice(0, html.indexOf('</head>'));
    const links = [...head.matchAll(/<link rel="icon"[^>]*>/g)].map((m) => m[0]);
    assert.deepEqual(links, ['<link rel="icon" type="image/png" sizes="128x128" href="favicon.png">']);
    // Tauri serves frontendDist as the app's root: dist/, the page as Trunk
    // built it from ui/ (Trunk.toml), with the favicon copied beside it
    // (`npm test` builds it before the tests run).
    assert.equal(JSON.parse(text('src-tauri/tauri.conf.json')).build.frontendDist, '../dist');
    assert.deepEqual(bytes('ui/favicon.png'), bytes('src-tauri/icons/128x128.png'));
    assert.deepEqual(bytes('dist/favicon.png'), bytes('ui/favicon.png'), 'Trunk copies the favicon as it is');
    for (const stale of ['ui/favicon.svg', 'ui/favicon.ico', 'dist/favicon.svg']) {
        assert.ok(!existsSync(new URL(stale, root)), `no separate, possibly stale ${stale}`);
    }
});

test('nothing points at the old SVG icon or its script', () => {
    const gone = ['packaging/icon.svg', 'packaging/icon.svg.sha256', 'packaging/generate-icons.sh'];
    for (const path of gone) assert.ok(!existsSync(new URL(path, root)), `${path} is gone`);
    // Everything that builds, packages or describes the app; the changelog
    // keeps its history, and this file names them to check for them.
    const self = new URL(import.meta.url).pathname;
    const files = [];
    const walk = (dir) => {
        for (const name of readdirSync(new URL(dir, root))) {
            const path = `${dir}${name}`;
            // Build output, and the Bible texts (19 MB of JSON).
            if (['node_modules', 'target', 'dist'].includes(name) || path === 'src-tauri/data') continue;
            if (statSync(new URL(path, root)).isDirectory()) walk(`${path}/`);
            else if (/\.(rs|mjs|js|html|md|sh|py|json|toml|yml|nsh|txt)$|PKGBUILD$/.test(name)) files.push(path);
        }
    };
    for (const dir of ['ui/', 'ui-leptos/', 'packaging/', 'tests/', 'scripts/', 'src-tauri/', 'tools/', '.github/']) walk(dir);
    files.push('README.md', 'CLAUDE.md', 'Trunk.toml');
    for (const path of files) {
        if (new URL(path, root).pathname === self) continue;
        const body = text(path);
        for (const old of [...gone, 'favicon.svg']) {
            assert.ok(!body.includes(old), `${path} still names ${old}`);
        }
    }
});
