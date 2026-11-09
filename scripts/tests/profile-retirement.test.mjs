import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkProfileRetirement } from "../rewrite/profile-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy profile service implementation is refused", () => {
  // Excerpts of the retired implementation (crates/rbit-services/src/profiles*.rs before this slice).
  const legacy = [
    ["crates/rbit-services/src/profiles.rs", [
      "const MAX_DISPLAY_NAME_LEN: usize = 80;",
      "return Err(ServiceError::Invalid(\"website must be a URL starting with http:// or https://\".to_owned()));",
      "if !self.store().set_email_if_password(user.id(), &email, &verified).await? {",
      "if !self.ctx.database.accounts().rename_if_password(user.id(), &name, &verified).await? {",
    ].join("\n")],
    ["crates/rbit-services/src/profiles/password_changes.rs", [
      "return Err(ServiceError::Invalid(\"choose a different password from your current password\".into()));",
      "let attempt = self.accounts.password_work().begin()?;",
      "if !store.set_password_hash_if_current(user.id(), &hash, &verified).await? {",
    ].join("\n")],
    ["crates/rbit-services/src/profiles/password_checks.rs", "let valid = self.accounts.password_work().begin()?.verify(password, hash).await?;"],
  ];
  const violations = checkProfileRetirement(legacy);
  for (const rule of [/compare-and-set write bypasses/u, /password checks run only/u, /limits and rejection text/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("owners, tests and the adapter's work handle are allowed", () => {
  const cas = "store.set_password_hash_if_current(account, hash, expected).await";
  assert.deepEqual(checkProfileRetirement([
    ["crates/rbit-storage/src/identity_profiles.rs", cas],
    ["crates/rbit-identity/src/profiles/credentials.rs", cas],
    ["crates/rbit-services/tests/profiles.rs", "accounts().rename_if_password(id, &name, \"stale\")"],
    ["crates/rbit-services/src/profiles.rs", "ProfileApplication::new(store, accounts, throttle, self.accounts.password_work(), min)"],
    ["crates/rbit-services/src/accounts/x.rs", "s.set_password_hash_if_current_preserving(a, b, c)"],
    ["crates/rbit-services/src/b.rs", "// const MAX_BIO_LEN: usize = 300;\n"],
  ]), []);
  assert.match(checkProfileRetirement([["crates/rbit-web/src/x.rs", cas]])[0], /bypasses/u);
});

test("the current tree has no retired profile path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkProfileRetirement(files), []);
});
