// Checks a running instance against what was written to it. Every check is recorded, not
// just the first failure, so a rollback's report lists everything it lost.
import { join } from "node:path";
import { Client } from "./http.mjs";
import { authenticatedUrl, lfsDownload, sha256 } from "./gitops.mjs";
import { actionsFacts } from "./actions.mjs";
import { signIn } from "./seed.mjs";

export class Checks {
  constructor(label) {
    this.label = label;
    this.results = [];
  }

  /** Runs one check; a failure named in `tolerated` is reported but fails nothing. */
  async check(name, task, tolerated) {
    try {
      const detail = await task();
      this.results.push({ name, ok: true, ...(detail === undefined ? {} : { detail }) });
    } catch (error) {
      const detail = error.message.split("\n").slice(0, 6).join("\n");
      this.results.push({ name, ok: false, ...(tolerated ? { expectedLoss: true, detail: `${tolerated}: ${detail}` } : { detail }) });
    }
  }

  get failed() {
    return this.results.filter((result) => !result.ok);
  }
}

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

let clones = 0;

/** Everything `seed` wrote must still be there and usable. */
/** `afterWrites`: the post-upgrade writes are in too, so `main` moved and comments grew. */
export async function verifySeed(instance, expected, { git, work, label, known = {}, afterWrites = false }) {
  const checks = new Checks(label);
  const base = instance.base;
  const anonymous = new Client(base);
  const token = expected.token;
  const api = (path) => anonymous.json("GET", `/api/v1${path}`, undefined, token);

  await checks.check("health endpoints answer", async () => {
    for (const path of ["/healthz", "/readyz"]) assert((await anonymous.get(path)).status === 200, `${path} is not healthy`);
  });
  await checks.check("alice signs in with her password", () => signIn(base, "alice").then(() => undefined));
  await checks.check("bob signs in with password and TOTP (sealed secret readable)", () => signIn(base, "bob", expected.totp).then(() => undefined));
  await checks.check("a session from before survives", async () => {
    const old = new Client(base);
    old.restoreCookies(expected.session);
    const page = await old.get("/settings/profile");
    assert(page.status === 200, `the old session cookie is no longer signed in (HTTP ${page.status} → ${page.location})`);
  }, known.session);
  await checks.check("the personal access token authenticates the API", async () => {
    const me = await api("/user");
    assert(me.status === 200 && me.body.login === "alice", `GET /user answered ${me.status}`);
  });
  for (const [name, repo] of Object.entries(expected.repos)) {
    const [owner, project] = name.split("/");
    await checks.check(`${name}: refs over smart HTTP are unchanged`, () => {
      const refs = git.refs(authenticatedUrl(base, owner, project, "alice", token));
      for (const [ref, id] of Object.entries(repo.refs)) {
        if (afterWrites && ["HEAD", "refs/heads/main"].includes(ref) && name === "alice/app") continue;
        assert(refs[ref] === id, `${ref} is ${refs[ref] ?? "missing"}, expected ${id}`);
      }
    });
    await checks.check(`${name}: a full clone passes git fsck --strict`, () => {
      git.cloneAndCheck(authenticatedUrl(base, owner, project, "alice", token), join(work, "clones", `${label}-${clones++}`));
    });
    await checks.check(`${name}: anonymous visibility (${repo.private ? "hidden" : "public"})`, async () => {
      const page = await anonymous.get(`/${name}`);
      assert(page.status === (repo.private ? 404 : 200), `anonymous GET /${name} answered ${page.status}`);
    });
    await checks.check(`${name}: tar.gz archive of main downloads (signed in)`, async () => {
      const archive = await (await signIn(base, "alice")).get(`/${name}/archive/main.tar.gz`);
      assert(archive.status === 200 && archive.bytes[0] === 0x1f && archive.bytes[1] === 0x8b, `archive answered ${archive.status}`);
    });
  }
  await checks.check("organisation acme lists bob", async () => {
    const members = await api("/orgs/acme/members");
    assert(members.status === 200 && members.body.some((member) => member.login === "bob"), `members answered ${members.status}`);
  });
  for (const issue of expected.issues) {
    await checks.check(`issue #${issue.number} (${issue.state}) with ${issue.comments} comment(s)`, async () => {
      const found = await api(`/repos/${issue.repo}/issues/${issue.number}`);
      assert(found.status === 200 && found.body.title === issue.title && found.body.state === issue.state, `issue answered ${found.status}`);
      const comments = await api(`/repos/${issue.repo}/issues/${issue.number}/comments`);
      const enough = afterWrites ? comments.body.length >= issue.comments : comments.body.length === issue.comments;
      assert(enough, `${comments.body.length} comments`);
    });
  }
  for (const pull of expected.pulls) {
    await checks.check(`pull request #${pull.number} and its page`, async () => {
      const found = await api(`/repos/${pull.repo}/pulls/${pull.number}`);
      assert(found.status === 200 && found.body.title === pull.title, `pull answered ${found.status}`);
      const page = await anonymous.get(`/${pull.repo}/pull/${pull.number}`);
      assert(page.status === 200, `the pull request page answered ${page.status}`);
    });
  }
  await checks.check("release file downloads with its recorded SHA-256", async () => {
    const { repo, assetId, sha256: digest } = expected.release;
    const file = await anonymous.request("GET", `/api/v1/repos/${repo}/releases/assets/${assetId}`, { headers: { accept: "application/octet-stream", authorization: `token ${token}` } });
    assert(file.status === 200 && sha256(file.bytes) === digest, `asset answered ${file.status}, ${file.bytes.length} bytes`);
  });
  await checks.check("Git LFS object downloads with its oid", async () => {
    const [owner, project] = expected.lfs.repo.split("/");
    const bytes = await lfsDownload(base, owner, project, ["alice", token], expected.lfs);
    assert(sha256(bytes) === expected.lfs.oid, "the LFS object's contents changed");
  });
  await checks.check("webhook (with stored secret) is listed", async () => {
    const hook = await api(`/repos/${expected.hook.repo}/hooks/${expected.hook.id}`);
    assert(hook.status === 200, `hook answered ${hook.status}`);
  });
  await checks.check("SSH key is listed", async () => {
    const page = await (await signIn(base, "alice")).get("/settings/keys");
    assert(page.text.includes(expected.sshKey), "the SSH key is missing");
  });
  if (expected.actions) await verifyActions(checks, base, token, expected.actions, known);
  return checks;
}

