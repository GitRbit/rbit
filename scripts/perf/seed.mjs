// Loads the performance fixture into a running rbit (see serve.mjs): signs up a test
// account, creates the public repository perf/perf-fixture, pushes `main` and `big-change`
// over HTTP and opens pull request #1 (big-change -> main). Safe to rerun: an existing
// account signs in instead, and an existing repository or pull request is kept.
//
//   node scripts/perf/seed.mjs [base-url]     # default: http://localhost:3005
//
// The account is a throwaway for a local server only. Its generated password is kept in
// target/perf/credentials.txt (target/ is gitignored) and never printed. Git receives it
// through GIT_CONFIG_* environment variables with the credential helper disabled, so it
// never lands in a command line, a remote URL or the system credential store.
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const base = (process.argv[2] ?? "http://localhost:3005").replace(/\/$/, "");
const fixture = join(root, "target", "perf", "fixture");
const credentialsFile = join(root, "target", "perf", "credentials.txt");
const USER = "perf";
const REPO = "perf-fixture";

if (!existsSync(join(fixture, ".git"))) {
  console.error("no fixture: run `node scripts/perf/fixture.mjs` first");
  process.exit(1);
}

// --- Credentials --------------------------------------------------------------------------
/** `login:password` on one line, the same shape Git's credential store uses. */
function credentials() {
  if (existsSync(credentialsFile)) {
    const [login, password] = readFileSync(credentialsFile, "utf8").trim().split(":");
    return { login, password };
  }
  const password = randomBytes(18).toString("hex");
  mkdirSync(dirname(credentialsFile), { recursive: true });
  writeFileSync(credentialsFile, `${USER}:${password}\n`);
  return { login: USER, password };
}
const { login, password } = credentials();

// --- A tiny cookie-keeping client -----------------------------------------------------------
// The server checks Origin on form posts, as browsers send it, so every request carries it.
const cookies = new Map();
async function request(method, path, form) {
  const headers = { Origin: base };
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
const redirected = (response) => response.status >= 300 && response.status < 400;

async function waitForServer() {
  for (let attempt = 0; attempt < 60; attempt++) {
    try {
      if ((await fetch(`${base}/healthz`)).ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 1000));
  }
  throw new Error(`${base} did not answer /healthz within a minute; is serve.mjs running?`);
}

// --- Seed ---------------------------------------------------------------------------------
await waitForServer();

let session = await post("/signup", { username: login, email: `${login}@example.test`, password });
if (redirected(session)) {
  console.log(`signed up ${login} (the first account on a fresh data directory is the admin)`);
} else {
  session = await post("/login", { login, password });
  if (!redirected(session)) throw new Error(`could not sign up or sign in as ${login} (HTTP ${session.status})`);
  console.log(`signed in as ${login}`);
}

if ((await get(`/${login}/${REPO}`)).status === 200) {
  console.log(`repository ${login}/${REPO} exists`);
} else {
  const created = await post("/new", { owner: login, name: REPO, description: "Frontend performance fixture", visibility: "public" });
  if (!redirected(created)) throw new Error(`could not create ${login}/${REPO} (HTTP ${created.status})`);
  console.log(`created ${login}/${REPO}`);
}

const basic = Buffer.from(`${login}:${password}`).toString("base64");
execFileSync("git", ["push", "--force", "--quiet", `${base}/${login}/${REPO}.git`, "main", "big-change"], {
  cwd: fixture,
  stdio: ["ignore", "inherit", "inherit"],
  env: {
    ...process.env,
    GIT_TERMINAL_PROMPT: "0",
    GIT_CONFIG_COUNT: "2",
    GIT_CONFIG_KEY_0: "credential.helper",
    GIT_CONFIG_VALUE_0: "",
    GIT_CONFIG_KEY_1: "http.extraHeader",
    GIT_CONFIG_VALUE_1: `Authorization: Basic ${basic}`,
  },
});
console.log("pushed main and big-change");

if ((await get(`/${login}/${REPO}/pull/1`)).status === 200) {
  console.log("pull request #1 exists");
} else {
  const opened = await post(`/${login}/${REPO}/pulls`, {
    base: "main",
    head: "big-change",
    title: "Big change",
    body: "A large pull request for the frontend performance harness.",
  });
  if (!redirected(opened)) throw new Error(`could not open the pull request (HTTP ${opened.status})`);
  console.log(`opened ${opened.headers.get("location")}`);
}

const pages = [
  `/${login}/${REPO}/blob/main/src/huge.rs`,
  `/${login}/${REPO}/blame/main/src/huge.rs`,
  `/${login}/${REPO}/compare/main...big-change`,
  `/${login}/${REPO}/pull/1/files`,
];
console.log("\npages to probe:");
for (const page of pages) console.log(`  ${base}${page}`);
