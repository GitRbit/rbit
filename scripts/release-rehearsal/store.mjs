// Read-only looks into a stopped instance's data directory (its SQLite database and
// files), digests that prove a refusing binary changed nothing, and the synthetic
// baseline-schema store the fast mode upgrades when no baseline binary is available.
import { createHash } from "node:crypto";
import { readdir, readFile, stat } from "node:fs/promises";
import { join, relative } from "node:path";
import { DatabaseSync } from "node:sqlite";

/** Facts about the database at `path`, read without changing it. */
export function inspectDatabase(path) {
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    const migrations = db.prepare("SELECT version, success FROM _sqlx_migrations ORDER BY version").all();
    const integrity = db.prepare("PRAGMA quick_check").all().map((row) => Object.values(row)[0]);
    const foreignKeys = db.prepare("PRAGMA foreign_key_check").all();
    const tables = db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations' ORDER BY name").all();
    const counts = {};
    for (const { name } of tables) counts[name] = Number(db.prepare(`SELECT COUNT(*) AS n FROM "${name.replaceAll("\"", "\"\"")}"`).get().n);
    return {
      latest: migrations.length ? Number(migrations.at(-1).version) : 0,
      failed: migrations.filter((row) => !row.success).map((row) => Number(row.version)),
      versions: migrations.map((row) => Number(row.version)),
      integrity: integrity.filter((line) => line !== "ok"),
      foreignKeyViolations: foreignKeys.length,
      counts,
    };
  } finally {
    db.close();
  }
}

/** Row counts that must survive: every table both schemas share, with its count. */
export function countsPreserved(before, after) {
  const changed = [];
  for (const [table, count] of Object.entries(before)) {
    if (!(table in after)) changed.push(`${table}: dropped`);
    else if (after[table] < count) changed.push(`${table}: ${count} → ${after[table]}`);
  }
  return changed;
}

async function* files(root, dir = root) {
  let entries;
  try { entries = await readdir(dir, { withFileTypes: true }); } catch (error) { if (error.code === "ENOENT") return; throw error; }
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* files(root, path);
    else yield relative(root, path).replaceAll("\\", "/");
  }
}

/** `relative path → sha256` of every file under `root` except those `skip` matches. */
export async function treeDigest(root, skip = () => false) {
  const digest = {};
  for await (const path of files(root)) {
    if (skip(path)) continue;
    digest[path] = createHash("sha256").update(await readFile(join(root, path))).digest("hex");
  }
  return digest;
}

/** Paths whose digest differs between two `treeDigest` results, added or removed. */
export function treeChanges(before, after) {
  const paths = new Set([...Object.keys(before), ...Object.keys(after)]);
  return [...paths].filter((path) => before[path] !== after[path]).sort();
}

export async function exists(path) {
  try { await stat(path); return true; } catch { return false; }
}

const MIGRATIONS_TABLE = `CREATE TABLE IF NOT EXISTS _sqlx_migrations (
  version BIGINT PRIMARY KEY, description TEXT NOT NULL,
  installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL,
  checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)`;

/**
 * Applies `migrations` (`{version, description, sql}` ascending) to the database at `path`
 * exactly as sqlx records them: the checksum is the SHA-384 of the file, so the real
 * migrator accepts the history and continues from it.
 */
export function applyMigrations(path, migrations, { after } = {}) {
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA journal_mode = WAL");
    db.exec(MIGRATIONS_TABLE);
    const record = db.prepare("INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, 1, ?, 0)");
    for (const migration of migrations) {
      db.exec(migration.sql);
      record.run(migration.version, migration.description, createHash("sha384").update(migration.bytes).digest());
      after?.(db, migration.version);
    }
    db.exec("PRAGMA wal_checkpoint(TRUNCATE)");
  } finally {
    db.close();
  }
}

/** The migration files of `dir` up to and including `through`. */
export async function readMigrations(dir, through) {
  const names = (await readdir(dir)).filter((name) => /^\d{4}_.+\.sql$/.test(name)).sort();
  const migrations = [];
  for (const name of names) {
    const version = Number(name.slice(0, 4));
    if (version > through) break;
    const bytes = await readFile(join(dir, name));
    migrations.push({ version, description: name.slice(5, -4).replaceAll("_", " "), bytes, sql: bytes.toString("utf8") });
  }
  return migrations;
}

/** Runs one statement against a stopped instance's database (fault injection only). */
export function execute(path, sql, ...parameters) {
  const db = new DatabaseSync(path);
  try { db.prepare(sql).run(...parameters); db.exec("PRAGMA wal_checkpoint(TRUNCATE)"); } finally { db.close(); }
}
