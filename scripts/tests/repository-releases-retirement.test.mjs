import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkRepositoryReleasesRetirement as check } from "../rewrite/repository-releases-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const root = new URL("../../", import.meta.url);

test("release and archive records and rules are defined by the applications only", () => {
  for (const source of [
    "pub struct ReleaseAsset { id: i64 }\n",
    "pub enum ReleaseSource { User }\n",
    "pub struct NewRelease<'a> { tag: &'a str }\n",
    "pub fn validate_asset_name(name: &str) -> Result<(), String> { Ok(()) }\n",
    "pub fn human_bytes(bytes: u64) -> String { String::new() }\n",
    "pub fn archive_base_name(repo: &str, revision: &str) -> String { String::new() }\n",
    "pub enum ArchiveError { Busy }\n",
    "pub const MAX_CONCURRENT_ARCHIVES: usize = 4;\n",
  ]) {
    assert.match(check([["crates/rbit-core/src/release.rs", source]])[0], /defined in rbit_repositories/u, source);
    assert.match(check([["crates/rbit-web/tests/x.rs", source]])[0], /defined in/u, "tests are covered too");
    assert.deepEqual(check([["crates/rbit-repositories/src/releases/model.rs", source]]), []);
  }
  assert.deepEqual(check([["crates/rbit-services/src/imports/api.rs", "pub struct Release { tag_name: String }\n"]]), []);
  assert.deepEqual(check([["crates/rbit-services/src/archives.rs", "pub type ArchiveError = archives::ArchiveError<ServiceError>;\n"]]), []);
});

test("release policy text stays in the application", () => {
  const text = "return Err(ServiceError::Invalid(\"choose the tag this release is for\".to_owned()));\n";
  assert.match(check([["crates/rbit-web/src/routes/releases.rs", text]])[0], /policy text belongs/u);
  assert.deepEqual(check([["crates/rbit-repositories/src/releases.rs", text]]), []);
  assert.deepEqual(check([["crates/rbit-services/tests/releases.rs", text]]), [], "tests assert the text");
});

test("release persistence is reached through the release adapter", () => {
  const store = "let store = self.ctx.database.releases();\n";
  assert.match(check([["crates/rbit-services/src/lfs.rs", store]])[0], /bypasses ReleaseService/u);
  for (const path of [
    "crates/rbit-services/src/releases.rs", "crates/rbit-services/src/releases/files.rs", "crates/rbit-services/src/webhooks.rs",
    "crates/rbit-services/src/imports/job.rs", "crates/rbit-storage/src/sqlite/mod.rs", "crates/rbit-server/tests/backup.rs",
  ]) assert.deepEqual(check([[path, store]]), [], path);
  assert.deepEqual(check([["crates/rbit-web/src/routes/releases.rs", "let url = urls.releases();\n"]]), []);
});

test("the release service holds no visibility, permission or limit decisions", () => {
  for (const source of [
    "let visible = release.filter(|r| !r.is_draft || repo.access.can_write());\n",
    "validate_asset_name(name).map_err(ServiceError::Invalid)?;\n",
    "if self.store().count_assets(id).await? >= limit {}\n",
    "repo.ensure_writable()?;\n",
    "return Err(ServiceError::Forbidden);\n",
  ]) {
    assert.match(check([["crates/rbit-services/src/releases.rs", source]])[0], /returned to the service adapter/u, source);
    assert.match(check([["crates/rbit-services/src/releases/files.rs", source]])[0], /service adapter/u, source);
  }
  const mapping = "ReleaseError::Forbidden => ServiceError::Forbidden,\n#[cfg(test)]\nmod tests { fn f() { let is_draft = false; } }\n";
  assert.deepEqual(check([["crates/rbit-services/src/releases.rs", mapping]]), [], "error mapping and unit-test seeds are allowed");
});

test("the archive service supplies slots and Git only", () => {
  for (const source of ["impl From<Refused> for ArchiveError {}\n", "#[derive(thiserror::Error)]\n", "let revision = sanitized.trim_matches('.');\n"]) {
    assert.match(check([["crates/rbit-services/src/archives.rs", source]])[0], /RepositoryArchives/u, source);
  }
  assert.deepEqual(check([["crates/rbit-services/src/lfs.rs", "#[derive(thiserror::Error)]\n"]]), []);
});

test("comments and non-Rust files are allowed", () => {
  assert.deepEqual(check([
    ["crates/rbit-services/src/a.rs", "// pub fn human_bytes was retired\n/* database.releases() */\n"],
    ["crates/rbit-services/src/releases.rs", "//! Drafts (`is_draft`) are decided by the application.\n"],
    ["docs/x.md", "pub struct ReleaseAsset"],
  ]), []);
});

test("the current tree has no retired release or archive path", () => {
  const { files } = authoredSources(fileURLToPath(root));
  assert.ok(files.length > 100);
  assert.deepEqual(check(files), []);
  assert.match(readFileSync(new URL("crates/rbit-services/src/releases.rs", root), "utf8"), /Releases::upload/u);
  assert.match(readFileSync(new URL("crates/rbit-services/src/archives.rs", root), "utf8"), /RepositoryArchives::start/u);
});
