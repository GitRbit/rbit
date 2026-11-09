// Git as a client of the rehearsal instances: an isolated configuration, deterministic
// history (fixed authors and dates, so object ids are reproducible), smart HTTP clone and
// push with tokens, and Git LFS objects through the batch API that `git lfs` uses.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";

export class Git {
  constructor(home) {
    this.home = home;
  }

  env(extra = {}) {
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")));
    return {
      ...env,
      HOME: this.home,
      XDG_CONFIG_HOME: this.home,
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_CONFIG_GLOBAL: join(this.home, "gitconfig"),
      GIT_TERMINAL_PROMPT: "0",
      GIT_AUTHOR_NAME: "Rehearsal",
      GIT_AUTHOR_EMAIL: "rehearsal@example.test",
      GIT_COMMITTER_NAME: "Rehearsal",
      GIT_COMMITTER_EMAIL: "rehearsal@example.test",
      ...extra,
    };
  }

  async init() {
    await mkdir(this.home, { recursive: true });
    await writeFile(join(this.home, "gitconfig"), "[init]\n\tdefaultBranch = main\n[protocol]\n\tversion = 2\n[credential]\n\thelper =\n");
  }

  run(cwd, args, { env, allowFailure = false } = {}) {
    try {
      return execFileSync("git", args, { cwd, env: this.env(env), encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
    } catch (error) {
      if (allowFailure) return { failed: true, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
      throw new Error(`git ${args.join(" ")} failed in ${cwd}:\n${error.stdout ?? ""}${error.stderr ?? ""}`);
    }
  }

  /** Commits `files` (path → contents) at a fixed time `tick` seconds after the epoch base. */
  async commit(dir, files, message, tick) {
    for (const [path, contents] of Object.entries(files)) {
      await mkdir(dirname(join(dir, path)), { recursive: true });
      await writeFile(join(dir, path), contents);
    }
    this.run(dir, ["add", "--all"]);
    const date = `${1_700_000_000 + tick} +0000`;
    this.run(dir, ["commit", "--quiet", "-m", message], { env: { GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date } });
    return this.run(dir, ["rev-parse", "HEAD"]);
  }

  /** A working repository with `main`, a `feature` branch and tags, as seeded projects have. */
  async sample(dir, project) {
    await mkdir(dir, { recursive: true });
    this.run(dir, ["init", "--quiet", "--initial-branch=main"]);
    await this.commit(dir, { "README.md": `# ${project}\n\nSeeded by the release rehearsal.\n`, "src/main.rs": "fn main() {}\n" }, "Initial commit", 1);
    await this.commit(dir, { "src/lib.rs": "pub fn answer() -> u32 { 42 }\n" }, "Add the library", 2);
    const date = `${1_700_000_003} +0000`;
    this.run(dir, ["tag", "-a", "v1.0", "-m", "Version 1.0"], { env: { GIT_COMMITTER_DATE: date } });
    this.run(dir, ["tag", "lightweight"]);
    this.run(dir, ["checkout", "--quiet", "-b", "feature"]);
    await this.commit(dir, { "src/feature.rs": "pub fn feature() {}\n" }, "Start a feature", 4);
    this.run(dir, ["checkout", "--quiet", "main"]);
    return dir;
  }

  /** `ref → object id` of a remote, as `git ls-remote` reports it. */
  refs(url) {
    const refs = {};
    for (const line of this.run(this.home, ["ls-remote", url]).split("\n").filter(Boolean)) {
      const [id, name] = line.split("\t");
      refs[name] = id;
    }
    return refs;
  }

  /** Clones `url` into `dir` and checks every object with `git fsck`. */
  cloneAndCheck(url, dir) {
    this.run(this.home, ["clone", "--quiet", "--mirror", url, dir]);
    this.run(dir, ["fsck", "--no-progress", "--strict"]);
    return dir;
  }
}

/** `http://login:secret@host/owner/repo.git` for a base URL. */
export function authenticatedUrl(base, owner, repo, login, secret) {
  const url = new URL(`/${owner}/${repo}.git`, base);
  if (login) {
    url.username = encodeURIComponent(login);
    url.password = encodeURIComponent(secret);
  }
  return url.toString();
}

export const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** The pointer file Git LFS commits in place of `bytes`. */
export function lfsPointer(bytes) {
  return `version https://git-lfs.github.com/spec/v1\noid sha256:${sha256(bytes)}\nsize ${bytes.length}\n`;
}

const LFS_TYPE = "application/vnd.git-lfs+json";

async function batch(base, owner, repo, auth, operation, objects) {
  const response = await fetch(`${base}/${owner}/${repo}.git/info/lfs/objects/batch`, {
    method: "POST",
    headers: { accept: LFS_TYPE, "content-type": LFS_TYPE, authorization: auth },
    body: JSON.stringify({ operation, transfers: ["basic"], objects }),
  });
  if (!response.ok) throw new Error(`LFS ${operation} batch failed: HTTP ${response.status} ${await response.text()}`);
  return (await response.json()).objects;
}

const basic = (login, secret) => `Basic ${Buffer.from(`${login}:${secret}`).toString("base64")}`;

/** Uploads one LFS object the way `git lfs push` does. */
export async function lfsUpload(base, owner, repo, [login, secret], bytes) {
  const object = { oid: sha256(bytes), size: bytes.length };
  const [answer] = await batch(base, owner, repo, basic(login, secret), "upload", [object]);
  if (answer.error) throw new Error(`LFS upload refused: ${JSON.stringify(answer.error)}`);
  const upload = answer.actions?.upload;
  if (upload) {
    const put = await fetch(upload.href, { method: "PUT", headers: { ...upload.header, "content-type": "application/octet-stream" }, body: bytes });
    if (!put.ok) throw new Error(`LFS PUT failed: HTTP ${put.status} ${await put.text()}`);
    const verify = answer.actions.verify;
    if (verify) {
      const done = await fetch(verify.href, { method: "POST", headers: { ...verify.header, "content-type": LFS_TYPE, accept: LFS_TYPE }, body: JSON.stringify(object) });
      if (!done.ok) throw new Error(`LFS verify failed: HTTP ${done.status}`);
    }
  }
  return object;
}

/** Downloads one LFS object and returns its bytes, or throws with the server's answer. */
export async function lfsDownload(base, owner, repo, [login, secret], { oid, size }) {
  const [answer] = await batch(base, owner, repo, basic(login, secret), "download", [{ oid, size }]);
  if (answer.error) throw new Error(`LFS object ${oid} unavailable: ${JSON.stringify(answer.error)}`);
  const download = answer.actions.download;
  const response = await fetch(download.href, { headers: download.header ?? {} });
  if (!response.ok) throw new Error(`LFS GET failed: HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}
