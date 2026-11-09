// Fills a running baseline instance the way people would: accounts made with the CLI,
// then the web forms, the REST API, smart HTTP and Git LFS. Returns what was written,
// which `verify.mjs` later checks against whatever binary serves the directory.
import { join } from "node:path";
import { Client, attributeAfter, expect, freshTotp, sshPublicKey } from "./http.mjs";
import { authenticatedUrl, lfsPointer, lfsUpload, sha256 } from "./gitops.mjs";
import { seedActions } from "./actions.mjs";

export const PASSWORDS = { alice: "rehearsal-alice-password", bob: "rehearsal-bob-password" };

/** Creates the accounts with `rbit user create` while the instance is stopped. */
export async function createAccounts(instance, binary) {
  for (const [name, admin] of [["alice", true], ["bob", false]]) {
    const args = ["user", "create", name, `${name}@example.test`, ...(admin ? ["--admin"] : [])];
    const result = await instance.run(binary, args, { input: `${PASSWORDS[name]}\n` });
    if (result.code !== 0) throw new Error(`user create ${name} failed:\n${result.output}`);
  }
}

export async function signIn(base, login, totpState, password = PASSWORDS[login]) {
  const client = new Client(base);
  const response = await client.form("/login", { login, password });
  if (response.status === 303 && response.location?.startsWith("/login/two-factor")) {
    expect(totpState, `${login} asks for a second factor nobody enrolled`, response);
    const [code, step] = await freshTotp(totpState.secret, totpState.lastStep);
    const second = await client.form("/login/two-factor", { code });
    expect(second.status === 303 && !second.location.startsWith("/login"), `${login}'s TOTP code is accepted`, second);
    totpState.lastStep = step;
  } else {
    expect(response.status === 303 && response.location === "/", `${login} signs in with a password`, response);
  }
  return client;
}

async function createToken(client, name) {
  const page = await client.form("/settings/tokens", { name, scope: "admin", expires: "never" });
  const secret = attributeAfter(page.text, 'id="new-token"', "value");
  expect(secret?.startsWith("rbit_"), "a personal access token is shown once", page);
  return secret;
}

async function createRepo(client, git, work, base, token, owner, name, visibility) {
  const created = await client.form("/new", { owner, name, description: `${name} for the rehearsal`, visibility });
  expect(created.status === 303, `${owner}/${name} is created`, created);
  const dir = await git.sample(join(work, "src", `${owner}-${name}`), name);
  const url = authenticatedUrl(base, owner, name, "alice", token);
  git.run(dir, ["push", "--quiet", url, "main", "feature", "v1.0", "lightweight"]);
  return { dir, url };
}

async function api(client, token, method, path, body, expected = [200, 201]) {
  const response = await client.json(method, `/api/v1${path}`, body, token);
  expect(expected.includes(response.status), `${method} ${path}`, response);
  return response.body;
}

