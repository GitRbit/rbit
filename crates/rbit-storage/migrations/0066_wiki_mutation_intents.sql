-- W2 preparation: no wiki write routes are enabled by installing this schema.
-- The catalog incarnation MUST commit before preparation; pending command/page
-- allocation below MUST also commit before native objects are written.
CREATE TEMP TABLE wiki_mutation_prerequisites (
    present INTEGER NOT NULL CONSTRAINT wiki_mutations_require_0063_0064_0065 CHECK (present = 3)
) STRICT;
INSERT INTO wiki_mutation_prerequisites
SELECT COUNT(*) FROM _sqlx_migrations WHERE version IN (63, 64, 65) AND success = 1;
DROP TABLE wiki_mutation_prerequisites;

-- Accounts also have reusable numeric IDs. Deletion removes the current binding;
-- the persistent sequence and intents retain its old, never-reused incarnation.
CREATE TABLE wiki_actor_incarnations (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    account_id INTEGER NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE
) STRICT;

CREATE TABLE wiki_page_allocations (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0)
) STRICT;

-- Official page allocation raises its floor above BOTH persistent sequences, so
-- existing catalog pages/deletions cannot collide with a future allocation.
INSERT INTO wiki_page_allocations(id)
SELECT seq FROM sqlite_sequence WHERE name = 'wiki_pages' AND seq > 0;
DELETE FROM wiki_page_allocations;

CREATE TABLE wiki_mutation_intents (
    wiki_id INTEGER NOT NULL CHECK (wiki_id > 0),
    repo_id INTEGER NOT NULL CHECK (repo_id > 0),
    actor_id INTEGER NOT NULL CHECK (actor_id > 0),
    actor_incarnation INTEGER NOT NULL CHECK (actor_incarnation > 0),
    command_id BLOB NOT NULL CHECK (length(command_id) = 16 AND command_id <> zeroblob(16)),
    operation TEXT NOT NULL CHECK (operation IN ('create', 'edit')),
    expected_generation INTEGER CHECK (expected_generation > 0),
    expected_revision TEXT CHECK (length(expected_revision) IN (40, 64) AND expected_revision NOT GLOB '*[^0-9a-f]*' AND ltrim(expected_revision, '0') <> ''),
    page_id INTEGER NOT NULL CHECK (page_id > 0),
    generation INTEGER NOT NULL CHECK (generation > 0),
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200 AND octet_length(title) <= 1024 AND instr(title, char(0)) = 0),
    content TEXT CHECK (octet_length(content) <= 262144 AND instr(content, char(0)) = 0),
    accepted_at INTEGER NOT NULL,
    target_revision TEXT CHECK (length(target_revision) IN (40, 64) AND target_revision NOT GLOB '*[^0-9a-f]*' AND ltrim(target_revision, '0') <> ''),
    state TEXT NOT NULL CHECK (state IN ('pending', 'visible', 'quarantined', 'aborted')),
    CHECK ((expected_generation IS NULL) = (expected_revision IS NULL)),
    CHECK (CASE WHEN expected_generation IS NULL THEN generation = 1
        ELSE expected_generation < 9223372036854775807 AND generation = expected_generation + 1 END),
    CHECK (operation <> 'edit' OR expected_generation IS NOT NULL),
    CHECK (state <> 'visible' OR target_revision IS NOT NULL),
    CHECK (state IN ('visible','aborted') OR content IS NOT NULL),
    PRIMARY KEY (wiki_id, actor_incarnation, command_id)
) STRICT, WITHOUT ROWID;

-- One accepted writer per incarnation. Rejection/uncertainty never erases the sole
-- recovery evidence. No parent FK cascades: deletion may not destroy pending facts.
CREATE UNIQUE INDEX wiki_mutation_one_active ON wiki_mutation_intents(wiki_id) WHERE state IN ('pending','quarantined');
CREATE INDEX wiki_mutation_recovery ON wiki_mutation_intents(state, wiki_id);

-- Binding is immutable even if a future caller accidentally uses a broad UPDATE.
CREATE TRIGGER wiki_mutation_fixed_command
BEFORE UPDATE OF wiki_id,repo_id,actor_id,actor_incarnation,command_id,operation,expected_generation,expected_revision,page_id,generation,path,title,accepted_at ON wiki_mutation_intents
WHEN NEW.wiki_id IS NOT OLD.wiki_id OR NEW.repo_id IS NOT OLD.repo_id OR NEW.actor_id IS NOT OLD.actor_id
 OR NEW.actor_incarnation IS NOT OLD.actor_incarnation
 OR NEW.command_id IS NOT OLD.command_id OR NEW.operation IS NOT OLD.operation
 OR NEW.expected_generation IS NOT OLD.expected_generation OR NEW.expected_revision IS NOT OLD.expected_revision
 OR NEW.page_id IS NOT OLD.page_id OR NEW.generation IS NOT OLD.generation
 OR NEW.path IS NOT OLD.path OR NEW.title IS NOT OLD.title OR NEW.accepted_at IS NOT OLD.accepted_at
BEGIN SELECT RAISE(ABORT, 'wiki command binding is immutable'); END;

CREATE TRIGGER wiki_mutation_fixed_target
BEFORE UPDATE OF target_revision ON wiki_mutation_intents
WHEN OLD.target_revision IS NOT NULL AND NEW.target_revision IS NOT OLD.target_revision
BEGIN SELECT RAISE(ABORT, 'wiki target binding is immutable'); END;

CREATE TRIGGER wiki_mutation_fixed_content
BEFORE UPDATE OF content ON wiki_mutation_intents
WHEN NEW.content IS NOT OLD.content
 AND NOT (OLD.state IN ('pending','quarantined') AND NEW.state IN ('visible','aborted') AND OLD.content IS NOT NULL AND NEW.content IS NULL)
BEGIN SELECT RAISE(ABORT, 'wiki payload binding is immutable'); END;

CREATE TRIGGER wiki_mutation_terminal_state
BEFORE UPDATE OF state ON wiki_mutation_intents
WHEN OLD.state IN ('visible','aborted') AND NEW.state IS NOT OLD.state
BEGIN SELECT RAISE(ABORT, 'wiki terminal state is immutable'); END;
