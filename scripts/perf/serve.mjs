// Runs the debug build of rbit for the performance harness on http://localhost:3005, with
// its own data directory (target/perf/data) so the fixture never mixes with ./data,
// ./data-live or ./data-alt. It runs its own build from target/perf-build, so a dev server
// holding target/debug/rbit.exe open (Windows locks running executables) never blocks it.
// This script does not build, so a measurement never waits on (or races) a compile:
//
//   CARGO_TARGET_DIR=target/perf-build cargo build -p rbit-server
//   node scripts/perf/serve.mjs            # Ctrl+C stops it
//
// Debug builds read assets/ from disk, so CSS and JS edits show after a refresh; template
// and Rust changes need a rebuild and a restart.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const exe = join(root, "target", "perf-build", "debug", process.platform === "win32" ? "rbit.exe" : "rbit");
if (!existsSync(exe)) {
  console.error(`${exe} is missing: build it with CARGO_TARGET_DIR=target/perf-build cargo build -p rbit-server`);
  process.exit(1);
}

const server = spawn(exe, ["serve"], {
  cwd: root,
  stdio: "inherit",
  env: {
    ...process.env,
    RBIT_SERVER__BIND: "127.0.0.1:3005",
    RBIT_SERVER__PUBLIC_URL: "http://localhost:3005",
    RBIT_STORAGE__DATA_DIR: join(root, "target", "perf", "data"),
    RBIT_MAIL__TRANSPORT: "log",
  },
});
server.on("exit", (code) => process.exit(code ?? 0));
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.kill());
