// Read-only review host: loopback binding and exact asset/route allowlists only.
import { createServer } from "node:http";
import { readFile, realpath, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { context, render } from "./render.mjs";
import { rawLog } from "./fixtures.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repository = resolve(here, "../../../..");
export async function assets(assetRoot = join(repository, "crates/rbit-web/assets")) {
  const root = await realpath(assetRoot);
  const app = await realpath(join(root, "app.css"));
  if (dirname(app) !== root || !(await stat(app)).isFile()) throw new Error("A public built app.css is required");
  const result = new Map([
    ["/assets/app.css", { type: "text/css; charset=utf-8", body: await readFile(app) }],
    ["/assets/logo-384.avif", { type: "image/avif", body: await readFile(join(repository, "crates/rbit-web/assets/logo-384.avif")) }],
    ["/prototype.css", { type: "text/css; charset=utf-8", body: await readFile(join(here, "prototype.css")) }],
    ["/prototype.js", { type: "text/javascript; charset=utf-8", body: await readFile(join(here, "prototype.js")) }],
  ]);
  for (const name of ["base", "overview", "run", "job", "responsive"]) {
    result.set(`/styles/${name}.css`, {
      type: "text/css; charset=utf-8",
      body: await readFile(join(here, "styles", `${name}.css`)),
    });
  }
  return result;
}
const intents = new Set(["dispatch", "toggle-workflow", "cancel-run", "rerun-failed", "rerun-all", "rerun-job", "approve-run", "approve-deployment", "reject-deployment", "artifact-zip", "artifact-tar", "view-commit", "workflow-files", "explore", "notifications", "code", "issues", "pulls", "settings"]);
export function handler(publicAssets) {
  return (request, response) => {
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("Content-Security-Policy", "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self'; connect-src 'none'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'");
    response.setHeader("X-Content-Type-Options", "nosniff");
    response.setHeader("Referrer-Policy", "no-referrer");
    if (!["GET", "HEAD"].includes(request.method)) { response.writeHead(405, { Allow: "GET, HEAD" }); response.end("Read-only design preview"); return; }
    try {
      const url = new URL(request.url, "http://127.0.0.1");
      const asset = publicAssets.get(url.pathname);
      let content, type = "text/html; charset=utf-8";
      if (asset) { content = asset.body; type = asset.type; }
      else if (url.pathname === "/raw-log.txt") { content = rawLog(); type = "text/plain; charset=utf-8"; }
      else if (url.pathname === "/intent") {
        const screen = ["overview", "run", "job"].includes(url.searchParams.get("return")) ? url.searchParams.get("return") : "overview";
        const intent = url.searchParams.get("intent");
        if (!intents.has(intent)) throw new Error("Unknown review intent");
        const ctx = context(`/${screen}`, url.searchParams);
        const write = ["dispatch", "toggle-workflow", "cancel-run", "rerun-failed", "rerun-all", "rerun-job", "approve-run"].includes(intent);
        const review = ["approve-deployment", "reject-deployment"].includes(intent);
        const allowed = write ? ctx.viewer === "writer" : review ? ctx.viewer === "reviewer" : true;
        content = render(ctx, allowed ? `Preview action: ${intent.replaceAll("-", " ")}.` : "This preview viewer is not authorized for that action.");
      } else if (["/", "/overview", "/run", "/job"].includes(url.pathname)) content = render(context(url.pathname, url.searchParams));
      else { response.writeHead(404); response.end("Not a prototype route"); return; }
      response.writeHead(200, { "Content-Type": type }); response.end(request.method === "HEAD" ? undefined : content);
    } catch { response.writeHead(400, { "Content-Type": "text/plain; charset=utf-8" }); response.end("Unknown prototype variant"); }
  };
}
export async function preview({ port = 0, assetRoot } = {}) {
  const server = createServer(handler(await assets(assetRoot)));
  await new Promise((done, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", done); });
  return server;
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const args = process.argv.slice(2); let port = 0, assetRoot;
  for (let index = 0; index < args.length; index += 2) {
    if (args[index] === "--port" && /^\d+$/.test(args[index + 1] ?? "")) port = Number(args[index + 1]);
    else if (args[index] === "--asset-root" && args[index + 1]) assetRoot = resolve(args[index + 1]);
    else throw new Error("Use --port N --asset-root BUILT_PUBLIC_ASSETS");
  }
  if (port > 65535) throw new Error("Invalid port");
  const server = await preview({ port, assetRoot });
  process.stdout.write(`Actions design preview: http://127.0.0.1:${server.address().port}/overview\n`);
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => server.close());
}
