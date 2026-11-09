-- Widen the saved interface language to every offered language. 0062 limited
-- user_logins.locale to 'en' and 'es', and SQLite cannot change a CHECK in place; rebuilding
-- user_logins would also run into its identity writer guards. The full choice therefore
-- gets a column of its own, read first. `locale` keeps mirroring English and Spanish
-- (NULL for the other languages), so an older build still reads a valid choice or follows
-- the browser. Diagnostics and arbitrary strings remain unstorable.
ALTER TABLE user_logins ADD COLUMN interface_locale TEXT CHECK (
    interface_locale IS NULL OR interface_locale IN ('en', 'es', 'zh-Hans', 'pt-BR', 'ja', 'de', 'ru', 'fr', 'ko')
);
