-- Interface preferences of a user that follow them to every device they sign in on.
-- `theme` is a theme slug (`system`, `light`, `dark`, or a tinted `base16-…`/`base24-…`
-- slug). The web layer validates it against the theme catalog when reading, so a slug a
-- later release no longer ships falls back to the default instead of failing.
CREATE TABLE user_preferences (
    account_id  INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    theme       TEXT    CHECK (theme IS NULL OR (length(theme) BETWEEN 1 AND 64)),
    updated_at  INTEGER NOT NULL
) STRICT;
