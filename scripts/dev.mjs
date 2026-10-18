// Live development server: rebuilds and restarts rbit whenever Rust sources,
// templates or manifests change, and keeps the stylesheet compiled with Tailwind's
// watcher. Debug builds read `assets/` from disk, so CSS and JS changes need no
// restart; refresh the browser to see any change.
//
//   npm run dev            # http://localhost:3000, data in ./data-live
//
// Builds go to target/live so other cargo commands never fight over a locked
// executable. RBIT_STORAGE__DATA_DIR overrides the data directory; the first run
// copies ./data into ./data-live when it exists, so experiments never touch ./data.

import { spawn } from "node:child_process";
import { copyFileSync, cpSync, existsSync, statSync, watch } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const targetDir = join(root, "target", "live");
const exe = join(targetDir, "debug", process.platform === "win32" ? "rbit.exe" : "rbit");
const runExe = join(targetDir, process.platform === "win32" ? "rbit-dev.exe" : "rbit-dev");
const dataDir = process.env.RBIT_STORAGE__DATA_DIR ?? join(root, "data-live");

if (!process.env.RBIT_STORAGE__DATA_DIR && !existsSync(dataDir) && existsSync(join(root, "data"))) {
  cpSync(join(root, "data"), dataDir, { recursive: true });
  log(`copied ./data to ${dataDir}`);
}

const env = {
  ...process.env,
  CARGO_TARGET_DIR: targetDir,
  RBIT_STORAGE__DATA_DIR: dataDir,
  RBIT_MAIL__TRANSPORT: process.env.RBIT_MAIL__TRANSPORT ?? "log",
};

let server = null;
let building = false;
let pending = false;
let timer = null;
/** Last seen modification time per watched path. */
const stamps = new Map();

function log(message) {
  console.log(`[dev ${new Date().toLocaleTimeString()}] ${message}`);
}

function run(command, args) {
  return new Promise((done) => {
    const child = spawn(command, args, { cwd: root, env, stdio: "inherit", shell: process.platform === "win32" });
    child.on("exit", (code) => done(code === 0));
  });
}

function stopServer() {
  if (!server) return Promise.resolve();
  const old = server;
  server = null;
  return new Promise((done) => {
    old.once("exit", done);
    old.kill();
  });
}

async function rebuild() {
  if (building) {
    pending = true;
    return;
  }
  building = true;
  log("building…");
  const ok = await run("cargo", ["build", "--package", "rbit-server"]);
  if (ok) {
    // Run a copy: Windows cannot relink an executable while it is running.
    await stopServer();
    copyFileSync(exe, runExe);
    server = spawn(runExe, ["serve"], { cwd: root, env, stdio: "inherit" });
    server.on("exit", (code, signal) => {
      if (signal || code === null) return;
      log(`server exited with code ${code}`);
    });
    log("server started on http://localhost:3000");
  } else {
    log("build failed; keeping the previous server running");
  }
  building = false;
  if (pending) {
    pending = false;
    rebuild();
  }
}

function schedule(path) {
  // Only what the binary is built from. Cargo rewrites Cargo.lock during builds and
  // Tailwind owns styles/ and assets/, so those would only cause needless restarts.
  if (!path || /(^|[\\/])(target|node_modules|assets|styles)([\\/]|$)/.test(path)) return;
  if (!/\.(rs|html|toml|sql|json|svg)$/.test(path)) return;
  // Windows reports reads (indexers, scanners, other builds) as changes: rebuild only
  // when the file's modification time or existence actually changed.
  let stamp = "gone";
  try {
    stamp = String(statSync(join(root, path)).mtimeMs);
  } catch {
    /* deleted */
  }
  if (stamps.get(path) === stamp) return;
  const first = !stamps.has(path);
  stamps.set(path, stamp);
  if (first && stamp !== "gone" && Date.now() - Number(stamp) > 5000) return;
  clearTimeout(timer);
  timer = setTimeout(() => {
    log(`change in ${path}`);
    rebuild();
  }, 400);
}

watch(join(root, "crates"), { recursive: true }, (_event, file) => schedule(file && join("crates", file)));
watch(join(root, "Cargo.toml"), () => schedule("Cargo.toml"));

const css = spawn("npm", ["run", "watch:css"], { cwd: root, stdio: "inherit", shell: process.platform === "win32" });
const js = spawn("npm", ["run", "watch:js"], { cwd: root, stdio: "inherit", shell: process.platform === "win32" });

function shutdown() {
  css.kill();
  js.kill();
  stopServer().then(() => process.exit(0));
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

rebuild();
