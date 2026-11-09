import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryLfsRetirement as check } from "../rewrite/repository-lfs-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("LFS limits, refusal texts and per-object decisions stay in the application", () => {
  for (const source of [
    "pub const MAX_BATCH_OBJECTS: usize = 1000;\n",
    "pub const MAX_LOCKS_PER_PAGE: u32 = 100;\n",
    'Some(format!("the upload is larger than the announced {size} bytes"))\n',
    'ServiceError::Invalid("the cursor is not valid".to_owned())\n',
    "(LfsOperation::Download, Some(stored)) => BatchOutcome::Download(token),\n",
  ]) {
    assert.ok(check([["crates/rbit-services/src/lfs.rs", source]]).length > 0, source);
    assert.deepEqual(check([["crates/rbit-repositories/src/lfs/batch.rs", source]]), []);
  }
  assert.deepEqual(check([["crates/rbit-services/src/lfs.rs", "pub use rbit_repositories::lfs::MAX_BATCH_OBJECTS;\n"]]), []);
  assert.deepEqual(check([["crates/rbit-web/tests/lfs.rs", 'assert!(body.contains("the cursor is not valid"));\n']]), []);
});

test("the current tree has no retired LFS decision", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(check(files), []);
});
