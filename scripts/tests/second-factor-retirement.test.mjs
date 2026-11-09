import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkSecondFactorRetirement } from "../rewrite/second-factor-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy second step is refused", () => {
  // Excerpts of the retired implementation (crates/rbit-services/src/two_factor* before this slice).
  const legacy = [
    ["crates/rbit-services/src/two_factor/pending.rs", "pub(super) struct PendingSignIn {\n    pub account: AccountId,\n}\n"],
    ["crates/rbit-services/src/two_factor.rs", [
      "pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_mins(5);",
      "pub enum SecondFactorProof<'a> { Code(&'a str) }",
    ].join("\n")],
    ["crates/rbit-services/src/two_factor/sign_in.rs", "Ok(store.spend_recovery_code(account, digest).await?)"],
  ];
  const violations = checkSecondFactorRetirement(legacy);
  for (const rule of [/defined only by/u, /claims bypass/u, /lifetimes and attempt limits/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("identity, storage, the claim adapter, tests and re-exports are allowed", () => {
  const claim = "self.store.claim_step_at_epoch(account, secret, step, epoch).await";
  assert.deepEqual(checkSecondFactorRetirement([
    ["crates/rbit-identity/src/second_factor/pending.rs", "pub struct PendingSignIn { pub account: AccountId }"],
    ["crates/rbit-identity/src/second_factor.rs", "pub const PENDING_SIGN_IN_TTL: Duration = Duration::from_mins(5);"],
    ["crates/rbit-services/src/two_factor/verification.rs", claim],
    ["crates/rbit-storage/src/sqlite/two_factor/claims.rs", claim],
    ["crates/rbit-services/tests/identity_races/second_factor.rs", claim],
    ["crates/rbit-services/src/two_factor.rs", "pub use rbit_identity::second_factor::{PENDING_SIGN_IN_TTL, SecondFactor, SecondFactorProof};"],
  ]), []);
});

test("the current tree has no retired second-step path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkSecondFactorRetirement(files), []);
});
