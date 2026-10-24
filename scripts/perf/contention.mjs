// Concurrent mixed workload against a running rbit, for SQLite lock contention: anonymous
// and signed-in page reads, issue and pull request comments (each notifies every other
// participant), pushes to an open pull request's branch and the webhook deliveries all of
// that queues. Before and after each concurrency level it scrapes `/metrics` and reports
// the change in the `rbit_sqlite_*` pool metrics next to client-side latency and failures.
//
//   node scripts/perf/contention.mjs setup <origin> [options]
//   node scripts/perf/contention.mjs run <origin> [--levels 4,16,64] [--seconds 20] [options]
//
// The server must run with `RBIT_OPERATIONS__METRICS=true`, `RBIT_SECURITY__RATE_LIMIT=false`
// and `RBIT_WEBHOOKS__ALLOW_PRIVATE_ADDRESSES=true`, on data seeded by seed.mjs. `setup`
// (once per data directory, rerunnable) adds the load accounts, four issues, a pull request
// whose branch the pusher advances, the webhook and everyone's participation. Load accounts
// share the seeded account's password from <perf-dir>/credentials.txt, which is never
// printed. See README.md ("SQLite contention workload").
import { execFile, execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import http from "node:http";
import { cpus, platform, release, totalmem } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { summarize } from "./support.mjs";

const execFileAsync = promisify(execFile);
const OWNER = "perf";
const REPO = "perf-fixture";
const ISSUES = [2, 3, 4, 5];
const PUSH_BRANCH = "load-push";

// --- Pure helpers (tested in contention.test.mjs) ------------------------------------------

export function parseArgs(argv) {
  const [command, origin, ...rest] = argv;
  const options = {
    command, origin: origin?.replace(/\/$/, ""),
    levels: [4, 16, 64], seconds: 20, users: 8, seed: 1, hookPort: 3032, pushIntervalMs: 1000,
    perfDir: null, work: null, output: null, label: "",
  };
  for (let i = 0; i < rest.length; i += 2) {
    const [key, value] = [rest[i], rest[i + 1]];
    if (value === undefined || value.startsWith("--")) throw new Error(`missing value for ${key}`);
    switch (key) {
      case "--levels": options.levels = value.split(",").map(Number); break;
      case "--seconds": options.seconds = Number(value); break;
      case "--users": options.users = Number(value); break;
      case "--seed": options.seed = Number(value); break;
      case "--hook-port": options.hookPort = Number(value); break;
      case "--push-interval-ms": options.pushIntervalMs = Number(value); break;
      case "--perf-dir": options.perfDir = value; break;
      case "--work": options.work = value; break;
      case "--output": options.output = value; break;
      case "--label": options.label = value; break;
      default: throw new Error(`unknown option ${key}`);
    }
  }
  if (!["setup", "run"].includes(command)) throw new Error("usage: contention.mjs setup|run <origin> [options]");
  const url = new URL(options.origin ?? "");
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) || url.pathname !== "/" || url.username) {
    throw new Error("origin must be a loopback http origin");
  }
  if (!options.levels.length || !options.levels.every((n) => Number.isInteger(n) && n > 0 && n <= 1024)) throw new Error("invalid --levels");
  for (const key of ["seconds", "users", "hookPort", "pushIntervalMs"]) {
    if (!Number.isInteger(options[key]) || options[key] < 1) throw new Error(`invalid --${key}`);
  }
  return options;
}

/** Deterministic PRNG (mulberry32), so the operation sequence of a run is reproducible. */
export function random(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The operation mix: weights out of 100. Pushes run in their own lane. */
export const MIX = [
  ["anonymous-read", 50],
  ["signed-in-read", 35],
  ["issue-comment", 10],
  ["pull-comment", 5],
];

export function pick(mix, r) {
  const total = mix.reduce((sum, [, weight]) => sum + weight, 0);
  let at = r * total;
  for (const [name, weight] of mix) {
    if (at < weight) return name;
    at -= weight;
  }
  return mix[mix.length - 1][0];
}

/** Prometheus text exposition into `Map("name{labels}" -> value)`. */
export function parseMetrics(text) {
  const samples = new Map();
  for (const line of text.split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const at = line.lastIndexOf(" ");
    const value = Number(line.slice(at + 1));
    if (Number.isFinite(value)) samples.set(line.slice(0, at), value);
  }
  return samples;
}

export function delta(before, after) {
  const out = new Map();
  for (const [key, value] of after) {
    out.set(key, value - (before.get(key) ?? 0));
  }
  return out;
}

function labelsOf(key) {
  const open = key.indexOf("{");
  if (open < 0) return [key, {}];
  const labels = {};
  for (const match of key.slice(open + 1, -1).matchAll(/(\w+)="((?:[^"\\]|\\.)*)"/g)) labels[match[1]] = match[2];
  return [key.slice(0, open), labels];
}

