import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkOperationsUpdatesRetirement } from "../rewrite/operations-updates-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkOperationsUpdatesRetirement([[path, source]]);

test("a second release checker, feed or decision fails outside the server checker", () => {
  const cases = [
    ["crates/rbit-services/src/updates.rs", "const SOURCE: &str = \"https://api.github.com/repos/GitRbit/rbit/releases/latest\";\n", /release feed/u],
    ["crates/rbit-web/src/x.rs", "Client::builder().user_agent(\"rbit-update-check\")\n", /release feed/u],
    ["crates/rbit-services/src/lib.rs", "pub updates: updates::UpdateService,\n", /retired services update checker/u],
    ["crates/rbit-web/src/views/admin.rs", "fn newer_release(release: Release, running: &str) {}\n", /release decision/u],
    ["crates/rbit-services/src/admin.rs", "candidate.cmp_precedence(&current).is_gt()\n", /release decision/u],
    ["crates/rbit-services/src/updates.rs", "use reqwest::Client;\n", /performs no checks/u],
    ["crates/rbit-services/src/updates.rs", "self.tasks.spawn(async move {});\n", /performs no checks/u],
    ["crates/rbit-services/src/lifecycle.rs", "self.updates.spawn_background();\n", /composed by the server/u],
    ["crates/rbit-server/src/serve.rs", "services.updates.shutdown().await;\n", /composed by the server/u],
  ];
  for (const [path, source, message] of cases) assert.match(check(path, source)[0] ?? "", message, source);
});

test("the server checker, release links, unrelated helpers and comments are allowed", () => {
  assert.deepEqual(checkOperationsUpdatesRetirement([
    ["crates/rbit-server/src/updates/github.rs", "const SOURCE: &str = \"https://api.github.com/repos/GitRbit/rbit/releases/latest\";\nconst USER_AGENT: &str = \"rbit-update-check\";\n"],
    ["crates/rbit-server/src/updates/release.rs", "fn newer_release() { candidate.cmp_precedence(&current); }\n"],
    ["crates/rbit-web/src/views/layout.rs", "url: \"https://github.com/GitRbit/rbit/releases/tag/v1.2.3\".into(),\n"],
    ["crates/rbit-services/src/webhooks/delivery.rs", "pub fn retry_delay(failed_attempts: u32, random: f64) -> Duration {}\n"],
    ["crates/rbit-services/src/updates.rs", "// The UpdateService and its reqwest client were retired.\n/* tokio::spawn( */\n"],
    ["crates/rbit-services/src/recovery.rs", "self.recovery.shutdown().await;\n"],
  ]), []);
});

test("the current tree has no retired update checker", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkOperationsUpdatesRetirement(files), []);
  assert.match(readFileSync(new URL("crates/rbit-server/src/updates/github.rs", root), "utf8"), /impl ReleaseSource for GithubReleases/u);
});
