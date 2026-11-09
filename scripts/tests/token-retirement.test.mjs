import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkTokenRetirement } from "../rewrite/token-retirement.mjs";
import { checkIdentityRetirement } from "../rewrite/identity-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("the legacy token service implementation is refused", () => {
  // Excerpts of the retired implementation (crates/rbit-services/src/tokens.rs before this slice).
  const legacy = [
    "use rbit_auth::AccessTokenSecret;",
    "const MAX_TOKENS_PER_USER: i64 = 50;",
    "pub const EXPIRY_CHOICES: &'static [Option<u32>] = &[Some(7), Some(30), Some(90), Some(365), None];",
    "let token = self.store.create_with_limit(new, MAX_TOKENS_PER_USER).await?;",
    "let Some((token, user)) = self.store.find_by_digest(&secret.digest()).await? else { return Ok(None) };",
  ].join("\n");
  const violations = checkTokenRetirement([["crates/rbit-services/src/tokens.rs", legacy]]);
  for (const rule of [/secrets only through/u, /limits and expiry choices/u, /direct access token store call/u]) {
    assert.ok(violations.some((violation) => rule.test(violation)), `${rule}: ${violations.join("\n")}`);
  }
  assert.deepEqual(checkIdentityRetirement([["crates/rbit-services/src/tokens.rs", legacy]]), violations, "the identity check runs these rules");
});

test("the secret format may only be defined by the identity token module", () => {
  const definition = "pub struct AccessTokenSecret([u8; 32]);\n";
  assert.match(checkTokenRetirement([["crates/rbit-auth/src/access_token.rs", definition]])[0], /defined only by rbit_identity::token/u);
  assert.match(checkTokenRetirement([["crates/rbit-web/tests/x.rs", definition]])[0], /defined only/u);
  assert.deepEqual(checkTokenRetirement([["crates/rbit-identity/src/token/access_token.rs", definition]]), []);
  assert.deepEqual(checkTokenRetirement([["crates/rbit-auth/src/access_token.rs", "pub use rbit_identity::token::AccessTokenSecret;\n"]]), []);
});

test("direct store calls are refused outside identity and its storage adapter", () => {
  const call = "async fn f(s: AccessTokenStore) { s.find_by_digest(&d).await; }\n";
  assert.match(checkTokenRetirement([["crates/rbit-web/src/x.rs", call]])[0], /bypasses the identity token application/u);
  assert.match(checkTokenRetirement([["crates/rbit-server/src/x.rs", "AccessTokenStore::touch(&s, id, now)"]])[0], /bypasses/u);
  assert.deepEqual(checkTokenRetirement([["crates/rbit-storage/src/identity_access_tokens.rs", call]]), []);
  assert.deepEqual(checkTokenRetirement([["crates/rbit-storage/tests/access_tokens.rs", call]]), [], "store tests exercise the store");
  assert.deepEqual(checkTokenRetirement([["crates/rbit-services/tests/tokens.rs", "AccessTokenSecret::generate()"]]), []);
});

test("delegating constants and comments are allowed", () => {
  assert.deepEqual(checkTokenRetirement([
    ["crates/rbit-services/src/tokens.rs", "pub const EXPIRY_CHOICES: &'static [Option<u32>] = access_tokens::EXPIRY_CHOICES;\n"],
    ["crates/rbit-services/src/a.rs", "// const MAX_TOKENS_PER_USER: i64 = 50; store.find_by_digest(&d)\n"],
    ["docs/x.md", "pub struct AccessTokenSecret"],
  ]), []);
  assert.match(checkTokenRetirement([["crates/rbit-web/src/a.rs", "const MAX_TOKENS_PER_USER: i64 = 50;\n"]])[0], /owned by/u);
});

test("the current tree has no retired token path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(checkTokenRetirement(files), []);
});
