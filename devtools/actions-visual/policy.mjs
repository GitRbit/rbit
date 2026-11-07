// Fixed synthetic review surface: no caller-supplied origin or arbitrary paths.
export const origin = "http://127.0.0.1:4187";
export const image = "mcr.microsoft.com/playwright:v1.65.0-noble@sha256:6e3fb709361521eb27161606e6da6d2674e5e89fa16516cb37d6fdd91372273d";
export const browserVersion = "158.0.8127.18";
export const limits = Object.freeze({ images: 40, bytes: 64 * 1024 * 1024 });
const paths = new Set([
  "/", "/overview", "/run", "/job", "/intent", "/raw-log.txt",
  "/assets/app.css", "/assets/logo-384.avif", "/prototype.css", "/prototype.js",
  ...["base", "overview", "run", "job", "responsive"].map(name => `/styles/${name}.css`),
]);
export function allowedRequest(value, method) {
  try {
    const url = new URL(value);
    return ["GET", "HEAD"].includes(method) && url.origin === origin
      && !url.username && !url.password && paths.has(url.pathname);
  } catch { return false; }
}
export function sandboxProof({ uid, arguments: args, rows }) {
  if (!Number.isInteger(uid) || uid <= 0) throw new Error("SANDBOX_ROOT_USER");
  const forbidden = ["--no-sandbox", "--disable-setuid-sandbox", "--disable-namespace-sandbox",
    "--disable-seccomp-filter-sandbox", "--single-process", "--no-zygote"];
  if (!Array.isArray(args) || !args.length || args.some(arg => typeof arg !== "string"
    || /^--(?:disable[^=]*sandbox|no-sandbox)(?:=|$)/.test(arg)
    || forbidden.some(flag => arg.split("=")[0] === flag))) {
    throw new Error("SANDBOX_DISABLED_FLAGS");
  }
  const yes = label => rows.some(row => row.label.toLowerCase() === label
    && /^(yes|enabled)$/i.test(row.value.trim()));
  // Exact pinned Chromium158 linuxHandler labels (see README source link).
  const namespace = rows.some(row => row.label.toLowerCase() === "layer 1 sandbox"
    && row.value.trim() === "Namespace");
  if (!namespace || !yes("pid namespaces") || !yes("network namespaces") || !yes("seccomp-bpf sandbox")) {
    throw new Error("SANDBOX_UNPROVED");
  }
  return { nonroot: true, forbiddenFlagsAbsent: true, namespace: true, seccompBpf: true };
}
export function artifactBudget(entries) {
  const images = entries.filter(entry => entry.name.endsWith(".png")).length;
  const bytes = entries.reduce((total, entry) => total + entry.bytes, 0);
  if (images > limits.images || bytes > limits.bytes) throw new Error("ARTIFACT_BUDGET");
  return { images, bytes };
}
export function cases() {
  const result = [];
  const add = (screen, theme, width, query = {}, suffix = "") => {
    const params = new URLSearchParams({ theme, ...query });
    result.push({ id: `${screen}-${theme}-${width}${suffix}`, screen, theme,
      viewport: { width, height: width === 360 ? 800 : 1000 }, path: `/${screen}?${params}` });
  };
  for (const screen of ["overview", "run", "job"]) {
    for (const theme of ["dna-day", "dna-dark"]) {
      for (const width of [1440, 360]) add(screen, theme, width);
      add(screen, theme, 360, { dir: "rtl", long: "1" }, "-rtl-long");
    }
  }
  for (const [screen, state, viewer] of [
    ["run", "approval", "writer"], ["run", "approval", "reader"],
    ["run", "deployment", "reviewer"], ["run", "deployment", "writer"],
    ["overview", "loading", "reader"], ["overview", "empty", "writer"],
    ["overview", "error", "reader"], ["overview", "denied", "reader"],
    ["job", "disconnected", "reader"], ["job", "expired", "reader"],
    ["job", "truncated", "reader"],
  ]) add(screen, "dna-day", 360, { state, viewer }, `-${state}-${viewer}`);
  return result;
}
