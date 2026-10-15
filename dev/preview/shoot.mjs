// Headless screenshots of the theme preview (DESIGN §10.8), for theme authors and visual QA.
//   node dev/preview/shoot.mjs --theme terminal --scenario boot --times 0,200,400,600,800,1000,1200 --out /tmp/x
//   node dev/preview/shoot.mjs --theme holo --speed slow --scenario all
//   node dev/preview/shoot.mjs --scenario desktop     (re-render assets/desktop.png, print its anchors)
// Starts its own Vite dev server on --port (1421) and drives window.preview (harness.ts).
// Browser: $CHROME, else the newest ~/.cache/ms-playwright/chromium-*/chrome-linux*/chrome, else
// an installed Google Chrome.
import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
// Shot once each, after the answer has finished streaming. "freeze" also drags a selection,
// then "capture" is the answer to that selection.
const ALL = ["lockon", "caption", "card", "rail", "scan", "freeze", "settings", "toast"];

const { values: o } = parseArgs({
  options: {
    theme: { type: "string", default: "terminal" },
    speed: { type: "string", default: "normal" },
    scenario: { type: "string", default: "all" },
    times: { type: "string" },
    out: { type: "string", default: join(here, "out") },
    port: { type: "string", default: "1421" },
  },
});

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const cache = join(homedir(), ".cache/ms-playwright");
  const builds = existsSync(cache) ? readdirSync(cache).filter((d) => /^chromium-\d+$/.test(d)) : [];
  builds.sort((a, b) => Number(b.slice(9)) - Number(a.slice(9)));
  for (const b of builds) {
    for (const d of readdirSync(join(cache, b)).filter((d) => d.startsWith("chrome-linux"))) {
      if (existsSync(join(cache, b, d, "chrome"))) return join(cache, b, d, "chrome");
    }
  }
  return null;
}

const server = await createServer({
  root,
  configFile: join(root, "vite.config.ts"),
  logLevel: "warn",
  server: { port: Number(o.port), strictPort: true },
});
await server.listen();
const base = `http://localhost:${o.port}/dev/preview`;
const exe = findChrome();
const browser = await chromium.launch(exe ? { executablePath: exe } : { channel: "chrome" });

try {
  if (o.scenario === "desktop") await desktop();
  else await harness();
} finally {
  await browser.close();
  await server.close();
}

async function desktop() {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  await page.goto(`${base}/assets/desktop.html`);
  await page.evaluate(() => document.fonts.ready);
  const path = join(here, "assets/desktop.png");
  await page.screenshot({ path });
  const anchors = await page.$$eval("[data-anchor]", (els) =>
    Object.fromEntries(
      els.map((e) => {
        const r = e.getBoundingClientRect();
        const [x, y, w, h] = [r.x, r.y, r.width, r.height].map(Math.round);
        return [e.getAttribute("data-anchor"), { x, y, w, h }];
      }),
    ),
  );
  console.log(path);
  console.log(JSON.stringify(anchors));
}

async function harness() {
  // Wide enough that the monitor is shown at scale 1 (no fit-to-window).
  const page = await browser.newPage({ viewport: { width: 1640, height: 1000 } });
  page.on("console", (m) => ["error", "warning"].includes(m.type()) && console.log(`[page ${m.type()}] ${m.text()}`));
  page.on("pageerror", (e) => console.log(`[page error] ${e.message}`));
  page.on("response", (r) => r.status() >= 400 && console.log(`[page] ${r.status()} ${r.url()}`));
  await page.goto(`${base}/?theme=${o.theme}&speed=${o.speed}&scenario=${o.scenario}&autoplay=0`);
  await page.waitForFunction(() => window.preview);
  await page.evaluate(() => window.preview.ready);
  mkdirSync(o.out, { recursive: true });
  const monitor = page.locator("#monitor");
  const shot = async (name) => {
    const path = join(o.out, `${o.theme}-${name}.png`);
    await monitor.screenshot({ path });
    console.log(path);
  };

  if (o.scenario === "boot" || o.scenario === "boot-bare") {
    const total = await page.evaluate(() => window.preview.bootTotal());
    const times = o.times
      ? o.times.split(",").map(Number)
      : Array.from({ length: 9 }, (_, i) => Math.round((total * i) / 8));
    const lens = page.frame({ name: "lens" });
    // While a frame is posed, hold the lens's JS timers (e.g. the one that unmounts the boot
    // stage after bootMs × speed) and run them when the shot is done.
    await lens.evaluate(() => {
      const setTimeout = window.setTimeout.bind(window);
      window.ogHeld = null;
      window.setTimeout = (fn, ms, ...args) =>
        setTimeout(function tick() {
          if (window.ogHeld) window.ogHeld.push(tick);
          else fn(...args);
        }, ms);
    });
    for (const t of times) {
      await lens.evaluate(() => (window.ogSeen = new Set(document.getAnimations())));
      await page.evaluate((s) => window.preview.play(s, false), o.scenario);
      // Freeze every CSS animation of the new boot stage at t (ms since mount, at this speed).
      const posed = await lens.evaluate(async (t) => {
        const frame = () => new Promise((r) => requestAnimationFrame(r));
        const fresh = () => document.getAnimations().some((a) => !window.ogSeen.has(a));
        for (let i = 0; i < 120 && !fresh(); i++) await frame();
        window.ogHeld = [];
        const anims = document.getAnimations();
        for (const a of anims) {
          a.pause();
          a.currentTime = t;
        }
        return anims.length;
      }, t);
      await page.waitForTimeout(100); // let the downscaled frames decode
      if (!posed) console.warn(`  no CSS animation running at the ${t} ms shot`);
      await shot(`${o.scenario}-${o.speed}-${String(t).padStart(4, "0")}`);
      await lens.evaluate(() => {
        document.getAnimations().forEach((a) => a.play());
        const held = window.ogHeld;
        window.ogHeld = null;
        held.forEach((f) => f());
      });
      // Let the stage finish and unmount, so every shot starts from a fresh mount.
      await page.waitForTimeout(total + 150);
    }
    return;
  }

  for (const name of o.scenario === "all" ? ALL : [o.scenario]) {
    await page.evaluate((n) => window.preview.play(n), name);
    if (name !== "freeze") {
      await shot(name);
      continue;
    }
    // Drag a selection over the terminal error (left button still down), then release.
    const r = await page.evaluate(() => window.preview.selectRect);
    const box = await monitor.boundingBox();
    await page.mouse.move(box.x + r.x, box.y + r.y);
    await page.mouse.down();
    await page.mouse.move(box.x + r.x + r.w, box.y + r.y + r.h, { steps: 8 });
    await shot("freeze");
    await page.mouse.up();
    await page.evaluate(() => window.preview.idle());
    await shot("capture");
  }
}
