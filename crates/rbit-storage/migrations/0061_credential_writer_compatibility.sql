-- Credential/account writer compatibility, not session-fencing activation.
-- Each connection performing protected DML must register the pure v1 collation.
-- Unknown/missing protocol refuses even previously cached legacy statements.
-- Reads and unrelated table writes do not require this connection capability.

CREATE TRIGGER credential_writer_accounts_insert BEFORE INSERT ON accounts BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_accounts_update BEFORE UPDATE ON accounts BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_accounts_delete BEFORE DELETE ON accounts BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_user_logins_insert BEFORE INSERT ON user_logins BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_user_logins_update BEFORE UPDATE ON user_logins BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_user_logins_delete BEFORE DELETE ON user_logins BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_credential_revocations_insert BEFORE INSERT ON credential_revocations BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_credential_revocations_update BEFORE UPDATE ON credential_revocations BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_credential_revocations_delete BEFORE DELETE ON credential_revocations BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_two_factor_insert BEFORE INSERT ON two_factor BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_two_factor_update BEFORE UPDATE ON two_factor BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_two_factor_delete BEFORE DELETE ON two_factor BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_recovery_codes_insert BEFORE INSERT ON recovery_codes BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_recovery_codes_update BEFORE UPDATE ON recovery_codes BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_recovery_codes_delete BEFORE DELETE ON recovery_codes BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_passkeys_insert BEFORE INSERT ON passkeys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_passkeys_update BEFORE UPDATE ON passkeys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_passkeys_delete BEFORE DELETE ON passkeys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_external_identities_insert BEFORE INSERT ON external_identities BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_external_identities_update BEFORE UPDATE ON external_identities BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_external_identities_delete BEFORE DELETE ON external_identities BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_email_tokens_insert BEFORE INSERT ON email_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_email_tokens_update BEFORE UPDATE ON email_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_email_tokens_delete BEFORE DELETE ON email_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_access_tokens_insert BEFORE INSERT ON access_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_access_tokens_update BEFORE UPDATE ON access_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_access_tokens_delete BEFORE DELETE ON access_tokens BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_ssh_keys_insert BEFORE INSERT ON ssh_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_ssh_keys_update BEFORE UPDATE ON ssh_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_ssh_keys_delete BEFORE DELETE ON ssh_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_deploy_keys_insert BEFORE INSERT ON deploy_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_deploy_keys_update BEFORE UPDATE ON deploy_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_deploy_keys_delete BEFORE DELETE ON deploy_keys BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_server_secrets_insert BEFORE INSERT ON server_secrets BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_server_secrets_update BEFORE UPDATE ON server_secrets BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_server_secrets_delete BEFORE DELETE ON server_secrets BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_instance_settings_insert BEFORE INSERT ON instance_settings BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_instance_settings_update BEFORE UPDATE ON instance_settings BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;

CREATE TRIGGER credential_writer_instance_settings_delete BEFORE DELETE ON instance_settings BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT, 'credential writer version refused') END;
END;
