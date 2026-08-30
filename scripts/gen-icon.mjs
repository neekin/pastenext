// Generates a 1024x1024 app icon (clipboard glyph on a gradient rounded square)
// as a valid PNG using only Node built-ins; `pnpm tauri icon` then builds all
// platform icon variants from it.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const S = 1024;
const px = new Uint8Array(S * S * 4);

// Rounded-rect hit test: true when (x,y) is inside the rect whose four corners
// are rounded with radius r.
function inRR(x, y, rx, ry, rw, rh, r) {
  if (x < rx || x >= rx + rw || y < ry || y >= ry + rh) return false;
  const cx = Math.max(rx + r, Math.min(x, rx + rw - r));
  const cy = Math.max(ry + r, Math.min(y, ry + rh - r));
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= r * r;
}

const tab = (x, y) => inRR(x, y, 432, 176, 160, 100, 34);
const body = (x, y) => inRR(x, y, 312, 232, 400, 568, 48);
const line1 = (x, y) => inRR(x, y, 376, 392, 272, 40, 20);
const line2 = (x, y) => inRR(x, y, 376, 472, 272, 40, 20);
const line3 = (x, y) => inRR(x, y, 376, 552, 168, 40, 20);

for (let y = 0; y < S; y++) {
  for (let x = 0; x < S; x++) {
    const i = (y * S + x) * 4;
    let r = 0,
      g = 0,
      b = 0,
      a = 0;
    // 黑白风格:近黑圆角底 + 白色剪贴板 + 深色细线
    if (line1(x, y) || line2(x, y) || line3(x, y)) {
      r = g = 26;
      b = 28;
      a = 255;
    } else if (tab(x, y) || body(x, y)) {
      r = 245;
      g = 245;
      b = 247;
      a = 255;
    } else if (inRR(x, y, 32, 32, 960, 960, 210)) {
      const t = y / S; // 垂直渐变:上浅下深
      r = Math.round(48 + (22 - 48) * t);
      g = Math.round(48 + (22 - 48) * t);
      b = Math.round(50 + (24 - 50) * t);
      a = 255;
    }
    px[i] = r;
    px[i + 1] = g;
    px[i + 2] = b;
    px[i + 3] = a;
  }
}

// --- PNG encoding ---
const crcTable = new Int32Array(256);
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const raw = Buffer.alloc(S * (S * 4 + 1));
for (let y = 0; y < S; y++) {
  raw[y * (S * 4 + 1)] = 0; // filter: none
  Buffer.from(px.buffer, y * S * 4, S * 4).copy(raw, y * (S * 4 + 1) + 1);
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = join(dirname(fileURLToPath(import.meta.url)), "icon-source.png");
writeFileSync(out, png);
console.log("icon written:", out, png.length, "bytes");
