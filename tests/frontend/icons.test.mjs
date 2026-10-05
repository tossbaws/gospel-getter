// Guards the app icon against drift: every shipped raster (packaging PNG,
// Tauri PNGs, every ICO frame, every PNG in the ICNS, the favicon) must
// come from the one canonical source, packaging/icon.svg, via
// packaging/generate-icons.sh. No dependencies: PNGs are decoded with
// Node's own zlib.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { inflateSync } from 'node:zlib';

const root = new URL('../../', import.meta.url);
const bytes = (path) => readFileSync(new URL(path, root));
const text = (path) => bytes(path).toString('utf8');

const BG = [0x14, 0x08, 0x2c];
const WHITE = [0xff, 0xff, 0xff];

/** Decodes an 8-bit RGBA, non-interlaced PNG into { width, height, rgba }. */
function decodePng(buf) {
    assert.deepEqual([...buf.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], 'PNG signature');
    let width;
    let height;
    const idat = [];
    for (let i = 8; i < buf.length;) {
        const length = buf.readUInt32BE(i);
        const type = buf.toString('latin1', i + 4, i + 8);
        const data = buf.subarray(i + 8, i + 8 + length);
        if (type === 'IHDR') {
            width = data.readUInt32BE(0);
            height = data.readUInt32BE(4);
            assert.equal(data[8], 8, 'bit depth 8');
            assert.equal(data[9], 6, 'colour type RGBA');
            assert.equal(data[12], 0, 'not interlaced');
        } else if (type === 'IDAT') {
            idat.push(data);
        }
        i += 12 + length;
    }
    const raw = inflateSync(Buffer.concat(idat));
    const stride = width * 4;
    const rgba = Buffer.alloc(stride * height);
    for (let y = 0; y < height; y++) {
        const filter = raw[y * (stride + 1)];
        const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
        for (let x = 0; x < stride; x++) {
            const a = x >= 4 ? rgba[y * stride + x - 4] : 0;
            const b = y > 0 ? rgba[(y - 1) * stride + x] : 0;
            const c = x >= 4 && y > 0 ? rgba[(y - 1) * stride + x - 4] : 0;
            let predictor = 0;
            if (filter === 1) predictor = a;
            else if (filter === 2) predictor = b;
            else if (filter === 3) predictor = (a + b) >> 1;
            else if (filter === 4) {
                const p = a + b - c;
                const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
                predictor = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
            }
            rgba[y * stride + x] = (line[x] + predictor) & 255;
        }
    }
    return { width, height, rgba };
}

/** Each frame of an ICO: its declared size and its (PNG) image. */
function icoFrames(buf) {
    const count = buf.readUInt16LE(4);
    return Array.from({ length: count }, (_, k) => {
        const entry = 6 + 16 * k;
        const size = buf[entry] || 256;
        const length = buf.readUInt32LE(entry + 8);
        const offset = buf.readUInt32LE(entry + 12);
        return { size, png: buf.subarray(offset, offset + length) };
    });
}

/** Each entry of an ICNS: its type and payload. */
function icnsEntries(buf) {
    assert.equal(buf.toString('latin1', 0, 4), 'icns');
    const entries = [];
    for (let i = 8; i < buf.length;) {
        const length = buf.readUInt32BE(i + 4);
        entries.push({ type: buf.toString('latin1', i, i + 4), data: buf.subarray(i + 8, i + length) });
        i += length;
    }
    return entries;
}

/** Box-averages an image down to n x n, composited over mid grey. */
function downsample({ width, height, rgba }, n) {
    const out = new Float64Array(n * n * 3);
    for (let ty = 0; ty < n; ty++) {
        for (let tx = 0; tx < n; tx++) {
            const [x0, x1] = [Math.floor((tx * width) / n), Math.floor(((tx + 1) * width) / n)];
            const [y0, y1] = [Math.floor((ty * height) / n), Math.floor(((ty + 1) * height) / n)];
            const sum = [0, 0, 0];
            for (let y = y0; y < y1; y++) {
                for (let x = x0; x < x1; x++) {
                    const i = (y * width + x) * 4;
                    const alpha = rgba[i + 3] / 255;
                    for (let ch = 0; ch < 3; ch++) sum[ch] += rgba[i + ch] * alpha + 128 * (1 - alpha);
                }
            }
            const count = (x1 - x0) * (y1 - y0);
            for (let ch = 0; ch < 3; ch++) out[(ty * n + tx) * 3 + ch] = sum[ch] / count;
        }
    }
    return out;
}

const master = decodePng(bytes('packaging/icon.png'));

/**
 * Checks one raster: its size; that every visible pixel is the background,
 * white, or an antialiased blend of the two (no third colour, like the old
 * cyan rim); that it has the same silhouette as the 512px master; and that
 * the Rho's counter is open where there are enough pixels for one.
 */
