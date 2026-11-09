import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkPasskeyRetirement } from "../rewrite/passkey-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy passkey implementation is refused", () => {
  // Excerpts of the retired implementation (rbit-auth webauthn and services two_factor before this slice).
  const legacy = [
    ["crates/rbit-auth/src/webauthn.rs", "pub struct RelyingParty {\n    id: String,\n}\npub struct WebauthnError(pub &'static str);\n"],
    ["crates/rbit-services/src/two_factor.rs", [
      "const CHALLENGE_TTL: Duration = Duration::from_mins(10);",
      "pub const MAX_PASSKEYS: usize = 20;",
      ".ok_or_else(|| ServiceError::Invalid(\"the passkey request expired; try again\".to_owned()))?;",
      "let credential = self.two_factor.relying_party()?.verify_registration(&challenge, response)?;",
      "let registered = match self.two_factor.store.add_passkey(user.id(), &passkey, &digests).await? {",
    ].join("\n")],
    ["crates/rbit-services/src/two_factor/verification.rs", "let credential_id = RelyingParty::assertion_credential_id(response).map_err(refused)?;"],
  ];
  const violations = checkPasskeyRetirement(legacy);
  for (const rule of [/defined only by/u, /bypasses the identity passkey/u, /challenge lifetimes and rejection/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("owners, tests, re-exports and the application entry points are allowed", () => {
  assert.deepEqual(checkPasskeyRetirement([
    ["crates/rbit-identity/src/passkeys/webauthn.rs", "pub struct RelyingParty { id: String }\nrp.verify_registration(&c, r)"],
    ["crates/rbit-identity/src/passkeys.rs", "self.store.add_passkey(user.id(), &passkey, &digests).await"],
    ["crates/rbit-storage/src/identity_two_factor.rs", "TwoFactorStore::add_passkey(self, account, passkey, recovery)"],
    ["crates/rbit-auth/src/webauthn.rs", "pub use rbit_identity::passkeys::webauthn::{RelyingParty, WebauthnError};"],
    ["crates/rbit-services/tests/passkeys.rs", "rp.verify_registration(&challenge, &response).unwrap();"],
    ["crates/rbit-services/src/two_factor/verification.rs", "self.passkeys().verify_assertion(user, challenge, response, &claims).await"],
    ["crates/rbit-services/src/two_factor.rs", "pub use rbit_identity::passkeys::{Ceremony, MAX_PASSKEY_NAME, MAX_PASSKEYS};"],
  ]), []);
});

test("the current tree has no retired passkey path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkPasskeyRetirement(files), []);
});
