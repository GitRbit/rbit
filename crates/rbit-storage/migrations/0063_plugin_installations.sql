-- Host package ledger only: these rows do not enable plugins or approve authority.
CREATE TABLE plugin_objects (
    digest TEXT PRIMARY KEY CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    manifest TEXT NOT NULL CHECK (length(CAST(manifest AS BLOB)) BETWEEN 1 AND 1048576),
    wire_bytes INTEGER NOT NULL CHECK (wire_bytes BETWEEN 1 AND 2147483648),
    recorded_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE plugin_installations (
    plugin_id TEXT NOT NULL CHECK (length(plugin_id) BETWEEN 1 AND 128),
    version TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 128),
    digest TEXT NOT NULL REFERENCES plugin_objects(digest),
    installed_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    installed_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, version)
) STRICT, WITHOUT ROWID;

CREATE TABLE plugin_package_operations (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 64 AND id NOT GLOB '*[^a-z0-9-]*'),
    plugin_id TEXT NOT NULL CHECK (length(plugin_id) BETWEEN 1 AND 128),
    version TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 128),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    manifest TEXT NOT NULL CHECK (length(CAST(manifest AS BLOB)) BETWEEN 1 AND 1048576),
    wire_bytes INTEGER NOT NULL CHECK (wire_bytes BETWEEN 1 AND 2147483648),
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    stage INTEGER NOT NULL CHECK (stage BETWEEN 0 AND 3),
    revision INTEGER NOT NULL CHECK (revision BETWEEN 0 AND 9223372036854775806),
    error INTEGER CHECK (error BETWEEN 0 AND 5),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK ((stage = 3 AND error IS NOT NULL) OR (stage <> 3 AND error IS NULL))
) STRICT, WITHOUT ROWID;

CREATE INDEX plugin_package_operations_recovery ON plugin_package_operations(id) WHERE stage <> 2;
CREATE INDEX plugin_installations_digest ON plugin_installations(digest);

CREATE TRIGGER plugin_objects_immutable BEFORE UPDATE ON plugin_objects BEGIN
    SELECT RAISE(ABORT, 'package objects are immutable');
END;
CREATE TRIGGER plugin_installations_pin BEFORE UPDATE OF plugin_id, version, digest ON plugin_installations BEGIN
    SELECT RAISE(ABORT, 'package identity is pinned');
END;
CREATE TRIGGER plugin_objects_capacity BEFORE INSERT ON plugin_objects
WHEN (SELECT COUNT(*) FROM plugin_objects) >= 1024 AND NOT EXISTS (SELECT 1 FROM plugin_objects WHERE digest = NEW.digest)
BEGIN SELECT RAISE(ABORT, 'package object capacity exhausted'); END;
CREATE TRIGGER plugin_installations_capacity BEFORE INSERT ON plugin_installations
WHEN (SELECT COUNT(*) FROM plugin_installations) >= 1024 AND NOT EXISTS (SELECT 1 FROM plugin_installations WHERE plugin_id = NEW.plugin_id AND version = NEW.version)
BEGIN SELECT RAISE(ABORT, 'package installation capacity exhausted'); END;
CREATE TRIGGER plugin_package_operations_capacity BEFORE INSERT ON plugin_package_operations
WHEN (SELECT COUNT(*) FROM plugin_package_operations) >= 4096
BEGIN SELECT RAISE(ABORT, 'package operation capacity exhausted'); END;