/** Sum of a metric's samples whose labels include `filter`. */
export function total(samples, name, filter = {}) {
  let sum = 0;
  for (const [key, value] of samples) {
    const [metric, labels] = labelsOf(key);
    if (metric === name && Object.entries(filter).every(([k, v]) => labels[k] === v)) sum += value;
  }
  return sum;
}

/** The `q` quantile of a histogram's (summed, filtered) cumulative buckets, interpolated. */
export function histogramQuantile(samples, name, q, filter = {}) {
  const buckets = new Map();
  for (const [key, value] of samples) {
    const [metric, labels] = labelsOf(key);
    if (metric !== `${name}_bucket`) continue;
    if (!Object.entries(filter).every(([k, v]) => labels[k] === v)) continue;
    const le = labels.le === "+Inf" ? Infinity : Number(labels.le);
    buckets.set(le, (buckets.get(le) ?? 0) + value);
  }
  const sorted = [...buckets].sort((a, b) => a[0] - b[0]);
  const count = sorted.at(-1)?.[1] ?? 0;
  if (!count) return null;
  const rank = q * count;
  let [lowerBound, lowerCount] = [0, 0];
  for (const [le, cumulative] of sorted) {
    if (cumulative >= rank) {
      if (le === Infinity) return lowerBound;
      return lowerBound + (le - lowerBound) * ((rank - lowerCount) / Math.max(cumulative - lowerCount, 1));
    }
    [lowerBound, lowerCount] = [le, cumulative];
  }
  return lowerBound;
}

/** Busy errors grouped by `store/phase/code`. */
export function busyBreakdown(samples) {
  const out = {};
  for (const [key, value] of samples) {
    const [metric, labels] = labelsOf(key);
    if (metric === "rbit_sqlite_busy_errors_total" && value > 0) out[`${labels.store}/${labels.phase}/${labels.code}`] = value;
  }
  return out;
}

/** The pool's view of one level, from the `/metrics` change across it. */
export function poolReport(change) {
  const ms = (seconds) => (seconds === null ? null : Math.round(seconds * 1e6) / 1e3);
  const histogram = (name, filter) => {
    const count = total(change, `${name}_count`, filter);
    return {
      count,
      meanMs: count ? ms(total(change, `${name}_sum`, filter) / count) : null,
      p50Ms: ms(histogramQuantile(change, name, 0.5, filter)),
      p95Ms: ms(histogramQuantile(change, name, 0.95, filter)),
      p99Ms: ms(histogramQuantile(change, name, 0.99, filter)),
    };
  };
  const stores = (name) => {
    const names = new Set();
    for (const key of change.keys()) {
      const [metric, labels] = labelsOf(key);
      if (metric === `${name}_count` && labels.store) names.add(labels.store);
    }
    const out = Object.fromEntries([...names].map((store) => [store, histogram(name, { store })]).filter(([, h]) => h.count > 0));
    return Object.fromEntries(Object.entries(out).sort((a, b) => b[1].count * b[1].meanMs - a[1].count * a[1].meanMs));
  };
  return {
    acquireQuery: histogram("rbit_sqlite_acquire_wait_seconds", { mode: "query" }),
    acquireTransaction: histogram("rbit_sqlite_acquire_wait_seconds", { mode: "transaction" }),
    acquireTimeouts: total(change, "rbit_sqlite_acquire_timeouts_total"),
    writeLockWait: histogram("rbit_sqlite_write_lock_wait_seconds"),
    writeTransactions: histogram("rbit_sqlite_write_transaction_seconds"),
    writeLockWaitByStore: stores("rbit_sqlite_write_lock_wait_seconds"),
    writeTransactionsByStore: stores("rbit_sqlite_write_transaction_seconds"),
    busyErrors: total(change, "rbit_sqlite_busy_errors_total"),
    busy: busyBreakdown(change),
    serverErrors: total(change, "rbit_http_requests_total", { status: "5xx" }),
    webhookAttempts: total(change, "rbit_webhook_attempts_total"),
  };
}

