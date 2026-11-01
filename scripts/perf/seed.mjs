// Loads the performance fixture into a running rbit (see serve.mjs): signs up a test
// account, creates the public repository perf/perf-fixture, pushes `main` and `big-change`
// over HTTP and opens pull request #1 (big-change -> main). Safe to rerun: an existing
// account signs in instead, and an existing repository or pull request is kept.
//
//   node scripts/perf/seed.mjs [base-url] [--setup-token-file PATH]
//
// The account is a throwaway for a local server only. Its generated password is kept in
// target/perf/credentials.txt (mode0600). Setup token bytes come only from an optional
// private file and the official signup form. Git gets only a file-reading credential
// helper scoped to the exact repository URL; secrets never enter child args/env/logs.
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { readCredentials, readSetupToken, repositoryUrl, requirePrivateFileSupport } from "./seed-credentials.mjs";
import { dirname, join, resolve } from "node:path";

export function seedOptions(argv, root) {
  let base = "http://127.0.0.1:3005", setupTokenFile;
  if (argv[0] && !argv[0].startsWith("--")) base = argv.shift();
  if (argv.length) {
    if (argv.length !== 2 || argv[0] !== "--setup-token-file" || !argv[1]) throw new Error("usage: seed.mjs [base-url] [--setup-token-file PATH]");
    setupTokenFile = resolve(argv[1]);
  }
  const url = repositoryUrl(base);
  return { base: url.href.replace(/\/$/, ""), setupTokenFile, fixture: join(root, "target/perf/fixture"), credentialsFile: join(root, "target/perf/credentials.txt") };
}
export async function seed(options, { git = execFileSync, log = console.log } = {}) {
  const { base, setupTokenFile, fixture, credentialsFile } = options;
  repositoryUrl(base); // Also defend direct programmatic callers before private reads or I/O.
  requirePrivateFileSupport(); // Refuse unsupported privacy checks before creating credentials or contacting the server.
  const USER = "perf";
  const REPO = "perf-fixture";

  if (!existsSync(join(fixture, ".git"))) {
    throw new Error("no fixture: run `node scripts/perf/fixture.mjs` first");
  }

  // --- Credentials --------------------------------------------------------------------------
  /** `login:password` on one line, the same shape Git's credential store uses. */
  function credentials() {
    if (existsSync(credentialsFile)) {
      return readCredentials(credentialsFile);
    }
    const password = randomBytes(18).toString("hex");
    mkdirSync(dirname(credentialsFile), { recursive: true });
    writeFileSync(credentialsFile, `${USER}:${password}\n`, { flag: "wx", mode: 0o600 });
    return { login: USER, password };
  }
  const setupToken = setupTokenFile ? readSetupToken(setupTokenFile) : "";
  const { login, password } = credentials();

  // --- A tiny cookie-keeping client -----------------------------------------------------------
  // The server checks Origin on form posts, as browsers send it, so every request carries it.
  const cookies = new Map();
  async function request(method, path, form) {
    const headers = { Origin: new URL(base).origin };
    if (cookies.size) headers.Cookie = [...cookies].map(([k, v]) => `${k}=${v}`).join("; ");
    if (form) headers["Content-Type"] = "application/x-www-form-urlencoded";
    const response = await fetch(base + path, {
      method,
      headers,
      body: form ? new URLSearchParams(form) : undefined,
      redirect: "manual",
    });
    for (const line of response.headers.getSetCookie()) {
      const [pair] = line.split(";");
      const at = pair.indexOf("=");
      cookies.set(pair.slice(0, at), pair.slice(at + 1));
    }
    return response;
  }
  const get = (path) => request("GET", path);
  const post = (path, form) => request("POST", path, form);
  function redirected(response, path) {
    if (response.status !== 303) return false;
    try {
      const destination = new URL(response.headers.get("location"), base + "/");
      const expected = new URL(base + path);
      return destination.origin === expected.origin && !destination.username && !destination.password && !destination.search && !destination.hash &&
        (destination.pathname === expected.pathname || destination.pathname === path);
    } catch { return false; }
  }
  const authenticated = (response) => redirected(response, "/") && response.headers.getSetCookie().some((line) => /^rbit_session=[^;]+;/.test(line));

  async function waitForServer() {
    for (let attempt = 0; attempt < 60; attempt++) {
      try {
        if ((await fetch(`${base}/healthz`, { redirect: "manual" })).ok) return;
      } catch {
        // Not listening yet.
      }
      await new Promise((done) => setTimeout(done, 1000));
    }
    throw new Error(`${base} did not answer /healthz within a minute; is serve.mjs running?`);
  }

  // --- Seed ---------------------------------------------------------------------------------
  await waitForServer();

  let session = await post("/signup", { username: login, email: `${login}@example.test`, password, setup_token: setupToken });
  if (authenticated(session)) {
    log(`signed up ${login} (the first account on a fresh data directory is the admin)`);
  } else {
    session = await post("/login", { login, password });
    if (!authenticated(session)) throw new Error(`could not sign up or sign in as ${login} (HTTP ${session.status})`);
    log(`signed in as ${login}`);
  }

  if ((await get(`/${login}/${REPO}`)).status === 200) {
    log(`repository ${login}/${REPO} exists`);
  } else {
    const created = await post("/new", { owner: login, name: REPO, description: "Frontend performance fixture", visibility: "public" });
    if (!redirected(created, `/${login}/${REPO}`)) throw new Error(`could not create ${login}/${REPO} (HTTP ${created.status})`);
    log(`created ${login}/${REPO}`);
  }

  const remote = `${base}/${login}/${REPO}.git`;
  const helper = `!${shellQuote(process.execPath)} ${shellQuote(fileURLToPath(new URL("./seed-credentials.mjs", import.meta.url)))} ${shellQuote(remote)} ${shellQuote(credentialsFile)}`;
  const config = [
    ["credential.helper", ""], [`credential.${remote}.helper`, ""], [`credential.${remote}.helper`, helper],
    ["credential.useHttpPath", "true"], [`credential.${remote}.useHttpPath`, "true"],
    ["http.extraHeader", ""], [`http.${remote}.extraHeader`, ""],
    ["http.followRedirects", "false"], [`http.${remote}.followRedirects`, "false"],
    ["http.proxy", ""], [`http.${remote}.proxy`, ""],
    ["protocol.allow", "never"], ["protocol.http.allow", "always"], ["protocol.https.allow", "always"],
  ];
  const env = {};
  for (const key of ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"]) if (process.env[key]) env[key] = process.env[key];
  Object.assign(env, { GIT_TERMINAL_PROMPT: "0", GIT_NO_LAZY_FETCH: "1", GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null", GIT_CONFIG_COUNT: String(config.length) });
  for (const [i, [key, value]] of config.entries()) { env[`GIT_CONFIG_KEY_${i}`] = key; env[`GIT_CONFIG_VALUE_${i}`] = value; }
  // Push from a private bare context: fixture-local configuration/hooks/URL rewrites,
  // the user's HOME/.netrc, and inherited Git environment cannot supply credentials.
  const temporary = mkdtempSync(join(tmpdir(), "rbit-perf-git-"));
  try {
    Object.assign(env, { HOME: temporary, XDG_CONFIG_HOME: temporary });
    const refs = ["main", "big-change"].map((name) => {
      const oid = String(git("git", ["rev-parse", "--verify", `refs/heads/${name}`], { cwd: fixture, stdio: ["ignore", "pipe", "pipe"], env, maxBuffer: 1024 })).trim();
      if (!/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(oid)) throw new Error();
      return [name, oid];
    });
    if (refs[0][1].length !== refs[1][1].length) throw new Error();
    const gitDir = join(temporary, "push.git");
    mkdirSync(join(gitDir, "objects", "info"), { recursive: true });
    mkdirSync(join(gitDir, "refs", "heads"), { recursive: true });
    writeFileSync(join(gitDir, "HEAD"), "ref: refs/heads/main\n");
    writeFileSync(join(gitDir, "config"), refs[0][1].length === 40 ? "[core]\nrepositoryformatversion = 0\nbare = true\n" : "[core]\nrepositoryformatversion = 1\nbare = true\n[extensions]\nobjectformat = sha256\n");
    // Git's alternates format is one absolute path per line. Reject rather than
    // interpreting an unusual filesystem path as additional alternate entries.
    const objects = resolve(fixture, ".git", "objects");
    if (/[\x00-\x1f\x7f]/.test(objects)) throw new Error();
    writeFileSync(join(gitDir, "objects", "info", "alternates"), `${objects}\n`);
    for (const [name, oid] of refs) writeFileSync(join(gitDir, "refs", "heads", name), `${oid}\n`);
    git("git", ["--git-dir", gitDir, "push", "--force", "--quiet", remote, "main", "big-change"], { cwd: temporary, stdio: ["ignore", "pipe", "pipe"], env, maxBuffer: 64 * 1024 });
  } catch { throw new Error("fixture Git push failed; captured child output was withheld"); }
  finally { rmSync(temporary, { recursive: true, force: true }); }
  log("pushed main and big-change");

  if ((await get(`/${login}/${REPO}/pull/1`)).status === 200) {
    log("pull request #1 exists");
  } else {
    const opened = await post(`/${login}/${REPO}/pulls`, {
      base: "main",
      head: "big-change",
      title: "Big change",
      body: "A large pull request for the frontend performance harness.",
    });
    if (!redirected(opened, `/${login}/${REPO}/pull/1`)) throw new Error(`could not open the pull request (HTTP ${opened.status})`);
    log(`opened /${login}/${REPO}/pull/1`);
  }

  const pages = [
    `/${login}/${REPO}/blob/main/src/huge.rs`,
    `/${login}/${REPO}/blame/main/src/huge.rs`,
    `/${login}/${REPO}/compare/main...big-change`,
    `/${login}/${REPO}/pull/1/files`,
  ];
  log("\npages to probe:");
  for (const page of pages) log(`  ${base}${page}`);

}
function shellQuote(value) { return "'" + value.replaceAll("'", "'\"'\"'") + "'"; }
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(import.meta.dirname, "..", "..");
  Promise.resolve().then(() => seed(seedOptions(process.argv.slice(2), root))).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
