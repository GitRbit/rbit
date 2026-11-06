import { lstat, realpath } from "node:fs/promises";
import { resolve, relative, isAbsolute, sep } from "node:path";
// The supplied repository anchor may itself be an explicit host alias. Nothing
// below it may redirect one of our narrow mounts into unrelated host content.
export async function checkedMount(repository, relativePath) {
  if (isAbsolute(relativePath)) throw new Error("MOUNT_ESCAPE");
  const root = await realpath(repository);
  const destination = resolve(root, relativePath);
  const local = relative(root, destination);
  if (!local || local.startsWith(`..${sep}`) || local === ".." || isAbsolute(local)) {
    throw new Error("MOUNT_ESCAPE");
  }
  let current = root;
  for (const part of local.split(sep)) {
    current = resolve(current, part);
    if ((await lstat(current)).isSymbolicLink()) throw new Error("MOUNT_LINK_REFUSED");
  }
  return destination;
}
