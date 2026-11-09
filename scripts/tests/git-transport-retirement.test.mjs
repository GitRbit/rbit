import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkGitTransportRetirement as check } from "../rewrite/git-transport-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const HTTP = "crates/rbit-web/src/routes/git_http.rs";
const SSH = "crates/rbit-ssh/src/session.rs";

test("protocol framing and refused-push answers stay in rbit-transport", () => {
  const packet = "let payload = format!(\"ERR {message}\\n\");\nlet packet = format!(\"{:04x}{payload}\", payload.len() + 4);\n";
  assert.match(check([[HTTP, packet]])[0], /pkt_line::error/u);
  assert.match(check([["crates/rbit-ssh/src/session/channel_reply.rs", "encode(&format!(\"ERR {message}\\n\"))"]])[0], /pkt_line::error/u);
  const report = "let report = receive::refusal(&commands, &refusals);\nreceive::refusal_message(&refusals)\n";
  assert.match(check([["crates/rbit-ssh/src/session/git_exchange.rs", report]])[0], /refused_push/u);
  assert.match(check([[HTTP, "headers.get(\"git-protocol\")"]])[0], /smart_http/u);
  assert.deepEqual(check([["crates/rbit-transport/src/receive.rs", packet + report]]), []);
});

test("smart HTTP credentials, challenges and access order are decided by the transport", () => {
  for (const source of [
    "let decoded = STANDARD.decode(encoded.trim()).ok()?;\n",
    "value.strip_prefix(\"Basic \")\n",
    "[(header::WWW_AUTHENTICATE, \"Basic realm=\\\"rbit\\\", charset=\\\"UTF-8\\\"\")]\n",
    "(scheme.eq_ignore_ascii_case(\"bearer\") || x)\n",
  ]) {
    assert.match(check([["crates/rbit-web/src/routes/git_http/denied.rs", source]])[0], /credentials/u, source);
  }
  assert.match(check([[HTTP, "v.as_bytes().eq_ignore_ascii_case(b\"gzip\")"]])[0], /smart_http/u);
  assert.match(check([["crates/rbit-web/src/routes/git_http/advertisement.rs", "(header::CACHE_CONTROL, \"no-cache\".to_owned())"]])[0], /smart_http/u);
  for (const source of ["state.services.mirrors.is_mirror(id).await", "services.repositories.open_following_renames(u, o, r)", "ctx.access = scope.limit(ctx.access);"]) {
    assert.match(check([[HTTP, source]])[0], /smart_http::access/u, source);
    assert.deepEqual(check([["crates/rbit-web/src/routes/git_http/repositories.rs", source]]), [], "the gate adapter opens repositories");
  }
  assert.deepEqual(check([["crates/rbit-web/src/routes/lfs.rs", "state.services.mirrors.is_mirror(id).await"]]), [], "LFS is another row");
});

test("SSH per-command authorization stays in rbit_ssh::access", () => {
  for (const source of [
    "services.accounts.is_suspended(user.id()).await",
    "services.ssh_keys.open_for_deploy_key(identity, o, r).await",
    "services.mirrors.is_mirror(id).await",
    "fn same_key_holder(a: &SshIdentity, b: &SshIdentity) -> bool { true }",
    "format!(\"this deploy key is read-only; it cannot push to {name}\")",
    "format!(\"repository {full_name} not found, or you do not have access to it\")",
  ]) {
    assert.match(check([[SSH, source]])[0], /rbit_ssh::access/u, source);
    assert.deepEqual(check([["crates/rbit-ssh/src/access.rs", source]]), []);
  }
});

test("comments, inline and integration tests, and non-Rust files are allowed", () => {
  assert.deepEqual(check([
    [HTTP, "// receive::refusal(&c, &r) was moved\nfn f() {}\n#[cfg(test)]\nmod tests {\n    const X: &str = \"no-cache\";\n}\n"],
    ["crates/rbit-ssh/src/access/tests.rs", "is_mirror(&self) \"deploy key is read-only\""],
    ["crates/rbit-e2e/tests/e2e/protection.rs", "format!(\"{:04x}{line}\", line.len() + 4)"],
    ["docs/x.md", "receive::refusal("],
  ]), []);
});

test("the current tree has no retired Git transport path, and the baseline had them", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  const baseline = (path) => [path, execFileSync("git", ["show", `ee9f958c:${path}`], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] })];
  let legacy;
  try {
    legacy = [HTTP, SSH, "crates/rbit-ssh/src/session/git_exchange.rs", "crates/rbit-ssh/src/session/channel_reply.rs"].map(baseline);
  } catch {
    return; // Shallow checkouts may not contain the baseline commit.
  }
  assert.ok(check(legacy).length >= 6, check(legacy).join("\n"));
});
