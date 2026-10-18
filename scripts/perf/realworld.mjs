// Full-history upstream repositories, exercised through the real HTTP transports.
// Run against a local development server: npm run test:realworld -- http://localhost:3000
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes, createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
const base = new URL(process.argv[2] ?? "http://localhost:3000");
assert(["localhost", "127.0.0.1", "[::1]"].includes(base.hostname) && !base.username && !base.password,
  "Use a local development server; this creates public test repositories.");
assert(base.pathname === "/" && !base.search && !base.hash, "Expected a server origin");
const origin = base.origin;
const work = join(root, "target/realworld");
mkdirSync(join(work, "sources"), { recursive: true });
const credentialsFile = join(work, `credentials-${base.port || "default"}.json`);
if (!existsSync(credentialsFile)) {
  writeFileSync(credentialsFile, JSON.stringify({ login: "realworld", password: randomBytes(24).toString("hex") }), { mode: 0o600 });
}
const { login, password } = JSON.parse(readFileSync(credentialsFile, "utf8"));
const cookies = new Map();
const report = { server: origin, startedAt: new Date().toISOString(), repositories: [], failures: [] };
const privateSources = new Set();
const projects = [
  { name: "rbit", file: "crates/rbit-server/src/main.rs", language: "Rust", readme: "README.md" },
  { name: "git", url: "https://github.com/git/git.git", file: "git.c", language: "C", readme: "README.md" },
];

function redactSources(message) {
  for (const source of privateSources) {
    message = message.replaceAll(source, "[test source]");
    try { message = message.replaceAll(new URL(source).hostname, "[source host]"); } catch { /* local source */ }
  }
  return message;
}

function git(cwd, args, authenticated = false) {
  return new Promise((done, reject) => {
    const env = { ...process.env, GIT_TERMINAL_PROMPT: "0" };
    if (authenticated) Object.assign(env, {
      GIT_CONFIG_COUNT: "2", GIT_CONFIG_KEY_0: "credential.helper", GIT_CONFIG_VALUE_0: "",
      GIT_CONFIG_KEY_1: `http.${origin}/.extraHeader`,
      GIT_CONFIG_VALUE_1: `Authorization: Basic ${Buffer.from(`${login}:${password}`).toString("base64")}`,
    });
    const child = spawn("git", args, { cwd, env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] });
    const out = [], err = [];
    child.stdout.on("data", chunk => out.push(chunk));
    child.stderr.on("data", chunk => err.push(chunk));
    child.on("error", reject);
    child.on("exit", code => code === 0 ? done(Buffer.concat(out)) : reject(new Error(redactSources(`git ${args[0]} failed (${code}): ${Buffer.concat(err)}`))));
  });
}
const text = async (cwd, args) => (await git(cwd, args)).toString("utf8").trim();
async function request(path, form) {
  const headers = { Origin: origin, Cookie: [...cookies].map(([key, value]) => `${key}=${value}`).join("; ") };
  const response = await fetch(origin + path, {
    method: form ? "POST" : "GET", headers, body: form ? new URLSearchParams(form) : undefined,
    redirect: "manual", signal: AbortSignal.timeout(120_000),
  });
  for (const cookie of response.headers.getSetCookie()) {
    const pair = cookie.split(";")[0], at = pair.indexOf("=");
    cookies.set(pair.slice(0, at), pair.slice(at + 1));
  }
  return response;
}
async function post(path, form) {
  const response = await request(path, form);
  await response.arrayBuffer();
  assert.equal(response.status, 303, `${path}: expected successful form redirect, got ${response.status}`);
}
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const encodePath = path => path.split("/").map(encodeURIComponent).join("/");

