// Derive scheduling provenance from bounded on-disk inputs, never caller observations.
import { createHash } from "node:crypto";
import { lstat, open } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { homedir } from "node:os";
import { normalizeGraph } from "./graph.mjs";
const digest = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const contentDigest = bytes => createHash("sha256").update(bytes).digest("hex");

export async function boundedInput(path, optional = false, maximumBytes = 4 * 1024 ** 2) {
  let file;
  try {
    const before = await lstat(path);
    if (!before.isFile() || before.isSymbolicLink() || before.size > maximumBytes) throw new Error("POLICY_INPUT_REFUSED");
    for (let ancestor = dirname(resolve(path));;) {
      if ((await lstat(ancestor)).isSymbolicLink()) throw new Error("POLICY_INPUT_REFUSED");
      const parent = dirname(ancestor);
      if (parent === ancestor) break;
      ancestor = parent;
    }
    file = await open(path, "r");
    // One lookahead byte detects growth without readFile allocating an unbounded
    // concurrently enlarged input. Retain only this bounded input until it is hashed.
    const buffer = Buffer.alloc(before.size + 1);
    let total = 0;
    while (total < buffer.length) {
      const { bytesRead } = await file.read(buffer, total, buffer.length - total, null);
      if (!bytesRead) break;
      total += bytesRead;
    }
    const after = await file.stat();
    if (total !== before.size || before.size !== after.size || before.mtimeMs !== after.mtimeMs
      || before.ino !== after.ino || before.dev !== after.dev) throw new Error("POLICY_INPUT_CHANGED");
    return buffer.subarray(0, total);
  } catch (error) { if (optional && error.code === "ENOENT") return null; throw error; }
  finally { if (file) await file.close(); }
}

function members(source) {
  // This repository uses a literal, non-glob member list. Other TOML forms are
  // conservative rather than guessed; Cargo still owns manifest parsing/freshness.
  const section = source.split(/^\[workspace\][ \t]*\r?$/m)[1]?.split(/^\[/m)[0];
  const list = section?.match(/^members\s*=\s*\[([\s\S]*?)^\]/m)?.[1];
  if (!list) throw new Error("WORKSPACE_LAYOUT_UNOBSERVED");
  const parsed = JSON.parse(`[${list.replace(/#[^\r\n]*/g, "").replace(/,\s*$/, "")}]`);
  if (!Array.isArray(parsed) || !parsed.length || parsed.length > 256 || new Set(parsed).size !== parsed.length
    || parsed.some(value => typeof value !== "string" || !/^(crates|tools)\/[a-z0-9][a-z0-9_-]*$/.test(value))) {
    throw new Error("WORKSPACE_LAYOUT_UNOBSERVED");
  }
  return parsed.sort();
}
function configurationPaths(checkout, env) {
  const paths = [];
  for (let directory = resolve(checkout);;) {
    paths.push(join(directory, ".cargo", "config"), join(directory, ".cargo", "config.toml"));
    const parent = dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  const home = resolve(checkout, env.CARGO_HOME ?? join(homedir(), ".cargo"));
  paths.push(join(home, "config"), join(home, "config.toml"));
  if (paths.length > 128) throw new Error("CONFIGURATION_DEPTH_UNOBSERVED");
  return [...new Set(paths)];
}
export async function policySnapshot(ctx, namespace, command, args, env, read = boundedInput) {
  try {
    const root = await read(join(ctx.checkout, "Cargo.toml"));
    const manifests = [["Cargo.toml", contentDigest(root)], ["Cargo.lock", contentDigest(await read(join(ctx.checkout, "Cargo.lock")))]];
    for (const member of members(root.toString("utf8"))) {
      const path = `${member}/Cargo.toml`;
      manifests.push([path, contentDigest(await read(join(ctx.checkout, path)))]);
    }
    const configuration = [];
    for (const path of configurationPaths(ctx.checkout, env)) {
      const bytes = await read(path, true);
      if (bytes && /^\s*(?:include\s*=|["'])/m.test(bytes.toString("utf8"))) {
        // Quoted keys may encode "include" with TOML escapes. Do not guess their
        // meaning or silently ignore an included file outside this supported subset.
        throw new Error("CONFIGURATION_SYNTAX_UNOBSERVED");
      }
      // Hash ordering/locations too, but never return private paths/content in a record.
      configuration.push([path, bytes ? contentDigest(bytes) : null]);
    }
    const ignored = new Set(["CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR"]);
    const environment = Object.entries(env).filter(([key]) => !ignored.has(key)
      && /^(CARGO_|RUST|LIBSQLITE3_FLAGS|CC$|CXX$|CFLAGS$|CXXFLAGS$)/.test(key)).sort(([a], [b]) => a.localeCompare(b));
    if (environment.length > 256 || environment.some(([, value]) => typeof value !== "string")
      || environment.reduce((bytes, [key, value]) => bytes + Buffer.byteLength(key) + Buffer.byteLength(value), 0) > 65536) {
      throw new Error("POLICY_ENVIRONMENT_UNOBSERVED");
    }
    const policy = { compatibility: namespace, manifestDigest: digest(manifests), configurationDigest: digest({ configuration, environment }) };
    const graph = normalizeGraph({ command, args, env, policy });
    return { graph: { id: graph.id, policyId: graph.policyId, confidence: graph.confidence, reasons: graph.reasons },
      inputId: digest(policy) };
  } catch {
    return { graph: { confidence: "conservative", reasons: ["POLICY_INPUTS_UNOBSERVED"] }, inputId: null };
  }
}
