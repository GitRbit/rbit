/** Retired backup database paths must not return; see docs/design/rewrite/operations-backup-acceptance.md. */

// The snapshot inventory and pruning moved to rbit_storage::sqlite::backup.
const LEGACY_MODULE = /^crates\/rbit-server\/src\/backup\/(?:inventory|prune)(?:\.rs|\/)/u;
// The server's backup adapter copies files and Git objects; every SQL statement is storage's.
const SERVER_BACKUP = "crates/rbit-server/src/backup/";
const DATABASE_WORK = /\bsqlx\b|\bwriter_compatibility\b|\b(?:SELECT|DELETE FROM|INSERT INTO|UPDATE \w+ SET|VACUUM)\b/u;
// Only storage takes a database snapshot in production.
const SNAPSHOT = /VACUUM\s+INTO/u;
const STORAGE = "crates/rbit-storage/";
// The backup format and the formats that hold each kind are defined once, in storage.
const FORMAT = /\bconst\s+(?:FORMAT_VERSION|SINCE_FORMAT)\b|\bfn\s+since_format\b/u;

/** Comments are not code; strip line and block comments before matching. */
function code(source) {
  return source.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

const production = (path) => /^crates\/[^/]+\/src\//u.test(path);

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkBackupRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!path.startsWith("crates/") || !path.endsWith(".rs")) continue;
    const text = code(source);
    if (LEGACY_MODULE.test(path)) {
      violations.push(`${path}: the backup inventory and pruning belong to rbit_storage::sqlite::backup`);
    }
    if (path.startsWith(SERVER_BACKUP) && DATABASE_WORK.test(text)) {
      violations.push(`${path}: database work returned to the backup adapter; use rbit_storage::sqlite::backup`);
    }
    if (production(path) && !path.startsWith(STORAGE) && SNAPSHOT.test(text)) {
      violations.push(`${path}: database snapshot outside rbit_storage::sqlite::backup::snapshot_into`);
    }
    if (path.startsWith("crates/rbit-server/") && FORMAT.test(text)) {
      violations.push(`${path}: backup format redefined; use rbit_storage::sqlite::backup::FORMAT_VERSION`);
    }
  }
  return violations;
}