async function verifyActions(checks, base, token, expected, known) {
  const alice = await signIn(base, "alice");
  let facts;
  await checks.check("Actions run is listed with its conclusion", async () => {
    facts = await actionsFacts(alice, token, expected.runId);
    assert(facts.conclusion === expected.conclusion, `conclusion ${facts.conclusion}`);
  });
  await checks.check("Actions job log is readable and still masks the secret", async () => {
    assert(facts, "the run is missing");
    assert(facts.logStatus === 200 && facts.logHasMarker, `raw log answered ${facts.logStatus}, marker ${facts.logHasMarker}`);
    assert(facts.secretMasked, "the secret appears unmasked");
  });
  await checks.check("Actions artifact downloads", async () => {
    assert(facts?.artifact, "the artifact is not listed");
    assert(facts.artifact.downloadStatus === 200 && facts.artifact.downloadBytes > 0,
      `download answered ${facts.artifact.downloadStatus} with ${facts.artifact.downloadBytes} bytes`);
  }, known.artifactDownload);
  await checks.check("Actions secret is listed", async () => {
    const secrets = await alice.json("GET", "/api/v1/repos/alice/app/actions/secrets", undefined, token);
    assert(secrets.status === 200 && secrets.body.secrets.some((secret) => secret.name === "GREETING"), `secrets answered ${secrets.status}`);
  });
}
