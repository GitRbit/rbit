import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRecoveryRetirement } from "../rewrite/recovery-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy recovery service implementation is refused", () => {
  // Excerpts of the retired implementation (crates/rbit-services/src/recovery*.rs before this slice).
  const legacy = [
    ["crates/rbit-services/src/recovery.rs", [
      "pub const RESET_LIFETIME_SECONDS: i64 = 60 * 60;",
      "let email: Email = address.trim().parse().map_err(|_| ServiceError::Invalid(\"enter a valid email address\".to_owned()))?;",
      "let record = store.find_valid(&digest, TokenPurpose::Reset, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;",
      "let record = store.consume(&digest, TokenPurpose::Verify, Timestamp::now()).await?.ok_or(ServiceError::NotFound)?;",
      "let token = NewEmailToken { digest: secret.digest(), account_id: account.id, purpose, email, expires_at };",
    ].join("\n")],
    ["crates/rbit-services/src/recovery/password_changes.rs", [
      "let attempt = self.passwords.begin()?;",
      "attempt.hash_if_different(std::mem::take(&mut *new_password), current).await?",
    ].join("\n")],
  ];
  const violations = checkRecoveryRetirement(legacy);
  for (const rule of [/link issue or redemption/u, /password checks and hashing/u, /lifetimes and rejection text/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("owners, tests, other services and re-exports are allowed", () => {
  const consume = "store.consume(digest, TokenPurpose::Verify, now).await";
  assert.deepEqual(checkRecoveryRetirement([
    ["crates/rbit-storage/src/identity_recovery.rs", consume],
    ["crates/rbit-identity/src/recovery/links.rs", consume],
    ["crates/rbit-services/tests/recovery.rs", consume],
    ["crates/rbit-services/src/external_auth/links/tests.rs", "f.database.recovery().mark_verified(a, e, now)"],
    ["crates/rbit-services/src/two_factor/password_checks.rs", "let attempt = self.two_factor.passwords.begin()?;"],
    ["crates/rbit-services/src/recovery.rs", "pub use rbit_identity::recovery::{EMAIL_UNAVAILABLE, RESET_LIFETIME_SECONDS};"],
  ]), []);
  assert.match(checkRecoveryRetirement([["crates/rbit-web/src/x.rs", consume]])[0], /bypasses/u);
});

test("the current tree has no retired recovery path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkRecoveryRetirement(files), []);
});
