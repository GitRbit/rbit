// Live development server: rebuilds and restarts rbit whenever Rust sources,
// templates or manifests change, and keeps the stylesheet compiled with Tailwind's
// watcher. Debug builds read `assets/` from disk, so CSS and JS changes need no
// restart; refresh the browser to see any change.
//
//   npm run dev            # isolated runtime, shared compiler output
// Builds enter the workspace lease; only generation-specific copies are executed.

import { spawn } from "node:child_process";
import { statSync, watch } from "node:fs";
import { join, resolve } from "node:path";
import { context } from "./workspace/paths.mjs";
import { cargo } from "./workspace/build.mjs";
import { pruneGenerations, runBinary } from "./workspace/runtime.mjs";
import { runtimeEnvironment } from "./workspace/ports.mjs";

const root = resolve(import.meta.dirname, "..");
const workspace = await context(root);
const env = await runtimeEnvironment(workspace);
const dataDir = env.RBIT_STORAGE__DATA_DIR;

let server = null;
let building = false;
let pending = false;
let timer = null;
/** Last seen modification time per watched path. */
const stamps = new Map();

function log(message) {
  console.log(`[dev ${new Date().toLocaleTimeString()}] ${message}`);
}

function stopServer() {
  if (!server) return Promise.resolve();
  const old = server;
  server = null;
  if (old.exitCode !== null || old.signalCode !== null) return Promise.resolve();
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
  try {
    const build = await cargo(workspace, "build", ["--package", "rbit-server"], { env });
    const executable = build.executables.find((item) => item.name === "rbit");
    if (!executable) throw new Error("Cargo did not produce rbit");
    await stopServer();
    server = await runBinary(workspace, executable.path, ["serve"], env);
    server.on("exit", (code, signal) => {
      if (signal || code === null) return;
      log(`server exited with code ${code}`);
    });
    await pruneGenerations(workspace);
    log(`server started (${env.RBIT_SERVER__BIND ?? "127.0.0.1:3000"}); data: ${dataDir}`);
  } catch (error) {
    log(`build/start failed: ${error.message}`);
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
