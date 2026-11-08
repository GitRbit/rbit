-- Wiki publication metadata: the display name that published each finalized
-- generation and the kind of change. Presentation data only, never an ACL fact.
-- Requires the Wiki writer protocol 0069 and startup scratch 0070.
CREATE TEMP TABLE wiki_revision_metadata_prerequisite (
    present INTEGER NOT NULL CONSTRAINT wiki_revision_metadata_requires_0070 CHECK (present = 1)
) STRICT;
INSERT INTO wiki_revision_metadata_prerequisite
SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 70 AND success = 1;
DROP TABLE wiki_revision_metadata_prerequisite;

-- Existing rows keep NULL: their author and change kind are unknown.
ALTER TABLE wiki_revisions ADD COLUMN author TEXT
    CHECK (author IS NULL OR (length(author) BETWEEN 1 AND 64 AND instr(author, char(0)) = 0));
ALTER TABLE wiki_revisions ADD COLUMN change TEXT
    CHECK (change IS NULL OR change IN ('create', 'edit', 'rename', 'delete', 'restore'));
