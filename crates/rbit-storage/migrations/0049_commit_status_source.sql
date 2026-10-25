-- rbit Actions (docs/design/actions.md §11.12): who posted a commit status. Statuses rbit
-- Actions posts for its jobs have no creator and `source = 'actions'`, so they can never be
-- mistaken for a report a person typed, and a later branch protection option can require a
-- check to come from Actions. Every existing status was posted by a person.
ALTER TABLE commit_statuses
    ADD COLUMN source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'actions'));
