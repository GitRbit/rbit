// Writes made on the candidate after the upgrade, and the probe for each: present on
// the candidate, and (after a rollback to the pre-upgrade backup) the report says which
// of them the rollback lost.
import { Checks } from "./verify.mjs";
import { authenticatedUrl } from "./gitops.mjs";
import { attributeAfter, expect } from "./http.mjs";
import { signIn } from "./seed.mjs";
import { probeWiki, seedWiki } from "./wiki.mjs";

export async function writeAfterUpgrade(instance, expected, { git, app }) {
  const base = instance.base;
  const token = expected.token;
  const alice = await signIn(base, "alice");
  const api = async (method, path, body) => {
    const response = await alice.json(method, `/api/v1${path}`, body, token);
    expect([200, 201].includes(response.status), `${method} ${path} after the upgrade`, response);
    return response.body;
  };
  const issue = await api("POST", "/repos/alice/app/issues", { title: "Written after the upgrade", body: "post-upgrade" });
  const comment = await api("POST", `/repos/alice/app/issues/${expected.issues[0].number}/comments`, { body: "A comment written after the upgrade." });
  const url = authenticatedUrl(base, "alice", "app", "alice", token);
  git.run(app.dir, ["pull", "--quiet", "--ff-only", url, "main"]);
  const commit = await git.commit(app.dir, { "after-upgrade.txt": "pushed to the candidate\n" }, "Push after the upgrade", 100);
  git.run(app.dir, ["push", "--quiet", url, "main", "main:refs/heads/after-upgrade"]);
  const page = await alice.form("/settings/tokens", { name: "after-upgrade", scope: "read", expires: "never" });
  const newToken = attributeAfter(page.text, 'id="new-token"', "value");
  expect(newToken, "a token can be created after the upgrade", page);
  const wiki = await seedWiki(alice, "alice/app");
  return { issue: issue.number, comment: comment.id, commit, newToken, wiki };
}

/** Probes each post-upgrade write; `present` says what the caller expects. */
export async function probeWrites(instance, expected, written, { git, label, present }) {
  const checks = new Checks(label);
  const base = instance.base;
  const alice = await signIn(base, "alice");
  const api = (path, token = expected.token) => alice.json("GET", `/api/v1${path}`, undefined, token);
  const want = (found, what) => {
    if (found !== present) throw new Error(`${what} is ${found ? "present" : "missing"}`);
    return found ? "present" : "absent";
  };
  await checks.check(`issue #${written.issue} written after the upgrade`, async () => want((await api(`/repos/alice/app/issues/${written.issue}`)).status === 200, "the issue"));
  await checks.check("comment written after the upgrade", async () => {
    const comments = await api(`/repos/alice/app/issues/${expected.issues[0].number}/comments`);
    return want(comments.body.some((comment) => comment.id === written.comment), "the comment");
  });
  await checks.check("commit pushed after the upgrade (main and branch after-upgrade)", () => {
    const refs = git.refs(authenticatedUrl(base, "alice", "app", "alice", expected.token));
    return want(refs["refs/heads/main"] === written.commit && refs["refs/heads/after-upgrade"] === written.commit, "the pushed commit");
  });
  await checks.check("access token created after the upgrade", async () => want((await api("/user", written.newToken)).status === 200, "the token"));
  await probeWiki(checks, alice, written.wiki, present);
  return checks;
}
