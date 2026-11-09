import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkOidcRetirement } from "../rewrite/oidc-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy OIDC definitions are refused outside identity", () => {
  // Excerpts of the retired definitions (crates/rbit-auth/src/oidc* before this slice).
  const legacy = [
    ["crates/rbit-auth/src/oidc/flow.rs", "pub struct FlowKey([u8; 32]);\npub struct Pkce(String);\n"],
    ["crates/rbit-auth/src/oidc/tokens.rs", "pub struct JwkSet {\n    keys: Vec<Key>,\n}\npub fn verify_id_token(token: &str) {}\n"],
    ["crates/rbit-services/src/external_auth.rs", "enum OidcError { Malformed }\n"],
  ];
  const violations = checkOidcRetirement(legacy);
  assert.equal(violations.length, 3, violations.join("\n"));
  assert.ok(checkIdentityRetirement(legacy).length >= violations.length, "the identity check runs these rules");
});

test("identity owns the definitions and other crates may use them", () => {
  assert.deepEqual(checkOidcRetirement([
    ["crates/rbit-identity/src/oidc/flow.rs", "pub struct FlowKey([u8; 32]);"],
    ["crates/rbit-identity/src/oidc.rs", "pub use self::tokens::verify_id_token;"],
    ["crates/rbit-auth/src/oidc.rs", "pub use rbit_identity::oidc::{FlowKey, JwkSet, verify_id_token};"],
    ["crates/rbit-services/src/external_auth.rs", "let verified = oidc::verify_id_token(id_token, &keys, &check);"],
  ]), []);
});

test("the current tree has no retired OIDC definition", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkOidcRetirement(files), []);
});
