// Cargo's encoded rustflags are the first, mutually exclusive source of extra
// compiler flags. A nonempty fixed lint-only argument prevents inherited/config
// codegen flags from enabling incremental storage outside the shared lane.
export const CONTROLLED_RUSTFLAGS = Object.freeze(["--cap-lints=allow"]);
export const CONTROLLED_ENCODED_RUSTFLAGS = CONTROLLED_RUSTFLAGS.join("\u001f");
const overridden = /^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_BUILD_RUSTFLAGS|CARGO_TARGET_.*_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_INCREMENTAL)$/i;

export function controlledFlags(input) {
  return { ...Object.fromEntries(Object.entries(input).filter(([key]) => !overridden.test(key))),
    CARGO_ENCODED_RUSTFLAGS: CONTROLLED_ENCODED_RUSTFLAGS, CARGO_INCREMENTAL: "0", CARGO_BUILD_INCREMENTAL: "false" };
}

export function hasControlledFlags(env) {
  return env.CARGO_ENCODED_RUSTFLAGS === CONTROLLED_ENCODED_RUSTFLAGS && env.CARGO_INCREMENTAL === "0"
    && env.CARGO_BUILD_INCREMENTAL === "false" && !Object.keys(env).some((key) => overridden.test(key)
      && !["CARGO_ENCODED_RUSTFLAGS", "CARGO_INCREMENTAL", "CARGO_BUILD_INCREMENTAL"].includes(key));
}

export function assertControlledFlags(env) {
  if (!hasControlledFlags(env)) throw new Error("Compiler measurement requires its controlled rustflags/incremental policy");
}
