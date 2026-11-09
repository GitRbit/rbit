import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkImportsRetirement } from "../rewrite/imports-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);
const check = (path, source) => checkImportsRetirement([[path, source]]);

test("moved admission and lifecycle policy cannot be redefined beside its adapters", () => {
  for (const source of [
    "const MAX_TOKEN_BYTES: usize = 1024;\n",
    "pub const STOPPED: &str = \"stopped\";\n",
    "return Err(ServiceError::Invalid(\"the token is not valid\".to_owned()));\n",
    "let d = truncate_chars(d.trim(), 350);\n",
  ]) {
    assert.match(check("crates/rbit-services/src/imports.rs", source)[0], /belongs to rbit-repositories/u, source);
    assert.match(check("crates/rbit-services/src/imports/ports.rs", source)[0], /belongs to rbit-repositories/u, source);
  }
  assert.deepEqual(check("crates/rbit-services/src/imports/job.rs", "let t = truncate_chars(title, 100);\n"), [], "the copy keeps its own limits");
  assert.deepEqual(check("crates/rbit-repositories/src/imports.rs", "pub const STOPPED: &str = \"x\";\n"), []);
  assert.deepEqual(check("crates/rbit-services/src/imports.rs", "// const MAX_TOKEN_BYTES was moved\n"), [], "comments");
});

test("the current tree has no retired import path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(checkImportsRetirement(files), []);
});
