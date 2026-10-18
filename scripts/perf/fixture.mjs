// Builds the deterministic Git repository the frontend performance harness measures:
// a 20,000-line `src/huge.rs` and a 3,000-line `src/medium.rs` on `main` (with a few
// follow-up commits so blame has more than one run), and a `big-change` branch that
// touches ~150 files with ~25,000 changed lines (edits, additions, deletions, renames,
// hunks over 1,000 lines, one file without a trailing newline).
//
//   node scripts/perf/fixture.mjs [path]      # default: target/perf/fixture
//
// Everything comes from a seeded PRNG and commits carry fixed authors and dates, so every
// run produces the same commit ids. An existing fixture at the path is replaced; any
// other existing directory is left alone and the script stops.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..", "..");
const dir = resolve(root, process.argv[2] ?? join("target", "perf", "fixture"));
/** Written into `.git` so a rerun knows the directory is ours to replace. */
const MARKER = "rbit-perf-fixture";
/** The blob view highlights and blame runs only up to 512 KiB (`MAX_HIGHLIGHT_BYTES`,
 *  `BlameLimits::max_file_bytes`), so `huge.rs` has to stay below that. */
const HIGHLIGHT_LIMIT = 512 * 1024;

// mulberry32: small, fast and good enough to vary text; the seed fixes the whole fixture.
let seed = 0x5eed_2026;
function random() {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const int = (min, max) => min + Math.floor(random() * (max - min + 1));
const pick = (list) => list[Math.floor(random() * list.length)];
const chance = (p) => random() < p;

const WORDS = ["id", "key", "len", "buf", "row", "map", "ref", "tree", "path", "blob", "line", "hunk", "repo",
  "user", "team", "page", "diff", "size", "mode", "head", "base", "tag", "scope", "token", "entry", "cache",
  "limit", "state", "event", "index", "query", "label", "issue", "pull", "file", "name", "kind", "span", "node"];
const TYPES = ["u32", "u64", "usize", "bool", "String", "&str", "Vec<u8>", "Option<u32>", "PathBuf", "Oid", "Entry",
  "Commit", "Repo", "Span", "Hunk", "Limits", "Error"];
const METHODS = ["len", "iter", "clone", "as_str", "to_owned", "is_empty", "unwrap_or", "get", "push", "insert", "map",
  "collect", "next", "parse", "trim", "split", "find", "max", "min", "saturating_add"];

const ident = () => (chance(0.3) ? `${pick(WORDS)}_${pick(WORDS)}` : pick(WORDS));
const typeName = () => pick(TYPES);
const camel = () => ident().split("_").map((w) => w[0].toUpperCase() + w.slice(1)).join("");

/** One statement at `indent`; occasionally a long one (> 200 characters). */
function statement(indent) {
  if (chance(0.006)) return indent + longLine();
  const a = ident();
  const b = ident();
  switch (int(0, 9)) {
    case 0: return `${indent}let ${a} = ${b}.${pick(METHODS)}();`;
    case 1: return `${indent}let ${a}: ${typeName()} = ${int(0, 4096)};`;
    case 2: return `${indent}${a}.${pick(METHODS)}(${b});`;
    case 3: return `${indent}// ${pick(WORDS)} ${pick(WORDS)} ${pick(WORDS)}`;
    case 4: return `${indent}${a} += ${b}.${pick(METHODS)}();`;
    case 5: return `${indent}let ${a} = ${b}?;`;
    case 6: return `${indent}self.${a} = ${b};`;
    case 7: return `${indent}${a}.${pick(METHODS)}()?;`;
    case 8: return "";
    default: return `${indent}${a}`;
  }
}

function longLine() {
  switch (int(0, 2)) {
    case 0: {
      const values = Array.from({ length: int(24, 40) }, () => `0x${int(0, 0xffff).toString(16).padStart(4, "0")}`);
      return `const ${camel().toUpperCase()}: [u16; ${values.length}] = [${values.join(", ")}];`;
    }
    case 1: {
      const words = Array.from({ length: int(30, 45) }, () => pick(WORDS));
      return `return Err(Error::new(format!("${words.join(" ")} {}", ${ident()})));`;
    }
    default: {
      const chain = Array.from({ length: int(14, 22) }, () => `.${pick(METHODS)}()`).join("");
      return `let ${ident()} = ${ident()}${chain};`;
    }
  }
}

/** A block of statements, nesting `if`/`for`/`match` up to `depth` levels. */
function block(indent, step, depth, lines) {
  const count = int(2, 9);
  for (let i = 0; i < count; i++) {
    if (depth > 0 && chance(0.18)) {
      const head = pick([`if ${ident()}.is_empty() {`, `for ${ident()} in ${ident()}.iter() {`,
        `while let Some(${ident()}) = ${ident()}.next() {`, `if let Some(${ident()}) = ${ident()} {`]);
      lines.push(indent + head);
      block(indent + step, step, depth - 1, lines);
      lines.push(`${indent}}`);
    } else if (depth > 0 && chance(0.06)) {
      lines.push(`${indent}match ${ident()} {`);
      for (let arm = int(2, 4); arm > 0; arm--) lines.push(`${indent}${step}${camel()}::${camel()} => ${ident()},`);
      lines.push(`${indent}${step}_ => {}`);
      lines.push(`${indent}}`);
    } else {
      lines.push(statement(indent));
    }
  }
}

/** One top-level item: a function, a struct, an impl block or a constant. */
function item(lines) {
  // About one item in ten is indented with tabs, as in code pasted from elsewhere.
  const step = chance(0.1) ? "\t" : "    ";
  if (chance(0.5)) lines.push(`/// ${pick(WORDS)} the ${pick(WORDS)} of a ${pick(WORDS)}.`);
  switch (int(0, 5)) {
    case 0: {
      lines.push(`pub struct ${camel()} {`);
      for (let f = int(2, 7); f > 0; f--) lines.push(`${step}pub ${ident()}: ${typeName()},`);
      lines.push("}");
      break;
    }
    case 1: {
      lines.push(`impl ${camel()} {`);
      for (let m = int(1, 3); m > 0; m--) {
        lines.push(`${step}pub fn ${ident()}(&self, ${ident()}: ${typeName()}) -> ${typeName()} {`);
        block(step + step, step, 2, lines);
        lines.push(`${step}}`);
      }
      lines.push("}");
      break;
    }
    case 2:
      lines.push(`const ${ident().toUpperCase()}: ${typeName()} = ${int(1, 1 << 20)};`);
      break;
    default: {
      lines.push(`fn ${ident()}(${ident()}: ${typeName()}) -> Result<${typeName()}, Error> {`);
      block(step, step, 2, lines);
      lines.push("}");
    }
  }
  lines.push("");
}

/** About `target` lines of plausible Rust, starting with a header of `use` lines. */
function rustFile(target) {
  const lines = [`//! ${pick(WORDS)} and ${pick(WORDS)} handling.`, ""];
  for (let u = int(2, 6); u > 0; u--) lines.push(`use crate::${ident()}::${camel()};`);
  lines.push("");
  while (lines.length < target) item(lines);
  return lines.slice(0, target);
}

function write(path, lines, trailingNewline = true) {
  const file = join(dir, path);
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, lines.join("\n") + (trailingNewline ? "\n" : ""));
}
const read = (path) => readFileSync(join(dir, path), "utf8").replace(/\n$/, "").split("\n");

