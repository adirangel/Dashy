// Regenerates the application icons under backend/icons from backend/icons/icon.svg.
//
// Every platform picks its own format, so all of them are rasterized straight
// from the vector source (no upscaled blur):
//
//   icon.ico        Windows. Carries every size that matters for the taskbar,
//                   Start, Explorer, and the installer. Small entries are stored as
//                   classic 32-bit DIBs, which every consumer (GDI, WiX, old shell
//                   code) reads; the large entries are PNG-encoded, which Windows
//                   Vista and later read natively.
//   icon.icns       macOS. An ICNS container of PNG payloads for each Retina and
//                   non-Retina slot the Finder, Dock, and DMG use.
//   icon.png,       Linux and the Tauri window/tray icon. The sized files follow
//   32x32.png, ...  the WIDTHxHEIGHT[@2x].png names the deb and AppImage bundlers
//                   parse.
//
//   npm run icons:build
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { Resvg } from "@resvg/resvg-js";

const here = dirname(fileURLToPath(import.meta.url));
const svgPath = resolve(here, "../../backend/icons/icon.svg");
const iconsDir = resolve(here, "../../backend/icons");
const icoPath = resolve(iconsDir, "icon.ico");
const icnsPath = resolve(iconsDir, "icon.icns");
const ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const PNG_FROM = 64;
// ICNS slot types and their pixel sizes (Icon Composer names): icp4/5/6 are the
// 16, 32, and 64 point slots; ic07–ic10 are 128–1024 pixels; ic11–ic14 are the
// 2x Retina variants of 16, 32, 128, and 256 points.
const ICNS_SLOTS = [
  ["icp4", 16], ["icp5", 32], ["icp6", 64],
  ["ic07", 128], ["ic08", 256], ["ic09", 512], ["ic10", 1024],
  ["ic11", 32], ["ic12", 64], ["ic13", 256], ["ic14", 512],
];
const PNG_FILES = [
  ["icon.png", 512],
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
];

const svg = readFileSync(svgPath, "utf8");
const renderCache = new Map();

function render(size) {
  if (!renderCache.has(size)) {
    renderCache.set(size, new Resvg(svg, { fitTo: { mode: "width", value: size } }).render());
  }
  return renderCache.get(size);
}

function png(size) {
  return render(size).asPng();
}

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

function buildIco() {
  const images = ICO_SIZES.map((size) => ({
    size,
    data: size >= PNG_FROM ? png(size) : dib(render(size)),
  }));

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
  console.log(`wrote ${icoPath} with ${images.length} sizes: ${ICO_SIZES.join(", ")}`);
}

function buildIcns() {
  // An ICNS file is a sequence of chunks: a 4-byte type, a big-endian 4-byte
  // length that includes the 8-byte chunk header, and the payload (PNG here).
  const chunks = ICNS_SLOTS.map(([type, size]) => {
    const data = png(size);
    const header = Buffer.alloc(8);
    header.write(type, 0, 4, "ascii");
    header.writeUInt32BE(8 + data.length, 4);
    return Buffer.concat([header, data]);
  });
  const body = Buffer.concat(chunks);
  const header = Buffer.alloc(8);
  header.write("icns", 0, 4, "ascii");
  header.writeUInt32BE(8 + body.length, 4);
  writeFileSync(icnsPath, Buffer.concat([header, body]));
  console.log(`wrote ${icnsPath} with ${ICNS_SLOTS.length} slots`);
}

function buildPngs() {
  for (const [name, size] of PNG_FILES) {
    const target = resolve(iconsDir, name);
    writeFileSync(target, png(size));
    console.log(`wrote ${target} (${size}px)`);
  }
}

buildIco();
buildIcns();
buildPngs();