try {
  assert.equal((await request("/healthz")).status, 200, "Server is unhealthy");
  let session = await request("/login", { login, password });
  await session.arrayBuffer();
  if (session.status !== 303) await post("/signup", { username: login, email: `${login}@example.test`, password });
  for (const project of projects) {
    project.url ??= process.env.RBIT_TEST_SOURCE ?? await text(root, ["remote", "get-url", "origin"]);
    privateSources.add(project.url);
    console.log(`Preparing ${project.name}: full upstream history, branches and tags`);
    const source = join(work, "sources", `${project.name}.git`);
    if (existsSync(source)) {
      assert.equal(await text(source, ["remote", "get-url", "origin"]), project.url, "Unexpected cached upstream");
      await git(source, ["fetch", "--prune", "origin"]);
    } else await git(root, ["clone", "--mirror", project.url, source]);
    assert.equal(await text(source, ["rev-parse", "--is-shallow-repository"]), "false", "Scale test requires full history");
    const branch = (await text(source, ["symbolic-ref", "--short", "HEAD"]));
    const head = await text(source, ["rev-parse", "HEAD"]);
    const parent = await text(source, ["rev-parse", "HEAD~1"]);
    const refs = await text(source, ["for-each-ref", "--format=%(objectname) %(refname)", "refs/heads", "refs/tags"]);
    const result = {
      name: project.name, head, branch,
      commits: Number(await text(source, ["rev-list", "--count", "--branches", "--tags"])),
      files: (await text(source, ["ls-tree", "-r", "--name-only", "HEAD"])).split("\n").length,
      refs: refs.split("\n").length, pages: [],
    };
    report.repositories.push(result);
    const home = `/${login}/${project.name}`;
    const existing = await request(home);
    await existing.arrayBuffer();
    if (existing.status === 404) await post("/new", {
      owner: login, name: project.name, description: `${project.name} — full-history test repository`, visibility: "public",
    });
    else assert.equal(existing.status, 200, "Unable to inspect existing test repository");
    const remote = origin + home + ".git";
    const pushStart = performance.now();
    await git(source, ["push", remote, "refs/heads/*:refs/heads/*", "refs/tags/*:refs/tags/*"], true);
    result.pushMs = Math.round(performance.now() - pushStart);
    console.log(`  pushed ${result.commits} commits and ${result.refs} refs in ${result.pushMs} ms`);
    await post(home + "/settings", { description: `${project.name} — full-history test repository`, visibility: "public", default_branch: branch });
    const remoteRefs = (await text(root, ["ls-remote", "--refs", remote])).split("\n").map(line => line.replace("\t", " ")).sort();
    assert.deepEqual(remoteRefs, refs.split("\n").sort(), `${project.name}: advertised refs differ from upstream`);

    // Unique destination: repeated runs never overwrite a previous clone.
    const clone = join(work, `${project.name}-clone-${Date.now()}.git`);
    const cloneStart = performance.now();
    await git(root, ["clone", "--bare", remote, clone]);
    result.cloneMs = Math.round(performance.now() - cloneStart);
    console.log(`  HTTP clone completed in ${result.cloneMs} ms; checking integrity`);
    assert.equal(await text(clone, ["rev-parse", "HEAD"]), head, "HTTP clone lost upstream HEAD");
    assert.equal(await text(clone, ["rev-list", "--count", "--branches", "--tags"]), String(result.commits), "HTTP clone lost history");
    assert.equal(await text(clone, ["for-each-ref", "--format=%(objectname) %(refname)", "refs/heads", "refs/tags"]), refs, "HTTP clone lost refs");
    await git(clone, ["fsck", "--full", "--strict"]);
    // Fetch a newly advertised branch, then delete only the branch this run created.
    const probe = `realworld/fetch-${randomBytes(8).toString("hex")}`;
    await git(source, ["push", remote, `${head}:refs/heads/${probe}`], true);
    try {
      await git(clone, ["fetch", "origin", `refs/heads/${probe}:refs/test/fetch-probe`]);
      assert.equal(await text(clone, ["rev-parse", "refs/test/fetch-probe"]), head, "Incremental fetch lost new branch");
    } finally {
      await git(source, ["push", remote, `:refs/heads/${probe}`], true);
      await git(clone, ["update-ref", "-d", "refs/test/fetch-probe"]);
    }
    result.incrementalFetch = "new branch fetched and deleted successfully";
    result.integrity = "full strict fsck passed; refs, HEAD and history match upstream";

    const paths = [
      ["tree", home, project.readme],
      ["entry commits", `${home}/tree-commits/${head}`, "hx-swap-oob"],
      ["history", `${home}/commits/${branch}`, "commit"],
      ["branches", `${home}/branches`, branch],
      ["branch graph", `${home}/branches?view=tree`, "Branch commit graph"],
      ["tags", `${home}/tags`, "tag"],
      ["blob", `${home}/blob/${head}/${encodePath(project.file)}`, project.file.split("/").at(-1)],
      ["blame", `${home}/blame/${head}/${encodePath(project.file)}`, project.file.split("/").at(-1)],
      ["commit", `${home}/commit/${head}`, head.slice(0, 7)],
      ["compare", `${home}/compare/${parent}...${head}`, "diff"],
      ["languages", `${home}/language-files?revision=${head}&language=${project.language}`, project.file.split("/").at(-1)],
    ];
    for (const [name, path, expected] of paths) {
      const start = performance.now();
      const response = await request(path);
      const body = await response.text();
      const page = { name, path, status: response.status, ms: Math.round(performance.now() - start), bytes: Buffer.byteLength(body) };
      result.pages.push(page);
      try {
        assert.equal(response.status, 200, `${path}: HTTP ${response.status}`);
        assert(body.includes(expected), `${path}: missing ${expected}`);
      } catch (error) { report.failures.push(error.message); }
      console.log(`  ${name}: HTTP ${page.status}, ${page.ms} ms, ${page.bytes} bytes`);
    }
    for (const file of [project.readme, project.file]) {
      const response = await request(`${home}/raw/${head}/${encodePath(file)}`);
      assert.equal(response.status, 200);
      assert.equal(hash(Buffer.from(await response.arrayBuffer())), hash(await git(source, ["show", `${head}:${file}`])), `${file}: raw bytes differ from upstream`);
    }
    result.rawBytesMatch = true;
    const burstStart = performance.now();
    result.concurrentReads = await Promise.all([home, `${home}/commits/${branch}`, `${home}/branches?view=tree`, `${home}/blob/${head}/${encodePath(project.file)}`].map(async path => {
      const response = await request(path);
      await response.arrayBuffer();
      assert.equal(response.status, 200, `Concurrent request failed: ${path}`);
      return { path, status: response.status };
    }));
    result.concurrentReadsMs = Math.round(performance.now() - burstStart);
    console.log(`${origin}${home}: ${result.commits} commits, ${result.files} files, ${result.refs} refs; clone integrity verified`);
  }
} catch (error) {
  report.failures.push(redactSources(error.stack ?? String(error)));
} finally {
  report.finishedAt = new Date().toISOString();
  writeFileSync(join(work, "report.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(`Report: ${join(work, "report.json")}`);
}
if (report.failures.length) {
  console.error(report.failures.join("\n"));
  process.exitCode = 1;
}
