/** Narrow manifest/source exception, in addition to Rust's normal lint enforcement. */
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const PACKAGE = "crates/rbit-platform";
const NATIVE = `${PACKAGE}/src/windows/native.rs`;
const ALLOW = "#![allow(unsafe_code)]";

function section(text, name) {
  let active = false;
  const values = new Map();
  for (const original of text.split(/\r?\n/)) {
    const line = original.replace(/#.*$/, "").trim();
    if (line.startsWith("[")) { active = line === `[${name}]`; continue; }
    if (!active || !line) continue;
    const entry = /^([a-zA-Z0-9_]+)\s*=\s*(.+)$/.exec(line);
    if (!entry || values.has(entry[1])) throw new Error(`Invalid/duplicate lint entry in ${name}`);
    values.set(entry[1], entry[2].replace(/\s/g, ""));
  }
  return values;
}
function same(left, right) {
  return left.size === right.size && [...left].every(([name, value]) => right.get(name) === value);
}

export function inspectPlatformBoundary({ workspace, platform, sources, manifests = [] }) {
  const violations = [];
  const rust = section(workspace, "workspace.lints.rust");
  if (rust.get("unsafe_code") !== '"forbid"') violations.push("Workspace unsafe prohibition changed");
  const expected = new Map(rust);
  expected.set("unsafe_code", '"deny"');
  expected.set("unsafe_op_in_unsafe_fn", '"deny"');
  if (!same(section(platform, "lints.rust"), expected)) violations.push("Platform Rust lint exception differs from reviewed policy");
  if (!same(section(platform, "lints.clippy"), section(workspace, "workspace.lints.clippy"))) {
    violations.push("Platform must retain every workspace Clippy lint");
  }
  if (section(platform, "lints").size) violations.push("Platform lint policy must be explicit rather than partially inherited");
  for (const [path, text] of manifests) {
    if (path === `${PACKAGE}/Cargo.toml`) continue;
    if (!same(section(text, "lints"), new Map([["workspace", "true"]]))) violations.push(`${path}: must inherit workspace lint prohibition`);
  }
  let exceptions = 0;
  for (const [path, text] of sources) {
    if (!path.endsWith(".rs")) continue;
    // Conservative fail-closed rule: cfg_attr can nest arbitrary attribute syntax
    // and lower Cargo's deny. No conditional unsafe policy is part of this review.
    if (/\bcfg_attr\b/.test(text) && /\bunsafe_code\b/.test(text)) {
      violations.push(`${path}: conditional unsafe lint policy is forbidden`);
    }
    // Closed unsafe-lint grammar, not an attribute-regex allowlist: remove only
    // the exact reviewed complete line. ANY remaining unsafe_code token is
    // refused, including inline items, comments in attributes, nested syntax,
    // expect and multi-lint variants. Compiler lints remain authoritative for
    // unsafe tokens/macros; no other authored unsafe policy spelling is supported.
    const line = path === NATIVE ? ALLOW : path === `${PACKAGE}/src/lib.rs` ? "#![deny(unsafe_code)]" : null;
    let remainder = text;
    if (line) {
      const lines = text.split(/\r?\n/);
      const position = lines.indexOf(line);
      if (position === -1) violations.push(`${path}: reviewed unsafe policy line missing`);
      else {
        lines.splice(position, 1);
        remainder = lines.join("\n");
        if (path === NATIVE) exceptions += 1;
      }
    }
    if (/\bunsafe_code\b/.test(remainder)) violations.push(`${path}: unreviewed unsafe lint exception`);
    if (!path.startsWith(`${PACKAGE}/`)) continue;
    if (/\bunsafe\s+impl\b/.test(text)) violations.push(`${path}: unsafe trait implementations are forbidden`);
    if (/^\s*pub(?:\([^)]*\))?\s+unsafe\b/m.test(text)) violations.push(`${path}: unsafe exports are forbidden`);
    if (/\binclude!\s*\(/.test(text)) violations.push(`${path}: platform source must use explicit reviewed modules`);
    if (/^\s*pub(?:\([^)]*\))?\s+mod\s+native\b/m.test(text)) violations.push(`${path}: native FFI module must remain private`);
  }
  if (exceptions !== 1) violations.push("Exactly one named private Windows FFI exception is required");
  const module = sources.find(([path]) => path === `${PACKAGE}/src/windows/mod.rs`)?.[1] ?? "";
  if (!/^mod native;$/m.test(module)) violations.push("Expected private Windows native module is missing");
  return violations;
}

export function checkPlatformBoundary(root, sources) {
  const path = join(root, PACKAGE, "Cargo.toml");
  if (!existsSync(path)) return sources.some(([source]) => source.startsWith(`${PACKAGE}/`)) ? ["Platform manifest missing"] : [];
  const manifests = [];
  for (const directory of ["crates", "tools"]) {
    for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const relative = `${directory}/${entry.name}/Cargo.toml`;
      if (existsSync(join(root, relative))) manifests.push([relative, readFileSync(join(root, relative), "utf8")]);
    }
  }
  return inspectPlatformBoundary({ workspace: readFileSync(join(root, "Cargo.toml"), "utf8"),
    platform: readFileSync(path, "utf8"), sources, manifests });
}
