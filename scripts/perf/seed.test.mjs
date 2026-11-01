import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { once } from "node:events";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { credentialReply, readCredentials, readSetupToken } from "./seed-credentials.mjs";
import { seed, seedOptions } from "./seed.mjs";

const posixTest = (name, fn) => test(name, { skip: process.platform === "win32" }, fn);
const PASSWORD = "synthetic-password-only-for-tests";
const TOKEN = "synthetic setup token only for tests";
function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "rbit-seed-test-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "target/perf/fixture/.git"), { recursive: true });
  const credentialsFile = join(root, "target/perf/credentials.txt");
  const setupTokenFile = join(root, "token.txt");
  writeFileSync(credentialsFile, `perf:${PASSWORD}\n`, { mode: 0o600 });
  writeFileSync(setupTokenFile, `${TOKEN}\n`, { mode: 0o600 });
  return { root, credentialsFile, setupTokenFile };
}
async function server(t, handler) {
  const requests = [];
  const http = createServer(async (req, res) => {
    let body = "";
    for await (const chunk of req) body += chunk;
    const request = { method: req.method, url: req.url, headers: req.headers, form: new URLSearchParams(body) };
    requests.push(request);
    const response = handler(request) ?? { status: 200 };
    res.writeHead(response.status, response.headers ?? {});
    res.end(response.body ?? "");
  });
  http.listen(0, "127.0.0.1");
  await once(http, "listening");
  t.after(() => new Promise((done) => { http.close(done); http.closeAllConnections(); }));
  return { base: `http://127.0.0.1:${http.address().port}/instance`, requests };
}
function gitProbe(calls, fail = false) {
  return (command, args, options) => {
    calls.push({ command, args, options });
    assert.equal(command, "git");
    assert.equal(JSON.stringify([args, options.env]).includes(PASSWORD), false);
    assert.equal(JSON.stringify([args, options.env]).includes(TOKEN), false);
    assert.equal(options.env.GIT_TRACE, undefined);
    assert.equal(options.env.GIT_CONFIG_PARAMETERS, undefined);
    assert.equal(options.env.GIT_CONFIG_NOSYSTEM, "1");
    assert.equal(options.env.HOME === process.env.HOME, false);
    if (args[0] === "rev-parse") return "a".repeat(40) + "\n";
    assert.deepEqual(args.slice(2, 5), ["push", "--force", "--quiet"]);
    assert.equal(readFileSync(join(args[1], "refs/heads/main"), "utf8"), "a".repeat(40) + "\n");
    const config = Array.from({ length: Number(options.env.GIT_CONFIG_COUNT) }, (_, i) => [options.env[`GIT_CONFIG_KEY_${i}`], options.env[`GIT_CONFIG_VALUE_${i}`]]);
    assert.ok(config.some(([k, v]) => k === "credential.useHttpPath" && v === "true"));
    assert.ok(config.some(([k, v]) => k === "http.followRedirects" && v === "false"));
    assert.ok(config.some(([k, v]) => k === "http.extraHeader" && v === ""));
    assert.equal(existsSync(join(options.env.HOME, ".netrc")), false);
    if (fail) throw new Error(`untrusted child output ${PASSWORD} ${TOKEN}`);
    return "";
  };
}

posixTest("official setup-token signup and push contain no secrets in child args/env/logs", async (t) => {
  const f = fixture(t), calls = [], logs = [];
  const poisoned = ["GIT_TRACE", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_VALUE_999", "HTTP_PROXY"];
  const before = poisoned.map((key) => [key, process.env[key]]);
  for (const key of poisoned) process.env[key] = PASSWORD;
  t.after(() => { for (const [key, value] of before) { if (value === undefined) delete process.env[key]; else process.env[key] = value; } });
  const s = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 303, headers: { "Set-Cookie": "rbit_session=synthetic; Path=/", Location: "/" } } : undefined);
  const options = seedOptions([s.base, "--setup-token-file", f.setupTokenFile], f.root);
  await seed(options, { git: gitProbe(calls), log: (s) => logs.push(s) });
  const signup = s.requests.find((r) => r.url.endsWith("/signup"));
  assert.equal(signup.form.get("setup_token"), TOKEN);
  assert.equal(signup.form.get("password"), PASSWORD);
  assert.equal(signup.headers.origin, new URL(s.base).origin);
  assert.equal(s.requests.some((r) => r.url.endsWith("/login")), false);
  assert.equal(calls.length, 3);
  assert.equal(existsSync(calls[0].options.env.HOME), false);
  assert.equal(logs.join("\n").includes(TOKEN), false);
  assert.equal(logs.join("\n").includes(PASSWORD), false);
});