// Fixed identity and dates keep commit ids stable across runs and machines.
let clock = Date.UTC(2026, 0, 5, 9, 0, 0) / 1000;
function git(...args) {
  clock += 3600;
  const date = `${clock} +0000`;
  return execFileSync("git", args, {
    cwd: dir,
    encoding: "utf8",
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "Perf Fixture", GIT_AUTHOR_EMAIL: "perf@example.test", GIT_AUTHOR_DATE: date,
      GIT_COMMITTER_NAME: "Perf Fixture", GIT_COMMITTER_EMAIL: "perf@example.test", GIT_COMMITTER_DATE: date,
      GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: process.platform === "win32" ? "NUL" : "/dev/null",
    },
  });
}
const commit = (message) => { git("add", "--all"); git("commit", "--quiet", "--no-verify", "-m", message); };

// --- Fresh repository -------------------------------------------------------------------
if (existsSync(dir)) {
  if (!existsSync(join(dir, ".git", MARKER))) {
    console.error(`${dir} exists and is not a perf fixture; remove it or pass another path`);
    process.exit(1);
  }
  rmSync(dir, { recursive: true, force: true });
}
mkdirSync(dir, { recursive: true });
git("init", "--quiet", "--initial-branch=main");
git("config", "core.autocrlf", "false");
writeFileSync(join(dir, ".git", MARKER), "generated by scripts/perf/fixture.mjs\n");

