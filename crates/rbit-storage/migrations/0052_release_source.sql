-- rbit Actions (docs/design/actions.md §11.3): who made a release or uploaded one of its
-- files. A workflow task writes as rbit Actions: no author or uploader and
-- `source = 'actions'`, so it shows as rbit-actions[bot] and is never mistaken for one
-- whose author was deleted. Every existing release and file was made by a person.
ALTER TABLE releases
    ADD COLUMN source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'actions'));
ALTER TABLE release_assets
    ADD COLUMN source TEXT NOT NULL DEFAULT 'user' CHECK (source IN ('user', 'actions'));
