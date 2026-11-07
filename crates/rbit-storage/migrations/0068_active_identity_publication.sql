-- Internal ACTIVE acceptance layout. Empty by default; no runtime selects it yet.
-- The original PREPARED checkpoint and migration0067 remain immutable.
CREATE TABLE credential_active_protocol (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 checkpoint BLOB NOT NULL CHECK(typeof(checkpoint)='blob' AND length(checkpoint)=64
  AND hex(substr(checkpoint,1,8))='0200000000000000')
) STRICT;
CREATE TRIGGER credential_active_insert BEFORE INSERT ON credential_active_protocol BEGIN
 SELECT CASE WHEN 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'private active identity writer required') END;
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  OR (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1
  OR NOT EXISTS(SELECT 1 FROM credential_protocol WHERE singleton=1 AND mode='prepared'
   AND generation=substr(NEW.checkpoint,9,16) AND pair=substr(NEW.checkpoint,25,16)
   AND printf('%016X',observed_floor)=hex(substr(NEW.checkpoint,41,8))
   AND printf('%016X',retired_intent)=hex(substr(NEW.checkpoint,49,8))
   AND printf('%016X',accounts)=hex(substr(NEW.checkpoint,57,8)))
  THEN RAISE(ABORT,'fixed prepared identity publication required') END;
END;
CREATE TRIGGER credential_active_update BEFORE UPDATE ON credential_active_protocol BEGIN
 SELECT RAISE(ABORT,'active identity publication is immutable');
END;
CREATE TRIGGER credential_active_delete BEFORE DELETE ON credential_active_protocol BEGIN
 SELECT RAISE(ABORT,'active identity publication is retained');
END;
CREATE TRIGGER identity_v3_accounts_insert BEFORE INSERT ON accounts BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_accounts_update BEFORE UPDATE ON accounts BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_accounts_delete BEFORE DELETE ON accounts BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_user_logins_insert BEFORE INSERT ON user_logins BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_user_logins_update BEFORE UPDATE ON user_logins BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_user_logins_delete BEFORE DELETE ON user_logins BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_revocations_insert BEFORE INSERT ON credential_revocations BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_revocations_update BEFORE UPDATE ON credential_revocations BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_revocations_delete BEFORE DELETE ON credential_revocations BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_two_factor_insert BEFORE INSERT ON two_factor BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_two_factor_update BEFORE UPDATE ON two_factor BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_two_factor_delete BEFORE DELETE ON two_factor BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_recovery_codes_insert BEFORE INSERT ON recovery_codes BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_recovery_codes_update BEFORE UPDATE ON recovery_codes BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_recovery_codes_delete BEFORE DELETE ON recovery_codes BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_passkeys_insert BEFORE INSERT ON passkeys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_passkeys_update BEFORE UPDATE ON passkeys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_passkeys_delete BEFORE DELETE ON passkeys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_external_identities_insert BEFORE INSERT ON external_identities BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_external_identities_update BEFORE UPDATE ON external_identities BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_external_identities_delete BEFORE DELETE ON external_identities BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_email_tokens_insert BEFORE INSERT ON email_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_email_tokens_update BEFORE UPDATE ON email_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_email_tokens_delete BEFORE DELETE ON email_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_access_tokens_insert BEFORE INSERT ON access_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_access_tokens_update BEFORE UPDATE ON access_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_access_tokens_delete BEFORE DELETE ON access_tokens BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_ssh_keys_insert BEFORE INSERT ON ssh_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_ssh_keys_update BEFORE UPDATE ON ssh_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_ssh_keys_delete BEFORE DELETE ON ssh_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_deploy_keys_insert BEFORE INSERT ON deploy_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_deploy_keys_update BEFORE UPDATE ON deploy_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_deploy_keys_delete BEFORE DELETE ON deploy_keys BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_server_secrets_insert BEFORE INSERT ON server_secrets BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_server_secrets_update BEFORE UPDATE ON server_secrets BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_server_secrets_delete BEFORE DELETE ON server_secrets BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_instance_settings_insert BEFORE INSERT ON instance_settings BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_instance_settings_update BEFORE UPDATE ON instance_settings BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_instance_settings_delete BEFORE DELETE ON instance_settings BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_epoch_allocator_insert BEFORE INSERT ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_epoch_allocator_update BEFORE UPDATE ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_epoch_allocator_delete BEFORE DELETE ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_session_pair_insert BEFORE INSERT ON credential_session_pair BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_session_pair_update BEFORE UPDATE ON credential_session_pair BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
CREATE TRIGGER identity_v3_credential_session_pair_delete BEFORE DELETE ON credential_session_pair BEGIN
 SELECT CASE WHEN EXISTS(SELECT 1 FROM credential_active_protocol)
  AND 'v3' COLLATE rbit_credential_active_v3 <> 'v3'
  THEN RAISE(ABORT,'active identity writer refused') END;
END;
