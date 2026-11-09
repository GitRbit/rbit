import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkAccessPolicyRetirement } from "../rewrite/access-policy-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

test("the retired policy adapter and its rules cannot return", () => {
  assert.match(checkAccessPolicyRetirement([["crates/rbit-auth/src/policy.rs", "//! anything\n"]])[0], /retired policy adapter/u);
  for (const source of ["pub fn repo_access(viewer: Option<&User>) -> RepoAccess { todo!() }", "fn grant_holder() {}", "pub struct Grants { pub team: Option<Permission> }"]) {
    assert.match(checkAccessPolicyRetirement([["crates/rbit-services/src/repositories.rs", source]])[0], /belong in rbit_access::grants/u, source);
  }
  const compat = "pub type Grants = RepositoryGrants<Permission>;\npub fn repo_access(viewer: Option<&User>) -> RepoAccess { todo!() }\n";
  assert.deepEqual(checkAccessPolicyRetirement([["crates/rbit-core/src/access.rs", compat]]), []);
  assert.match(checkAccessPolicyRetirement([["crates/rbit-core/src/access.rs", "fn grants_apply() {}"]])[0], /projects records only/u);
  assert.deepEqual(checkAccessPolicyRetirement([["crates/rbit-access/src/grants.rs", "pub fn grants_apply() {}\npub struct RepositoryGrants<P> {}"]]), []);
});

test("fact types are built only by the record adapters", () => {
  const facts = "let viewer = ViewerAccessFacts { site_admin: false, ..ViewerAccessFacts::default() };";
  assert.match(checkAccessPolicyRetirement([["crates/rbit-services/src/lfs.rs", facts]])[0], /build access facts/u);
  assert.deepEqual(checkAccessPolicyRetirement([["crates/rbit-storage/src/sqlite/wiki_mutations/authorization.rs", facts]]), []);
});

test("SQL readability is written once in readable_by!", () => {
  const sql = `"WHERE (r.visibility = 'public' OR r.owner_id = ?1)"`;
  assert.match(checkAccessPolicyRetirement([["crates/rbit-storage/src/sqlite/stars.rs", sql]])[0], /readable_by!/u);
  assert.deepEqual(checkAccessPolicyRetirement([["crates/rbit-storage/src/sqlite/repositories.rs", sql]]), []);
  const counts = `"SELECT COUNT(*) FROM repositories WHERE visibility = 'public'"`;
  assert.deepEqual(checkAccessPolicyRetirement([["crates/rbit-storage/src/sqlite/admin.rs", counts]]), [], "counting is not a readability filter");
});

test("the current tree has no retired access path", () => {
  const { files } = authoredSources(fileURLToPath(new URL("../../", import.meta.url)));
  assert.deepEqual(checkAccessPolicyRetirement(files), []);
});