posixTest("existing account fallback remains usable without a token; fresh account fails without one", async (t) => {
  const f = fixture(t), calls = [];
  const existing = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 422 } : url.endsWith("/login") ? { status: 303, headers: { Location: "/", "Set-Cookie": "rbit_session=synthetic; Path=/" } } : undefined);
  await seed(seedOptions([existing.base], f.root), { git: gitProbe(calls), log() {} });
  assert.equal(existing.requests.find((r) => r.url.endsWith("/signup")).form.get("setup_token"), "");
  const fresh = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 403 } : url.endsWith("/login") ? { status: 401 } : undefined);
  await assert.rejects(seed(seedOptions([fresh.base], f.root), { git() { throw new Error("must not push"); }, log() {} }), /could not sign up or sign in/);
});

posixTest("manual HTTP redirects never replay signup/login bodies", async (t) => {
  const f = fixture(t);
  const s = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 307, headers: { Location: "/trap" } } : undefined);
  await assert.rejects(seed(seedOptions([s.base], f.root), { git() { throw new Error("must not push"); }, log() {} }), /could not sign up or sign in/);
  assert.equal(s.requests.some((r) => r.url === "/trap"), false);
});

posixTest("private files reject malformed, missing, oversized and insecure inputs with static errors", (t) => {
  const f = fixture(t);
  assert.throws(() => readSetupToken(join(f.root, "missing")), /^Error: private input/);
  for (const bytes of ["", "\n", "bad\0token", Buffer.from([0xff]), "x".repeat(4097)]) {
    writeFileSync(f.setupTokenFile, bytes);
    assert.throws(() => readSetupToken(f.setupTokenFile), /private input|malformed private setup token/);
  }
  writeFileSync(f.credentialsFile, "wrong:secret");
  assert.throws(() => readCredentials(f.credentialsFile), /malformed private fixture credentials/);
  if (process.platform !== "win32") {
    const link = join(f.root, "token-link");
    symlinkSync(f.setupTokenFile, link);
    assert.throws(() => readSetupToken(link), /private input/);
    chmodSync(f.setupTokenFile, 0o644);
    assert.throws(() => readSetupToken(f.setupTokenFile), /private input/);
  }
});

posixTest("invalid setup file fails before network, and child errors never expose captured output", async (t) => {
  const f = fixture(t), s = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 303, headers: { Location: "/", "Set-Cookie": "rbit_session=synthetic; Path=/" } } : undefined);
  writeFileSync(f.setupTokenFile, "bad\0token");
  await assert.rejects(seed(seedOptions([s.base, "--setup-token-file", f.setupTokenFile], f.root)), /malformed private setup token/);
  assert.deepEqual(s.requests, []);
  await assert.rejects(seed(seedOptions([s.base], f.root), { git: gitProbe([], true), log() {} }), { message: "fixture Git push failed; captured child output was withheld" });
});

posixTest("credential helper binds protocol, literal host+port and exact repository path before reading secrets", (t) => {
  const f = fixture(t), remote = "http://127.0.0.1:3005/instance/perf/perf-fixture.git";
  const input = "protocol=http\nhost=127.0.0.1:3005\npath=instance/perf/perf-fixture.git\n\n";
  assert.equal(credentialReply("get", remote, f.credentialsFile, input), `username=perf\npassword=${PASSWORD}\n\n`);
  for (const altered of [input.replace("http\n", "https\n"), input.replace(":3005", ":3006"), input.replace("127.0.0.1:", "localhost:"), input.replace("instance/", "other/"), input.replace(".git\n", ".git/\n"), input.replace("path=", "other="), input.replace("host=", "host=perf@"), input.replace("\n\n", "\nprotocol=http\n\n"), input.replace("instance/", "instance%2f"), "x".repeat(8193)]) {
    assert.equal(credentialReply("get", remote, join(f.root, "missing"), altered), "");
  }
  assert.equal(credentialReply("store", remote, join(f.root, "missing"), input), "");
  assert.equal(credentialReply("erase", remote, join(f.root, "missing"), input), "");
  const helper = fileURLToPath(new URL("./seed-credentials.mjs", import.meta.url));
  const env = { PATH: process.env.PATH };
  const args = [helper, remote, f.credentialsFile, "get"];
  assert.equal(JSON.stringify([args, env]).includes(PASSWORD), false);
  assert.equal(execFileSync(process.execPath, args, { input, env, encoding: "utf8" }), `username=perf\npassword=${PASSWORD}\n\n`);
});

// This is an explicit platform limit, not a chmod-based ACL claim.
test("Windows private-file access and seeding fail before any fixture I/O until ACL verification exists", { skip: process.platform !== "win32" }, async () => {
  assert.throws(() => readSetupToken("unused"), /Windows ACL verification is unavailable/);
  await assert.rejects(seed({ base: "http://127.0.0.1:3005", fixture: "unused", credentialsFile: "unused" }), /Windows ACL verification is unavailable/);
});

