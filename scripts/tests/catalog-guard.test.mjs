// Every user-visible word of rbit-web comes from the message catalog, so a translator can
// add a language without touching templates. Templates show catalog text only through
// `{{ }}` expressions and `{% call ui::localized(...) %}`; anything else that reads as a
// word is hard-coded text. Templates not converted yet are listed, one per line, in
// fixtures/catalog-exemptions.txt. The list only shrinks: a listed template that passes
// must leave it, and once it is empty this guard covers every page.
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { findHardCodedText } from "./support/template-text.mjs";

const templates = new URL("../../crates/rbit-web/templates/", import.meta.url);
const root = fileURLToPath(templates);
const read = async (path) => (await readFile(new URL(path, templates), "utf8")).replace(/\r\n/g, "\n");

// Reviewed literal fragments of one template that are not prose: [template, fragment, why].
const SNIPPETS = [
  ["themes/picker.html", "<b>core</b>", "the preview's sample repository name"],
  ["themes/picker.html", "<b>Aa</b>", "a type specimen of each theme's font"],
  ["auth/two_factor.html", 'placeholder="xxxx-xxxx-xxxx"', "the shape of a recovery code"],
  ["repo/deploy_keys.html", 'placeholder="ssh-ed25519 AAAA… deploy@server"', "the shape of a public key"],
  ["repo/new_import.html", 'placeholder="https://gitea.example.com"', "the shape of a server address"],
  ["repo/new_mirror.html", 'placeholder="https://example.com/group/project.git"', "the shape of a clone URL"],
  ["repo/push_mirror_fields.html", 'placeholder="https://github.com/acme/rocket.git"', "the shape of a clone URL"],
];

async function templatePaths() {
  const entries = await readdir(templates, { recursive: true, withFileTypes: true });
  return entries.filter((entry) => entry.isFile() && entry.name.endsWith(".html"))
    .map((entry) => join(entry.parentPath, entry.name).slice(root.length).replace(/\\/g, "/")).sort();
}

async function exemptions() {
  const text = await readFile(new URL("fixtures/catalog-exemptions.txt", import.meta.url), "utf8");
  return text.split(/\r?\n/).map((line) => line.trim()).filter((line) => line && !line.startsWith("#"));
}

test("the exemption list is sorted, unique and names existing templates", async () => {
  const listed = await exemptions();
  const paths = new Set(await templatePaths());
  assert.deepEqual(listed, [...new Set(listed)].sort(), "keep catalog-exemptions.txt sorted and unique");
  for (const path of listed) assert.ok(paths.has(path), `catalog-exemptions.txt names a missing template: ${path}`);
});

test("every reviewed fragment still occurs in its template", async () => {
  for (const [path, fragment, why] of SNIPPETS) {
    assert.ok((await read(path)).includes(fragment), `${path} no longer contains ${fragment} (${why}); drop it from SNIPPETS`);
  }
});

test("templates show catalog text, not hard-coded words", async () => {
  const exempt = new Set(await exemptions());
  const paths = await templatePaths();
  assert.ok(paths.length > 150, "found the templates");
  for (const path of paths) {
    const snippets = SNIPPETS.filter(([template]) => template === path).map(([, fragment]) => fragment);
    const findings = findHardCodedText(await read(path), snippets);
    if (exempt.has(path)) {
      assert.ok(findings.length > 0, `${path} has no hard-coded text left: remove it from catalog-exemptions.txt`);
      continue;
    }
    const listed = findings.map(({ kind, text }) => `  ${kind}: ${text}`).join("\n");
    assert.equal(findings.length, 0, `${path} hard-codes text; route it through the catalog and ui::localized:\n${listed}`);
  }
});
