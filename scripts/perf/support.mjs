// Shared, dependency-free provenance and summary helpers for local HTTP measurements.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { resolve } from "node:path";

export async function hashFile(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

export function fingerprint(cwd) {
  const git = (...args) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const normalized = (path) => process.platform === "win32" ? resolve(path).toLowerCase() : resolve(path);
  let repositoryRoot;
  try { repositoryRoot = git("rev-parse", "--show-toplevel"); }
  catch { return { repository: false, revision: null, reason: "source is not a Git checkout" }; }
  if (normalized(repositoryRoot) !== normalized(cwd)) {
    return { repository: false, revision: null, reason: "source is not its own Git root; ancestor revision intentionally omitted" };
  }
  return {
    repository: true,
    revision: git("rev-parse", "HEAD"), refs: git("for-each-ref", "--format=%(refname) %(objectname)", "refs/heads"), status: git("status", "--porcelain"),
    trackedDiffSha256: createHash("sha256").update(git("diff", "--binary", "HEAD")).digest("hex"),
  };
}

export function orderForRound(variants, round) {
  return round % 2 === 0 ? [...variants] : [...variants].reverse();
}

export function summarize(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  const quantile = (p) => {
    if (!sorted.length) return null;
    const position = (sorted.length - 1) * p;
    const lower = Math.floor(position);
    return sorted[lower] + (sorted[Math.ceil(position)] - sorted[lower]) * (position - lower);
  };
  return { count: sorted.length, median: quantile(0.5), p25: quantile(0.25), p75: quantile(0.75), p95: quantile(0.95) };
}
