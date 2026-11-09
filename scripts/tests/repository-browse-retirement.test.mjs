import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryBrowseRetirement as check } from "../rewrite/repository-browse-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("browse limits and helpers stay out of the web routes", () => {
  for (const source of [
    "const COMMITS_PER_PAGE: usize = 30;\n",
    "const MAX_RAW_BYTES: usize = 100 * 1024 * 1024;\n",
    "fn parent_path(path: &str) -> &str { path }\n",
    "fn image_content_type(path: &str) -> Option<&'static str> { None }\n",
    ".filter(|r| combined.strip_prefix(r.name.as_str()).is_some())\n",
  ]) {
    assert.match(check([["crates/rbit-web/src/routes/repo.rs", source]])[0], /browse decision returned/u, source);
    assert.match(check([["crates/rbit-web/src/routes/repo/files.rs", source]])[0], /browse decision/u, source);
  }
  assert.deepEqual(check([["crates/rbit-repositories/src/browse.rs", "const COMMITS_PER_PAGE: usize = 30;\n"]]), []);
  assert.deepEqual(check([["crates/rbit-web/src/routes/repo.rs", "use rbit_services::repositories::browse::{self, COMMITS_PER_PAGE};\n"]]), []);
});

test("the current tree has no retired browse decision", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(check(files), []);
});
