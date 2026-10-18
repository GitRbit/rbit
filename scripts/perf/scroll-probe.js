// Scroll probe for large rbit pages. Paste the whole file into the devtools console (or
// evaluate it with a browser automation tool) on a loaded page; it evaluates to a promise
// that resolves to a plain JSON-serialisable report. It touches nothing on the server.
// The pages' CSP forbids eval and cross-origin fetches, so the source has to be pasted or
// sent through the supported browser tool; it cannot be loaded from elsewhere.
//
// Options come from `globalThis.PERF_PROBE_OPTIONS`, set before evaluating:
//   noBackdrop        true: turn off backdrop-filter on the sticky bars for the run (the
//                     blur suspect), through a constructed stylesheet removed afterwards
//   backdropSelector  which bars `noBackdrop` affects (default: top bar and repo tabs)
//   speed             viewports per 100 ms for the sweep (default 3)
//   maxSweepMs        stop the sweep after this long even if not at the bottom (60 s)
//
// The report:
//   dom         element count, document height, which suspects the page has
//   navigation  responseStart, DOMContentLoaded, load, first-contentful-paint (ms since
//               navigation start), Server-Timing render and the footer's "rendered in"
//   sweep       top-to-bottom scroll at a fixed speed driven by requestAnimationFrame:
//               frame-duration stats, scroll position corrections (the page moving the
//               scroll position under us: scroll anchoring after size estimates resolve),
//               document height changes and blank samples taken every few frames
//   jumps       scrollbar-drag simulation: a jump to the top, then to 25/50/75/100 % and
//               ten 10 % steps one frame apart; the viewport is sampled synchronously
//               after each jump and again after one and two frames
//   loaf        long-animation-frame entries during the run (when supported)
//   layoutShift layout-shift entries during the run, plus the page's CLS before it
//
// A "blank" sample is a point on a code line (`.code-line`, a diff row) whose content is
// not rendered: the line is skipped by `content-visibility: auto` (checkVisibility with
// contentVisibilityAuto is false) or its text has an empty box. A point inside a diff file
// or code block that hits no line while the block's content is skipped is blank as well.
(async (options = {}) => {
  const {
    noBackdrop = false,
    backdropSelector = "body > header, .site-subnav",
    speed = 3,
    maxSweepMs = 60_000,
  } = options;

  const startedAt = new Date().toISOString();
  const invalid = new Set();
  const observers = [];
  let sheet = null;
  const viewport = [innerWidth, innerHeight, devicePixelRatio];
  const checkVisibility = () => { if (document.visibilityState !== "visible") invalid.add("page became hidden"); };
  const checkSize = () => { if (viewport.some((value, index) => value !== [innerWidth, innerHeight, devicePixelRatio][index])) invalid.add("viewport changed"); };
  const stylesheets = [...document.querySelectorAll('link[rel="stylesheet"], style[data-app-stylesheet], style[data-theme-stylesheet]')].map((element) => ({ url: element.href ?? `${location.origin}/assets/${element.hasAttribute("data-app-stylesheet") ? "app.css" : "themes/inline.css"}`, loaded: Boolean(element.sheet) && !element.disabled }));
  const header = document.querySelector("body > header");
  checkVisibility();
  if (document.readyState !== "complete") invalid.add("page has not finished loading");
  if (!stylesheets.some((sheet) => new URL(sheet.url).pathname.endsWith("/app.css")) || stylesheets.some((sheet) => !sheet.loaded)) invalid.add("required stylesheet missing or unloaded");
  if (!header || getComputedStyle(header).position !== "sticky" || getComputedStyle(document.body).display !== "flex") invalid.add("expected page styling is not applied");
  if (innerWidth <= 0 || innerHeight <= 0 || !document.querySelector("#main")) invalid.add("page has no visible content viewport");
  document.addEventListener("visibilitychange", checkVisibility);
  globalThis.addEventListener("resize", checkSize);
  try {
  if (invalid.size) throw new Error([...invalid].join("; "));

  // DiffLineView kinds work with direct short-diff rows and virtual blocks alike.
  // Keep legacy tables, but never treat review comments (including their tables) as rows.
  const DIFF_ROW = ".diff-view :is(tr, .diff-add, .diff-del, .diff-ctx, .diff-hunk, .diff-eof):not(.diff-comment, .diff-comment *)";
  const LINE = `.code-line, ${DIFF_ROW}`;
  const BLOCK = ".vblock, .code-lines, .diff-file[open]";
  const CHROME = "body > header, .site-subnav, .diff-file-header, .diff-toolbar, .diff-sidebar";
  const frame = () => new Promise((done, reject) => {
    const timer = setTimeout(() => { cancelAnimationFrame(id); reject(new Error("animation frame stalled for 5 seconds")); }, 5000);
    const id = requestAnimationFrame((time) => {
      clearTimeout(timer);
      checkVisibility(); checkSize();
      if (invalid.size) reject(new Error([...invalid].join("; ")));
      else done(time);
    });
  });
  const round = (n) => Math.round(n * 10) / 10;
  const shiftSum = (entries) => Math.round(entries.reduce((sum, e) => sum + e.value, 0) * 1000) / 1000;
  const scroller = document.scrollingElement;
  const maxScroll = () => scroller.scrollHeight - innerHeight;

  // --- Static facts -----------------------------------------------------------------------
  const [nav] = performance.getEntriesByType("navigation");
  const fcp = performance.getEntriesByName("first-contentful-paint")[0];
  const navigation = nav && {
    responseStart: round(nav.responseStart),
    domContentLoaded: round(nav.domContentLoadedEventEnd),
    load: round(nav.loadEventEnd),
    firstContentfulPaint: fcp ? round(fcp.startTime) : null,
    transferKiB: round(nav.transferSize / 1024),
    htmlKiB: round(nav.decodedBodySize / 1024),
    serverTiming: Object.fromEntries((nav.serverTiming ?? []).map((t) => [t.name, round(t.duration)])),
    footerRendered: document.querySelector("[data-render-time]")?.textContent.trim() ?? null,
  };
  const dom = {
    elements: document.getElementsByTagName("*").length,
    codeLines: document.querySelectorAll(".code-line").length,
    diffRows: document.querySelectorAll(DIFF_ROW).length,
    diffFiles: document.querySelectorAll(".diff-file").length,
    scrollHeight: scroller.scrollHeight,
    viewport: [innerWidth, innerHeight],
    devicePixelRatio: devicePixelRatio,
    headerBackdrop: getComputedStyle(header).backdropFilter,
  };

  // --- Observers ----------------------------------------------------------------------------
  const loafs = [];
  const shifts = [];
  const observe = (type, sink, buffered = false) => {
    if (!PerformanceObserver.supportedEntryTypes?.includes(type)) return false;
    const observer = new PerformanceObserver((list) => sink(list.getEntries()));
    observer.observe({ type, buffered });
    observers.push(observer);
    return true;
  };
  // Layout shifts are buffered so the page's own (before the probe) count as well.
  const probeStart = performance.now();
  const loafSupported = observe("long-animation-frame", (entries) => loafs.push(...entries));
  observe("layout-shift", (entries) => shifts.push(...entries.filter((e) => !e.hadRecentInput)), true);

  // --- Blank sampling -----------------------------------------------------------------------
  /** Whether the text of a line has a laid-out, non-empty box. */
  function textRendered(line) {
    const cell = line.querySelector(".line-code, td.code") ?? line;
    if (!cell.textContent.trim()) return true;
    const range = document.createRange();
    range.selectNodeContents(cell);
    const box = range.getBoundingClientRect();
    return box.width > 0 && box.height > 0;
  }

  /** Samples a `columns` x `rows` grid over the viewport. */
  function sample(columns = 6, rows = 24) {
    const tally = { points: 0, line: 0, blank: 0, gap: 0, chrome: 0, other: 0 };
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < columns; c++) {
        const x = ((c + 0.5) / columns) * innerWidth;
        const y = ((r + 0.5) / rows) * innerHeight;
        const stack = document.elementsFromPoint(x, y);
        tally.points++;
        if (!stack.length) { tally.other++; continue; }
        if (stack[0].closest(CHROME)) { tally.chrome++; continue; }
        const line = stack.find((element) => element.matches(LINE));
        if (line) {
          // The line itself is never "skipped" by its own content-visibility; its contents are.
          const inner = line.querySelector(".line-code, td.code") ?? line.firstElementChild ?? line;
          const visible = inner.checkVisibility({ contentVisibilityAuto: true });
          tally[visible && textRendered(line) ? "line" : "blank"]++;
          continue;
        }
        const block = stack.find((element) => element.matches(BLOCK));
        if (block) {
          const inner = block.matches(".diff-file") ? block.querySelector("table, .diff-placeholder") : block.firstElementChild;
          tally[inner && !inner.checkVisibility({ contentVisibilityAuto: true }) ? "blank" : "gap"]++;
          continue;
        }
        tally.other++;
      }
    }
    return tally;
  }
  const addTally = (into, tally) => { for (const key in tally) into[key] = (into[key] ?? 0) + tally[key]; return into; };

  // --- Backdrop suspect ---------------------------------------------------------------------
  // A constructed sheet: the page's CSP (`style-src 'self'`) blocks an inline <style>.
  if (noBackdrop) {
    sheet = new CSSStyleSheet();
    sheet.replaceSync(`${backdropSelector} { backdrop-filter: none !important; -webkit-backdrop-filter: none !important; }`);
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
    await frame();
  }

  // --- Sweep --------------------------------------------------------------------------------
  scroller.scrollTop = 0;
  await frame();
  await frame();
  const durations = [];
  const sweepBlank = {};
  let corrections = 0;
  let largestCorrection = 0;
  let heightChanges = 0;
  let height = scroller.scrollHeight;
  let expected = 0;
  let last = await frame();
  const started = last;
  const pxPerMs = (speed * innerHeight) / 100;
  for (let index = 0; ; index++) {
    const now = await frame();
    durations.push(now - last);
    // Did the page move us since the last frame (scroll anchoring, size estimates)?
    const drift = scroller.scrollTop - expected;
    if (Math.abs(drift) > 2 && expected < maxScroll()) {
      corrections++;
      largestCorrection = Math.max(largestCorrection, Math.abs(drift));
    }
    if (scroller.scrollHeight !== height) { heightChanges++; height = scroller.scrollHeight; }
    if (index % 8 === 4) addTally(sweepBlank, sample(3, 8));
    if (scroller.scrollTop >= maxScroll() - 1 || now - started > maxSweepMs) break;
    expected = Math.min(maxScroll(), scroller.scrollTop + (now - last) * pxPerMs);
    scroller.scrollTop = expected;
    expected = scroller.scrollTop;
    last = now;
  }
  const sorted = [...durations].sort((a, b) => a - b);
  const sweep = {
    ms: round(last - started),
    reachedBottom: scroller.scrollTop >= maxScroll() - 1,
    frames: durations.length,
    over16_7ms: durations.filter((d) => d > 16.7).length,
    over33ms: durations.filter((d) => d > 33.4).length,
    worstMs: round(sorted.at(-1) ?? 0),
    medianMs: round(sorted[Math.floor(sorted.length / 2)] ?? 0),
    p95Ms: round(sorted[Math.floor(sorted.length * 0.95)] ?? 0),
    positionCorrections: corrections,
    largestCorrectionPx: round(largestCorrection),
    heightChanges,
    scrollHeightAfter: scroller.scrollHeight,
    blank: sweepBlank,
  };

  // --- Jumps --------------------------------------------------------------------------------
  const jumps = [];
  const jumpBlank = { sync: {}, frame1: {}, frame2: {} };
  async function jump(label, fraction, settle = true) {
    scroller.scrollTop = 0;
    if (settle) { await frame(); await frame(); }
    const t0 = performance.now();
    scroller.scrollTop = fraction * maxScroll();
    const sync = sample();
    const at1 = await frame();
    const frame1 = sample();
    const at2 = await frame();
    const frame2 = sample();
    addTally(jumpBlank.sync, sync);
    addTally(jumpBlank.frame1, frame1);
    addTally(jumpBlank.frame2, frame2);
    jumps.push({ label, firstFrameMs: round(at1 - t0), secondFrameMs: round(at2 - at1), blank: [sync.blank, frame1.blank, frame2.blank] });
  }
  for (const fraction of [0.25, 0.5, 0.75, 1]) await jump(`top→${fraction * 100}%`, fraction);
  // A scrollbar drag: ten steps down, one frame apart, sampled after each.
  scroller.scrollTop = 0;
  await frame();
  await frame();
  const drag = { frames: [], blank: {} };
  for (let step = 1; step <= 10; step++) {
    const t0 = performance.now();
    scroller.scrollTop = (step / 10) * maxScroll();
    const at = await frame();
    drag.frames.push(round(at - t0));
    addTally(drag.blank, sample(3, 12));
  }
  scroller.scrollTop = 0;
  await frame();

  // --- Report -------------------------------------------------------------------------------
  await new Promise((done) => setTimeout(done, 100)); // let observers deliver
  checkVisibility(); checkSize();
  const report = {
    valid: invalid.size === 0,
    invalidReasons: [...invalid],
    startedAt,
    stylesheets,
    url: location.href,
    options: { noBackdrop, backdropSelector: noBackdrop ? backdropSelector : null, speed },
    userAgent: navigator.userAgent,
    dom,
    navigation,
    sweep,
    jumps: { each: jumps, blank: jumpBlank, drag },
    loaf: loafSupported
      ? {
          count: loafs.length,
          worstMs: round(Math.max(0, ...loafs.map((e) => e.duration))),
          totalBlockingMs: round(loafs.reduce((sum, e) => sum + (e.blockingDuration ?? 0), 0)),
          worst: [...loafs].sort((a, b) => b.duration - a.duration).slice(0, 5).map((e) => ({
            ms: round(e.duration),
            renderMs: round(e.startTime + e.duration - e.renderStart),
            styleLayoutMs: e.styleAndLayoutStart ? round(e.startTime + e.duration - e.styleAndLayoutStart) : null,
            scriptMs: round(e.scripts.reduce((sum, s) => sum + s.duration, 0)),
          })),
        }
      : "unsupported",
    layoutShift: {
      clsBeforeProbe: shiftSum(shifts.filter((e) => e.startTime < probeStart)),
      duringProbe: shifts.filter((e) => e.startTime >= probeStart).length,
      duringProbeSum: shiftSum(shifts.filter((e) => e.startTime >= probeStart)),
    },
  };
  globalThis.PERF_PROBE_RESULT = report;
  return report;
  } catch (error) {
    invalid.add(error.message);
    const report = { valid: false, invalidReasons: [...invalid], startedAt, url: location.href, viewport, stylesheets };
    globalThis.PERF_PROBE_RESULT = report;
    return report;
  } finally {
    for (const observer of observers) observer.disconnect();
    if (sheet) document.adoptedStyleSheets = document.adoptedStyleSheets.filter((candidate) => candidate !== sheet);
    document.removeEventListener("visibilitychange", checkVisibility);
    globalThis.removeEventListener("resize", checkSize);
  }
})(globalThis.PERF_PROBE_OPTIONS);
