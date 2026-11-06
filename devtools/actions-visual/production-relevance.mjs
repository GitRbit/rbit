// Only fixed source families can request the contained production fixture.
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { appendFile, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { relevance } from "./relevance.mjs";
const execute = promisify(execFile);

export function productionRelevantPath(path) {
  return [".github/workflows/ci.yml", "Cargo.toml", "Cargo.lock", "package.json", "package-lock.json",
    "crates/rbit-web/Cargo.toml", "crates/rbit-web/tests/actions_pages.rs"].includes(path)
    // Shared middleware, persistence and dependency changes can affect these real
    // pages. Deliberately cover all crates instead of guessing a transitive graph.
    || ["devtools/actions-visual/", "crates/", ".github/actions/prepare-build/", ".cargo/"]
      .some(prefix => path.startsWith(prefix))
    || /^scripts\/tests\/rewrite-actions-.*\.test\.mjs$/.test(path);
}

async function main() {
  let result;
  try {
    const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
    result = await relevance(process.env.GITHUB_EVENT_NAME, event, async args => {
      const { stdout } = await execute("git", args, {
        timeout: 20000, maxBuffer: 2 * 1024 * 1024, windowsHide: true,
      });
      return stdout;
    }, productionRelevantPath);
  } catch { result = { run: true, reason: "EVENT_UNAVAILABLE" }; }
  if (process.env.GITHUB_OUTPUT) {
    await appendFile(process.env.GITHUB_OUTPUT, `run=${result.run}\nreason=${result.reason}\n`);
  }
  console.log(JSON.stringify(result)); // Fixed categories/counts; no source paths or event data.
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
