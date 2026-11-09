import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkExternalSignInRetirement as check } from "../rewrite/external-sign-in-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const SERVICE = "crates/rbit-services/src/external_auth.rs";
const OWNER = "crates/rbit-identity/src/external/completion.rs";

test("decision types and helpers are defined only by rbit_identity::external", () => {
  for (const source of ["pub struct VerifiedIdentity {}", "pub struct CallbackParams<'a> {}", "fn suggest_username() {}", "fn may_register() {}", "pub(super) enum Completion<A> {}"]) {
    assert.match(check([[SERVICE, source]])[0], /defined only by rbit_identity::external/u, source);
    assert.match(check([["crates/rbit-services/src/external_auth/flows.rs", source]])[0], /defined only/u, source);
    assert.deepEqual(check([[OWNER, source]]), []);
  }
  assert.deepEqual(check([["crates/rbit-core/src/config/auth.rs", "pub enum ProviderRegistration {}"]]), [], "configuration keeps its own type");
});

test("refusal and audit text belongs to identity", () => {
  for (const text of [
    "format!(\"this {} account is already linked to another account here\", name)",
    "format!(\"{} couldn't confirm who you are; try again\", name)",
    "\"the sign-in took too long or was started in another browser; try again\"",
    "format!(\"{} (provider MFA)\", name)",
    "format!(\"{}: account suspended\", config.name)",
  ]) {
    assert.match(check([["crates/rbit-services/src/external_auth/links.rs", text]])[0], /text belongs/u, text);
    assert.deepEqual(check([[OWNER, text]]), []);
  }
  assert.deepEqual(check([["crates/rbit-services/src/two_factor/password_sign_in.rs", "ServiceError::Forbidden => \"account suspended\","]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/external_auth.rs", "\"couldn't confirm who you are\""]]), [], "suites assert it");
});

test("the service does not re-check callbacks or completion", () => {
  for (const source of ["if !params.state.is_some_and(|state| flow.state_matches(state)) {}", "let trusted = config.trust_mfa && identity.mfa;", "identity.email_verified && identity.email.is_some()",
    ".append_pair(\"code_challenge_method\", Pkce::METHOD)", "mfa: token.amr.iter().any(|m| m == \"mfa\")", "info.get(\"email_verified\")",
    "const MAX_PENDING_REGISTRATIONS: usize = 10_000;", "fn lock_pending(&self) {}"]) {
    assert.match(check([[SERVICE, source]])[0], /decided by rbit_identity::external/u, source);
  }
  assert.deepEqual(check([[SERVICE, "// state_matches( moved\n#[cfg(test)]\nmod tests { fn suggest_username() {} }\n"]]), []);
});

test("the current tree is clean and the baseline was not", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  const at = (path) => [path, execFileSync("git", ["show", `08dafe3a:${path}`], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] })];
  let legacy;
  try {
    legacy = [SERVICE, "crates/rbit-services/src/external_auth/flows.rs", "crates/rbit-services/src/external_auth/original.rs"].map(at);
  } catch {
    return; // Shallow checkouts may not contain the baseline commit.
  }
  assert.ok(check(legacy).length >= 6, check(legacy).join("\n"));
});
