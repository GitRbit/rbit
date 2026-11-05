// Stable per-worktree development ports, allocated under the same management lease.
import { readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join } from "node:path";
import { leased } from "./lease.mjs";
import { managedDirectory } from "./paths.mjs";

function available(port) {
  return new Promise((done) => {
    const server = createServer();
    server.once("error", () => done(false));
    server.listen(port, "127.0.0.1", () => server.close(() => done(true)));
  });
}

export async function runtimeEnvironment(ctx, input = process.env) {
  const data = await managedDirectory(ctx, join(ctx.state, "data"));
  await managedDirectory(ctx, ctx.cache);
  const port = await leased(join(ctx.cache, ".lease"), async () => {
    const file = join(ctx.root, ".worktree-state", "ports.json");
    let ports = {};
    try { ports = JSON.parse(await readFile(file, "utf8")); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    if (ports[ctx.id]) return ports[ctx.id];
    for (let candidate = 3100; candidate < 4000; candidate++) {
      if (!Object.values(ports).includes(candidate) && await available(candidate)) {
        ports[ctx.id] = candidate;
        await writeFile(file, JSON.stringify(ports, null, 2));
        return candidate;
      }
    }
    throw new Error("No development port available between 3100 and 3999");
  });
  return {
    ...input,
    RBIT_STORAGE__DATA_DIR: input.RBIT_STORAGE__DATA_DIR ?? data,
    RBIT_SERVER__BIND: input.RBIT_SERVER__BIND ?? `127.0.0.1:${port}`,
    RBIT_SERVER__PUBLIC_URL: input.RBIT_SERVER__PUBLIC_URL ?? `http://localhost:${port}`,
    RBIT_MAIL__TRANSPORT: input.RBIT_MAIL__TRANSPORT ?? "log",
  };
}
