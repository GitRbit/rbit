-- rbit Actions (docs/design/actions.md §11.3): who wrote a comment. A workflow task commenting
-- with its token writes as rbit Actions: no author and `source = 'actions'`, so the comment
-- shows as rbit-actions[bot] and is never mistaken for one whose author was deleted (an
-- author of NULL with `source = 'user'`, a ghost). Every existing comment was written by a
-- person.
ALTER TABLE issue_comments
    ADD COLUMN source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'actions'));
