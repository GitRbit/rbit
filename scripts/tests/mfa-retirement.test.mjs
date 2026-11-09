import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkMfaRetirement } from "../rewrite/mfa-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy authenticator implementation is refused", () => {
  // Excerpts of the retired implementation (crates/rbit-services/src/two_factor*.rs before this slice).
  const legacy = [
    ["crates/rbit-auth/src/totp.rs", "pub struct TotpSecret(Vec<u8>);\npub struct RecoveryCode(String);\n"],
    ["crates/rbit-services/src/two_factor.rs", [
      "pub const RECOVERY_CODE_COUNT: usize = 10;",
      "let already = || ServiceError::Invalid(\"an authenticator app is already set up\".to_owned());",
      "let secret = TotpSecret::generate();",
      "if !self.store.begin_enrolment(user.id(), secret.as_bytes()).await? {",
      "let Some(first_factor) = self.store.confirm(user.id(), &record.secret, step, &digests(user, &codes)).await? else {",
    ].join("\n")],
    ["crates/rbit-services/src/two_factor/verification.rs", "if let Some(recovery) = RecoveryCode::parse(code) {"],
  ];
  const violations = checkMfaRetirement(legacy);
  for (const rule of [/defined only by/u, /handled only by/u, /write bypasses/u, /count and enrolment rejection/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("owners, tests, re-exports and service entry points are allowed", () => {
  assert.deepEqual(checkMfaRetirement([
    ["crates/rbit-identity/src/two_factor/totp.rs", "pub struct TotpSecret(Vec<u8>);\nlet s = TotpSecret::generate();"],
    ["crates/rbit-identity/src/two_factor.rs", "self.store.confirm(user.id(), &record.secret, step, &digests).await"],
    ["crates/rbit-storage/src/identity_two_factor.rs", "TwoFactorStore::confirm(self, account, secret, step, recovery)"],
    ["crates/rbit-auth/src/totp.rs", "pub use rbit_identity::two_factor::totp::{RecoveryCode, TotpSecret};"],
    ["crates/rbit-services/tests/two_factor.rs", "let secret = TotpSecret::from_bytes(&bytes).unwrap();"],
    ["crates/rbit-web/src/routes/settings.rs", "services.two_factor.begin_enrolment(&user, &issuer).await"],
    ["crates/rbit-services/src/two_factor.rs", "pub use rbit_identity::two_factor::{Enrolment, RECOVERY_CODE_COUNT};"],
  ]), []);
});

test("the current tree has no retired authenticator path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkMfaRetirement(files), []);
});
