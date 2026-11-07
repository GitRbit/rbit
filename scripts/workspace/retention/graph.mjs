// Scheduling identity only: Cargo remains responsible for dependency freshness.
import { createHash } from "node:crypto";
const hash = value => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const sorted = values => [...new Set(values)].sort();
const sha = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const modes = new Set(["build", "check", "clippy", "test", "nextest"]);
const groups = new Set(["--lib", "--bins", "--examples", "--tests", "--benches", "--doc"]);
const namedTargets = new Set(["--bin", "--example", "--test", "--bench"]);
const flagsOnly = new Set(["--locked", "--frozen", "--offline", "--quiet", "-q", "-v", "-vv",
  "--verbose", "--keep-going", "--no-run", "--no-fail-fast", "--future-incompat-report"]);
const outputValues = new Set(["--color", "--message-format", "--jobs", "-j", "--build-jobs"]);
const nextestValues = new Set(["--profile", "-P", "--filterset", "--filter-expr", "-E", "--test-threads",
  "--retries", "--partition", "--run-ignored", "--status-level", "--final-status-level"]);
const nextestFlags = new Set(["--ignore-default-filter", "--no-capture", "--fail-fast"]);

function policyIdentity(policy, env) {
  if (!policy || !/^[a-f0-9]{20}$/.test(policy.compatibility ?? "")
    || !sha(policy.manifestDigest) || !sha(policy.configurationDigest)) return null;
  // Profile overrides are namespace policy, including inheritance/custom profiles.
  // Hash values without exposing flags, private configuration paths or environment.
  const profileEnvironment = Object.entries(env).filter(([key]) => /^CARGO_PROFILE_[A-Z0-9_]+$/.test(key)
    || key === "CARGO_INCREMENTAL").sort(([a], [b]) => a.localeCompare(b));
  if (profileEnvironment.some(([, value]) => typeof value !== "string")) return null;
  return hash({ version: 1, compatibility: policy.compatibility, manifestDigest: policy.manifestDigest,
    configurationDigest: policy.configurationDigest, profileEnvironment });
}

function clippyLints(args) {
  for (let index = 0; index < args.length; index++) {
    if (/^-[AWDF].+/.test(args[index])) continue;
    if (/^-[AWDF]$/.test(args[index]) && args[index + 1] && !args[index + 1].startsWith("-")) { index++; continue; }
    return false;
  }
  return true;
}

