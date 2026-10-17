-- Milestone 2 wave B, workstream I: full-text search over issue and pull request titles
-- and bodies. An external-content FTS5 index kept in step with `issues` by triggers
-- (they also fire for rows removed by `ON DELETE CASCADE`).

CREATE VIRTUAL TABLE issues_fts USING fts5(
    title,
    body,
    content = 'issues',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER issues_fts_insert AFTER INSERT ON issues BEGIN
    INSERT INTO issues_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;

CREATE TRIGGER issues_fts_delete AFTER DELETE ON issues BEGIN
    INSERT INTO issues_fts (issues_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
END;

CREATE TRIGGER issues_fts_update AFTER UPDATE OF title, body ON issues BEGIN
    INSERT INTO issues_fts (issues_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
    INSERT INTO issues_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;

INSERT INTO issues_fts (issues_fts) VALUES ('rebuild');
