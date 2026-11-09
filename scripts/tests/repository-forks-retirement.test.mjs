import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryForksRetirement as check } from "../rewrite/repository-forks-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("fork store reads outside storage and the service fork module fail, tests included", () => {
  const flag = "let fork = ctx.database.forks().forks_among(&[repo]).await?.contains(&repo);\n";
  assert.match(check([["crates/rbit-services/src/actions/hooks.rs", flag]])[0], /bypasses rbit_repositories::forks/u);
  assert.match(check([["crates/rbit-web/tests/x.rs", flag]])[0], /bypasses/u);
  assert.match(check([["crates/rbit-services/src/x.rs", "use rbit_storage::sqlite::forks::ForkStore;\n"]])[0], /bypasses/u);
  assert.deepEqual(check([["crates/rbit-services/src/forks.rs", flag]]), []);
  assert.deepEqual(check([["crates/rbit-services/src/forks/effects.rs", flag]]), []);
  assert.deepEqual(check([["crates/rbit-storage/tests/pulls.rs", flag]]), []);
});

test("the fork network is read only through ForkQueries", () => {
  const network = "store.network_readable(id, viewer, admin).await?\n";
  assert.match(check([["crates/rbit-services/src/lfs.rs", network]])[0], /ForkQueries::network/u);
  assert.match(check([["crates/rbit-services/src/forks.rs", network]])[0], /ForkQueries::network/u);
  assert.deepEqual(check([["crates/rbit-repositories/src/forks/port.rs", network]]), []);
  assert.deepEqual(check([["crates/rbit-storage/src/repository_forks.rs", network]]), []);
});

test("the native copy stays in the fork effects", () => {
  const copy = "fork_repository(&git, &source, &target).await?;\n";
  assert.match(check([["crates/rbit-services/src/imports.rs", copy]])[0], /native fork copy/u);
  assert.deepEqual(check([["crates/rbit-services/src/forks/effects.rs", copy]]), []);
  assert.deepEqual(check([["crates/rbit-git/src/fork.rs", copy]]), []);
});

test("comments and non-Rust files are allowed", () => {
  assert.deepEqual(check([
    ["crates/rbit-services/src/a.rs", "// database.forks() was retired\n/* network_readable */\n"],
    ["docs/x.md", "database.forks()"],
  ]), []);
});

test("the current tree has no fork-store bypass", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
});
