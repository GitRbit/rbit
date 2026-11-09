import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkAdministrationRetirement } from "../rewrite/administration-retirement.mjs";
import { authoredSources } from "../rewrite/check.mjs";

const SERVICE = "crates/rbit-services/src/admin.rs";
const STORE = "crates/rbit-storage/src/sqlite/admin.rs";

test("listing and moderation types stay in rbit-access", () => {
  for (const source of ["pub enum Moderation { Applied }", "pub struct ListFilter<'a> { pub search: &'a str }", "pub const PAGE_SIZE: u32 = 30;"]) {
    assert.match(checkAdministrationRetirement([[STORE, source]])[0], /owned by rbit-access/u, source);
    assert.match(checkAdministrationRetirement([[SERVICE, source]])[0], /owned by rbit-access/u, source);
  }
  assert.deepEqual(checkAdministrationRetirement([[STORE, "pub use rbit_access::administration::{ListFilter, Listing, Moderation};"]]), []);
  assert.deepEqual(checkAdministrationRetirement([["crates/rbit-web/src/routes/issues.rs", "pub struct ListQuery { pub page: u32 }"]]), []);
});

test("the service adapter holds no administration decisions", () => {
  for (const source of [
    "fn refuse_self(actor: &User, target: &UserRecord, action: &str) {}",
    "match outcome { Moderation::LastAdmin => {} }",
    "if confirmation.trim() != target.user.name().as_str() {}",
    ".filter(|(_, old, new)| old != new)",
    "if actor.is_admin { Ok(()) } else { Err(ServiceError::Forbidden) }",
  ]) {
    assert.match(checkAdministrationRetirement([[SERVICE, source]])[0], /belongs in rbit_access::administration/u, source);
  }
  assert.deepEqual(checkAdministrationRetirement([[SERVICE, "// fn refuse_self retired\nself.rules().set_admin(&actor, id, true)"]]), []);
});

test("the current tree has no retired administration path", () => {
  const { files } = authoredSources(fileURLToPath(new URL("../../", import.meta.url)));
  assert.deepEqual(checkAdministrationRetirement(files), []);
});
