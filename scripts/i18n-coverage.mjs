/**
 * Translation coverage: for every locale table under
 * `crates/rbit-localization/src/catalog/translations/<locale>/`, how many messages of the
 * table-backed families have an entry and how many still fall back to English.
 *
 *   node scripts/i18n-coverage.mjs              per-locale totals
 *   node scripts/i18n-coverage.mjs de fr        per-family detail for those locales
 *
 * A progress report for translators, never a gate: it always exits 0 once it has read the
 * sources. It counts messages (enum variants, or configuration schema entries for
 * `ConfigText`), the unit a translator adds one arm for.
 */
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const CATALOG = "crates/rbit-localization/src/catalog";

function rustFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? rustFiles(path) : entry.name.endsWith(".rs") ? [path] : [];
  });
}

const read = (path) => readFileSync(path, "utf8").replace(/\r\n/g, "\n");
const indent = (line) => line.length - line.trimStart().length;

/** Lines of the brace block opened on `lines[start]`, up to its closing line. */
function block(lines, start) {
  if (/\{\s*\}\s*$/.test(lines[start])) return [];
  const depth = indent(lines[start]);
  const body = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    if (indent(lines[index]) === depth && lines[index].trim().startsWith("}")) return body;
    body.push(lines[index]);
  }
  throw new Error(`unclosed block: ${lines[start].trim()}`);
}

/** Families of one table file, with the distinct message patterns each has an arm for. */
export function tableEntries(source) {
  const lines = source.split("\n");
  const families = new Map();
  lines.forEach((line, index) => {
    const header = /^\s*(\w+)(?:<'\w+>)? => (?:MessageWriter|RichWriter) \|\w+\| \{/.exec(line);
    if (!header) return;
    const family = header[1];
    const body = block(lines, index).join("\n");
    const pattern = family === "ConfigText"
      ? /ConfigText::\w+\s*(?:\{[^}]*\}|\([^)]*\))?/g
      : new RegExp(`\\b${family}::(\\w+)`, "g");
    const arms = new Set([...body.matchAll(pattern)].map((match) => (match[1] ?? match[0]).replace(/\s+/g, " ")));
    families.set(family, arms.size);
  });
  return families;
}

/** Variant count of an enum or a `labels!`/`messages!` family declared in `source`. */
export function variantCount(source, family) {
  const lines = source.split("\n");
  const start = lines.findIndex((line) =>
    new RegExp(`^(?:pub enum ${family}(?:<'\\w+>)? \\{|\\s+${family}(?:<'\\w+>)?(?:, "[^"]*")? \\{)$`).test(line));
  if (start < 0) return undefined;
  const depth = indent(lines[start]) + 4;
  return block(lines, start).filter((line) => indent(line) === depth && /^[A-Z]\w*\b/.test(line.trimStart())).length;
}

/** `ConfigText` has one message per field label and help, section title and help, and choice. */
function configCount(root) {
  const config = join(root, CATALOG, "settings/config");
  const entries = /^\s*"[^"]+"\s*=>\s*"/gm;
  const fields = ["fields_a.rs", "fields_b.rs"].reduce((sum, file) => sum + (read(join(config, file)).match(entries) ?? []).length, 0);
  const main = read(`${config}.rs`);
  const sections = main.slice(main.indexOf("config_sections! {"), main.indexOf("macro_rules! config_choices"));
  const choices = (main.match(/^\s*"\w+" "\w+" "\w+" =>/gm) ?? []).length;
  return 2 * fields + 2 * (sections.match(entries) ?? []).length + choices;
}

/** Per locale: per family `{ family, file, total, translated }`. */
export function coverage(root) {
  const catalog = join(root, CATALOG);
  const sources = rustFiles(catalog).filter((path) => !relative(catalog, path).startsWith("translations")).map(read);
  const totals = new Map();
  const total = (family) => {
    if (!totals.has(family)) {
      const count = family === "ConfigText" ? configCount(root)
        : sources.map((source) => variantCount(source, family)).find((value) => value !== undefined);
      if (!count) throw new Error(`no English declaration found for ${family}`);
      totals.set(family, count);
    }
    return totals.get(family);
  };
  const translations = join(catalog, "translations");
  const locales = readdirSync(translations, { withFileTypes: true }).filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort();
  return new Map(locales.map((locale) => {
    const directory = join(translations, locale);
    // A large family may spread its arms over sibling modules: one row per family, named
    // after the first file that has it, with the arms of every file counted.
    const rows = new Map();
    for (const path of rustFiles(directory).sort()) {
      for (const [family, translated] of tableEntries(read(path))) {
        const row = rows.get(family) ?? { family, file: relative(directory, path).replace(/\\/g, "/"), total: total(family), translated: 0 };
        row.translated = Math.min(row.translated + translated, row.total);
        rows.set(family, row);
      }
    }
    return [locale, [...rows.values()]];
  }));
}

function report(root, detailed) {
  const results = coverage(root);
  const lines = ["locale     translated   fallback   total   coverage"];
  for (const [locale, rows] of results) {
    const total = rows.reduce((sum, row) => sum + row.total, 0);
    const translated = rows.reduce((sum, row) => sum + row.translated, 0);
    const percent = total ? ((100 * translated) / total).toFixed(1) : "0.0";
    lines.push(`${locale.padEnd(10)} ${String(translated).padStart(10)} ${String(total - translated).padStart(10)} ${String(total).padStart(7)} ${percent.padStart(9)}%`);
  }
  for (const locale of detailed) {
    const rows = results.get(locale);
    if (!rows) { lines.push("", `unknown locale directory: ${locale}`); continue; }
    lines.push("", `${locale}: family (file) translated/total`);
    for (const row of rows) lines.push(`  ${row.family} (${row.file}) ${row.translated}/${row.total}`);
  }
  return lines.join("\n");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  console.log(report(root, process.argv.slice(2)));
}