posixTest("real Git credential lookup runs only inside the private push context", async (t) => {
  const f = fixture(t);
  writeFileSync(join(f.root, "target/perf/fixture/.git/config"), "[credential]\nhelper = !exit 99\n[http]\nextraHeader = Authorization: poisoned\n");
  const s = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 303, headers: { Location: "/", "Set-Cookie": "rbit_session=synthetic; Path=/" } } : undefined);
  let checked = false;
  await seed(seedOptions([s.base], f.root), { log() {}, git(command, args, options) {
    if (args[0] === "rev-parse") return "a".repeat(40) + "\n";
    const url = new URL(`${s.base}/perf/perf-fixture.git`);
    const input = `protocol=${url.protocol.slice(0, -1)}\nhost=${url.host}\npath=${url.pathname.slice(1)}\n\n`;
    const reply = execFileSync(command, ["--git-dir", args[1], "credential", "fill"], { ...options, input, stdio: ["pipe", "pipe", "pipe"], encoding: "utf8" });
    assert.ok(reply.includes(`password=${PASSWORD}\n`));
    assert.equal(options.env.GIT_NO_LAZY_FETCH, "1");
    checked = true;
    return "";
  } });
  assert.equal(checked, true);
});

posixTest("repository and PR creation accept only exact local redirect destinations", async (t) => {
  const f = fixture(t), logs = [];
  const s = await server(t, ({ method, url }) => {
    if (url.endsWith("/signup")) return { status: 303, headers: { Location: "/", "Set-Cookie": "rbit_session=synthetic; Path=/" } };
    if (method === "GET" && (url.endsWith("/perf/perf-fixture") || url.endsWith("/pull/1"))) return { status: 404 };
    if (url.endsWith("/new")) return { status: 303, headers: { Location: "/instance/perf/perf-fixture" } };
    if (url.endsWith("/pulls")) return { status: 303, headers: { Location: "/perf/perf-fixture/pull/1" } };
  });
  await seed(seedOptions([s.base], f.root), { git: gitProbe([]), log: (line) => logs.push(line) });
  assert.ok(logs.includes("opened /perf/perf-fixture/pull/1"));
  const bad = await server(t, ({ url }) => url.endsWith("/signup") ? { status: 303, headers: { Location: `http://elsewhere.invalid/?${TOKEN}`, "Set-Cookie": "rbit_session=synthetic; Path=/" } } : url.endsWith("/login") ? { status: 401 } : undefined);
  await assert.rejects(seed(seedOptions([bad.base], f.root), { git() { throw new Error("must not push"); }, log: (line) => logs.push(line) }), /could not sign up or sign in/);
  assert.equal(logs.join("\n").includes(TOKEN), false);
});

posixTest("FIFO private input is rejected without waiting for a writer", (t) => {
  const f = fixture(t), fifo = join(f.root, "token-fifo");
  execFileSync("mkfifo", [fifo]);
  const module = new URL("./seed-credentials.mjs", import.meta.url).href;
  const child = spawnSync(process.execPath, ["--input-type=module", "-e", `import { readSetupToken } from ${JSON.stringify(module)}; try { readSetupToken(process.argv[1]); process.exitCode = 2; } catch (error) { console.error(error.message); process.exitCode = 1; }`, fifo], { encoding: "utf8", timeout: 2000 });
  assert.equal(child.error, undefined);
  assert.equal(child.signal, null);
  assert.equal(child.status, 1);
  assert.match(child.stderr, /private input must be an owned regular/);
});

posixTest("external and ambiguous fixture origins fail before private reads or network", async (t) => {
  const f = fixture(t);
  // Nonexistent secret paths distinguish URL rejection from private-file access.
  const missing = join(f.root, "missing-secret");
  for (const base of ["https://example.invalid", "http://localhost:3005", "http://127.1:3005", "http://2130706433:3005", "http://0x7f000001:3005", "http://0177.0.0.1:3005", "http://127.0.0.1.example.invalid", "http://127.0.0.1@elsewhere.invalid", "http://elsewhere.invalid@127.0.0.1", "http://127%2e0.0.1", " http://127.0.0.1", "http://127.0.0.1\\@elsewhere.invalid", "http://[0:0:0:0:0:0:0:1]:3005", "file:///tmp/fixture"]) {
    assert.throws(() => seedOptions([base, "--setup-token-file", missing], f.root), /fixture URL must use literal loopback/);
    await assert.rejects(seed({ base, setupTokenFile: missing, fixture: join(f.root, "target/perf/fixture"), credentialsFile: missing }, { git() { throw new Error("must not push"); }, log() {} }), /fixture URL must use literal loopback/);
  }
  assert.equal(seedOptions(["http://127.0.0.1:3005/instance"], f.root).base, "http://127.0.0.1:3005/instance");
  assert.equal(seedOptions(["http://[::1]:3005/instance"], f.root).base, "http://[::1]:3005/instance");
});
