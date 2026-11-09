import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryStarsRetirement as check } from "../rewrite/repository-stars-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("star store types and handles stay behind the application", () => {
  assert.match(check([["crates/rbit-services/src/stars.rs", "use rbit_storage::sqlite::stars::StarStore;\n"]])[0], /store type outside storage/u);
  assert.match(check([["crates/rbit-web/src/routes/stars.rs", "pub use rbit_storage::sqlite::stars::Stargazer;\n"]])[0], /outside storage/u);
  const open = "state.database.stars().add(user, repo).await?;\n";
  assert.match(check([["crates/rbit-services/src/repositories.rs", open]])[0], /opened outside the star service/u);
  assert.deepEqual(check([["crates/rbit-services/src/stars.rs", "RepositoryStars::new(&self.ctx.database.stars())\n"]]), []);
  assert.deepEqual(check([["crates/rbit-storage/src/repository_stars.rs", "impl StarRepository for StarStore {}\n"]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/access.rs", open]]), [], "fixtures seed stars directly");
});

test("comments and non-Rust files are allowed", () => {
  assert.deepEqual(check([["crates/rbit-web/src/a.rs", "// StarStore and .stars() were retired\n"], ["docs/x.md", "StarStore"]]), []);
});

test("the current tree has no retired star path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.deepEqual(check(files), []);
});