/** Seeds everything except Actions (see `actions.mjs`); returns the expectations. */
export async function seed({ instance, git, work, runner }) {
  const base = instance.base;
  const alice = await signIn(base, "alice");
  const token = await createToken(alice, "rehearsal");
  const expected = { token, repos: {}, totp: null };

  const org = await alice.form("/organizations/new", { name: "acme", display_name: "Acme", description: "Rehearsal organisation" });
  expect(org.status === 303, "the organisation is created", org);
  const member = await alice.form("/acme/members", { username: "bob", role: "member" });
  expect(member.status === 303, "bob joins acme", member);

  const app = await createRepo(alice, git, work, base, token, "alice", "app", "public");
  const tools = await createRepo(alice, git, work, base, token, "acme", "tools", "private");

  // Git LFS: the object through the batch API, its pointer through a push.
  const lfsBytes = Buffer.from(`large file contents ${"x".repeat(4096)}\n`);
  expected.lfs = { repo: "alice/app", ...(await lfsUpload(base, "alice", "app", ["alice", token], lfsBytes)) };
  await git.commit(app.dir, { ".gitattributes": "*.bin filter=lfs diff=lfs merge=lfs -text\n", "assets/blob.bin": lfsPointer(lfsBytes) }, "Track a binary with LFS", 5);
  git.run(app.dir, ["push", "--quiet", app.url, "main"]);

  // Issues, comments, a closed issue and a pull request between the seeded branches.
  const first = await api(alice, token, "POST", "/repos/alice/app/issues", { title: "Seeded open issue", body: "Opened before the upgrade." });
  await api(alice, token, "POST", `/repos/alice/app/issues/${first.number}/comments`, { body: "A comment from before the upgrade." });
  const second = await api(alice, token, "POST", "/repos/alice/app/issues", { title: "Seeded closed issue", body: "Closed before the upgrade." });
  await api(alice, token, "PATCH", `/repos/alice/app/issues/${second.number}`, { state: "closed" });
  const pull = await api(alice, token, "POST", "/repos/alice/app/pulls", { title: "Seeded pull request", head: "feature", base: "main", body: "Pull request from before the upgrade." });
  expected.issues = [
    { repo: "alice/app", number: first.number, title: "Seeded open issue", state: "open", comments: 1 },
    { repo: "alice/app", number: second.number, title: "Seeded closed issue", state: "closed", comments: 0 },
  ];
  expected.pulls = [{ repo: "alice/app", number: pull.number, title: "Seeded pull request" }];

  // A release with an uploaded file.
  const release = await api(alice, token, "POST", "/repos/alice/app/releases", { tag_name: "v1.0", name: "Version 1.0", body: "Seeded release." });
  const assetBytes = Buffer.from(Array.from({ length: 70_000 }, (_, index) => index % 251));
  const upload = await alice.request("POST", `/api/v1/repos/alice/app/releases/${release.id}/assets?name=payload.bin`, {
    body: assetBytes, headers: { authorization: `token ${token}`, "content-type": "application/octet-stream", accept: "application/json" },
  });
  expect(upload.status === 201, "the release file uploads", upload);
  const asset = JSON.parse(upload.text);
  expected.release = { repo: "alice/app", id: release.id, tag: "v1.0", assetId: asset.id, sha256: sha256(assetBytes), size: assetBytes.length };

  // A webhook (its secret is stored for HMAC), a star and an SSH key.
  const hook = await api(alice, token, "POST", "/repos/alice/app/hooks", {
    config: { url: "http://127.0.0.1:9/rehearsal-hook", content_type: "json", secret: "rehearsal-hook-secret" }, events: ["issues"], active: false,
  });
  expected.hook = { repo: "alice/app", id: hook.id };
  await api(alice, token, "PUT", "/user/starred/alice/app", undefined, [204]);
  const key = sshPublicKey("rehearsal@example.test");
  const added = await alice.form("/settings/keys", { title: "rehearsal laptop", key });
  expect(added.status === 303, "an SSH key is added", added);
  expected.sshKey = "rehearsal laptop";

  // Bob turns on TOTP two-factor authentication: its secret is sealed with the key.
  const bob = await signIn(base, "bob");
  const setup = await bob.get("/settings/security/two-factor");
  const secret = attributeAfter(setup.text, 'id="totp-secret"', "value");
  expect(secret, "the authenticator setup shows a key", setup);
  const totpState = { secret, lastStep: -1 };
  const [code, step] = await freshTotp(secret);
  const confirmed = await bob.form("/settings/security/two-factor", { code });
  expect([200, 303].includes(confirmed.status), "two-factor authentication is confirmed", confirmed);
  totpState.lastStep = step;
  expected.totp = totpState;

  expected.actions = runner ? await seedActions({ client: alice, token, git, app, work, base, runner }) : null;

  for (const [name, repo] of [["alice/app", app], ["acme/tools", tools]]) {
    expected.repos[name] = { refs: git.refs(repo.url), private: name === "acme/tools" };
  }
  // A session that outlives the upgrade if the session store survives it.
  expected.session = alice.snapshotCookies();
  return expected;
}
