-- DORMANT: no current credential/session writer uses this allocator.
-- This one-time guard proves only the supported pre-global history:
-- initial epoch0; each committed local increment inserted one automatic intent ID.
CREATE TEMP TABLE credential_epoch_seed_guard (
    history_ok INTEGER NOT NULL CONSTRAINT credential_epoch_history_invalid CHECK (history_ok = 1),
    capacity_ok INTEGER NOT NULL CONSTRAINT credential_epoch_space_exhausted CHECK (capacity_ok = 1)
) STRICT;

INSERT INTO credential_epoch_seed_guard
SELECT
    CASE WHEN
        NOT EXISTS (SELECT 1 FROM user_logins WHERE typeof(authentication_epoch) <> 'integer' OR authentication_epoch < 0)
        AND NOT EXISTS (SELECT 1 FROM credential_revocations
            WHERE typeof(id) <> 'integer' OR id <= 0 OR typeof(target_epoch) <> 'integer'
            OR target_epoch <= 0 OR target_epoch > id)
        AND (SELECT COUNT(*) FROM sqlite_sequence WHERE name = 'credential_revocations') <= 1
        AND NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = 'credential_revocations'
            AND (typeof(seq) <> 'integer' OR seq <= 0))
        AND COALESCE((SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'credential_revocations'), 0)
            >= COALESCE((SELECT MAX(id) FROM credential_revocations), 0)
        AND COALESCE((SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'credential_revocations'), 0)
            >= COALESCE((SELECT MAX(authentication_epoch) FROM user_logins), 0)
    THEN 1 ELSE 0 END,
    CASE WHEN MAX(
        COALESCE((SELECT MAX(authentication_epoch) FROM user_logins), 0),
        COALESCE((SELECT MAX(target_epoch) FROM credential_revocations), 0),
        COALESCE((SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'credential_revocations'), 0)
    ) < 9223372036854775807 THEN 1 ELSE 0 END;

CREATE TABLE credential_epoch_allocator (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    high_water INTEGER NOT NULL CHECK (high_water >= 0)
) STRICT;
INSERT INTO credential_epoch_allocator(singleton, high_water)
SELECT 1, MAX(
    COALESCE((SELECT MAX(authentication_epoch) FROM user_logins), 0),
    COALESCE((SELECT MAX(target_epoch) FROM credential_revocations), 0),
    COALESCE((SELECT MAX(seq) FROM sqlite_sequence WHERE name = 'credential_revocations'), 0)
);
DROP TABLE credential_epoch_seed_guard;

-- The allocator is credential authority too. Only registered current writers may
-- allocate/raise its floor; the v1 marker still does NOT activate fenced mode.
CREATE TRIGGER credential_epoch_writer_insert BEFORE INSERT ON credential_epoch_allocator BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;
CREATE TRIGGER credential_epoch_writer_update BEFORE UPDATE ON credential_epoch_allocator BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;
CREATE TRIGGER credential_epoch_writer_delete BEFORE DELETE ON credential_epoch_allocator BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;