// --- HTTP client -------------------------------------------------------------------------

const agent = new http.Agent({ keepAlive: true, maxSockets: Infinity });

class Client {
  constructor(base) {
    this.base = base;
    this.cookies = new Map();
  }

  /** `{ status, location, ms }`; the body is read and dropped. Network errors become status 0. */
  request(method, path, form) {
    const headers = { Origin: this.base, "Accept-Encoding": "identity" };
    if (this.cookies.size) headers.Cookie = [...this.cookies].map(([k, v]) => `${k}=${v}`).join("; ");
    const body = form ? new URLSearchParams(form).toString() : null;
    if (body !== null) {
      headers["Content-Type"] = "application/x-www-form-urlencoded";
      headers["Content-Length"] = Buffer.byteLength(body);
    }
    return new Promise((done) => {
      const started = performance.now();
      const request = http.request(this.base + path, { method, headers, agent }, (response) => {
        for (const line of response.headers["set-cookie"] ?? []) {
          const [pair] = line.split(";");
          const at = pair.indexOf("=");
          this.cookies.set(pair.slice(0, at), pair.slice(at + 1));
        }
        const chunks = [];
        response.on("data", (chunk) => { if (this.keepBody) chunks.push(chunk); });
        response.on("end", () => done({
          status: response.statusCode, location: response.headers.location ?? null, ms: performance.now() - started,
          text: this.keepBody ? Buffer.concat(chunks).toString("utf8") : null,
        }));
        response.on("error", (error) => done({ status: 0, error: error.message, ms: performance.now() - started }));
      });
      request.setTimeout(120_000, () => request.destroy(new Error("timeout")));
      request.on("error", (error) => done({ status: 0, error: error.message, ms: performance.now() - started }));
      if (body !== null) request.write(body);
      request.end();
    });
  }

  async text(path) {
    this.keepBody = true;
    try { return await this.request("GET", path); } finally { this.keepBody = false; }
  }
}

const ok = (response) => response.status >= 200 && response.status < 400;
const redirected = (response) => response.status >= 300 && response.status < 400;

async function waitForServer(base) {
  for (let attempt = 0; attempt < 60; attempt++) {
    const response = await new Client(base).request("GET", "/healthz");
    if (response.status === 200) return;
    await new Promise((done) => setTimeout(done, 1000));
  }
  throw new Error(`${base} did not answer /healthz within a minute`);
}

async function scrape(base) {
  const response = await new Client(base).text("/metrics");
  if (response.status !== 200) throw new Error(`/metrics answered ${response.status}: start the server with RBIT_OPERATIONS__METRICS=true`);
  return parseMetrics(response.text);
}

// --- Accounts, fixture and webhook listener ----------------------------------------------

function paths(options) {
  const root = resolve(import.meta.dirname, "..", "..");
  const perfDir = resolve(options.perfDir ?? join(root, "target", "perf"));
  return { perfDir, work: resolve(options.work ?? join(perfDir, "contention")), credentials: join(perfDir, "credentials.txt") };
}

function password(file) {
  if (!existsSync(file)) throw new Error(`no ${file}: run seed.mjs first`);
  const [login, secret] = readFileSync(file, "utf8").trim().split(":");
  if (login !== OWNER || !secret) throw new Error(`${file} does not hold the ${OWNER} account`);
  return secret;
}

async function signIn(base, login, secret, signUp) {
  const client = new Client(base);
  if (signUp) {
    const created = await client.request("POST", "/signup", { username: login, email: `${login}@example.test`, password: secret });
    if (redirected(created)) return client;
  }
  const session = await client.request("POST", "/login", { login, password: secret });
  if (!redirected(session)) throw new Error(`could not sign in as ${login} (HTTP ${session.status})`);
  return client;
}

async function accounts(base, options, files, signUp) {
  const secret = password(files.credentials);
  const owner = await signIn(base, OWNER, secret, false);
  const users = [];
  for (let i = 1; i <= options.users; i++) users.push(await signIn(base, `load${i}`, secret, signUp));
  return { owner, users, secret };
}

