-- PREPARED-only stopped identity upgrade foundation; no runtime activation.
-- Default mode retains existing sessions. Only the private stopped composition may
-- leave legacy mode; ordinary compatible pools carry a DENY fenced-writer marker.
CREATE TEMP TABLE identity_upgrade_prerequisites (
 present INTEGER NOT NULL CONSTRAINT identity_upgrade_requires_0063_0064 CHECK(present=2)
) STRICT;
INSERT INTO identity_upgrade_prerequisites
SELECT count(*) FROM _sqlx_migrations WHERE version IN (63,64) AND success=1;
DROP TABLE identity_upgrade_prerequisites;
CREATE TABLE credential_protocol (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 mode TEXT NOT NULL CHECK(mode IN ('legacy','preparing','prepared')),
 generation BLOB CHECK(generation IS NULL OR (typeof(generation)='blob' AND length(generation)=16 AND generation<>zeroblob(16))),
 pair BLOB CHECK(pair IS NULL OR (typeof(pair)='blob' AND length(pair)=16)),
 observed_floor INTEGER NOT NULL CHECK(observed_floor>=0),
 retired_intent INTEGER NOT NULL CHECK(retired_intent>=0),
 accounts INTEGER NOT NULL CHECK(accounts>=0),
 final_floor INTEGER NOT NULL CHECK(final_floor>=0),
 CHECK((mode='legacy' AND generation IS NULL AND pair IS NULL AND observed_floor=0 AND retired_intent=0 AND accounts=0 AND final_floor=0)
  OR (mode='preparing' AND generation IS NOT NULL AND pair IS NOT NULL AND final_floor=0)
  OR (mode='prepared' AND generation IS NOT NULL AND pair IS NOT NULL AND final_floor>=observed_floor AND final_floor-observed_floor=accounts))
) STRICT;
INSERT INTO credential_protocol VALUES(1,'legacy',NULL,NULL,0,0,0,0);
CREATE TRIGGER credential_protocol_retained BEFORE DELETE ON credential_protocol BEGIN
 SELECT RAISE(ABORT,'identity protocol state is retained');
END;
CREATE TRIGGER credential_protocol_insert BEFORE INSERT ON credential_protocol BEGIN
 SELECT RAISE(ABORT,'identity protocol state already exists');
END;
CREATE TRIGGER credential_protocol_update BEFORE UPDATE ON credential_protocol BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_fenced_v2 <> 'v2' THEN RAISE(ABORT,'stopped identity writer required') END;
 SELECT CASE WHEN NEW.singleton IS NOT OLD.singleton OR NOT (
  (OLD.mode='legacy' AND NEW.mode='preparing') OR
  (OLD.mode='preparing' AND NEW.mode='prepared' AND NEW.generation IS OLD.generation AND NEW.pair IS OLD.pair
   AND NEW.observed_floor=OLD.observed_floor AND NEW.retired_intent=OLD.retired_intent AND NEW.accounts=OLD.accounts)
 ) THEN RAISE(ABORT,'identity protocol transition refused') END;
END;
CREATE TRIGGER identity_v2_accounts_insert BEFORE INSERT ON accounts BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_accounts_update BEFORE UPDATE ON accounts BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_accounts_delete BEFORE DELETE ON accounts BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_user_logins_insert BEFORE INSERT ON user_logins BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_user_logins_update BEFORE UPDATE ON user_logins BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_user_logins_delete BEFORE DELETE ON user_logins BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_revocations_insert BEFORE INSERT ON credential_revocations BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_revocations_update BEFORE UPDATE ON credential_revocations BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_revocations_delete BEFORE DELETE ON credential_revocations BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_two_factor_insert BEFORE INSERT ON two_factor BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_two_factor_update BEFORE UPDATE ON two_factor BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_two_factor_delete BEFORE DELETE ON two_factor BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_recovery_codes_insert BEFORE INSERT ON recovery_codes BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_recovery_codes_update BEFORE UPDATE ON recovery_codes BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_recovery_codes_delete BEFORE DELETE ON recovery_codes BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_passkeys_insert BEFORE INSERT ON passkeys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_passkeys_update BEFORE UPDATE ON passkeys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_passkeys_delete BEFORE DELETE ON passkeys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_external_identities_insert BEFORE INSERT ON external_identities BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_external_identities_update BEFORE UPDATE ON external_identities BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_external_identities_delete BEFORE DELETE ON external_identities BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_email_tokens_insert BEFORE INSERT ON email_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_email_tokens_update BEFORE UPDATE ON email_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_email_tokens_delete BEFORE DELETE ON email_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_access_tokens_insert BEFORE INSERT ON access_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_access_tokens_update BEFORE UPDATE ON access_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_access_tokens_delete BEFORE DELETE ON access_tokens BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_ssh_keys_insert BEFORE INSERT ON ssh_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_ssh_keys_update BEFORE UPDATE ON ssh_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_ssh_keys_delete BEFORE DELETE ON ssh_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_deploy_keys_insert BEFORE INSERT ON deploy_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_deploy_keys_update BEFORE UPDATE ON deploy_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_deploy_keys_delete BEFORE DELETE ON deploy_keys BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_server_secrets_insert BEFORE INSERT ON server_secrets BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_server_secrets_update BEFORE UPDATE ON server_secrets BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_server_secrets_delete BEFORE DELETE ON server_secrets BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_instance_settings_insert BEFORE INSERT ON instance_settings BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_instance_settings_update BEFORE UPDATE ON instance_settings BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_instance_settings_delete BEFORE DELETE ON instance_settings BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_epoch_allocator_insert BEFORE INSERT ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_epoch_allocator_update BEFORE UPDATE ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_epoch_allocator_delete BEFORE DELETE ON credential_epoch_allocator BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_session_pair_insert BEFORE INSERT ON credential_session_pair BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_session_pair_update BEFORE UPDATE ON credential_session_pair BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
CREATE TRIGGER identity_v2_credential_session_pair_delete BEFORE DELETE ON credential_session_pair BEGIN
 SELECT CASE WHEN 'v2' COLLATE rbit_credential_layout_v2 <> 'v2' THEN RAISE(ABORT,'identity writer layout refused') END;
 SELECT CASE WHEN (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))<>1 OR
  (SELECT CASE WHEN singleton=1 AND mode='legacy' THEN 1 WHEN singleton=1 AND mode IN ('preparing','prepared')
   AND 'v2' COLLATE rbit_credential_fenced_v2 = 'v2' THEN 1 ELSE 0 END FROM credential_protocol LIMIT 1) IS NOT 1
  THEN RAISE(ABORT,'identity writer mode refused') END;
END;
