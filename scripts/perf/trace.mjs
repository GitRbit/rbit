// Where the renderer's main thread spends its time while a page scrolls: a DevTools trace
// of one scroll sweep per page and round, summed per phase (style, layout, pre-paint,
// paint, intersection observers, which include `content-visibility: auto` relevance
// checks, …). Pages are visited in turn each round, so drift on the machine hits all of
// them alike; the table shows the median and interquartile range over the rounds.
//
//   node scripts/perf/trace.mjs [options] name=url [name=url …]
//     --rounds 5      sweeps per page
//     --ms 6000       sweep length cap (the sweep also stops at the bottom)
//     --vps 5         scroll speed in viewports per second
//     --throttle 6    CPU slowdown (1 = none); 6 approximates a mid-range phone
//     --width 1280 --height 1000
//     --json out.json also write every run
//
// e.g. node scripts/perf/trace.mjs --rounds 5 --throttle 6 \
//        old=http://localhost:3032/perf/perf-fixture/compare/main...big-change \
//        new=http://localhost:3031/perf/perf-fixture/compare/main...big-change
//
// Durations are milliseconds of complete ("X") trace events on the renderer's main thread.
// RunTask is the top-level task, so it is the total; the phases nest inside it (and Layout
// can nest in other phases), so they do not add up to it. Compare like with like. A second
// table divides by the frames drawn: a page that keeps up draws more of them in a sweep.
import { writeFileSync } from "node:fs";
import { launch, parseArgs, summary, sweepExpression } from "./edge.mjs";

const { options, pages } = parseArgs(process.argv.slice(2), {
  rounds: 5,
  ms: 6000,
  vps: 5,
  throttle: 6,
  width: 1280,
  height: 1000,
  json: "",
});
if (!pages.length) {
  console.error("usage: node scripts/perf/trace.mjs [--rounds 5] [--ms 6000] [--throttle 6] name=url …");
  process.exit(2);
}

const PHASES = {
  RunTask: "RunTask",
  UpdateLayoutTree: "UpdateLayoutTree",
  Layout: "Layout",
  PrePaint: "PrePaint",
  Paint: "Paint",
  Layerize: "Layerize",
  "IntersectionObserverController::computeIntersections": "computeIntersections",
  HitTest: "HitTest",
  FunctionCall: "FunctionCall",
  ParseHTML: "ParseHTML",
};

const browser = await launch(options);
const traceOnce = async (url) => {
  await browser.goto(url);
  const events = [];
  const complete = new Promise((resolve) => {
    const off = browser.on((message) => {
      if (message.method === "Tracing.dataCollected") events.push(...message.params.value);
      if (message.method === "Tracing.tracingComplete") {
        off();
        resolve();
      }
    });
  });
  await browser.send("Tracing.start", {
    traceConfig: {
      includedCategories: ["devtools.timeline", "disabled-by-default-devtools.timeline", "blink", "cc"],
      recordMode: "recordAsMuchAsPossible",
    },
    transferMode: "ReportEvents",
  });
  const sweep = await browser.evaluate(sweepExpression(options));
  await browser.send("Tracing.end");
  await complete;

  const main = events.find((e) => e.name === "thread_name" && e.args?.name === "CrRendererMain");
  const run = { frames: sweep.frames.length, seconds: sweep.seconds };
  for (const name of Object.values(PHASES)) run[name] = 0;
  let layouts = 0;
  for (const e of events) {
    if (e.pid !== main?.pid || e.tid !== main?.tid || e.ph !== "X" || !e.dur) continue;
    const phase = PHASES[e.name];
    if (phase) run[phase] += e.dur / 1000;
    if (e.name === "Layout") layouts++;
  }
  run.layouts = layouts;
  return run;
};

const runs = Object.fromEntries(pages.map((page) => [page.name, []]));
for (let round = 1; round <= options.rounds; round++) {
  for (const page of pages) {
    runs[page.name].push(await traceOnce(page.url));
    process.stderr.write(`round ${round}/${options.rounds}: ${page.name}\n`);
  }
}

const keys = ["RunTask", ...Object.values(PHASES).filter((k) => k !== "RunTask"), "layouts", "frames"];
console.log(`main-thread ms per sweep, median [IQR] of ${options.rounds}: throttle ${options.throttle}×, ${options.vps} viewports/s, ≤ ${options.ms} ms, ${options.width}×${options.height}`);
console.table(Object.fromEntries(keys.map((key) => [key, Object.fromEntries(pages.map(({ name }) => [name, summary(runs[name].map((r) => r[key]))]))])));
// A page that keeps up draws more frames in the same sweep, and most phases run once per
// frame: per frame is the fairer comparison of what each frame costs.
const perFrame = ["RunTask", "UpdateLayoutTree", "Layout", "PrePaint", "Paint", "Layerize", "computeIntersections"];
console.log("main-thread ms per frame, median [IQR]");
console.table(Object.fromEntries(perFrame.map((key) => [key, Object.fromEntries(pages.map(({ name }) => [name, summary(runs[name].map((r) => r[key] / r.frames), 2)]))])));
if (options.json) writeFileSync(options.json, JSON.stringify({ options, runs }, null, 2));
process.exit(0);