function gitEnv(secret) {
  const basic = Buffer.from(`${OWNER}:${secret}`).toString("base64");
  return {
    ...process.env,
    GIT_TERMINAL_PROMPT: "0",
    GIT_AUTHOR_NAME: "Load", GIT_AUTHOR_EMAIL: "load@example.test", GIT_COMMITTER_NAME: "Load", GIT_COMMITTER_EMAIL: "load@example.test",
    GIT_CONFIG_COUNT: "2",
    GIT_CONFIG_KEY_0: "credential.helper", GIT_CONFIG_VALUE_0: "",
    GIT_CONFIG_KEY_1: "http.extraHeader", GIT_CONFIG_VALUE_1: `Authorization: Basic ${basic}`,
  };
}

/** Counts webhook deliveries; answers each with 204. */
function listen(port) {
  const counts = { deliveries: 0 };
  const server = http.createServer((request, response) => {
    request.resume();
    request.on("end", () => {
      counts.deliveries++;
      response.writeHead(204).end();
    });
  });
  return new Promise((done, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => done({ counts, close: () => new Promise((closed) => server.close(closed)) }));
  });
}

async function setup(options) {
  const base = options.origin;
  const files = paths(options);
  await waitForServer(base);
  await scrape(base);
  const { owner, users, secret } = await accounts(base, options, files, true);
  console.log(`signed in ${OWNER} and ${users.length} load accounts`);
  const repo = `/${OWNER}/${REPO}`;

  for (const number of ISSUES) {
    if ((await owner.request("GET", `${repo}/issues/${number}`)).status === 200) continue;
    const created = await owner.request("POST", `${repo}/issues`, { title: `Load issue ${number}`, body: "Comments from the contention workload." });
    if (!redirected(created) || !created.location.endsWith(`/issues/${number}`)) {
      throw new Error(`expected to create issue #${number}, got HTTP ${created.status} ${created.location}`);
    }
  }
  console.log(`issues ${ISSUES.join(", ")} exist`);

  const listener = await listen(options.hookPort);
  try {
    const hookUrl = `http://127.0.0.1:${options.hookPort}/hook`;
    const hooks = await owner.text(`${repo}/settings/hooks`);
    if (!hooks.text.includes(hookUrl)) {
      const created = await owner.request("POST", `${repo}/settings/hooks`, {
        url: hookUrl, active: "on", event_push: "on", event_issues: "on", event_issue_comment: "on", event_pull_request: "on",
      });
      if (!redirected(created)) throw new Error(`could not create the webhook (HTTP ${created.status})`);
    }
    console.log(`webhook ${hookUrl} exists`);

    const clone = join(files.work, "pusher");
    const env = gitEnv(secret);
    if (!existsSync(join(clone, ".git"))) {
      mkdirSync(files.work, { recursive: true });
      execFileSync("git", ["clone", "--quiet", "--branch", "main", `${base}${repo}.git`, clone], { env, stdio: ["ignore", "inherit", "inherit"] });
    }
    const git = (...args) => execFileSync("git", args, { cwd: clone, env, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }).trim();
    const remoteBranch = git("ls-remote", "origin", `refs/heads/${PUSH_BRANCH}`);
    if (!remoteBranch) {
      git("checkout", "--quiet", "-B", PUSH_BRANCH, "origin/main");
      writeFileSync(join(clone, "LOAD.md"), "Advanced by the contention workload.\n");
      git("add", "LOAD.md");
      git("commit", "--quiet", "-m", "Start the load branch");
      git("push", "--quiet", "origin", PUSH_BRANCH);
    }
    const setupFile = join(files.work, "setup.json");
    let pull = existsSync(setupFile) ? JSON.parse(readFileSync(setupFile, "utf8")).pull : null;
    if (pull !== null && (await owner.request("GET", `${repo}/pull/${pull}`)).status !== 200) pull = null;
    if (pull === null) {
      const opened = await owner.request("POST", `${repo}/pulls`, { base: "main", head: PUSH_BRANCH, title: "Load push", body: "Advanced by the contention workload." });
      if (!redirected(opened)) throw new Error(`could not open the load pull request (HTTP ${opened.status})`);
      pull = Number(opened.location.split("/").at(-1));
    }
    console.log(`pull request #${pull} tracks ${PUSH_BRANCH}`);
    writeFileSync(setupFile,`${JSON.stringify({ pull, hookPort: options.hookPort, users: options.users }, null, 2)}\n`);

    // Everyone participates in every thread, so each comment notifies the others.
    for (const [index, client] of [owner, ...users].entries()) {
      for (const number of [...ISSUES, pull]) {
        const posted = await client.request("POST", `${repo}/issues/${number}/comments`, { body: `Participant ${index} joins.` });
        if (!redirected(posted)) throw new Error(`participant ${index} could not comment on #${number} (HTTP ${posted.status})`);
      }
    }
    console.log("every account participates in every load thread");
  } finally {
    await listener.close();
  }
}

