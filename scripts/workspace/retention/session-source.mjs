// Complete Git input plus ignored public CSS identity; no private paths leave the seal.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { guarded } from "../paths.mjs";
import { absentInput } from "../source-proof/inputs.mjs";
import { boundedInput } from "./policy.mjs";
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const identity = info => [info.dev, info.ino, info.size, info.mtimeNs, info.ctimeNs].map(String);

export function requireSourceSeal(value) {
  const keys = ["bytes", "files", "revision", "sha256", "stabilitySha256"];
  if (!value || Object.keys(value).sort().join(",") !== keys.join(",")
    || !/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(value.revision ?? "")
    || !/^[a-f0-9]{64}$/.test(value.sha256 ?? "") || !/^[a-f0-9]{64}$/.test(value.stabilitySha256 ?? "")
    || !Number.isSafeInteger(value.files) || value.files < 1 || value.files > 20000
    || !Number.isSafeInteger(value.bytes) || value.bytes < 0 || value.bytes > 512 * 1024 ** 2) throw Error("SESSION_SOURCE_UNPROVED");
  return Object.freeze({ ...value });
}

export async function sourceSeal(ctx) {
  const git = args => new TextDecoder("utf-8", { fatal: true }).decode(execFileSync("git", args,
    { cwd: ctx.checkout, windowsHide: true, maxBuffer: 4 * 1024 ** 2, stdio: ["ignore", "pipe", "pipe"] }));
  const revision = git(["rev-parse", "HEAD"]).trim();
  const names = new Set(git(["ls-files", "-z", "--cached", "--others", "--exclude-standard"]).split("\0").filter(Boolean));
  // Cargo package inputSnapshot additionally covers ignored modules/assets and
  // effective configuration per invocation. Do not walk installed Node toolchains.
  const css = "crates/rbit-web/assets/app.css";
  try { await lstat(join(ctx.checkout, "crates/rbit-web")); names.add(css); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  if (!names.size || names.size > 20000) throw Error("SESSION_SOURCE_BOUND");
  const contents = [], stability = [], budget = { stability };
  let bytes = 0;
  for (const name of [...names].sort()) {
    if (isAbsolute(name) || name.includes("\\") || name.split("/").some(part => ["", ".", "..", ".git"].includes(part))) {
      throw Error("SESSION_SOURCE_REFUSED");
    }
    const path = await guarded(ctx.checkout, join(ctx.checkout, name));
    try {
      const before = await lstat(path, { bigint: true });
      const data = await boundedInput(path, false, 32 * 1024 ** 2);
      const after = await lstat(path, { bigint: true });
      if (JSON.stringify(identity(before)) !== JSON.stringify(identity(after))) throw Error("SESSION_SOURCE_CHANGED");
      bytes += data.length;
      if (bytes > 512 * 1024 ** 2) throw Error("SESSION_SOURCE_BOUND");
      contents.push([name, sha(data)]);
      stability.push([sha(path), identity(after)]);
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      contents.push([name, null]);
      await absentInput(path, budget);
    }
  }
  return requireSourceSeal({ revision, sha256: sha(JSON.stringify([ctx.checkout, contents])),
    stabilitySha256: sha(JSON.stringify(stability)), files: names.size, bytes });
}
