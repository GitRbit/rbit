// Fixed probe matrix; this tool never accepts arbitrary Cargo packages or commands.
export const PACKAGE = "rbit-render-probe";
export const MODES = ["inheritance", "segments", "blocks"];
export const COUNTS = [1, 8, 32, 96];
const permutations = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

export function options(argv, platform = process.platform) {
  const parsed = { command: "", profile: "", jobs: "", rounds: "6", "sample-ms": "250", memory: platform === "win32" ? "windows" : "none", output: "" };
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index].startsWith("--") ? argv[index].slice(2) : "";
    if (!Object.hasOwn(parsed, name) || !argv[index + 1] || argv[index + 1].startsWith("--")) throw new Error("Use --command check|build --profile dev|release --jobs N --output FILE [--rounds MULTIPLE_OF_6] [--sample-ms N] [--memory windows|none]");
    parsed[name] = argv[index + 1];
  }
  if (!["check", "build"].includes(parsed.command) || !["dev", "release"].includes(parsed.profile) || !parsed.output) throw new Error("command, profile, jobs and output must be explicit");
  for (const name of ["jobs", "rounds", "sample-ms"]) {
    if (!/^\d+$/.test(parsed[name]) || !Number.isSafeInteger(Number(parsed[name]))) throw new Error(`${name} must be an integer`);
    parsed[name] = Number(parsed[name]);
  }
  if (parsed.jobs < 1 || parsed.jobs > 64 || parsed.rounds < 6 || parsed.rounds > 60 || parsed.rounds % 6 || parsed["sample-ms"] < 50 || parsed["sample-ms"] > 5000) throw new Error("jobs:1–64; rounds:6–60 in multiples of6; sample-ms:50–5000");
  if (!["windows", "none"].includes(parsed.memory) || (parsed.memory === "windows" && platform !== "win32")) throw new Error("Windows memory sampling requires Windows; select --memory none elsewhere");
  return parsed;
}

export function plan(rounds) {
  return Array.from({ length: rounds }, (_, round) => (round % 2 ? [...COUNTS].reverse() : COUNTS)
    .flatMap((count) => permutations[round % 6].map((index, position) => ({ round, position, count, mode: MODES[index] })))).flat();
}

export function cargoArgs(job, config) {
  return ["--package", PACKAGE, "--lib", "--locked", "--no-default-features", "--features", `${job.mode},count-${job.count}`, "--profile", config.profile, "--jobs", String(config.jobs)];
}

export function cleanArgs(config) {
  return ["clean", "--package", PACKAGE, "--profile", config.profile, "--locked"];
}

export function measurementEnv(input) {
  return { ...input, RUSTC: input.RUSTC || "rustc", CARGO_BUILD_RUSTC: input.RUSTC || "rustc", RBIT_WORKSPACE_JSON: "1", CARGO_INCREMENTAL: "0", RUSTC_WRAPPER: "", RUSTC_WORKSPACE_WRAPPER: "",
    CARGO_BUILD_RUSTC_WRAPPER: "", CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER: "", RBIT_BUILD_CACHE_MAX_BYTES: "0" };
}
