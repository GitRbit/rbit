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
import { stopTree } from "./workspace/process-tree.mjs";

const root = resolve(import.meta.dirname, "..");
const workspace = await context(root);
const env = await runtimeEnvironment(workspace);
const dataDir = env.RBIT_STORAGE__DATA_DIR;

let server = null;
let building = false;
let pending = false;
let timer = null;
let stopping = false;
let activeBuild = null;
/** Last seen modification time per watched path. */
const stamps = new Map();

function log(message) {
  console.log(`[dev ${new Date().toLocaleTimeString()}] ${message}`);
}

async function stopServer() {
  if (!server) return;
  const old = server;
  await stopTree(old);
  server = null;
}

async function rebuild() {
  if (building) {
    pending = true;
    return;
  }
  building = true;
  log("building…");
  try {
    activeBuild = cargo(workspace, "build", ["--package", "rbit-server"], { env });
    const build = await activeBuild;
    if (stopping) return;
    const executable = build.executables.find((item) => item.name === "rbit");
    if (!executable) throw new Error("Cargo did not produce rbit");
    await stopServer();
    server = await runBinary(workspace, executable.path, ["serve"], env);
    server.runtimeCompletion.catch((error) => log(error.message));
    server.on("exit", (code, signal) => {
      if (signal || code === null) return;
      log(`server exited with code ${code}`);
    });
    await pruneGenerations(workspace);
    log(`server started (${env.RBIT_SERVER__BIND ?? "127.0.0.1:3000"}); data: ${dataDir}`);
  } catch (error) {
    log(`build/start failed: ${error.message}`);
  } finally {
    activeBuild = null;
    building = false;
  }
  if (pending && !stopping) {
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

async function shutdown() {
  if (stopping) return;
  stopping = true;
  clearTimeout(timer);
  css.kill();
  js.kill();
  try {
    if (activeBuild) { log("waiting for the current build before shutdown"); await activeBuild.catch(() => {}); }
    await stopServer();
    process.exit(0);
  } catch (error) { log(error.message); process.exit(1); }
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

rebuild();
