// Compiler paths stay private; reports receive only the candidate binary hash/policy.
import { execFileSync } from "node:child_process";
import { readFileSync, realpathSync, statSync } from "node:fs";
import { isAbsolute, join } from "node:path";
import { createHash } from "node:crypto";

export class CompilerIdentity {
  #path;
  constructor(path, provenance) {
    this.#path = path;
    this.provenance = Object.freeze(provenance);
  }
  observerArguments() { return this.#path ? ["-ExpectedCompilerPath", this.#path] : []; }
}

/** A custom driver is supported only if its real compiler is the reported sysroot binary. */
export function compilerIdentity(ctx, env, { run = execFileSync, canonical = realpathSync,
  stat = statSync, read = readFileSync, platform = process.platform } = {}) {
  const provenance = { policy: "actual-executable-v1", resolution: "sysroot-bin", verifiedCandidate: false,
    customDriver: !["rustc", "rustc.exe"].includes(env.RUSTC || "rustc"), binarySha256: null, reason: null };
  try {
    const sysroot = run(env.RUSTC || "rustc", ["--print", "sysroot"], {
      cwd: ctx.checkout, env, encoding: "utf8", windowsHide: true, maxBuffer: 1024 * 1024, timeout: 15_000,
    }).trim();
    if (!isAbsolute(sysroot) || /[\r\n\0]/.test(sysroot)) throw new Error("Invalid sysroot");
    const path = canonical(join(sysroot, "bin", platform === "win32" ? "rustc.exe" : "rustc"));
    if (!stat(path).isFile()) throw new Error("Compiler candidate unavailable");
    provenance.binarySha256 = createHash("sha256").update(read(path)).digest("hex");
    provenance.verifiedCandidate = true;
    return new CompilerIdentity(path, provenance);
  } catch {
    // Query failures, nonstandard custom layouts and inaccessible files fail closed for
    // memory only. Never publish exception messages, sysroots or compiler paths.
    provenance.reason = provenance.customDriver ? "custom-rustc-memory-identity-unverified" : "compiler-memory-identity-unverified";
    return new CompilerIdentity(null, provenance);
  }
}
