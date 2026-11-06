// Census rows are transient. Only proved descendants and anonymous contamination counts
// enter reports; command lines, executable paths and unrelated process identities never do.
const integer = (value) => Number.isSafeInteger(value) && value >= 0;
const key = (row) => `${row.pid}@${row.createdMs}`;

export class OwnedMemory {
  constructor(root) { this.root = root; this.known = new Set(); this.frames = []; this.reasons = new Set(); this.anchored = false; }

  observe(frame) {
    if (!integer(frame.startMs) || !integer(frame.endMs) || frame.endMs < frame.startMs || !Array.isArray(frame.rows)) {
      this.reasons.add("invalid-sampler-frame"); return;
    }
    if (frame.endMs < this.root.beforeMs) return;
    const rows = new Map();
    for (const row of frame.rows) {
      if (!integer(row.pid) || !integer(row.parentPid) || !integer(row.createdMs) || !["cargo", "rustc", "other"].includes(row.kind)
        || (row.kind !== "other" && (!integer(row.workingSetBytes) || !integer(row.privateBytes))) || rows.has(row.pid)) {
        this.reasons.add("invalid-process-row"); continue;
      }
      rows.set(row.pid, row);
    }
    const root = rows.get(this.root.pid);
    if (root && !this.anchored) {
      if (root.parentPid === this.root.parentPid && root.createdMs >= this.root.beforeMs - 100 && root.createdMs <= this.root.afterMs + 100) {
        this.known.add(key(root)); this.anchored = true;
      } else this.reasons.add("root-identity-mismatch");
    }
    const owned = new Map([...rows.values()].filter((row) => this.known.has(key(row))).map((row) => [row.pid, row]));
    for (;;) {
      const before = owned.size;
      for (const row of rows.values()) {
        const parent = owned.get(row.parentPid);
        if (parent && row.createdMs >= parent.createdMs && !owned.has(row.pid)) { owned.set(row.pid, row); this.known.add(key(row)); }
      }
      if (owned.size === before) break;
    }
    const compilers = [...owned.values()].filter((row) => row.kind !== "other").map((row) => {
      const crateName = typeof row.crateName === "string" && /^[A-Za-z_][A-Za-z0-9_]{0,127}$/.test(row.crateName) ? row.crateName : null;
      const targetTriple = typeof row.targetTriple === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(row.targetTriple) ? row.targetTriple : null;
      return { pid: row.pid, parentPid: row.parentPid, createdMs: row.createdMs, kind: row.kind, crateName, targetTriple,
        workingSetBytes: row.workingSetBytes, privateBytes: row.privateBytes };
    });
    const externalCompilerCount = [...rows.values()].filter((row) => row.kind !== "other" && !owned.has(row.pid)).length;
    if (externalCompilerCount) this.reasons.add("external-compiler-observed");
    if (root && this.anchored && !this.known.has(key(root))) this.reasons.add("root-pid-reused");
    this.frames.push({ elapsedMs: frame.endMs - this.root.beforeMs, windowMs: frame.endMs - frame.startMs, owned: compilers, externalCompilerCount });
  }

  finish(extraReasons = []) {
    for (const reason of extraReasons) this.reasons.add(reason);
    if (!this.anchored) this.reasons.add("cargo-root-not-observed");
    const peaks = new Map();
    for (const frame of this.frames) for (const row of frame.owned) {
      const prior = peaks.get(key(row)) ?? { pid: row.pid, createdMs: row.createdMs, kind: row.kind, crateName: row.crateName, targetTriple: row.targetTriple, workingSetBytes: 0, privateBytes: 0 };
      prior.crateName ??= row.crateName; prior.targetTriple ??= row.targetTriple;
      prior.workingSetBytes = Math.max(prior.workingSetBytes, row.workingSetBytes);
      prior.privateBytes = Math.max(prior.privateBytes, row.privateBytes);
      peaks.set(key(row), prior);
    }
    if (![...peaks.values()].some((row) => row.kind === "rustc")) this.reasons.add("rustc-not-observed-short-lived-processes-may-be-missed");
    return { frames: this.frames, perProcessSampledPeaks: [...peaks.values()], reasons: [...this.reasons],
      sampledPeaksAreLowerBounds: true, completeLifetimeCoverage: false, atomicSnapshot: false,
      concurrentTreePeakBytes: null, scope: "owned Cargo/rustc only; per-process peaks are never summed" };
  }
}
