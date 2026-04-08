/**
 * Zterm icon generator
 * Generates all required app icons from SVG source (Logo D)
 * Outputs to:
 *   crates/zed/resources/
 *   crates/zed/resources/windows/
 *   crates/explorer_command_injector/resources[-channel]/
 *   crates/auto_update_helper/
 *
 * Usage: node gen-icons.mjs
 * Run from: zterm-logos/ directory (or adjust BASE_DIR)
 */

import { createRequire } from "module";
import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const require = createRequire(import.meta.url);
const sharp = require("C:/Users/zero/AppData/Local/Temp/node_modules/sharp");

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");

// ── SVG source ────────────────────────────────────────────────────────────────
// Read the canonical Logo-D SVG and patch the accent colour per channel.

const BASE_SVG = readFileSync(join(__dirname, "logo-D-minimal.svg"), "utf8");

/**
 * Returns SVG string with the accent colour substituted.
 * The base SVG uses #f97316 as the stable/amber accent; we replace every
 * occurrence of that hex value (case-insensitive) with the channel colour.
 */
function makeSvg(accentColor) {
  // Replace the stable orange (#f97316) with the channel accent.
  // The base SVG also contains a gradient stop #dc2626 that we keep as-is for
  // stable; for other channels we replace both stops with the same accent to
  // give a solid look, which is clearer at small sizes.
  if (accentColor === "#f97316") {
    return BASE_SVG; // stable — use the file verbatim
  }
  return BASE_SVG.replace(/#f97316/gi, accentColor).replace(/#dc2626/gi, accentColor);
}

// Channel accent colours
const CHANNELS = {
  stable: { accent: "#f97316" }, // amber-orange
  preview: { accent: "#3b82f6" }, // blue
  nightly: { accent: "#8b5cf6" }, // purple
  dev: { accent: "#22c55e" }, // green
};

// ── ICO builder (embeds PNG streams) ─────────────────────────────────────────
function buildIco(pngBuffers) {
  const count = pngBuffers.length;
  const headerSize = 6;
  const entrySize = 16;
  const dirSize = headerSize + count * entrySize;

  let offset = dirSize;
  const offsets = [];
  for (const { data } of pngBuffers) {
    offsets.push(offset);
    offset += data.length;
  }

  const buf = Buffer.alloc(offset);
  let pos = 0;

  // ICONDIR
  buf.writeUInt16LE(0, pos);
  pos += 2;
  buf.writeUInt16LE(1, pos);
  pos += 2;
  buf.writeUInt16LE(count, pos);
  pos += 2;

  // ICONDIRENTRY
  for (let i = 0; i < count; i++) {
    const { size, data } = pngBuffers[i];
    const w = size >= 256 ? 0 : size;
    const h = size >= 256 ? 0 : size;
    buf.writeUInt8(w, pos);
    pos += 1;
    buf.writeUInt8(h, pos);
    pos += 1;
    buf.writeUInt8(0, pos);
    pos += 1;
    buf.writeUInt8(0, pos);
    pos += 1;
    buf.writeUInt16LE(1, pos);
    pos += 2;
    buf.writeUInt16LE(32, pos);
    pos += 2;
    buf.writeUInt32LE(data.length, pos);
    pos += 4;
    buf.writeUInt32LE(offsets[i], pos);
    pos += 4;
  }

  for (const { data } of pngBuffers) {
    data.copy(buf, pos);
    pos += data.length;
  }

  return buf;
}

// ── Rasterise SVG → PNG at given size ────────────────────────────────────────
async function svgToPng(svgStr, size) {
  return sharp(Buffer.from(svgStr)).resize(size, size).png().toBuffer();
}

// ── Build an ICO from an SVG string ──────────────────────────────────────────
const ICO_SIZES = [16, 24, 32, 48, 64, 128, 256];

async function svgToIco(svgStr) {
  const pngBufs = await Promise.all(ICO_SIZES.map(async (s) => ({ size: s, data: await svgToPng(svgStr, s) })));
  return buildIco(pngBufs);
}

// ── Main ──────────────────────────────────────────────────────────────────────
async function main() {
  const resDir = join(ROOT, "crates/zed/resources");
  const winDir = join(ROOT, "crates/zed/resources/windows");
  const appxDir = join(ROOT, "crates/explorer_command_injector/resources");
  const autoDir = join(ROOT, "crates/auto_update_helper");

  mkdirSync(winDir, { recursive: true });
  mkdirSync(appxDir, { recursive: true });

  console.log("Generating Zterm icons (Logo D)…\n");

  for (const [channel, { accent }] of Object.entries(CHANNELS)) {
    const svg = makeSvg(accent);
    const suffix = channel === "stable" ? "" : `-${channel}`;

    // ── macOS / Linux PNGs ─────────────────────────────────────────────────
    const png512 = await svgToPng(svg, 512);
    const png1024 = await svgToPng(svg, 1024);

    const pngPath1x = join(resDir, `app-icon${suffix}.png`);
    const pngPath2x = join(resDir, `app-icon${suffix}@2x.png`);
    writeFileSync(pngPath1x, png512);
    writeFileSync(pngPath2x, png1024);
    console.log(`  [PNG] ${pngPath1x}  (512×512)`);
    console.log(`  [PNG] ${pngPath2x}  (1024×1024)`);

    // ── Windows ICO ────────────────────────────────────────────────────────
    const ico = await svgToIco(svg);
    const icoPath = join(winDir, `app-icon${suffix}.ico`);
    writeFileSync(icoPath, ico);
    console.log(`  [ICO] ${icoPath}  (${ICO_SIZES.join(",")}px)`);

    console.log();
  }

  // ── AppX logos (per-channel, 3 sizes each) ─────────────────────────────────
  for (const [channel, { accent }] of Object.entries(CHANNELS)) {
    const svg = makeSvg(accent);
    const suffix = channel === "stable" ? "" : `-${channel}`;
    const dir = join(ROOT, `crates/explorer_command_injector/resources${suffix}`);
    mkdirSync(dir, { recursive: true });

    writeFileSync(join(dir, "logo_150x150.png"), await svgToPng(svg, 150));
    writeFileSync(join(dir, "logo_70x70.png"), await svgToPng(svg, 70));
    writeFileSync(join(dir, "logo_44x44.png"), await svgToPng(svg, 44));
    console.log(`  [AppX] ${dir}/logo_{{44,70,150}}x{{44,70,150}}.png`);
  }
  console.log();

  // ── auto_update_helper ICO (stable colour, same multi-size ICO) ────────────
  const stableSvg = makeSvg(CHANNELS.stable.accent);
  const autoIco = await svgToIco(stableSvg);
  const autoIcoPath = join(autoDir, "app-icon.ico");
  writeFileSync(autoIcoPath, autoIco);
  console.log(`  [ICO] ${autoIcoPath}  (${ICO_SIZES.join(",")}px)`);
  console.log();

  // ── macOS iconset (stable, for iconutil → Document.icns) ──────────────────
  const iconsetDir = join(resDir, "zterm.iconset");
  mkdirSync(iconsetDir, { recursive: true });

  const iconsetSizes = [
    { file: "icon_16x16.png", size: 16 },
    { file: "icon_16x16@2x.png", size: 32 },
    { file: "icon_32x32.png", size: 32 },
    { file: "icon_32x32@2x.png", size: 64 },
    { file: "icon_64x64.png", size: 64 },
    { file: "icon_64x64@2x.png", size: 128 },
    { file: "icon_128x128.png", size: 128 },
    { file: "icon_128x128@2x.png", size: 256 },
    { file: "icon_256x256.png", size: 256 },
    { file: "icon_256x256@2x.png", size: 512 },
    { file: "icon_512x512.png", size: 512 },
    { file: "icon_512x512@2x.png", size: 1024 },
  ];

  for (const { file, size } of iconsetSizes) {
    const png = await svgToPng(stableSvg, size);
    writeFileSync(join(iconsetDir, file), png);
  }
  console.log(`  [iconset] ${iconsetDir}  (${iconsetSizes.length} files)`);
  console.log(`            → run on macOS: cd crates/zed/resources && sh gen-icns.sh`);
  console.log();

  // ── gen-icns.sh helper ────────────────────────────────────────────────────
  writeFileSync(
    join(resDir, "gen-icns.sh"),
    "#!/bin/sh\n" +
      "# Run this on macOS to regenerate Document.icns\n" +
      'cd "$(dirname "$0")"\n' +
      "iconutil -c icns zterm.iconset -o Document.icns\n" +
      'echo "Document.icns generated"\n',
  );

  console.log("Done. All icons written.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