// --- main ---------------------------------------------------------------------------------
write("README.md", ["# perf-fixture", "", "Generated by `scripts/perf/fixture.mjs` to measure large pages."]);
write("src/huge.rs", rustFile(20_000));
write("src/medium.rs", rustFile(3_000));
// The files the big change touches: 160 modules of 60-700 lines.
const modules = Array.from({ length: 160 }, (_, i) => `src/modules/m${String(i).padStart(3, "0")}.rs`);
for (const path of modules) write(path, rustFile(int(60, 700)));
commit("Initial import");

// A few small follow-ups so blame shows several runs, not one.
for (const round of [1, 2, 3, 4]) {
  const lines = read("src/huge.rs");
  for (let edit = 0; edit < 40; edit++) {
    const at = int(0, lines.length - 1);
    lines[at] = `${lines[at].match(/^\s*/)[0]}let ${ident()} = ${ident()}.${pick(METHODS)}(); // r${round}`;
  }
  write("src/huge.rs", lines);
  commit(`Touch up huge.rs, round ${round}`);
}

// --- big-change ---------------------------------------------------------------------------
git("switch", "--quiet", "--create", "big-change");

/** Rewrites `lines` with `hunks` scattered hunks of up to `size` lines each. */
function scatter(lines, hunks, size) {
  for (let h = 0; h < hunks; h++) {
    const at = int(0, Math.max(0, lines.length - 1));
    const cut = Math.min(int(0, size), lines.length - at);
    const fresh = [];
    block("    ", "    ", 2, fresh);
    lines.splice(at, cut, ...fresh.slice(0, int(1, size)));
  }
  return lines;
}

const edits = modules.slice(0, 115);
const expanded = modules.slice(115, 120);
const renames = modules.slice(120, 132);
const deletions = modules.slice(132, 145);

for (const path of edits) write(path, scatter(read(path), int(2, 12), 18));
commit("Rework the modules");

// Five files with one hunk over 1,000 lines each: up to 300 lines replaced by 1,050-1,400.
for (const path of expanded) {
  const lines = read(path);
  const fresh = rustFile(int(1_050, 1_400)).slice(4);
  const at = int(0, lines.length - 1);
  const cut = Math.min(lines.length - at, int(0, 300));
  lines.splice(at, cut, ...fresh);
  write(path, lines);
}
commit("Expand five modules");

// Renames: half pure, half with a few edits (still detected as renames).
for (const [i, path] of renames.entries()) {
  const target = path.replace("/modules/", "/renamed/");
  mkdirSync(dirname(join(dir, target)), { recursive: true });
  renameSync(join(dir, path), join(dir, target));
  if (i % 2) write(target, scatter(read(target), 2, 4));
}
for (const path of deletions) rmSync(join(dir, path));
// New files, the last one without a trailing newline.
for (let i = 0; i < 7; i++) {
  const lines = rustFile(int(200, 900));
  write(`src/fresh/f${i}.rs`, lines, i !== 6);
}
commit("Rename, remove and add modules");

const size = statSync(join(dir, "src/huge.rs")).size;
const stat = git("diff", "--shortstat", "-M", "main...big-change").trim();
console.log(`fixture at ${dir}`);
console.log(`src/huge.rs: ${read("src/huge.rs").length} lines, ${size} bytes${size > HIGHLIGHT_LIMIT ? " (OVER the 512 KiB highlight/blame limit)" : ""}`);
console.log(`big-change vs main: ${stat}`);
console.log(`main ${git("rev-parse", "main").trim()}, big-change ${git("rev-parse", "big-change").trim()}`);