function assertIconRaster(name, image, size) {
    assert.equal(image.width, size, `${name} width`);
    assert.equal(image.height, size, `${name} height`);

    let brightest = 0;
    let background = 0;
    for (let i = 0; i < image.rgba.length; i += 4) {
        // Mostly transparent edge pixels (the rounded corners) carry too
        // little colour precision to judge.
        if (image.rgba[i + 3] < 192) continue;
        const px = [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]];
        const t = (px[1] - BG[1]) / (WHITE[1] - BG[1]);
        const expected = BG.map((c, ch) => c + t * (WHITE[ch] - c));
        const off = Math.max(...px.map((c, ch) => Math.abs(c - expected[ch])));
        assert.ok(off <= 12, `${name}: pixel ${i / 4} rgb(${px}) isn't a blend of #14082c and #ffffff`);
        if (image.rgba[i + 3] === 255) brightest = Math.max(brightest, t);
        if (image.rgba[i + 3] === 255 && t < 0.03) background++;
    }
    // Below 32px the strokes are thinner than a pixel, so antialiasing
    // never quite reaches pure white.
    assert.ok(brightest > (size >= 32 ? 0.97 : 0.6), `${name}: the emblem is white (brightest ${brightest.toFixed(2)})`);
    assert.ok(background > 0, `${name} has the dark background`);
    assert.ok(image.rgba[3] < 255, `${name}: the top-left corner is rounded off`);

    const n = Math.min(size, 512);
    const [a, b] = [downsample(image, n), downsample(master, n)];
    let diff = 0;
    for (let i = 0; i < a.length; i++) diff += Math.abs(a[i] - b[i]);
    const mean = diff / a.length / 255;
    assert.ok(mean < 0.04, `${name} differs from packaging/icon.png (mean ${mean.toFixed(3)})`);

    // The counter's centre, (146.75, 60.55) in the SVG's 256-unit space.
    if (size >= 32) {
        const [x, y] = [Math.floor((146.75 * size) / 256), Math.floor((60.55 * size) / 256)];
        const i = (y * size + x) * 4;
        assert.ok(image.rgba[i + 1] < 64, `${name}: the Rho's counter is filled in at (${x}, ${y})`);
    }
}

test('the icon source is the approved white-on-dark Chi-Rho', () => {
    const svg = text('packaging/icon.svg');
    assert.deepEqual([...new Set(svg.match(/#[0-9a-f]{6}\b/gi))].sort(), ['#14082c', '#ffffff']);
    assert.match(svg, /<rect width="256" height="256" rx="48" fill="#14082c"\/>/);
    assert.match(svg, /<path fill="#ffffff" d="M 9\.2814416,182\.9088 /);
    assert.ok(!/stroke/.test(svg), 'no outline');
});

test('the generated icons are up to date with the source', () => {
    const recorded = text('packaging/icon.svg.sha256').split(/\s+/)[0];
    const actual = createHash('sha256').update(bytes('packaging/icon.svg')).digest('hex');
    assert.equal(recorded, actual, 'icon.svg changed since packaging/generate-icons.sh last ran');
});

test('every PNG icon matches the source', () => {
    assertIconRaster('packaging/icon.png', master, 512);
    for (const [file, size] of [
        ['32x32.png', 32],
        ['64x64.png', 64],
        ['128x128.png', 128],
        ['128x128@2x.png', 256],
        ['icon.png', 512],
    ]) {
        assertIconRaster(`src-tauri/icons/${file}`, decodePng(bytes(`src-tauri/icons/${file}`)), size);
    }
});

test('the Windows ICO has every size, all matching the source', () => {
    const frames = icoFrames(bytes('src-tauri/icons/icon.ico'));
    assert.deepEqual(frames.map((f) => f.size).sort((a, b) => a - b), [16, 24, 32, 48, 64, 256]);
    for (const { size, png } of frames) assertIconRaster(`icon.ico ${size}px`, decodePng(png), size);
});

test('the macOS ICNS PNG entries all match the source', () => {
    const expected = { ic07: 128, ic08: 256, ic09: 512, ic10: 1024, ic11: 32, ic12: 64, ic13: 256, ic14: 512 };
    const entries = icnsEntries(bytes('src-tauri/icons/icon.icns'));
    const pngs = entries.filter((e) => e.data.subarray(1, 4).toString('latin1') === 'PNG');
    assert.deepEqual(pngs.map((e) => e.type).sort(), Object.keys(expected).sort());
    for (const { type, data } of pngs) assertIconRaster(`icon.icns ${type}`, decodePng(data), expected[type]);
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

test('the webview favicon is the same art, served from the frontend', () => {
    const html = text('ui/index.html');
    const head = html.slice(0, html.indexOf('</head>'));
    const links = [...head.matchAll(/<link rel="icon"[^>]*>/g)].map((m) => m[0]);
    assert.deepEqual(links, [
        '<link rel="icon" type="image/svg+xml" href="favicon.svg">',
        '<link rel="icon" type="image/png" sizes="64x64" href="favicon.png">',
    ]);
    // Tauri serves frontendDist (../ui) as the app's root.
    assert.equal(JSON.parse(text('src-tauri/tauri.conf.json')).build.frontendDist, '../ui');
    assert.deepEqual(bytes('ui/favicon.svg'), bytes('packaging/icon.svg'));
    assertIconRaster('ui/favicon.png', decodePng(bytes('ui/favicon.png')), 64);
    assert.ok(!existsSync(new URL('ui/favicon.ico', root)), 'no separate, possibly stale favicon.ico');
});
