-- Issue and pull request lists default to one kind and one state, newest first. With
-- this index such a page is read in order and stops at its `LIMIT` instead of reading
-- every thread of that kind and state and sorting them. Ties are ordered by issue id,
-- the rowid every index entry ends with, so walking it backwards gives newest first.
CREATE INDEX issues_by_repo_created ON issues (repo_id, kind, state, created_at);
