import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsForkPolicyRetirement } from "../rewrite/actions-fork-policy-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsForkPolicyRetirement([[path, source]]);

test("a second trust decision, grant check or isolation scope fails outside rbit-actions", () => {
  const cases = [
    ["crates/rbit-core/src/actions.rs", "pub enum RunTrust { Trusted }\n", /type redefined/u],
    ["crates/rbit-core/src/actions.rs", "pub fn classify_pull_request(a: A, b: B) -> RunTrust { todo!() }\n", /decided outside/u],
    ["crates/rbit-services/src/actions/trust.rs", "async fn author_standing(&self) -> AuthorStanding { AuthorStanding::Vouched }\n", /decided outside/u],
    ["crates/rbit-web/src/x.rs", "let actor = TriggeringActor::Known { access, suspended, is_author };\n", /decision inputs/u],
    ["crates/rbit-services/src/actions/runs.rs", "(RunTrust::Untrusted { .. }, Some(n)) => format!(\"pr-{n}/{group}\"),\n", /isolation scope/u],
    ["crates/rbit-services/src/actions/scheduler.rs", "(false, None) => format!(\"untrusted/{}\", run.id.0),\n", /isolation scope/u],
    ["crates/rbit-services/src/actions/secrets.rs", "if !run.trusted { return Ok(BTreeMap::new()); }\n", /read directly/u],
    ["crates/rbit-server/src/x.rs", "let write = match (run.trusted, run.pull_number) {};\n", /read directly/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source).join("\n"), message, source);
});

test("the owner, the adapter, presentation and comments are allowed", () => {
  assert.deepEqual(checkActionsForkPolicyRetirement([
    ["crates/rbit-actions/src/trust.rs", "pub enum RunTrust { Trusted }\npub fn classify_pull_request() {}\n"],
    ["crates/rbit-services/src/actions/trust.rs", "pub async fn classify_pull_request(&self) { PullRequestTrust::classify(&facts, a, b, f).await }\n"],
    ["crates/rbit-services/src/actions/secrets.rs", "if !Grants::of_stored(run.trusted).secrets_and_variables {}\n"],
    ["crates/rbit-services/src/actions/tokens.rs", "let trust = rbit_actions::trust::RunTrust::from_stored(run.trusted);\n"],
    ["crates/rbit-web/src/views/actions/run.rs", "untrusted: !run.trusted,\n"],
    ["crates/rbit-services/src/actions/caches.rs", "Some(number) => format!(\"pull/{number}\"),\n"],
    ["crates/rbit-services/tests/actions_pulls.rs", "if !run.trusted { RunTrust::Untrusted { awaits_approval: true } }\n"],
    ["crates/rbit-services/src/actions/runs.rs", "// if run.trusted was read directly\n"],
  ]), []);
});

test("the current tree has no retired fork-policy path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsForkPolicyRetirement(files), []);
});
