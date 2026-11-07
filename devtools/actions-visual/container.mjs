// Host-side Linux CI launcher. Only explicit prototype, public assets and harness are mounted.
import { mkdtemp, mkdir, writeFile, chmod, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { image } from "./policy.mjs";
import { digest, revision } from "./provenance.mjs";
import { browserSeccompPolicy } from "./seccomp.mjs";
import { checkedMount } from "./mounts.mjs";
import { docker, interruptDocker } from "./docker.mjs";
import { createOwnership, cleanupOwned } from "./ownership.mjs";
import { containerMode, productionAssetRoot, productionBinary } from "./production-policy.mjs";
import { verifyProductionArtifact } from "./production-artifact.mjs";
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const commit = revision(process.env.GITHUB_SHA);
const mode = containerMode(process.argv.slice(2));
if (process.platform !== "linux" || process.getuid?.() === 0) throw new Error("Nonroot Linux CI required");
const fixture = mode === "production" ? await verifyProductionArtifact(root, commit) : null;
if (fixture) await chmod(fixture.binary, 0o500);
const temporary = await mkdtemp(join(tmpdir(), "rbit-actions-visual-"));
const artifacts = join(root, mode === "production" ? "actions-production-artifacts" : "actions-visual-artifacts");
await mkdir(artifacts, { recursive: false });
await chmod(artifacts, 0o700);
const policy = join(temporary, "seccomp.json");
const serializedPolicy = JSON.stringify(await browserSeccompPolicy());
const seccompPolicySha256 = digest(serializedPolicy);
await writeFile(policy, serializedPolicy, { flag: "wx", mode: 0o600 });
const sources = fixture ? [await checkedMount(root, "devtools/actions-visual"), fixture.binary, fixture.assets,
  await checkedMount(root, "actions-production-fixture/fixture.json")] : await Promise.all([
  "docs/design/prototypes/actions", "crates/rbit-web/assets/app.css",
  "crates/rbit-web/assets/logo-384.avif", "devtools/actions-visual",
].map(path => checkedMount(root, path)));
const controlDirectory = join(root, mode === "production" ? ".actions-production-control" : ".actions-visual-control");
const ownership = await createOwnership(controlDirectory);
let finalizing;
const finalize = () => finalizing ??= cleanupOwned(controlDirectory, docker);
for (const [signal, exitCode] of [["SIGTERM", 143], ["SIGINT", 130]]) {
  process.once(signal, () => {
    void interruptDocker().then(finalize).then(() => process.exit(exitCode), () => process.exit(1));
  });
}
const pullStart = performance.now();
await docker(["pull", "--quiet", "--platform", "linux/amd64", image], 300000);
const pullMs = Math.round(performance.now() - pullStart);
const mount = (host, target, readonly = true) => ["--mount",
  `type=bind,source=${host},target=${target}${readonly ? ",readonly" : ""}`];
const bindings = fixture ? [
  ...mount(sources[0], "/work/devtools/actions-visual"), ...mount(sources[1], productionBinary),
  ...mount(sources[2], productionAssetRoot), ...mount(sources[3], "/fixture/fixture.json"),
] : [
  ...mount(sources[0], "/work/docs/design/prototypes/actions"), ...mount(sources[1], "/work/crates/rbit-web/assets/app.css"),
  ...mount(sources[2], "/work/crates/rbit-web/assets/logo-384.avif"), ...mount(sources[3], "/work/devtools/actions-visual"),
];
const started = performance.now();
let succeeded = false;
try {
  await docker(["create", "--cidfile", ownership.cidfile, "--label", ownership.label, "--init",
    "--platform", "linux/amd64", "--network", "none", "--cap-drop", "ALL",
    "--security-opt", "no-new-privileges", "--security-opt", `seccomp=${policy}`,
    "--user", `${process.getuid()}:${process.getgid()}`, "--read-only", "--shm-size", "1g",
    "--tmpfs", "/tmp:rw,nosuid,nodev,size=1g,mode=1777",
    "--memory", "3g", "--cpus", "2", "--pids-limit", "512",
    "--env", "HOME=/tmp/visual-home", "--env", "PLAYWRIGHT_BROWSERS_PATH=/ms-playwright",
    "--env", `VISUAL_REVISION=${commit}`,
    ...bindings,
    ...mount(artifacts, "/artifacts", false),
    "--workdir", "/work/devtools/actions-visual", image, "node", fixture ? "production-run.mjs" : "run.mjs"], 300000);
  const cid = (await readFile(ownership.cidfile, "utf8")).trim();
  if (!/^[a-f0-9]{64}$/.test(cid)) throw new Error("CONTROL_CID_INVALID");
  await docker(["start", "--attach", cid], 300000);
  if (await docker(["inspect", "--format", "{{.State.ExitCode}} {{.State.Running}}", cid]) !== "0 false") {
    throw new Error("CONTAINER_EXIT_UNPROVED");
  }
  succeeded = true;
} finally {
  await finalize(); // Verify exact CID ownership before removal; errors remain failures.
  await writeFile(join(artifacts, "container.json"), JSON.stringify({
    schema: 1, revision: commit, mode, image, pullMs, seccompPolicySha256,
    seccompDelta: "namespace-chroot-only-v1",
    testMs: Math.round(performance.now() - started), succeeded,
    note: "Pull wall time is measured; actual network bytes and peak RSS are unmeasured.",
  }, null, 2) + "\n");
}