export function normalizeGraph({ command, args = [], env = {}, policy } = {}) {
  if (typeof command !== "string" || !Array.isArray(args) || args.some(arg => typeof arg !== "string" || arg.includes("\0"))) {
    throw new Error("Graph arguments must be strings without NUL");
  }
  if (!env || typeof env !== "object" || Array.isArray(env)) throw new Error("Graph environment must be an object");
  const reasons = new Set();
  if (!modes.has(command)) reasons.add("UNKNOWN_COMMAND");
  const separator = args.indexOf("--");
  const cargo = separator < 0 ? [...args] : args.slice(0, separator);
  const child = separator < 0 ? [] : args.slice(separator + 1);
  if (child.length && !["test", "nextest"].includes(command)
    && !(command === "clippy" && clippyLints(child))) reasons.add("UNKNOWN_COMPILER_ARGUMENTS");
  if (command === "nextest" && !["run", "list"].includes(cargo.shift())) reasons.add("UNKNOWN_NEXTEST_COMMAND");
  const packages = [], excludes = [], features = [], targets = [], selected = [];
  let workspace = false, allFeatures = false, defaultFeatures = true, allTargets = false;
  let profile = command === "test" || command === "nextest" ? "test" : "dev";
  let profileFlag = false, releaseFlag = false;
  for (let index = 0; index < cargo.length; index++) {
    const arg = cargo[index];
    let flag = arg, inline;
    if (arg.startsWith("--") && arg.includes("=")) [flag, inline] = [arg.slice(0, arg.indexOf("=")), arg.slice(arg.indexOf("=") + 1)];
    else if (/^-[pFj].+/.test(arg)) [flag, inline] = [arg.slice(0, 2), arg.slice(2)];
    const value = () => {
      if (inline !== undefined) return inline;
      const next = cargo[index + 1];
      if (next === undefined || (next.startsWith("-") && !((flag === "-j" || flag === "--jobs") && /^-\d+$/.test(next)))) {
        reasons.add("MISSING_OPTION_VALUE"); return "";
      }
      index++; return next;
    };
    if (inline !== undefined && ["--workspace", "--all", "--all-features", "--no-default-features",
      "--all-targets", "--release", "-r", ...groups].includes(flag)) reasons.add("UNKNOWN_OPTION_VALUE");
    if (["-p", "--package"].includes(flag)) packages.push(value());
    else if (flag === "--exclude") excludes.push(value());
    else if (["-F", "--features"].includes(flag)) features.push(...value().split(/[\s,]+/).filter(Boolean));
    else if (["--workspace", "--all"].includes(flag)) workspace = true;
    else if (flag === "--all-features") allFeatures = true;
    else if (flag === "--no-default-features") defaultFeatures = false;
    else if (flag === "--all-targets") allTargets = true;
    else if (groups.has(flag)) selected.push([flag.slice(2), "*"]);
    else if (namedTargets.has(flag)) selected.push([flag.slice(2), value()]);
    else if (flag === "--target") targets.push(value());
    else if (["--release", "-r"].includes(flag)) { profile = "release"; releaseFlag = true; }
    else if (flag === (command === "nextest" ? "--cargo-profile" : "--profile")) {
      const next = value();
      if (profileFlag && profile !== next) reasons.add("CONFLICTING_PROFILE");
      profile = next; profileFlag = true;
    } else if (flagsOnly.has(flag) || (command === "nextest" && nextestFlags.has(flag))) {
      if (inline !== undefined) reasons.add("UNKNOWN_OPTION_VALUE");
    } else if (outputValues.has(flag) || (command === "nextest" && nextestValues.has(flag))) value();
    else if (flag === "--timings") { /* Optional output format does not select artifacts. */ }
    else if (!arg.startsWith("-") && ["test", "nextest"].includes(command)) { /* Test-name filter. */ }
    else reasons.add("UNKNOWN_SWITCH");
  }
  if (selected.some(([kind]) => kind === "doc") && (command !== "test" || allTargets || selected.length > 1)) reasons.add("CONFLICTING_DOC_SELECTION");
  if (profileFlag && releaseFlag) reasons.add("CONFLICTING_PROFILE");
  if (!profile || !/^[A-Za-z0-9_-]+$/.test(profile)) reasons.add("INVALID_PROFILE");
  if (workspace && packages.length) reasons.add("MIXED_PACKAGE_SELECTION");
  if (!workspace && excludes.length) reasons.add("EXCLUDE_WITHOUT_WORKSPACE");
  if ([...packages, ...excludes, ...targets].some(value => !value)) reasons.add("EMPTY_SELECTION");
  const policyId = policyIdentity(policy, env);
  if (!policyId) reasons.add("POLICY_UNOBSERVED");
  if (env.CARGO_BUILD_TARGET !== undefined && (typeof env.CARGO_BUILD_TARGET !== "string" || !env.CARGO_BUILD_TARGET || env.CARGO_BUILD_TARGET.includes("\0"))) reasons.add("INVALID_TARGET_ENVIRONMENT");
  const selection = {
    mode: command, profile,
    packages: { kind: workspace ? "workspace" : packages.length ? "packages" : "default-members", names: sorted(packages), excludes: sorted(excludes) },
    features: { names: sorted(features), all: allFeatures, defaults: allFeatures || defaultFeatures },
    target: targets.length ? { source: "explicit", names: sorted(targets) }
      : typeof env.CARGO_BUILD_TARGET === "string" ? { source: "environment", names: [env.CARGO_BUILD_TARGET] }
      : { source: "configured-default" },
    artifactTargets: allTargets ? { all: true } : { all: false, selected: sorted(selected.map(value => JSON.stringify(value))).map(value => JSON.parse(value)) },
  };
  return { version: 1, id: hash({ selection, policyId }), policyId, selection,
    confidence: reasons.size ? "conservative" : "known", reasons: [...reasons].sort() };
}
