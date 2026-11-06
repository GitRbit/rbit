// Whole-PR path filters remain only a coarse trigger. A synchronize event's
// exact before/after tree delta decides whether another expensive capture is useful.
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { appendFile, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
const execute = promisify(execFile);
const commit = value => typeof value === "string" && /^[a-f0-9]{40}$/i.test(value) && !/^0{40}$/.test(value);
export function relevantPath(path) {
  return [".github/workflows/actions-visual.yml", "package.json", "package-lock.json",
    "crates/rbit-web/assets/logo-384.avif"].includes(path)
    || ["devtools/actions-visual/", "docs/design/prototypes/actions/", "crates/rbit-web/styles/",
      "scripts/tests/fixtures/actions-"].some(prefix => path.startsWith(prefix) || (prefix.endsWith("/") && path === prefix.slice(0, -1)))
    || /^scripts\/tests\/rewrite-actions-.*\.test\.mjs$/.test(path);
}
export async function relevance(eventName, event, git) {
  if (eventName !== "pull_request" || event?.action !== "synchronize") {
    return { run: true, reason: "INITIAL_MANUAL_OR_NON_SYNCHRONIZE" };
  }
  const before = event.before, after = event.after, head = event.pull_request?.head?.sha;
  if (![before, after, head].every(commit) || after.toLowerCase() !== head.toLowerCase()) {
    return { run: true, reason: "UNKNOWN_COMMIT_IDENTITY" };
  }
  try {
    for (const sha of [before, after]) {
      try { await git(["cat-file", "-e", `${sha}^{commit}`]); }
      catch {
        // Shallow checkout often lacks the event endpoints. Commit IDs have been
        // validated; no event URL/ref/branch/shell text is executed or fetched.
        await git(["fetch", "--no-tags", "--depth=1", "origin", before, after]);
        await git(["cat-file", "-e", `${sha}^{commit}`]);
      }
    }
    const output = await git(["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", before, after, "--"]);
    if (output && !output.endsWith("\0")) throw new Error("Malformed path framing");
    const paths = output.split("\0").filter(Boolean);
    const run = paths.some(relevantPath);
    return { run, reason: run ? "RELEVANT_EVENT_DELTA" : "NO_RELEVANT_EVENT_DELTA", changedPaths: paths.length };
  } catch { return { run: true, reason: "COMPARISON_UNAVAILABLE" }; }
}
async function main() {
  let result;
  try {
    const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
    result = await relevance(process.env.GITHUB_EVENT_NAME, event, async args => {
      const { stdout } = await execute("git", args, { timeout: 20000, maxBuffer: 2 * 1024 * 1024, windowsHide: true });
      return stdout;
    });
  } catch { result = { run: true, reason: "EVENT_UNAVAILABLE" }; }
  if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `run=${result.run}\nreason=${result.reason}\n`);
  console.log(JSON.stringify(result)); // Fixed reason and count only; never paths/event data.
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
