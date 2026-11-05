// Stable per-worktree development ports, allocated under the same management lease.
import { readFile, rename, rm, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { createServer } from "node:net";
import { join } from "node:path";
import { leased } from "./lease.mjs";
import { guarded, managedDirectory } from "./paths.mjs";

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
    const file = await guarded(ctx.root, join(ctx.root, ".worktree-state", "ports.json"));
    let ports = {};
    try { ports = JSON.parse(await readFile(file, "utf8")); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    if (!ports || typeof ports !== "object" || Array.isArray(ports) || Object.values(ports).some((value) => !Number.isInteger(value) || value < 3100 || value >= 4000)) throw new Error("Invalid development port registry");
    if (ports[ctx.id]) return ports[ctx.id];
    for (let candidate = 3100; candidate < 4000; candidate++) {
      if (!Object.values(ports).includes(candidate) && await available(candidate)) {
        ports[ctx.id] = candidate;
        const temporary = await guarded(ctx.root, `${file}.${randomUUID()}.tmp`);
        try {
          await writeFile(temporary, JSON.stringify(ports, null, 2), { flag: "wx" });
          await guarded(ctx.root, file);
          await rename(temporary, file);
        } finally { await rm(temporary, { force: true }); }
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
    // Inherited TOML may enable SSH on2222; managed developer servers opt out unless
    // explicitly enabled with independently chosen bind/public ports by their owner.
    RBIT_SSH__ENABLED: input.RBIT_SSH__ENABLED ?? "false",
  };
}
