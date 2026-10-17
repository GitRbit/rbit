-- Representative rows of a milestone 1 database (schema of migrations 0001–0008), used by
-- `tests/upgrade.rs`. Ids are deliberately sparse so a rebuilt table that renumbered its
-- rows would be caught.

INSERT INTO accounts (id, kind, name, display_name, bio, created_at, updated_at) VALUES
    (1, 'user', 'alice', 'Alice A.', 'Admin of this instance', 1700000000, 1700000100),
    (2, 'user', 'bob', NULL, NULL, 1700000200, 1700000200),
    (3, 'user', 'carol', 'Carol', NULL, 1700000300, 1700000300),
    (10, 'organization', 'acme', 'Acme Inc.', NULL, 1700000400, 1700000400),
    -- An organisation created before org profiles existed has no `org_profiles` row.
    (11, 'organization', 'bare', NULL, NULL, 1700000500, 1700000500);

INSERT INTO user_logins (account_id, email, password_hash, is_admin, suspended_at, email_verified_at) VALUES
    (1, 'alice@example.com', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA', 1, NULL, 1700000050),
    (2, 'bob@example.com', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA', 0, NULL, NULL),
    (3, 'carol@example.com', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA', 0, 1700009000, NULL);

INSERT INTO org_profiles (account_id, description, website) VALUES
    (10, 'Makers of everything', 'https://acme.example');

INSERT INTO org_members (org_id, user_id, role) VALUES
    (10, 1, 'owner'),
    (10, 2, 'member'),
    (11, 1, 'owner');

INSERT INTO account_profiles (account_id, website, location) VALUES
    (1, 'https://alice.example', 'Lisbon');

INSERT INTO repositories (id, owner_id, name, description, visibility, default_branch, created_at, updated_at, pushed_at) VALUES
    (5, 1, 'dotfiles', 'My dotfiles', 'public', 'main', 1700001000, 1700001000, 1700001500),
    (6, 10, 'rocket', NULL, 'private', 'trunk', 1700002000, 1700002000, NULL),
    (9, 11, 'empty', NULL, 'public', 'main', 1700003000, 1700003000, NULL);

INSERT INTO stars (account_id, repo_id, created_at) VALUES
    (2, 5, 1700004000);

INSERT INTO access_tokens (id, account_id, name, token_digest, hint, scope, created_at, last_used_at, expires_at) VALUES
    (7, 1, 'laptop', X'0101010101010101010101010101010101010101010101010101010101010101', 'ab12', 'write', 1700005000, 1700005500, NULL),
    (42, 2, 'ci', X'0202020202020202020202020202020202020202020202020202020202020202', 'cd34', 'read', 1700006000, NULL, 4000000000);

INSERT INTO instance_settings (key, value, updated_at) VALUES
    ('registration', 'closed', 1700007000);

INSERT INTO two_factor (account_id, secret, created_at, confirmed_at, last_used_step) VALUES
    (1, X'00112233445566778899AABBCCDDEEFF00112233', 1700008000, 1700008100, 56666700);

INSERT INTO recovery_codes (account_id, code_digest, used_at) VALUES
    (1, X'0303030303030303030303030303030303030303030303030303030303030303', NULL),
    (1, X'0404040404040404040404040404040404040404040404040404040404040404', 1700008200);

INSERT INTO email_tokens (digest, account_id, purpose, email, created_at, expires_at, used_at) VALUES
    (X'0505050505050505050505050505050505050505050505050505050505050505', 2, 'verify', 'bob@example.com', 1700009000, 1700095400, NULL);

INSERT INTO audit_events (id, created_at, category, action, actor_id, actor_name, target_name, ip_address, detail) VALUES
    (1, 1700000060, 'auth', 'sign_in', 1, 'alice', 'alice', '127.0.0.1', ''),
    (2, 1700009000, 'admin', 'user_suspended', 1, 'alice', 'carol', '127.0.0.1', 'spam');

INSERT INTO ssh_keys (id, account_id, title, public_key, algorithm, fingerprint, created_at, last_used_at) VALUES
    (3, 1, 'laptop', 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAliceAliceAliceAliceAliceAliceAliceAlice', 'ssh-ed25519',
     'SHA256:aliceAliceAliceAliceAliceAliceAliceAliceAli', 1700010000, 1700010500);

INSERT INTO deploy_keys (id, repo_id, title, public_key, algorithm, fingerprint, read_only, created_at, last_used_at) VALUES
    (4, 6, 'ci', 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDeployDeployDeployDeployDeployDeployDeplo', 'ssh-ed25519',
     'SHA256:deployDeployDeployDeployDeployDeployDeployDe', 0, 1700011000, NULL);
