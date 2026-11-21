// Reads the app's icon files with no dependencies: PNGs are decoded with
// Node's own zlib.

import assert from 'node:assert/strict';
import { inflateSync } from 'node:zlib';

const CHANNELS = { 2: 3, 6: 4 };

/**
 * Decodes an 8-bit RGB or RGBA, non-interlaced PNG into
 * { width, height, colourType, rgba }, RGB widened to opaque RGBA.
 */
export function decodePng(buf) {
    assert.deepEqual([...buf.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], 'PNG signature');
    let width;
    let height;
    let colourType;
    const idat = [];
    for (let i = 8; i < buf.length;) {
        const length = buf.readUInt32BE(i);
        const type = buf.toString('latin1', i + 4, i + 8);
        const data = buf.subarray(i + 8, i + 8 + length);
        if (type === 'IHDR') {
            width = data.readUInt32BE(0);
            height = data.readUInt32BE(4);
            colourType = data[9];
            assert.equal(data[8], 8, 'bit depth 8');
            assert.ok(colourType in CHANNELS, `colour type ${colourType} is RGB or RGBA`);
            assert.equal(data[12], 0, 'not interlaced');
        } else if (type === 'IDAT') {
            idat.push(data);
        }
        i += 12 + length;
    }
    const bpp = CHANNELS[colourType];
    const raw = inflateSync(Buffer.concat(idat));
    const stride = width * bpp;
    const pixels = Buffer.alloc(stride * height);
    for (let y = 0; y < height; y++) {
        const filter = raw[y * (stride + 1)];
        const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
        const row = y * stride;
        for (let x = 0; x < stride; x++) {
            const a = x >= bpp ? pixels[row + x - bpp] : 0;
            const b = y > 0 ? pixels[row - stride + x] : 0;
            const c = x >= bpp && y > 0 ? pixels[row - stride + x - bpp] : 0;
            let predictor = 0;
            if (filter === 1) predictor = a;
            else if (filter === 2) predictor = b;
            else if (filter === 3) predictor = (a + b) >> 1;
            else if (filter === 4) {
                const p = a + b - c;
                const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
                predictor = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
            }
            pixels[row + x] = (line[x] + predictor) & 255;
        }
    }
    if (bpp === 4) return { width, height, colourType, rgba: pixels };
    const rgba = Buffer.alloc(width * height * 4, 255);
    for (let i = 0, j = 0; i < pixels.length; i += 3, j += 4) pixels.copy(rgba, j, i, i + 3);
    return { width, height, colourType, rgba };
}

/** Each frame of an ICO: its declared size and its (PNG) image. */
export function icoFrames(buf) {
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
export function icnsEntries(buf) {
    assert.equal(buf.toString('latin1', 0, 4), 'icns');
    const entries = [];
    for (let i = 8; i < buf.length;) {
        const length = buf.readUInt32BE(i + 4);
        entries.push({ type: buf.toString('latin1', i, i + 4), data: buf.subarray(i + 8, i + length) });
        i += length;
    }
    return entries;
}
