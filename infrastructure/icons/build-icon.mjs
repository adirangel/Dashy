// Regenerates backend/icons/icon.ico from backend/icons/icon.svg.
//
// Windows picks the closest embedded size for the taskbar, Start, Explorer, and
// the installer, so the .ico carries every size that matters, each rasterized
// straight from the vector source (no upscaled blur). Small entries are stored
// as classic 32-bit DIBs, which every consumer (GDI, WiX, old shell code) reads;
// the large entries are PNG-encoded, which Windows Vista and later read natively.
//
//   npm run icons:build
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { Resvg } from "@resvg/resvg-js";

const here = dirname(fileURLToPath(import.meta.url));
const svgPath = resolve(here, "../../backend/icons/icon.svg");
const icoPath = resolve(here, "../../backend/icons/icon.ico");
const SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const PNG_FROM = 64;

function dib(rendered) {
  const { width, height, pixels } = rendered; // RGBA, top-down
  const rowBytes = width * 4;
  const maskRowBytes = Math.ceil(width / 32) * 4;
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0); // BITMAPINFOHEADER size
  header.writeInt32LE(width, 4);
  header.writeInt32LE(height * 2, 8); // XOR + AND mask
  header.writeUInt16LE(1, 12); // planes
  header.writeUInt16LE(32, 14); // bits per pixel
  header.writeUInt32LE(0, 16); // BI_RGB
  header.writeUInt32LE(rowBytes * height + maskRowBytes * height, 20);
  const xor = Buffer.alloc(rowBytes * height);
  for (let y = 0; y < height; y += 1) {
    const src = (height - 1 - y) * rowBytes; // DIB rows are bottom-up
    const dst = y * rowBytes;
    for (let x = 0; x < width; x += 1) {
      const s = src + x * 4;
      const d = dst + x * 4;
      xor[d] = pixels[s + 2]; // B
      xor[d + 1] = pixels[s + 1]; // G
      xor[d + 2] = pixels[s]; // R
      xor[d + 3] = pixels[s + 3]; // A
    }
  }
  const and = Buffer.alloc(maskRowBytes * height); // alpha channel carries transparency
  return Buffer.concat([header, xor, and]);
}

const images = SIZES.map((size) => {
  const rendered = new Resvg(readFileSync(svgPath, "utf8"), { fitTo: { mode: "width", value: size } }).render();
  return { size, data: size >= PNG_FROM ? rendered.asPng() : dib(rendered) };
});

const HEADER = 6;
const ENTRY = 16;
const header = Buffer.alloc(HEADER);
header.writeUInt16LE(0, 0); // reserved
header.writeUInt16LE(1, 2); // type: icon
header.writeUInt16LE(images.length, 4);

const entries = Buffer.alloc(ENTRY * images.length);
let offset = HEADER + entries.length;
images.forEach(({ size, data }, index) => {
  const at = index * ENTRY;
  entries.writeUInt8(size === 256 ? 0 : size, at); // 0 encodes 256
  entries.writeUInt8(size === 256 ? 0 : size, at + 1);
  entries.writeUInt8(0, at + 2); // palette colours (none)
  entries.writeUInt8(0, at + 3); // reserved
  entries.writeUInt16LE(1, at + 4); // colour planes
  entries.writeUInt16LE(32, at + 6); // bits per pixel
  entries.writeUInt32LE(data.length, at + 8);
  entries.writeUInt32LE(offset, at + 12);
  offset += data.length;
});

writeFileSync(icoPath, Buffer.concat([header, entries, ...images.map(({ data }) => data)]));
console.log(`wrote ${icoPath} with ${images.length} sizes: ${SIZES.join(", ")}`);
