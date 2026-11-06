// Fixed startup checks inside the isolated CI container; no shell/URL input.
import { execFile } from "node:child_process";
import { mkdtemp, rmdir } from "node:fs/promises";
import { join } from "node:path";
import { promisify } from "node:util";
import { ownedHome } from "./environment.mjs";
const execute = promisify(execFile);
const createScratch = () => mkdtemp(join(ownedHome, "chroot-probe-"));
async function probe(childNamespace, executeProbe, scratch) {
  let directory;
  try {
    directory = await scratch.create();
    const command = childNamespace ? "/usr/bin/unshare" : "/usr/sbin/chroot";
    const chroot = [directory, "/__rbit_probe_no_command__"];
    const args = childNamespace
      ? ["--user", "--map-root-user", "--root", directory,
        "--", "/__rbit_probe_no_command__"]
      : chroot;
    let error;
    try {
      await executeProbe(command, args, {
        encoding: "utf8", windowsHide: true, timeout: 5000, killSignal: "SIGKILL", maxBuffer: 2048,
        env: { PATH: "/usr/sbin:/usr/bin:/sbin:/bin", HOME: ownedHome, LANG: "C", LC_ALL: "C" },
      });
    } catch (failure) { error = failure; }
    const text = typeof error?.stderr === "string" ? error.stderr.slice(0, 2048) : "";
    const code = !error ? 0 : Number.isInteger(error.code) && error.code >= 0 && error.code <= 255 ? error.code : null;
    // chroot succeeds only in the child namespace, then fails to exec the fixed
    // nonexistent command in our fresh empty directory. Code127 alone is not proof.
    const confirmed = childNamespace && code === 127 && !error.signal
      && /unshare:.*failed to execute.*\/__rbit_probe_no_command__/i.test(text)
      && /no such file or directory/i.test(text);
    const denied = !childNamespace && code === 125 && !error.signal
      && /cannot change root directory/i.test(text) && /operation not permitted/i.test(text);
    return { confirmed, denied, code, reason: confirmed ? "NAMESPACE_CHROOT_CONFIRMED"
      : denied ? "INITIAL_NAMESPACE_EPERM" : error?.code === "ENOENT" ? "PROBE_TOOL_MISSING"
      : /unshare:.*operation not permitted/i.test(text) ? "USER_NAMESPACE_DENIED" : "CHROOT_PROBE_UNPROVED" };
  } catch { return { confirmed: false, denied: false, code: null, reason: "CHROOT_PROBE_UNAVAILABLE" }; }
  finally { if (directory) await scratch.remove(directory); }
}
const scratch = { create: createScratch, remove: rmdir };
export function initialNamespaceChroot(executeProbe = execute, storage = scratch) {
  return probe(false, executeProbe, storage);
}
export function childNamespaceChroot(executeProbe = execute, storage = scratch) {
  return probe(true, executeProbe, storage);
}
