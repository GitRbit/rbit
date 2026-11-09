// Content for paired release comparisons beyond the scrolling fixture: Markdown
// documents (including nesting deeper than the renderer's flattening bound), fresh
// per-commit documents and source files that miss every render cache, and issue and
// pull request lists with Markdown bodies and comments.
//
//   node scripts/perf/content.mjs build [path]       # default: target/perf/content
//   node scripts/perf/content.mjs seed BASE-URL [path]
//
// `build` is deterministic (seeded PRNG, fixed identity and dates), so commit ids never
// change. `seed` signs in with the account `seed.mjs` created (same credentials file),
// creates the public repository perf/perf-content, pushes every branch, opens issues
// with comments and pull requests, and writes `content.json` next to the repository
// with the `cold` commit ids for per-round paths. It refuses to seed a repository that
// already has issues, so every seeded data directory holds the same rows.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const MARKER = "rbit-perf-content";
/** Commits on `cold`: enough for warmups plus rounds of one page per comparison. */
export const COLD_COMMITS = 64;
export const ISSUES = 120;
export const COMMENTS = 40;
export const PULLS = 24;

let seed = 0xc0_47e47;
function random() {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const int = (min, max) => min + Math.floor(random() * (max - min + 1));
const pick = (list) => list[Math.floor(random() * list.length)];
const WORDS = ["cache", "tree", "commit", "render", "branch", "review", "token", "label", "issue", "merge", "diff",
  "blob", "index", "query", "theme", "locale", "number", "limit", "budget", "worker", "stream", "page", "list"];
const words = (n) => Array.from({ length: n }, () => pick(WORDS)).join(" ");

function inline() {
  switch (int(0, 5)) {
    case 0: return `**${words(2)}**`;
    case 1: return `_${words(2)}_`;
    case 2: return `\`${pick(WORDS)}_${pick(WORDS)}()\``;
    case 3: return `[${words(2)}](https://example.test/${pick(WORDS)})`;
    case 4: return `~~${pick(WORDS)}~~`;
    default: return words(int(3, 8));
  }
}
const paragraph = () => Array.from({ length: int(3, 7) }, () => `${words(int(2, 6))} ${inline()}`).join(" ") + ".";

function rustLines(count) {
  const lines = [];
  while (lines.length < count) {
    lines.push(`/// ${words(int(3, 8))}`, `pub fn ${pick(WORDS)}_${lines.length}(${pick(WORDS)}: &str, limit: usize) -> Option<usize> {`);
    for (let i = int(3, 12); i > 0; i--) lines.push(`    let ${pick(WORDS)} = ${pick(WORDS)}.len().saturating_add(${int(0, 4096)}); // ${words(2)}`);
    lines.push("    None", "}", "");
  }
  return lines.slice(0, count);
}

/** A Markdown document of roughly `sections` sections, with `deep` very deep nestings. */
export function markdown(title, sections, deep) {
  const out = [`# ${title}`, "", paragraph(), ""];
  for (let s = 0; s < sections; s++) {
    out.push(`## ${words(3)} ${s}`, "", paragraph(), "");
    for (let i = int(3, 8); i > 0; i--) out.push(`${"  ".repeat(int(0, 3))}- ${inline()} ${words(4)}`);
    out.push("", `> ${paragraph()}`, ">", `> > ${inline()}`, "");
    out.push("| Name | Count | Note |", "| --- | ---: | --- |");
    for (let i = int(3, 6); i > 0; i--) out.push(`| ${pick(WORDS)} | ${int(0, 99999)} | ${inline()} |`);
    out.push("", "```rust", ...rustLines(int(8, 24)), "```", "", `1. ${words(4)}`, `2. ${inline()}`, "");
  }
  for (let d = 0; d < deep; d++) {
    const depth = int(40, 64);
    out.push(`## Deep nesting ${d}`, "");
    for (let i = 0; i < depth; i++) out.push(`${"  ".repeat(i)}- level ${i} ${inline()}`);
    out.push("", `${"> ".repeat(depth)}${paragraph()}`, "");
  }
  return out;
}

function git(dir, clock, ...args) {
  const date = `${clock} +0000`;
  return execFileSync("git", args, { cwd: dir, encoding: "utf8", env: { ...process.env,
    GIT_AUTHOR_NAME: "Perf Content", GIT_AUTHOR_EMAIL: "perf@example.test", GIT_AUTHOR_DATE: date,
    GIT_COMMITTER_NAME: "Perf Content", GIT_COMMITTER_EMAIL: "perf@example.test", GIT_COMMITTER_DATE: date,
    GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null" } });
}

export function build(dir) {
  if (existsSync(dir)) {
    if (!existsSync(join(dir, ".git", MARKER))) throw new Error(`${dir} exists and is not perf content`);
    rmSync(dir, { recursive: true, force: true });
  }
  mkdirSync(dir, { recursive: true });
  let clock = Date.UTC(2026, 1, 2, 9, 0, 0) / 1000;
  const run = (...args) => git(dir, (clock += 600), ...args);
  const write = (path, lines) => {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), `${lines.join("\n")}\n`);
  };
  const commit = (message) => { run("add", "--all"); run("commit", "--quiet", "--no-verify", "-m", message); };
  run("init", "--quiet", "--initial-branch=main");
  run("config", "core.autocrlf", "false");
  writeFileSync(join(dir, ".git", MARKER), "generated by scripts/perf/content.mjs\n");
  write("README.md", markdown("perf-content", 40, 2));
  write("docs/guide.md", markdown("Guide", 60, 0));
  write("docs/deep.md", markdown("Deep", 4, 24));
  for (let i = 0; i < 40; i++) write(`src/module_${String(i).padStart(2, "0")}.rs`, rustLines(int(80, 400)));
  commit("Import the content fixture");
  run("switch", "--quiet", "--create", "cold");
  for (let i = 0; i < COLD_COMMITS; i++) {
    write("docs/cold.md", markdown(`Cold ${i}`, 24, 1));
    write("src/cold.rs", rustLines(2_000));
    commit(`Rewrite the cold documents, ${i}`);
  }
  for (let p = 1; p <= PULLS; p++) {
    run("switch", "--quiet", "main");
    run("switch", "--quiet", "--create", `topic-${p}`);
    write(`src/module_${String(p % 40).padStart(2, "0")}.rs`, rustLines(int(80, 400)));
    write(`docs/topic-${p}.md`, markdown(`Topic ${p}`, 3, 0));
    commit(`Change topic ${p}`);
  }
  run("switch", "--quiet", "main");
  const cold = run("rev-list", "--reverse", "main..cold").trim().split("\n");
  if (cold.length !== COLD_COMMITS) throw new Error("unexpected cold history");
  return { main: run("rev-parse", "main").trim(), cold };
}

// --- Seeding over HTTP ---------------------------------------------------------------------
function client(base) {
  const cookies = new Map();
  return async function request(method, path, form) {
    for (let attempt = 0; ; attempt++) {
      const headers = { Origin: base };
      if (cookies.size) headers.Cookie = [...cookies].map(([k, v]) => `${k}=${v}`).join("; ");
      if (form) headers["Content-Type"] = "application/x-www-form-urlencoded";
      const response = await fetch(base + path, { method, headers, body: form ? new URLSearchParams(form) : undefined, redirect: "manual" });
      for (const line of response.headers.getSetCookie()) {
        const [pair] = line.split(";");
        const at = pair.indexOf("=");
        cookies.set(pair.slice(0, at), pair.slice(at + 1));
      }
      if (response.status !== 429 || attempt >= 30) return response;
      await response.arrayBuffer();
      await new Promise((done) => setTimeout(done, 1000 * Math.max(1, Number(response.headers.get("retry-after")) || 2)));
    }
  };
}

async function expectRedirect(response, what) {
  await response.arrayBuffer();
  if (response.status < 300 || response.status >= 400) throw new Error(`${what} failed (HTTP ${response.status})`);
  return response.headers.get("location");
}

export async function seedContent(base, dir, credentialsFile) {
  const origin = new URL(base);
  if (origin.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(origin.hostname) || origin.pathname !== "/") throw new Error("seed origin must be an HTTP loopback origin");
  base = origin.origin;
  const line = readFileSync(credentialsFile, "utf8").trim();
  const login = line.slice(0, line.indexOf(":"));
  const password = line.slice(line.indexOf(":") + 1);
  const request = client(base);
  await expectRedirect(await request("POST", "/login", { login, password, next: "/" }), "sign-in");
  const repo = `/${login}/perf-content`;
  if ((await request("GET", repo)).status !== 200) {
    await expectRedirect(await request("POST", "/new", { owner: login, name: "perf-content", description: "Markdown, list and cold-render fixture", visibility: "public" }), "repository creation");
  }
  if ((await request("GET", `${repo}/issues/1`)).status === 200) throw new Error("perf-content already has issues; seed a fresh data directory");
  const branches = ["main", "cold", ...Array.from({ length: PULLS }, (_, i) => `topic-${i + 1}`)];
  const basic = Buffer.from(`${login}:${password}`).toString("base64");
  execFileSync("git", ["push", "--force", "--quiet", `${base}${repo}.git`, ...branches], { cwd: dir, stdio: ["ignore", "inherit", "inherit"], env: { ...process.env,
    GIT_TERMINAL_PROMPT: "0", GIT_CONFIG_COUNT: "2", GIT_CONFIG_KEY_0: "credential.helper", GIT_CONFIG_VALUE_0: "",
    GIT_CONFIG_KEY_1: "http.extraHeader", GIT_CONFIG_VALUE_1: `Authorization: Basic ${basic}` } });
  seed = 0x155_0e5;
  for (let i = 1; i <= ISSUES; i++) {
    await expectRedirect(await request("POST", `${repo}/issues`, { title: `${words(int(3, 7))} ${i}`, body: markdown(`Issue ${i}`, 1, 0).join("\n") }), `issue ${i}`);
  }
  for (let c = 0; c < COMMENTS; c++) {
    await expectRedirect(await request("POST", `${repo}/issues/1/comments`, { body: markdown(`Comment ${c}`, 1, c % 10 === 0 ? 1 : 0).join("\n") }), `comment ${c}`);
  }
  for (let p = 1; p <= PULLS; p++) {
    await expectRedirect(await request("POST", `${repo}/pulls`, { base: "main", head: `topic-${p}`, title: `Topic ${p}: ${words(4)}`, body: markdown(`Topic ${p}`, 1, 0).join("\n") }), `pull request ${p}`);
  }
  const cold = git(dir, 0, "rev-list", "--reverse", "main..cold").trim().split("\n");
  const record = { repository: repo, main: git(dir, 0, "rev-parse", "main").trim(), cold, issues: ISSUES, comments: COMMENTS, pulls: PULLS, firstPull: ISSUES + 1 };
  writeFileSync(join(dirname(dir), "content.json"), `${JSON.stringify(record, null, 2)}\n`);
  return record;
}

async function main() {
  const [command, ...rest] = process.argv.slice(2);
  if (command === "build") {
    const dir = resolve(root, rest[0] ?? join("target", "perf", "content"));
    const result = build(dir);
    console.log(`content at ${dir}\nmain ${result.main}\ncold ${result.cold[0]}..${result.cold.at(-1)} (${result.cold.length})`);
  } else if (command === "seed" && rest[0]) {
    const dir = resolve(root, rest[1] ?? process.env.RBIT_PERF_CONTENT ?? join("target", "perf", "content"));
    const credentials = resolve(root, process.env.RBIT_PERF_CREDENTIALS_FILE ?? join("target", "perf", "credentials.txt"));
    const record = await seedContent(rest[0], dir, credentials);
    console.log(`seeded ${record.repository}: ${record.issues} issues, ${record.comments} comments on #1, pull requests #${record.firstPull}-#${record.firstPull + record.pulls - 1}`);
  } else {
    throw new Error("usage: content.mjs build [path] | content.mjs seed BASE-URL [path]");
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
