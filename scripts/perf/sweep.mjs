// Scroll smoothness and stability of pages in headless Edge: one scroll sweep per page and
// round (pages interleaved, so drift on the machine hits all of them alike), then a
// scrollbar-drag simulation and a check of every `content-visibility: auto` placeholder.
// The table shows the median and interquartile range over the rounds.
//
//   node scripts/perf/sweep.mjs [options] name=url [name=url …]
//     --rounds 5      sweeps per page
//     --ms 6000       sweep length cap (the sweep also stops at the bottom)
//     --vps 5         scroll speed in viewports per second
//     --throttle 6    CPU slowdown (1 = none); 6 approximates a mid-range phone
//     --width 1280 --height 1000
//     --json out.json also write every run
//     --credentials target/perf/credentials.txt  sign in first (as seed.mjs's account) at
//                     every page's origin; one host per signed-in server, since cookies
//                     ignore the port (e.g. 127.0.0.1:3021 and localhost:3022)
//     --no-backdrop a,b  pages (by name) swept with `backdrop-filter` off on the sticky bars,
//                     as `scroll-probe.js`'s `noBackdrop` does; give one URL two names to
//                     compare, e.g. `blur=URL plain=URL --no-backdrop plain`
//
// Columns:
//   fps, p95, worst   animation frames per second and frame durations (ms)
//   over33            frames longer than two 60 Hz frames (count per sweep)
//   blocking          long-animation-frame blocking time (ms)
//   heightChanges     times the document height changed during the sweep: a size estimate
//                     being replaced by the real size (the scrollbar thumb jumps)
//   corrections       frames where the scroll position moved by itself (scroll anchoring
//                     compensating for such a change: the content jumps); biggestJump in px
//   dragMedian/Worst  ms from setting scrollTop to 10 %, 20 %, … to the next frame
//   placeholders      elements with `content-visibility: auto`, and how many of them have a
//                     placeholder (`contain-intrinsic-block-size` length plus padding and
//                     borders) more than 2 px off their rendered height; `--json` lists them
import { writeFileSync } from "node:fs";
import { launch, parseArgs, signIn, quantile, summary, sweepExpression } from "./edge.mjs";

const { options, pages } = parseArgs(process.argv.slice(2), {
  rounds: 5,
  ms: 6000,
  vps: 5,
  throttle: 6,
  width: 1280,
  height: 1000,
  json: "",
  credentials: "",
  "no-backdrop": "",
});
const noBackdrop = new Set(options["no-backdrop"].split(",").filter(Boolean));
for (const name of noBackdrop) {
  if (!pages.some((page) => page.name === name)) throw new Error(`--no-backdrop names no page called ${name}`);
}
if (!pages.length) {
  console.error("usage: node scripts/perf/sweep.mjs [--rounds 5] [--ms 6000] [--throttle 6] name=url …");
  process.exit(2);
}

// The scrollbar-drag simulation: jumps to 10 %, 20 %, … of the page, one frame apart.
const DRAG = `(async () => {
  const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
  const sc = document.scrollingElement;
  const max = () => sc.scrollHeight - innerHeight;
  sc.scrollTop = 0; await frame(); await frame();
  const drag = [];
  for (let step = 1; step <= 10; step++) {
    const t0 = performance.now(); sc.scrollTop = (step / 10) * max(); await frame(); drag.push(performance.now() - t0);
  }
  drag.sort((a, b) => a - b);
  return { dragMedian: drag[5], dragWorst: drag.at(-1) };
})()`;