// --- The workload ------------------------------------------------------------------------

function readPages(pull) {
  const repo = `/${OWNER}/${REPO}`;
  return [
    repo, `${repo}/issues`, `${repo}/pulls`, `${repo}/commits/main`, `${repo}/pull/${pull}`,
    ...ISSUES.map((n) => `${repo}/issues/${n}`),
  ];
}

async function level(options, state, concurrency) {
  const { base, users, pull, clone, env, listener } = state;
  const pages = readPages(pull);
  const signedInPages = [...pages, "/notifications", "/issues"];
  const samples = Object.fromEntries([...MIX.map(([name]) => name), "push"].map((name) => [name, []]));
  const failures = Object.fromEntries(Object.keys(samples).map((name) => [name, 0]));
  const failureExamples = [];
  const anonymous = new Client(base);
  const record = (name, response) => {
    samples[name].push(response.ms);
    if (!ok(response)) {
      failures[name]++;
      if (failureExamples.length < 10) failureExamples.push({ name, status: response.status, error: response.error ?? null });
    }
  };

  const before = await scrape(base);
  const deliveriesBefore = listener.counts.deliveries;
  const deadline = performance.now() + options.seconds * 1000;
  let sequence = 0;

  const worker = async (id) => {
    const next = random(options.seed * 1_000_003 + concurrency * 1009 + id);
    while (performance.now() < deadline) {
      const operation = pick(MIX, next());
      const user = users[Math.floor(next() * users.length)];
      switch (operation) {
        case "anonymous-read":
          record(operation, await anonymous.request("GET", pages[Math.floor(next() * pages.length)]));
          break;
        case "signed-in-read":
          record(operation, await user.request("GET", signedInPages[Math.floor(next() * signedInPages.length)]));
          break;
        case "issue-comment": {
          const number = ISSUES[Math.floor(next() * ISSUES.length)];
          record(operation, await user.request("POST", `/${OWNER}/${REPO}/issues/${number}/comments`, { body: `Load comment ${sequence++} at concurrency ${concurrency}.` }));
          break;
        }
        case "pull-comment":
          record(operation, await user.request("POST", `/${OWNER}/${REPO}/issues/${pull}/comments`, { body: `Load review note ${sequence++}.` }));
          break;
      }
    }
  };

  const pusher = async () => {
    let n = 0;
    while (performance.now() < deadline) {
      const started = performance.now();
      try {
        await execFileAsync("git", ["commit", "--quiet", "--allow-empty", "-m", `Load push ${concurrency}-${n++}`], { cwd: clone, env });
        await execFileAsync("git", ["push", "--quiet", "origin", PUSH_BRANCH], { cwd: clone, env });
        record("push", { status: 200, ms: performance.now() - started });
      } catch (error) {
        record("push", { status: 0, error: String(error.stderr || error.message).trim().slice(0, 200), ms: performance.now() - started });
      }
      const wait = options.pushIntervalMs - (performance.now() - started);
      if (wait > 0) await new Promise((done) => setTimeout(done, wait));
    }
  };

  const started = performance.now();
  await Promise.all([...Array.from({ length: concurrency }, (_, id) => worker(id)), pusher()]);
  const elapsed = (performance.now() - started) / 1000;
  // Let queued webhook deliveries drain into the same window before the second scrape.
  await new Promise((done) => setTimeout(done, 2000));
  const change = delta(before, await scrape(base));

  const round = (value) => (value === null ? null : Math.round(value * 10) / 10);
  const operations = Object.fromEntries(Object.entries(samples).map(([name, values]) => {
    const summary = summarize(values);
    return [name, { count: values.length, failed: failures[name], perSecond: round(values.length / elapsed), p50Ms: round(summary.median), p95Ms: round(summary.p95) }];
  }));
  return {
    concurrency, seconds: round(elapsed), operations,
    failed: Object.values(failures).reduce((a, b) => a + b, 0),
    failureExamples,
    webhookDeliveriesReceived: listener.counts.deliveries - deliveriesBefore,
    pool: poolReport(change),
  };
}

