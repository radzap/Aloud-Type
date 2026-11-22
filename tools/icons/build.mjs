// Regenerates every icon under assets/icons from docs/logo.svg.
//   cd tools/icons && npm install && npm run build
import { Resvg } from "@resvg/resvg-js";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const out = join(root, "assets", "icons");
const svgDir = join(out, "svg");
for (const d of [svgDir, join(out, "png"), join(out, "tray")]) mkdirSync(d, { recursive: true });

const INDIGO = "#20027f";
const CORAL = "#fc5e53";

// ---- logo geometry, taken from docs/logo.svg so there is one source of truth
const logo = readFileSync(join(root, "docs", "logo.svg"), "utf8");
const bubbleD = /<path[^>]* d="([^"]+)"/.exec(logo)[1];
const bubbleOuter = bubbleD.split(/Z\s*M/)[0] + "Z"; // first subpath = outer outline

const svg = (viewBox, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${viewBox} ${viewBox}" width="${viewBox}" height="${viewBox}">${body}</svg>\n`;

// ---- app icon, detailed (48 px and up): the logo with the bubble filled white
const logoBody = logo.replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "");
const appLarge = svg(2048, `<path fill="#fff" d="${bubbleOuter}"/>${logoBody}`);

// ---- app icon, small (16 to 40 px): solid bubble, five thick white bars
const barW = 150, gap = 90, bars = [460, 760, 1000, 760, 460];
const x0 = 1024 - (bars.length * barW + (bars.length - 1) * gap) / 2;
const smallBars = bars
  .map((h, i) => {
    const mid = i === 2;
    return `<rect x="${x0 + i * (barW + gap)}" y="${1022 - h / 2}" width="${barW}" height="${h}" rx="${barW / 2}" fill="${mid ? CORAL : "#fff"}"/>`;
  })
  .join("");
const appSmall = svg(2048, `<path fill="${INDIGO}" d="${bubbleOuter}"/>${smallBars}`);

// ---- tray glyphs on a 16 unit grid so 16 and 32 px (and 48 px) stay pixel-crisp
const PROFILE = {
  idle: [4, 8, 12, 8, 4],
  listening: [8, 12, 16, 12, 8],
  fault: [4, 8, 12, 8, 4],
};
const THEME = {
  // light taskbar -> dark glyph; dark taskbar -> light glyph
  light: { neutral: "#2b2b3d", listening: "#e8402f", fault: "#d71f26", dim: 0.5 },
  dark: { neutral: "#f1f0fa", listening: "#ff6a5c", fault: "#ff5a5f", dim: 0.55 },
};
function tray(state, theme) {
  const t = THEME[theme];
  const color = state === "listening" ? t.listening : t.neutral;
  const opacity = state === "fault" ? t.dim : 1;
  const rects = PROFILE[state]
    .map((h, i) => `<rect x="${1 + i * 3}" y="${(16 - h) / 2}" width="2" height="${h}" rx="1"/>`)
    .join("");
  let body = `<g fill="${color}" opacity="${opacity}">${rects}</g>`;
  if (state === "fault") {
    // alert badge: filled disc with a white exclamation mark
    body +=
      `<circle cx="11.5" cy="11.5" r="4.5" fill="${t.fault}"/>` +
      `<rect x="11" y="9" width="1" height="3" fill="#fff"/>` +
      `<rect x="11" y="12.8" width="1" height="1" fill="#fff"/>`;
  }
  return svg(16, body);
}

// ---- rasterise + ICO packing (PNG-compressed entries, valid since Vista)
const render = (source, px) =>
  new Resvg(source, { fitTo: { mode: "width", value: px } }).render().asPng();

function ico(entries) {
  // entries: [{ size, png }]
  const head = Buffer.alloc(6);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(entries.length, 4);
  let offset = 6 + 16 * entries.length;
  const dirs = [];
  for (const { size, png } of entries) {
    const d = Buffer.alloc(16);
    d.writeUInt8(size >= 256 ? 0 : size, 0);
    d.writeUInt8(size >= 256 ? 0 : size, 1);
    d.writeUInt16LE(1, 4); // planes
    d.writeUInt16LE(32, 6); // bpp
    d.writeUInt32LE(png.length, 8);
    d.writeUInt32LE(offset, 12);
    offset += png.length;
    dirs.push(d);
  }
  return Buffer.concat([head, ...dirs, ...entries.map((e) => e.png)]);
}

const write = (rel, data) => writeFileSync(join(out, rel), data);

// app icon
write("svg/app.svg", appLarge);
write("svg/app-small.svg", appSmall);
const APP_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const appEntries = APP_SIZES.map((size) => ({
  size,
  png: render(size <= 40 ? appSmall : appLarge, size),
}));
write("app.ico", ico(appEntries));
write("png/app-256.png", render(appLarge, 256));
write("png/app-512.png", render(appLarge, 512));

// tray icons
const TRAY_SIZES = [16, 20, 24, 32, 48];
for (const theme of Object.keys(THEME)) {
  for (const state of Object.keys(PROFILE)) {
    const source = tray(state, theme);
    const name = `tray-${state}-${theme}`;
    write(`svg/${name}.svg`, source);
    const entries = TRAY_SIZES.map((size) => ({ size, png: render(source, size) }));
    write(`tray/${name}.ico`, ico(entries));
  }
}

// preview sheet: every state at real size (1x) and magnified, on both taskbar colours
{
  const bgs = { light: "#f3f3f3", dark: "#202020" };
  const states = Object.keys(PROFILE);
  const cell = 190, rowH = 64;
  let y = 0;
  let body = "";
  for (const theme of ["light", "dark"]) {
    body += `<rect x="0" y="${y}" width="${cell * states.length}" height="${rowH}" fill="${bgs[theme]}"/>`;
    states.forEach((state, i) => {
      const inner = tray(state, theme).replace(/^<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "");
      const cx = i * cell + 20;
      // real taskbar sizes: 16, 24, 32 px
      let x = cx;
      for (const px of [16, 24, 32]) {
        body += `<g transform="translate(${x} ${y + 16}) scale(${px / 16})">${inner}</g>`;
        x += px + 10;
      }
      // magnified 4x
      body += `<g transform="translate(${cx + 110} ${y + 8}) scale(3)">${inner}</g>`;
    });
    y += rowH;
  }
  const sheet = `<svg xmlns="http://www.w3.org/2000/svg" width="${cell * states.length}" height="${y}" viewBox="0 0 ${cell * states.length} ${y}">${body}</svg>`;
  write("preview-tray.png", render(sheet, cell * states.length * 2));
}

console.log("icons written to", out);