// Run last (and unthrottled, it reads every element's style): forcing every element
// visible to measure it would hand a later sweep remembered sizes (`auto`).
const PLACEHOLDERS = `(() => {
  const px = (value) => parseFloat(value) || 0;
  const name = (el) => el.localName + (el.id ? "#" + el.id : "") + (el.classList.length ? "." + [...el.classList].join(".") : "");
  const contained = [...document.querySelectorAll("*")].filter((el) => getComputedStyle(el).contentVisibility === "auto");
  const placeholders = contained.map((el) => {
    const style = getComputedStyle(el);
    const length = style.containIntrinsicBlockSize.split(" ").find((part) => part.endsWith("px"));
    const chrome = px(style.paddingTop) + px(style.paddingBottom) + px(style.borderTopWidth) + px(style.borderBottomWidth);
    return length === undefined ? null : px(length) + chrome;
  });
  const visible = new CSSStyleSheet();
  visible.replaceSync("* { content-visibility: visible !important; }");
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, visible];
  const off = [];
  contained.forEach((el, i) => {
    if (!el.getClientRects().length) return;
    const rendered = el.getBoundingClientRect().height;
    if (placeholders[i] === null || Math.abs(placeholders[i] - rendered) > 2) off.push({ element: name(el), placeholder: placeholders[i], rendered });
  });
  document.adoptedStyleSheets = document.adoptedStyleSheets.filter((sheet) => sheet !== visible);
  return { placeholders: contained.length, placeholdersOff: off.length, off: off.slice(0, 20), elements: document.getElementsByTagName("*").length };
})()`;

const browser = await launch(options);
if (options.credentials) await signIn(browser, pages, options.credentials);
// A constructed sheet: the page's CSP (`style-src 'self'`) blocks an inline <style>.
const BACKDROP_OFF = `(() => {
  const sheet = new CSSStyleSheet();
  sheet.replaceSync("body > header, .site-subnav { backdrop-filter: none !important; -webkit-backdrop-filter: none !important; }");
  document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  return getComputedStyle(document.querySelector("body > header")).backdropFilter;
})()`;

const measure = async ({ name, url }) => {
  await browser.goto(url);
  if (noBackdrop.has(name)) {
    const filter = await browser.evaluate(BACKDROP_OFF);
    if (filter !== "none") throw new Error(`${name}: backdrop-filter is still ${filter}`);
  }
  await browser.evaluate(`window.__loafs = []; new PerformanceObserver((list) => window.__loafs.push(...list.getEntries())).observe({ type: "long-animation-frame" }); 1`);
  const sweep = await browser.evaluate(sweepExpression(options));
  const blocking = await browser.evaluate(`window.__loafs.reduce((sum, e) => sum + e.blockingDuration, 0)`);
  const drag = await browser.evaluate(DRAG);
  await browser.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  const placeholders = await browser.evaluate(PLACEHOLDERS);
  await browser.send("Emulation.setCPUThrottlingRate", { rate: options.throttle });
  const frames = sweep.frames;
  return {
    fps: frames.length / sweep.seconds,
    p95: quantile(frames, 0.95),
    worst: Math.max(...frames),
    over33: frames.filter((ms) => ms > 33.4).length,
    blocking,
    heightChanges: sweep.heightChanges,
    corrections: sweep.corrections,
    biggestJump: sweep.biggestJump,
    ...drag,
    ...placeholders,
  };
};

const runs = Object.fromEntries(pages.map((page) => [page.name, []]));
for (let round = 1; round <= options.rounds; round++) {
  for (const page of pages) {
    runs[page.name].push(await measure(page));
    process.stderr.write(`round ${round}/${options.rounds}: ${page.name}\n`);
  }
}

const COLUMNS = [
  ["fps", 1],
  ["over33", 0],
  ["p95", 1],
  ["worst", 0],
  ["blocking", 0],
  ["heightChanges", 0],
  ["corrections", 0],
  ["biggestJump", 0],
  ["dragMedian", 0],
  ["dragWorst", 0],
  ["placeholders", 0],
  ["placeholdersOff", 0],
  ["elements", 0],
];
console.log(`median [IQR] of ${options.rounds}: throttle ${options.throttle}×, ${options.vps} viewports/s, ≤ ${options.ms} ms, ${options.width}×${options.height}`);
console.table(Object.fromEntries(pages.map(({ name }) => [name, Object.fromEntries(COLUMNS.map(([key, digits]) => [key, summary(runs[name].map((r) => r[key]), digits)]))])));
for (const { name } of pages) {
  const off = runs[name].at(-1)?.off ?? [];
  if (off.length) console.log(`${name}: placeholders off by more than 2 px (first ${off.length}):`, off);
}
if (options.json) writeFileSync(options.json, JSON.stringify({ options, runs }, null, 2));
process.exit(0);