async function run(options) {
  const base = options.origin;
  const files = paths(options);
  const setupFile = join(files.work, "setup.json");
  if (!existsSync(setupFile)) throw new Error(`no ${setupFile}: run \`contention.mjs setup ${base}\` first`);
  const { pull } = JSON.parse(readFileSync(setupFile, "utf8"));
  await waitForServer(base);
  const { users, secret } = await accounts(base, options, files, false);
  const clone = join(files.work, "pusher");
  const env = gitEnv(secret);
  // Data directories are copied between runs, so start from the server's branch tip.
  execFileSync("git", ["fetch", "--quiet", "origin", PUSH_BRANCH], { cwd: clone, env, stdio: ["ignore", "inherit", "inherit"] });
  execFileSync("git", ["checkout", "--quiet", "-B", PUSH_BRANCH, `origin/${PUSH_BRANCH}`], { cwd: clone, env, stdio: ["ignore", "inherit", "inherit"] });

  const listener = await listen(options.hookPort);
  const report = {
    schema: 1, label: options.label, startedAt: new Date().toISOString(), origin: base,
    machine: { platform: platform(), release: release(), cpus: cpus().length, cpu: cpus()[0]?.model, memoryGiB: Math.round(totalmem() / 2 ** 30) },
    options: { levels: options.levels, seconds: options.seconds, users: options.users, seed: options.seed, pushIntervalMs: options.pushIntervalMs, mix: MIX },
    levels: [],
  };
  try {
    for (const concurrency of options.levels) {
      const result = await level(options, { base, users, pull, clone, env, listener }, concurrency);
      report.levels.push(result);
      print(result);
    }
  } finally {
    await listener.close();
  }
  if (options.output) {
    mkdirSync(dirname(resolve(options.output)), { recursive: true });
    writeFileSync(resolve(options.output), `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
    console.log(`report: ${resolve(options.output)}`);
  }
  if (report.levels.some((result) => result.failed > 0)) process.exitCode = 1;
}

function print(result) {
  const { pool, operations } = result;
  console.log(`\nconcurrency ${result.concurrency} for ${result.seconds} s: ${result.failed} failed, ${result.webhookDeliveriesReceived} webhook deliveries`);
  for (const [name, op] of Object.entries(operations)) {
    console.log(`  ${name.padEnd(15)} ${String(op.count).padStart(6)} (${op.perSecond}/s)  p50 ${op.p50Ms} ms  p95 ${op.p95Ms} ms  failed ${op.failed}`);
  }
  const line = (name, h) => `  ${name.padEnd(22)} n=${h.count} mean ${h.meanMs} ms  p95 ${h.p95Ms} ms  p99 ${h.p99Ms} ms`;
  console.log(line("acquire (query)", pool.acquireQuery));
  console.log(line("acquire (transaction)", pool.acquireTransaction));
  console.log(line("write lock wait", pool.writeLockWait));
  console.log(line("write transaction", pool.writeTransactions));
  for (const [store, h] of Object.entries(pool.writeTransactionsByStore)) console.log(`  ${line(`  tx ${store}`, h)}`);
  for (const [store, h] of Object.entries(pool.writeLockWaitByStore)) console.log(`  ${line(`  lock ${store}`, h)}`);
  console.log(`  busy errors ${pool.busyErrors} ${JSON.stringify(pool.busy)}, acquire timeouts ${pool.acquireTimeouts}, 5xx ${pool.serverErrors}`);
  for (const example of result.failureExamples) console.log(`  failure: ${JSON.stringify(example)}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = parseArgs(process.argv.slice(2));
  (options.command === "setup" ? setup(options) : run(options)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  }).finally(() => agent.destroy());
}
