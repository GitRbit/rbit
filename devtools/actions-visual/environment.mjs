// Container-local preflight: known kernel fields and owned HOME, never an env dump.
import { access, lstat, mkdir, open, readFile, realpath, unlink } from "node:fs/promises";
import { constants } from "node:fs";
import { randomUUID } from "node:crypto";
import { isAbsolute, join, relative, sep } from "node:path";
export const ownedHome = "/tmp/visual-home";

export async function prepareHome(directory, uid) {
  const result = { created: false, owned: false, writable: false };
  try { await mkdir(directory, { mode: 0o700 }); result.created = true; }
  catch (error) { if (error.code !== "EEXIST") return result; }
  try {
    const info = await lstat(directory);
    result.owned = info.isDirectory() && !info.isSymbolicLink() && info.uid === uid && (info.mode & 0o077) === 0;
    if (!result.owned) return result;
    const probe = join(directory, `write-probe-${randomUUID()}`);
    let handle;
    try {
      handle = await open(probe, "wx+", 0o600);
      await handle.writeFile("probe");
      const bytes = Buffer.alloc(5);
      const { bytesRead } = await handle.read(bytes, 0, 5, 0);
      result.writable = bytesRead === 5 && bytes.toString("utf8") === "probe";
    } finally {
      if (handle) { await handle.close(); await unlink(probe); }
    }
  } catch { result.writable = false; }
  return result;
}

export function integerSetting(text, maximum = 2147483647) {
  if (typeof text !== "string" || !/^\d{1,10}\s*$/.test(text)) return null;
  const value = Number(text.trim());
  return Number.isSafeInteger(value) && value <= maximum ? value : null;
}
export function processSettings(text) {
  const field = name => text.match(new RegExp(`^${name}:\\s*([^\\r\\n]+)$`, "m"))?.[1];
  const zero = name => {
    const value = field(name);
    return /^[0-9a-f]{1,16}$/i.test(value ?? "") ? BigInt(`0x${value}`) === 0n : null;
  };
  return { noNewPrivileges: integerSetting(field("NoNewPrivs"), 1), seccomp: integerSetting(field("Seccomp"), 2),
    seccompFilters: integerSetting(field("Seccomp_filters")),
    effectiveCapabilitiesZero: zero("CapEff"), boundingCapabilitiesZero: zero("CapBnd"),
    permittedCapabilitiesZero: zero("CapPrm"), ambientCapabilitiesZero: zero("CapAmb") };
}
async function knownText(path) {
  try { return (await readFile(path, "utf8")).slice(0, 16384); } catch { return ""; }
}
export async function environment(expectedExecutable) {
  const uid = process.getuid?.();
  const home = await prepareHome(ownedHome, uid);
  // This path is part of the fixed ephemeral container contract, not caller input.
  process.env.HOME = ownedHome;
  const executable = { exists: false, executable: false, bundled: false };
  try {
    const [target, root] = await Promise.all([realpath(expectedExecutable), realpath("/ms-playwright")]);
    const local = relative(root, target);
    executable.bundled = !!local && !isAbsolute(local) && local !== ".." && !local.startsWith(`..${sep}`);
    if (executable.bundled) {
      executable.exists = (await lstat(target)).isFile();
      await access(target, constants.X_OK);
      executable.executable = true;
    }
  } catch { /* Only booleans are retained; no OS error text or executable path. */ }
  const status = processSettings(await knownText("/proc/self/status"));
  const kernel = {};
  for (const [key, path, maximum] of [
    ["maxUserNamespaces", "/proc/sys/user/max_user_namespaces", 2147483647],
    ["unprivilegedUsernsClone", "/proc/sys/kernel/unprivileged_userns_clone", 1],
    ["apparmorRestrictUnprivilegedUserns", "/proc/sys/kernel/apparmor_restrict_unprivileged_userns", 1],
  ]) kernel[key] = integerSetting(await knownText(path), maximum);
  return { uid, home, executable, status, kernel };
}

export function namespaceStartupReady(facts) {
  const status = facts?.status;
  return Number.isInteger(facts?.uid) && facts.uid > 0 && status?.noNewPrivileges === 1 && status.seccomp === 2
    && status.effectiveCapabilitiesZero === true && status.boundingCapabilitiesZero === true
    && status.permittedCapabilitiesZero === true && status.ambientCapabilitiesZero === true;
}
