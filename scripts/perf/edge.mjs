// Headless Edge (or Chrome) over the DevTools protocol, shared by trace.mjs and sweep.mjs.
// No dependencies: Node's built-in fetch and WebSocket talk to the browser directly.
//
// The browser runs in its own throwaway profile and, on exit (normal, an exception or
// Ctrl+C), its whole process tree is killed: Edge's renderers and GPU process outlive the
// launcher otherwise, and stray headless browsers left from earlier runs piled up to over a
// hundred processes once, slowing every later measurement on the machine.
//
// Set PERF_BROWSER to use another Chromium build.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CANDIDATES = [
  process.env.PERF_BROWSER,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "/usr/bin/microsoft-edge",
  "/usr/bin/google-chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
].filter(Boolean);

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Parses `--key value` options and `name=url` pages from the command line. */
export function parseArgs(argv, defaults) {
  const options = { ...defaults };
  const pages = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg.startsWith("--")) {
      const key = arg.slice(2);
      if (!(key in defaults)) throw new Error(`unknown option ${arg}`);
      options[key] = typeof defaults[key] === "number" ? Number(argv[++i]) : argv[++i];
    } else {
      const at = arg.indexOf("=");
      if (at < 1) throw new Error(`expected name=url, got ${arg}`);
      pages.push({ name: arg.slice(0, at), url: arg.slice(at + 1) });
    }
  }
  return { options, pages };
}

/**
 * Kills every process of the browser using `profile` on Windows. The process Node starts
 * is a stub that hands over to a new browser process and exits, so its pid is useless:
 * the browser and its children are found by the profile directory on their command lines.
 */
function killProfileTree(profile) {
  const query =
    `Get-CimInstance Win32_Process -Filter "name='msedge.exe' or name='chrome.exe'" | ` +
    `Where-Object { $_.CommandLine -like '*${profile}*' } | ForEach-Object { $_.ProcessId }`;
  const pids = String(execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", query]))
    .split(/\s+/)
    .filter(Boolean);
  for (const pid of pids) {
    try {
      execFileSync("taskkill", ["/PID", pid, "/T", "/F"], { stdio: "ignore" });
    } catch {} // already gone with its parent's tree
  }
}

/** Launches a headless browser of `width`×`height` CSS pixels with the CPU slowed `throttle` times. */
export async function launch({ width = 1280, height = 1000, throttle = 1 } = {}) {
  const exe = CANDIDATES.find((path) => existsSync(path));
  if (!exe) throw new Error("no Edge or Chrome found: set PERF_BROWSER");
  const port = 9700 + Math.floor(Math.random() * 250);
  const profile = mkdtempSync(join(tmpdir(), "rbit-perf-edge-"));
  const browser = spawn(
    exe,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      `--window-size=${width},${height}`,
      "about:blank",
    ],
    // Its own process group elsewhere, so the whole tree can be killed at once.
    { stdio: "ignore", detached: process.platform !== "win32" },
  );

  // Synchronous, so it also runs from the `exit` event.
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    try {
      if (process.platform === "win32") killProfileTree(profile);
      else process.kill(-browser.pid, "SIGKILL");
    } catch {
      browser.kill("SIGKILL");
    }
    try {
      rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
    } catch {}
  };
  process.on("exit", close);
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => process.exit(130));
  process.on("uncaughtException", (error) => {
    console.error(error);
    process.exit(1);
  });

  let targets = [];
  for (let i = 0; i < 150 && !targets.some((t) => t.type === "page"); i++) {
    try {
      targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    } catch {}
    await sleep(200);
  }
  const target = targets.find((t) => t.type === "page");
  if (!target) throw new Error("the browser did not start");
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", reject);
  });

  let nextId = 0;
  const pending = new Map();
  const listeners = new Set();
  ws.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
    } else if (message.method) {
      for (const listener of listeners) listener(message);
    }
  });

  /** Sends a protocol command and resolves to its result. */
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });
  /** Calls `listener` with every protocol event until the returned function is called. */
  const on = (listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  };
  /** Evaluates `expression` in the page (awaiting a promise) and returns its JSON value. */
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails).slice(0, 600));
    return result.result.value;
  };
  /**
   * Loads `url`, waits for `load`, then for the page to go quiet (ten animation frames in
   * a row of under 25 ms, at most 20 s): on a slowed CPU, scripts, fonts and the first
   * rendering of a large page run on well after `load`, and a sweep started during them
   * measures them instead of scrolling.
   */
  const goto = async (url) => {
    const loaded = new Promise((resolve) => {
      const off = on((message) => {
        if (message.method === "Page.loadEventFired") {
          off();
          resolve();
        }
      });
    });
    await send("Page.navigate", { url });
    await loaded;
    await evaluate(`(async () => {
      const start = performance.now();
      let quiet = 0, last = start;
      while (quiet < 10 && performance.now() - start < 20000) {
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const now = performance.now();
        quiet = now - last < 25 ? quiet + 1 : 0;
        last = now;
      }
    })()`);
    await sleep(500);
  };

  await send("Page.enable");
  if (throttle > 1) await send("Emulation.setCPUThrottlingRate", { rate: throttle });
  return { send, on, evaluate, goto, close };
}

/** The `p` quantile (0–1) of `values`, nearest rank. */
export function quantile(values, p) {
  const sorted = values.filter((x) => Number.isFinite(x)).sort((a, b) => a - b);
  if (!sorted.length) return Number.NaN;
  return sorted[Math.min(sorted.length - 1, Math.round(p * (sorted.length - 1)))];
}

/** `median [q1–q3]`, rounded to `digits` decimals. */
export function summary(values, digits = 0) {
  const round = (x) => (Number.isFinite(x) ? x.toFixed(digits) : "–");
  return `${round(quantile(values, 0.5))} [${round(quantile(values, 0.25))}–${round(quantile(values, 0.75))}]`;
}

/**
 * The scroll sweep both scripts drive, as an expression evaluated in the page: from the
 * top, the page scrolls `vps` viewports per second, one step per animation frame, for at
 * most `ms` or until the bottom. Resolves to per-frame durations and what moved by itself.
 */
export function sweepExpression({ vps, ms }) {
  return `(async () => {
    const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
    const sc = document.scrollingElement;
    const max = () => sc.scrollHeight - innerHeight;
    sc.scrollTop = 0; await frame(); await frame();
    const frames = []; let heightChanges = 0, height = sc.scrollHeight, corrections = 0, biggestJump = 0, expected = 0;
    let last = await frame(); const start = last; const pxPerMs = ${vps} * innerHeight / 1000;
    for (;;) {
      const now = await frame(); frames.push(now - last);
      const drift = sc.scrollTop - expected;
      if (Math.abs(drift) > 2 && expected < max()) { corrections++; biggestJump = Math.max(biggestJump, Math.abs(drift)); }
      if (sc.scrollHeight !== height) { heightChanges++; height = sc.scrollHeight; }
      if (sc.scrollTop >= max() - 1 || now - start > ${ms}) break;
      sc.scrollTop = Math.min(max(), sc.scrollTop + (now - last) * pxPerMs); expected = sc.scrollTop; last = now;
    }
    return { frames, seconds: (last - start) / 1000, heightChanges, corrections, biggestJump };
  })()`;
}
