// Launch an already-built benchmark server; never compile during a measurement.
// See README.md for paired release builds, isolated data and provenance limitations.
import { spawn } from "node:child_process";
import { createWriteStream, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fingerprint, hashFile } from "./support.mjs";

export function serveOptions(argv, root) {
  const options = { profile: "release", port: "3005", "target-dir": "target/perf-build", "data-dir": "target/perf/data", metadata: "", "source-revision": "", "source-dir": "." };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i].slice(2);
    if (!argv[i].startsWith("--") || !Object.hasOwn(options, key) || !argv[i + 1] || argv[i + 1].startsWith("--")) {
      throw new Error("usage: serve.mjs [--profile release|debug] [--port 3005] [--target-dir PATH] [--data-dir PATH] [--metadata FILE] [--source-dir PATH] [--source-revision LABEL]");
    }
    options[key] = argv[i + 1];
  }
  if (!["release", "debug"].includes(options.profile)) throw new Error("profile must be release or debug");
  if (!/^\d+$/.test(options.port) || Number(options.port) < 1 || Number(options.port) > 65535) throw new Error("invalid port");
  const port = Number(options.port);
  return {
    profile: options.profile, port,
    executable: resolve(root, options["target-dir"], options.profile, process.platform === "win32" ? "rbit.exe" : "rbit"),
    dataDir: resolve(root, options["data-dir"]),
    metadata: resolve(root, options.metadata || `target/perf/runs/server-${port}-${Date.now()}.json`),
    baseUrl: `http://127.0.0.1:${port}`,
    declaredSourceRevision: options["source-revision"] || null,
    sourceDir: resolve(root, options["source-dir"]),
  };
}

async function main() {
  const root = resolve(import.meta.dirname, "..", "..");
  const options = serveOptions(process.argv.slice(2), root);
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
  const server = spawn(options.executable, ["serve"], {
    cwd: options.sourceDir, stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      RBIT_SERVER__BIND: `127.0.0.1:${options.port}`,
      RBIT_SERVER__PUBLIC_URL: options.baseUrl,
      RBIT_STORAGE__DATA_DIR: options.dataDir,
      RBIT_MAIL__TRANSPORT: "log",
    },
  });
  for (const stream of [server.stdout, server.stderr]) {
    stream.pipe(log, { end: false });
    stream.pipe(process.stdout, { end: false });
  }
  console.log(`Benchmark server: ${options.baseUrl}\nMetadata: ${options.metadata}\nRaw server log: ${options.metadata}.log`);
  server.once("error", (error) => { console.error(error.message); process.exitCode = 1; log.end(); });
  server.once("close", (code, signal) => {
    log.end();
    writeFileSync(`${options.metadata}.exit.json`, JSON.stringify({ endedAt: new Date().toISOString(), code, signal }));
    process.exitCode = code ?? (signal ? 1 : 0);
  });
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.kill());
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
