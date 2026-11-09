// Repository wiki pages (new in 0.2.0) through their forms: a kept page, a renamed one and
// a deleted one, and the probe that finds each where it should be, or nowhere.
import { attributeAfter, expect } from "./http.mjs";

const pageId = (html) => html.match(/\/wiki\/-\/page\/(\d+)\/edit/)?.[1];

async function create(client, wiki, title, content) {
  const created = await client.form(`${wiki}/-/new`, { title, path: "", content, intent: "save" });
  expect(created.status === 303, `wiki page "${title}" is created`, created);
  const page = await client.get(created.location);
  const id = pageId(page.text);
  expect(id, `wiki page "${title}" offers an edit link`, page);
  return { id, url: created.location };
}

/** Turns the wiki of `repo` on and writes a kept, a renamed and a deleted page. */
export async function seedWiki(client, repo) {
  const wiki = `/${repo}/wiki`;
  const enabled = await client.form(`${wiki}/-/settings`, { enabled: "on" });
  expect(enabled.status === 303, "the wiki is turned on (wikis start off)", enabled);
  const kept = await create(client, wiki, "Upgrade notes", "Written after the upgrade.");

  const original = await create(client, wiki, "Old name", "This page is renamed.");
  const form = await client.get(`${wiki}/-/page/${original.id}/edit`);
  const base = attributeAfter(form.text, 'name="base"', "value");
  expect(base, "the edit form carries its base revision", form);
  const renamed = await client.form(`${wiki}/-/page/${original.id}/edit`, { title: "New name", path: "", content: "This page was renamed.", base, intent: "save" });
  expect(renamed.status === 303, "the wiki page is renamed", renamed);

  const doomed = await create(client, wiki, "Doomed page", "This page is deleted.");
  const confirm = await client.get(`${wiki}/-/page/${doomed.id}/delete`);
  const deleteBase = attributeAfter(confirm.text, 'name="base"', "value");
  expect(deleteBase, "the delete confirmation carries its base revision", confirm);
  const deleted = await client.form(`${wiki}/-/page/${doomed.id}/delete`, { base: deleteBase });
  expect(deleted.status === 303, "the wiki page is deleted", deleted);

  return { kept: kept.url, renamedFrom: original.url, renamedTo: renamed.location, deleted: doomed.url };
}

/**
 * Probes the seeded wiki. `present`: the kept and renamed pages answer 200 and the old
 * name and the deleted page do not; otherwise (rolled back) none of them answers.
 */
export async function probeWiki(checks, client, wiki, present) {
  const status = async (url) => (await client.get(url)).status;
  await checks.check(`wiki pages written after the upgrade are ${present ? "present" : "absent"}`, async () => {
    const kept = await status(wiki.kept);
    const renamed = await status(wiki.renamedTo);
    if ((kept === 200) !== present || (renamed === 200) !== present) throw new Error(`kept page ${kept}, renamed page ${renamed}`);
    return `kept ${kept}, renamed ${renamed}`;
  });
  await checks.check("renamed wiki page left its old name and the deleted page stays deleted", async () => {
    const old = await status(wiki.renamedFrom);
    const deleted = await status(wiki.deleted);
    if (old === 200 || deleted === 200) throw new Error(`old name ${old}, deleted page ${deleted}`);
    return `old name ${old}, deleted page ${deleted}`;
  });
}
