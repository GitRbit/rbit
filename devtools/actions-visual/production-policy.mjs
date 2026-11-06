// One synthetic production run, never a caller-selected server or executable.
export const productionOrigin = "http://127.0.0.1:4189";
export const productionRun = "/alice/rocket/actions/runs/1";
export const productionLanguage = "/settings/language";
export const productionExplore = "/explore";
export const productionOverview = "/alice/rocket/actions";
export const productionJob = "/alice/rocket/actions/runs/2/jobs/2";
export const productionSocket = "/tmp/actions-production/control.sock";
export const productionTest = "actions_pages::production_fixture::serve";
export const productionAssetRoot = "/home/runner/work/rbit/rbit/crates/rbit-web/assets";
export const productionWorkspace = "/home/runner/work/rbit/rbit";
export const productionBinary = "/fixture/actions-fixture";
export const fixtureArguments = Object.freeze(["--exact", productionTest, "--ignored", "--nocapture", "--test-threads=1"]);
export const fixtureLimits = Object.freeze({ binaryBytes: 512 * 1024 * 1024, assetBytes: 64 * 1024 * 1024,
  assetFiles: 2048, manifestBytes: 1024 * 1024 });
export function containerMode(args) {
  if (args.length === 0) return "prototype";
  if (args.length === 1 && args[0] === "--production") return "production";
  throw Error("VISUAL_MODE_REFUSED");
}

export function publicAssetPath(path) {
  if (typeof path !== "string" || path.length > 180 || path.includes("\\")) return false;
  const parts = path.split("/");
  if (parts.some(part => !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(part) || part.includes(".."))) return false;
  if (parts.length === 1) return path === "mermaid-frame.html" || /\.(?:js|css|png|avif)$/.test(path);
  return ["themes", "vendor"].includes(parts[0]) && /\.(?:js|css|png|avif|html|txt)$/.test(path);
}

// Repository navigation preloads are actual application GET behavior. Permit only
// these exact synthetic-repository read routes, not arbitrary repository prefixes.
const readPaths = new Set([productionLanguage, productionExplore, productionRun, "/alice/rocket/actions/runs/2", productionJob,
  `${productionJob}/logs`, `${productionJob}/logs/raw`, "/alice/rocket", productionOverview,
  "/alice/rocket/actions/workflows/ci.yml", ...["issues", "pulls", "commits", "branches", "tags", "releases", "activity"]
    .map(name => `/alice/rocket/${name}`), "/manifest.webmanifest", "/sw.js"]);
const languageBodies = new Set(["locale=", "locale=en", "locale=es"]);
const languageContentTypes = new Set(["application/x-www-form-urlencoded",
  "application/x-www-form-urlencoded;charset=utf-8", "application/x-www-form-urlencoded; charset=utf-8"]);
export function allowedProductionRequest(value, method, form = {}) {
  try {
    const url = new URL(value);
    if (url.origin !== productionOrigin || url.username || url.password) return false;
    if (method === "POST") {
      return url.pathname === productionLanguage && !url.search && !url.hash
        && typeof form.body === "string" && languageBodies.has(form.body)
        && form.origin === productionOrigin && typeof form.contentType === "string" && form.contentType.length <= 64
        && languageContentTypes.has(form.contentType.toLowerCase());
    }
    if (!["GET", "HEAD"].includes(method)) return false;
    if (url.pathname === productionLanguage && url.search
      && (url.searchParams.getAll("saved").length !== 1 || url.searchParams.get("saved") !== "1"
        || [...url.searchParams.keys()].some(key => key !== "saved"))) return false;
    if (url.pathname === productionExplore && url.search) return false;
    if (url.pathname === `${productionJob}/logs`
      && (url.searchParams.getAll("step").length !== 1 || !["0", "1"].includes(url.searchParams.get("step"))
        || [...url.searchParams.keys()].some(key => key !== "step"))) return false;
    if ([productionJob, `${productionJob}/logs/raw`].includes(url.pathname) && url.search) return false;
    return url.origin === productionOrigin && !url.username && !url.password
      && (readPaths.has(url.pathname) || (url.pathname.startsWith("/assets/") && publicAssetPath(url.pathname.slice(8))));
  } catch { return false; }
}
