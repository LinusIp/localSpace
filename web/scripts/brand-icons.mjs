// The icon set, from the brand mark alone (web/public/brand/README.md):
// the desktop's window and installer icons under crates/localspace-shell/icons,
// and the web client's favicon. The lockup is never an icon.
//
//   node scripts/brand-icons.mjs
//
// Renders web/public/brand/localspace-mark.svg with the Chromium the
// end-to-end walk already uses (Edge or Chrome, through playwright-core),
// one bitmap per size, and packs the Windows .ico itself: an .ico is a
// small directory of PNGs, so no image library is needed.

import { chromium } from "playwright-core";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(web, "..");
const mark = join(web, "public", "brand", "localspace-mark.svg");
const icons = join(root, "crates", "localspace-shell", "icons");

/** Tauri's icon files and the size each is rendered at. */
const TAURI = [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
  ["icon.png", 512],
];
/** The sizes packed into icon.ico: Explorer, the taskbar, the title bar, the Start menu. */
const ICO_SIZES = [16, 24, 32, 48, 64, 128, 256];

async function launch() {
  let last;
  for (const channel of ["msedge", "chrome", undefined]) {
    try {
      return await chromium.launch({ channel, headless: true });
    } catch (err) {
      last = err;
    }
  }
  throw last;
}

/** A transparent PNG of the mark, `size` pixels square. */
async function render(browser, svg, size) {
  const context = await browser.newContext({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const data = `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
  await page.setContent(
    `<!doctype html><html><head><style>html,body{margin:0;padding:0;background:transparent}img{display:block;width:${size}px;height:${size}px}</style></head><body><img src="${data}" alt=""></body></html>`,
  );
  await page.waitForFunction(() => document.images[0]?.complete);
  const png = await page.screenshot({ omitBackground: true, type: "png", clip: { x: 0, y: 0, width: size, height: size } });
  await context.close();
  return png;
}

/** Pack PNGs into a Windows icon: a header, one directory entry per image, then the images. */
function ico(entries) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // 1 = icon
  header.writeUInt16LE(entries.length, 4);
  const directory = Buffer.alloc(16 * entries.length);
  let offset = header.length + directory.length;
  entries.forEach(([size, png], i) => {
    const at = i * 16;
    directory.writeUInt8(size >= 256 ? 0 : size, at); // 0 means 256
    directory.writeUInt8(size >= 256 ? 0 : size, at + 1);
    directory.writeUInt8(0, at + 2); // colours in palette: none
    directory.writeUInt8(0, at + 3); // reserved
    directory.writeUInt16LE(1, at + 4); // colour planes
    directory.writeUInt16LE(32, at + 6); // bits per pixel
    directory.writeUInt32LE(png.length, at + 8);
    directory.writeUInt32LE(offset, at + 12);
    offset += png.length;
  });
  return Buffer.concat([header, directory, ...entries.map(([, png]) => png)]);
}

/** The favicon: the mark, switching to the light ink when the browser is dark. */
function favicon(svg) {
  const style = `<style>@media (prefers-color-scheme: dark){ g[stroke]{stroke:#FAFAFA} g[fill="#1C1E20"]{fill:#FAFAFA} }</style>`;
  return svg.replace("<title>localSpace</title>", `<title>localSpace</title>${style}`);
}

const svg = readFileSync(mark, "utf8");
const browser = await launch();
try {
  mkdirSync(icons, { recursive: true });
  const rendered = new Map();
  for (const size of new Set([...TAURI.map(([, s]) => s), ...ICO_SIZES])) rendered.set(size, await render(browser, svg, size));
  for (const [name, size] of TAURI) {
    writeFileSync(join(icons, name), rendered.get(size));
    console.log(`${name}  ${size}px  ${rendered.get(size).length} B`);
  }
  const packed = ico(ICO_SIZES.map((s) => [s, rendered.get(s)]));
  writeFileSync(join(icons, "icon.ico"), packed);
  console.log(`icon.ico  ${ICO_SIZES.join(",")}px  ${packed.length} B`);
  writeFileSync(join(web, "public", "favicon.svg"), favicon(svg));
  console.log("favicon.svg  the mark, light ink in dark browsers");
} finally {
  await browser.close();
}
