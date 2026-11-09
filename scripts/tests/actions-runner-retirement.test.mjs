import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkActionsRunnerRetirement } from "../rewrite/actions-runner-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkActionsRunnerRetirement([[path, source]]);

test("a second copy of a runner-contract decision fails on either side", () => {
  const cases = [
    ["crates/rbit-services/src/actions/caches.rs", "pub const MAX_CACHE_KEY_CHARS: usize = 512;\n", /key limit/u],
    ["crates/rbit-runner/src/steps/cache.rs", "pub const MAX_KEY_CHARS: usize = 512;\n", /key limit/u],
    ["crates/rbit-services/src/actions/caches.rs", "const MAX_RESTORE_KEYS: usize = 10;\n", /key limit/u],
    ["crates/rbit-services/src/actions/caches.rs", "fn valid_version(version: &str) -> bool { true }\n", /contract rule/u],
    ["crates/rbit-web/src/routes/api/actions.rs", "pub fn scope_ref(scope: &str) -> String { todo!() }\n", /contract rule/u],
    ["crates/rbit-services/src/actions/artifacts.rs", "pub(super) fn digest_from_hex(hex: &str) -> Option<[u8; 32]> { None }\n", /contract rule/u],
    ["crates/rbit-services/src/actions/caches.rs", "for prefix in lookup.restore_keys {\n}\n", /restore order/u],
    ["crates/rbit-runner/src/steps/cache.rs", "for prefix in restore_keys {\n}\n", /restore order/u],
    ["crates/rbit-services/src/actions/scheduler.rs", "CacheScopes { repository_id: run.repo_id.0, read, write }\n", /cache scopes built/u],
    ["crates/rbit-services/src/actions/downloads.rs", "const STORED_ROOT: &str = \"w\";\n", /archive directory/u],
    ["crates/rbit-runner/src/config.rs", "self.labels.iter().filter(|label| label.backend == Backend::Host)\n", /host-label/u],
    ["crates/rbit-services/src/actions/runners.rs", "labels.iter().filter(|(label, _)| label.backend == Backend::Host)\n", /host-label/u],
    ["crates/rbit-services/src/actions/x.rs", "args.push(\"--privileged\");\n", /isolation arguments/u],
    ["crates/rbit-web/src/x.rs", "let socket = \"/var/run/docker.sock\";\n", /isolation arguments/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the contract, the runner's own backend, tests and comments are allowed", () => {
  assert.deepEqual(checkActionsRunnerRetirement([
    ["crates/rbit-workflow/src/protocol/caches.rs", "pub const MAX_CACHE_KEY_CHARS: usize = 512;\npub fn scope_ref(s: &str) -> String { todo!() }\n"],
    ["crates/rbit-workflow/src/spec/cache_scopes.rs", "Self { repository_id: run.repository_id, read, write }\n"],
    ["crates/rbit-runner/src/backend/container.rs", "args.extend([\"--security-opt\".into(), \"no-new-privileges\".into(), \"--cap-drop\".into()]);\n"],
    ["crates/rbit-runner/src/exec.rs", "cache: CacheScopes { repository_id: 0, read: vec![scope.clone()], write: scope },\n"],
    ["crates/rbit-services/src/actions/scheduler.rs", "pub fn cache_scopes(run: &Run, default_branch: &str) -> CacheScopes {\n    CacheScopes::for_run(&scope)\n}\n"],
    ["crates/rbit-services/src/actions/runs.rs", "(RunTrust::Untrusted { .. }, None) => format!(\"untrusted/{group}\"),\n"],
    ["crates/rbit-services/src/actions/caches.rs", "// fn valid_key was retired\n/* for prefix in restore_keys {} */\n"],
    ["crates/rbit-services/src/actions/caches.rs", "fn live() {}\n#[cfg(test)]\nmod tests {\n    fn valid_key() {}\n}\n"],
    ["crates/rbit-web/tests/actions_runner_tasks.rs", "let scopes = CacheScopes { repository_id: 1, read: vec![], write: String::new() };\n"],
  ]), []);
});

test("the current tree has no retired runner-contract copy", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkActionsRunnerRetirement(files), []);
});
