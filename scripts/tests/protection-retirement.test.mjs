import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkProtectionRetirement } from "../rewrite/protection-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const CORE = "crates/rbit-core/src/protection.rs";
const ADAPTER = "crates/rbit-services/src/protection.rs";

test("distinctive rule and decision items are defined only in rbit-access", () => {
  for (const source of [
    "pub struct BranchPattern(String);\n",
    "pub struct RuleSettings { pub deny_force_push: bool }\n",
    "pub enum RuleChange { Saved(ProtectionId) }\n",
    "pub struct PushVerdict { pub refusals: Vec<Refusal> }\n",
    "pub fn decide_push(rules: &[ProtectionRule]) -> PushDecision { todo!() }\n",
  ]) {
    assert.match(checkProtectionRetirement([["crates/rbit-web/src/x.rs", source]])[0], /owned by rbit-access/u, source);
    assert.deepEqual(checkProtectionRetirement([["crates/rbit-access/src/protection/rules.rs", source]]), []);
  }
});

test("generic names are checked in the legacy protection files only", () => {
  for (const source of ["pub struct Refusal { pub name: String }\n", "pub enum Pusher { DeployKey }\n", "pub mod reasons {}\n", "fn is_protected() {}\n"]) {
    assert.match(checkProtectionRetirement([[CORE, source]])[0], /owned by rbit-access/u, source);
    assert.deepEqual(checkProtectionRetirement([["crates/rbit-web/src/routes/actions_runner.rs", source]]), []);
  }
  const aliases = "pub type MergeRules = rules::MergeRules<StatusContext>;\npub type RuleInput = rbit_access::protection::RuleInput<A, T, C>;\n";
  assert.deepEqual(checkProtectionRetirement([[CORE, aliases]]), []);
});

test("the service adapter composes the applications without deciding", () => {
  for (const source of [
    "fn require_admin(repo: &RepoContext) {}",
    "fn required_contexts(contexts: &[String]) {}",
    "match change { RuleChange::TooManyRules => {} }",
    "let decision = protection::decide_push(&rules, &pusher, updates);",
    "Ok(may_delete(&rules, &pusher, branch))",
  ]) {
    assert.match(checkProtectionRetirement([[ADAPTER, source]])[0], /decision belongs in rbit-access/u, source);
  }
  const composed = "// fn require_admin was retired\nOk(ProtectionGate::new(&store).may_delete(repo, user, access, branch).await?)";
  assert.deepEqual(checkProtectionRetirement([[ADAPTER, composed]]), []);
});

test("the current tree has no retired protection path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkProtectionRetirement(files), []);
  assert.match(new Map(files).get(ADAPTER), /RuleManagement::new/u);
});
