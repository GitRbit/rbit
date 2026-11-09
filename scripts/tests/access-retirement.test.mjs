import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkAccessRetirement } from "../rewrite/access-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const ORGS = "crates/rbit-services/src/orgs.rs";
const TEAMS = "crates/rbit-services/src/teams.rs";

test("organisation and team rule types are defined only in rbit-access", () => {
  for (const source of [
    "pub enum OrgRole { Member, Owner }\n",
    "pub enum OrgCreationPolicy { AnyUser }\n",
    "pub enum MembershipChange { Done }\n",
    "pub struct NewOrganization { pub name: AccountName }\n",
    "pub struct TeamInput { pub name: String }\n",
    "pub(crate) enum TeamChange { Done }\n",
  ]) {
    assert.match(checkAccessRetirement([["crates/rbit-core/src/org.rs", source]])[0], /owned by rbit-access/u, source);
    assert.deepEqual(checkAccessRetirement([["crates/rbit-access/src/organizations/model.rs", source]]), []);
  }
  const alias = "pub type NewOrganization = rbit_access::organizations::NewOrganization<AccountName, AccountId>;\n";
  assert.deepEqual(checkAccessRetirement([["crates/rbit-storage/src/sqlite/orgs.rs", alias]]), []);
  assert.deepEqual(checkAccessRetirement([["crates/rbit-core/src/org.rs", "pub use rbit_access::OrgRole;\n"]]), []);
});

test("production store writes bypassing the rules fail outside storage", () => {
  for (const call of [
    "self.ctx.database.orgs().add_member(org, user, role).await?;",
    "database.orgs().set_base_permission(org, permission).await?;",
    "self.ctx.database.teams()\n    .set_repository(team, repo, permission).await?;",
    "database.orgs().delete_if_empty(org).await?;",
  ]) {
    assert.match(checkAccessRetirement([["crates/rbit-services/src/webhooks.rs", call]])[0], /bypasses the rbit-access rules/u, call);
    assert.deepEqual(checkAccessRetirement([["crates/rbit-storage/src/access_organizations.rs", call]]), []);
    assert.deepEqual(checkAccessRetirement([["crates/rbit-services/tests/access.rs", call]]), [], "fixtures seed stores directly");
  }
  const fixture = "fn live() {}\n#[cfg(test)]\nmod tests {\n    fn seed() { database.orgs().create(new_org); }\n}\n";
  assert.deepEqual(checkAccessRetirement([["crates/rbit-web/src/routes/home.rs", fixture]]), [], "unit-test modules may seed");
  const reads = "database.orgs().role_of(org, user).await?; database.teams().list(org).await?;";
  assert.deepEqual(checkAccessRetirement([["crates/rbit-services/src/protection.rs", reads]]), []);
});

test("the service adapters hold no decisions and no raw stores", () => {
  for (const [path, source] of [
    [ORGS, "async fn require_manager(&self) {}"],
    [ORGS, "fn website(value: &str) {}"],
    [ORGS, "match change { MembershipChange::LastOwner => {} }"],
    [ORGS, "match policy { OrgCreationPolicy::AdminsOnly => actor.is_admin }"],
    [TEAMS, "fn validate(input: &TeamInput) {}"],
    [TEAMS, "Ok(TeamChange::OutsideOrganization) => {}"],
  ]) {
    assert.match(checkAccessRetirement([[path, source]])[0], /decision belongs in rbit-access/u, source);
  }
  for (const source of ["fn store(&self) -> TeamStore { todo!() }", "self.ctx.database.orgs().role_of(org, user)", "self.ctx.database.accounts()"]) {
    assert.match(checkAccessRetirement([[TEAMS, source]])[0], /must use the access store adapters/u, source);
  }
  assert.deepEqual(checkAccessRetirement([[ORGS, "// fn require_manager was retired\nfn rules(&self) {}\n"]]), []);
});

test("the current tree has no retired organisation or team path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkAccessRetirement(files), []);
  const adapters = new Map(files.filter(([path]) => [ORGS, TEAMS].includes(path)));
  assert.match(adapters.get(ORGS), /Organizations::new/u);
  assert.match(adapters.get(TEAMS), /Teams::new/u);
});
