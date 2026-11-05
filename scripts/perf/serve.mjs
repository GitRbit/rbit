// Launch an already-built benchmark server; never compile during a measurement.
// See README.md for paired release builds, isolated data and provenance limitations.
import { spawn } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fingerprint, hashFile } from "./support.mjs";
import { contains, context } from "../workspace/paths.mjs";
import { runBinary } from "../workspace/runtime.mjs";

export function serveOptions(argv, root) {
  const options = { profile: "release", port: "3005", executable: "", "build-record": "", "target-dir": "", "data-dir": "target/perf/data", metadata: "", "source-revision": "", "source-dir": "." };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].slice(2);
    if (!argv[i].startsWith("--") || !Object.hasOwn(options, key) || !argv[i + 1] || argv[i + 1].startsWith("--")) {
      throw new Error("usage: serve.mjs --executable PATH|--build-record FILE [--profile release|debug] [--port 3005] [--data-dir PATH] [--metadata FILE] [--source-dir PATH] [--source-revision LABEL]; explicit legacy --target-dir PATH is supported");
    }
    options[key] = argv[i + 1];
  }
  if (!["release", "debug"].includes(options.profile)) throw new Error("profile must be release or debug");
  if (!/^\d+$/.test(options.port) || Number(options.port) < 1 || Number(options.port) > 65535) throw new Error("invalid port");
  if ([options.executable, options["build-record"], options["target-dir"]].filter(Boolean).length > 1) throw new Error("Select exactly one executable, build record or legacy target directory");
  const port = Number(options.port);
  return {
    profile: options.profile, port,
    executable: options.executable ? resolve(root, options.executable) : options["target-dir"] ? resolve(root, options["target-dir"], options.profile, process.platform === "win32" ? "rbit.exe" : "rbit") : null,
    buildRecord: options["build-record"] ? resolve(root, options["build-record"]) : null,
    dataDir: resolve(root, options["data-dir"]),
    metadata: resolve(root, options.metadata || `target/perf/runs/server-${port}-${Date.now()}.json`),
    baseUrl: `http://127.0.0.1:${port}`,
    declaredSourceRevision: options["source-revision"] || null,
    sourceDir: resolve(root, options["source-dir"]),
  };
}

export function executableFromRecord(file, root) {
  const record = JSON.parse(readFileSync(file, "utf8").replace(/^\uFEFF/, ""));
  const binaries = record.executables?.filter((entry) => entry.name === "rbit");
  if (binaries?.length !== 1 || typeof binaries[0].path !== "string" || !binaries[0].path) throw new Error("Build record must identify exactly one rbit executable");
  return resolve(root, binaries[0].path);
}

async function main() {
  const root = resolve(import.meta.dirname, "..", "..");
  const options = serveOptions(process.argv.slice(2), root);
  if (options.buildRecord) options.executable = executableFromRecord(options.buildRecord, root);
  if (!options.executable) throw new Error("Select --executable from a workspace build result or --build-record FILE; the launcher never chooses a mutable shared Cargo output implicitly");
  if (!existsSync(options.executable)) throw new Error(`missing ${options.executable}; build the selected profile before launching`);
  const metadata = {
    schema: 1, startedAt: new Date().toISOString(), ...options,
    executableSha256: await hashFile(options.executable), sourceAtLaunch: fingerprint(options.sourceDir),
    profileEvidence: "declared by launcher; retain the build command and log separately",
  };
  mkdirSync(dirname(options.metadata), { recursive: true });
  writeFileSync(options.metadata, `${JSON.stringify(metadata, null, 2)}\n`, { flag: "wx" });
  const log = createWriteStream(`${options.metadata}.log`, { flags: "wx" });
  await new Promise((done, reject) => { log.once("open", done); log.once("error", reject); });
  const spawnOptions = {
    cwd: options.sourceDir, stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
    env: {
      ...process.env,
      RBIT_SERVER__BIND: `127.0.0.1:${options.port}`,
      RBIT_SERVER__PUBLIC_URL: options.baseUrl,
      RBIT_STORAGE__DATA_DIR: options.dataDir,
      RBIT_MAIL__TRANSPORT: "log",
    },
  };
  // Managed generation markers protect benchmark binaries from helper cleanup. The
  // fixture data, declared port, source fingerprint and log semantics stay unchanged.
  let workspace;
  try { workspace = await context(root); } catch { /* archived standalone harness */ }
  const managed = workspace && contains(resolve(workspace.root, ".worktree-state"), options.executable);
  let server;
  try {
    server = managed
      ? await runBinary(workspace, options.executable, ["serve"], spawnOptions.env, spawnOptions)
      : spawn(options.executable, ["serve"], spawnOptions);
  } catch (error) {
    log.end();
    writeFileSync(`${options.metadata}.exit.json`, JSON.stringify({ endedAt: new Date().toISOString(), error: error.message }));
    throw error;
  }
  for (const stream of [server.stdout, server.stderr]) {
    stream.pipe(log, { end: false });
    stream.pipe(process.stdout, { end: false });
  }
  console.log(`Benchmark server: ${options.baseUrl}\nMetadata: ${options.metadata}\nRaw server log: ${options.metadata}.log`);
  server.once("error", (error) => { console.error(error.message); process.exitCode = 1; log.end(); });
  let finished = false;
  function finish(code, signal) {
    if (finished) return;
    finished = true;
    log.end();
    writeFileSync(`${options.metadata}.exit.json`, JSON.stringify({ endedAt: new Date().toISOString(), code, signal }));
    process.exitCode = code ?? (signal ? 1 : 0);
  }
  server.once("close", finish);
  if (server.exitCode !== null && server.stdout.destroyed && server.stderr.destroyed) finish(server.exitCode, server.signalCode);
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.kill());
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
