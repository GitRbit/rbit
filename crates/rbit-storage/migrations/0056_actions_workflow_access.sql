-- rbit Actions: who may call a private repository's reusable workflows (GitHub's
-- `actions/permissions/access`): `none` (the default), or `user` / `organization`, the
-- other repositories of its owner.
ALTER TABLE actions_repositories ADD COLUMN access_level TEXT NOT NULL DEFAULT 'none'
    CHECK (access_level IN ('none', 'user', 'organization'));
