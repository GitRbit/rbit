-- Actions: environments. GitHub's wait timer: minutes a job in the environment waits, once
-- ready, before a runner may take it (0 to 43200, as on GitHub).
ALTER TABLE actions_environments ADD COLUMN wait_timer INTEGER NOT NULL DEFAULT 0 CHECK (wait_timer BETWEEN 0 AND 43200);
