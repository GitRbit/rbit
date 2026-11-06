// Pure whole-lane policy. No filesystem, Cargo, process or rotation implementation.
const sha = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const identifier = value => typeof value === "string" && /^[a-z0-9][a-z0-9_-]{0,63}$/.test(value);
const isByteCount = value => Number.isSafeInteger(value) && value >= 0;
const sumBytes = values => {
  const total = values.reduce((a, b) => a + b, 0);
  if (!isByteCount(total)) throw new Error("Invalid byte observation");
  return total;
};
function validateLane(value) {
  if (!value || !identifier(value.id) || !isByteCount(value.bytes) || !isByteCount(value.lastUsed)) {
    throw new Error("Invalid lane observation");
  }
}
function isKnownGraph(graph) { return graph?.confidence === "known" && sha(graph.id) && sha(graph.policyId); }

export function planRetention(input = {}) {
  // Even simulation cannot turn this module into an executor. Activation belongs
  // to a future separately reviewed helper adapter holding the same live lease.
  const result = (kind, reason, extra = {}) => ({ version: 1, apply: false, kind, reason, ...extra });
  const { lease, requested, current, candidate, retired = [], usage, budgetBytes,
    phase = "before-command", pendingCompaction = false, rotationEnabled = false,
    protection, measurements } = input;
  if (!lease || lease.state !== "owned" || lease.tokenVerified !== true) {
    return result("blocked", "LEASE_UNPROVED");
  }
  if (lease.processTreeCertain !== true || candidate?.status === "uncertain") {
    return result("blocked", "PROCESS_TREE_UNCERTAIN");
  }
  try {
    if (!["before-command", "after-success", "candidate-complete"].includes(phase)
      || typeof rotationEnabled !== "boolean" || typeof pendingCompaction !== "boolean"
      || !isByteCount(budgetBytes) || !usage || !isByteCount(usage.namespaceBytes)
      || !(usage.freeBytes === null || isByteCount(usage.freeBytes)) || !Array.isArray(retired)) throw new Error("Invalid observations");
    validateLane(current);
    for (const item of retired) {
      validateLane(item);
      if (item.successful !== true) throw new Error("Unproved retired success");
    }
    if (candidate) validateLane(candidate);
    if (retired.length > 1 || (input.candidates !== undefined)) return result("blocked", "GENERATION_BOUND_EXCEEDED");
    const lanes = [current, ...retired, ...(candidate ? [candidate] : [])];
    if (new Set(lanes.map(item => item.id)).size !== lanes.length) throw new Error("Duplicate lane identity");
    const accounted = sumBytes(lanes.map(item => item.bytes));
    if (accounted > usage.namespaceBytes) throw new Error("Inconsistent byte observation");
    if (current.successful !== true) return result("blocked", "CURRENT_SUCCESS_UNPROVED");
    if (usage.complete !== true) return result("defer", "MEASUREMENT_INCOMPLETE");
    const unattributedBytes = usage.namespaceBytes - accounted;
    const overageBytes = budgetBytes === 0 ? 0 : Math.max(0, usage.namespaceBytes - budgetBytes);
    const report = { currentId: current.id, overageBytes, unattributedBytes, pendingCompaction };
    const decide = (kind, reason, extra = {}) => result(kind, reason, { ...report, ...extra });
    if (budgetBytes === 0) return decide("reuse", "BUDGET_DISABLED");
    if (candidate) {
      if (phase !== "candidate-complete" || candidate.status === "building") return decide("defer", "CANDIDATE_IN_PROGRESS");
      if (!protectedCopies(protection)) return decide("blocked", "RUNTIME_OR_HANDOFF_UNPROVED");
      if (candidate.status === "failed") return decide("discard-candidate", "FAILED_REPLACEMENT", {
        discardIds: [candidate.id], retainIds: [current.id, ...retired.map(item => item.id)], pendingCompaction: true,
      });
      if (candidate.status !== "succeeded" || candidate.executionComplete !== true) return decide("blocked", "CANDIDATE_SUCCESS_UNPROVED");
      if (!isKnownGraph(requested) || !isKnownGraph(candidate.graph) || candidate.graph.id !== requested.id) return decide("blocked", "CANDIDATE_GRAPH_UNPROVED");
      if (!isKnownGraph(current.graph) || candidate.graph.policyId !== current.graph.policyId) return decide("blocked", "CANDIDATE_POLICY_UNPROVED");
      if (!candidate.handoff || candidate.handoff.complete !== true || candidate.handoff.exact !== true) return decide("defer", "EXACT_PUBLICATION_PENDING");
      if (!rotationEnabled) return decide("defer", "ROTATION_DISABLED");
      const older = [current, ...retired].sort((a, b) => b.lastUsed - a.lastUsed || a.id.localeCompare(b.id));
      const warm = older.find(item => sumBytes([candidate.bytes, unattributedBytes, item.bytes]) <= budgetBytes);
      const discardIds = older.filter(item => item !== warm).map(item => item.id);
      const retainedBytes = sumBytes([candidate.bytes, unattributedBytes, ...(warm ? [warm.bytes] : [])]);
      return decide("promote-candidate", "SUCCESSFUL_EXACT_HANDOFF", { currentId: candidate.id,
        retiredId: warm?.id ?? null, discardIds, retainedBytes,
        overageBytes: Math.max(0, retainedBytes - budgetBytes), pendingCompaction: retainedBytes > budgetBytes });
    }
    if (phase === "candidate-complete") return decide("blocked", "CANDIDATE_MISSING");
    if (phase === "after-success") {
      if (overageBytes && rotationEnabled && retired[0]?.bytes > 0) {
        if (!protectedCopies(protection)) return decide("blocked", "RUNTIME_OR_HANDOFF_UNPROVED");
        if (current.handoff?.complete !== true || current.handoff?.exact !== true) return decide("defer", "EXACT_PUBLICATION_PENDING");
        const retainedBytes = usage.namespaceBytes - retired[0].bytes;
        return decide("evict-retired", "COMPLETED_OVERAGE", { discardIds: [retired[0].id],
          retainIds: [current.id], retainedBytes, overageBytes: Math.max(0, retainedBytes - budgetBytes), pendingCompaction: retainedBytes > budgetBytes });
      }
      return decide(overageBytes ? "mark-pending" : "reuse", overageBytes ? "COMPLETED_OVERAGE" : "UNDER_BUDGET", { pendingCompaction: overageBytes > 0 });
    }
    if (!overageBytes) return decide("reuse", "UNDER_BUDGET", { pendingCompaction: false });
    if (!isKnownGraph(requested) || !isKnownGraph(current.graph)) return decide("defer", "UNKNOWN_GRAPH");
    if (requested.id === current.graph.id) return decide("reuse", current.bytes > budgetBytes ? "OVERSIZED_SAME_GRAPH" : "SAME_GRAPH_OVERAGE", { pendingCompaction: true });
    if (!pendingCompaction) return decide("mark-pending", "COMPLETED_OVERAGE", { pendingCompaction: true });
    if (requested.policyId !== current.graph.policyId) return decide("defer", "COMPATIBILITY_POLICY_CHANGED");
    if (!rotationEnabled) return decide("defer", "ROTATION_DISABLED");
    if (!protectedCopies(protection)) return decide("blocked", "RUNTIME_OR_HANDOFF_UNPROVED");
    if (!measurements || measurements.coldCostMeasured !== true || !isByteCount(measurements.estimatedCandidateBytes)
      || measurements.estimatedCandidateBytes === 0 || !isByteCount(measurements.reserveBytes)) return decide("defer", "COLD_COST_OR_HEADROOM_UNMEASURED");
    const requiredFreeBytes = sumBytes([measurements.estimatedCandidateBytes, measurements.reserveBytes]);
    if (usage.freeBytes === null || usage.freeBytes < requiredFreeBytes) return decide("defer", "INSUFFICIENT_HEADROOM", { requiredFreeBytes });
    return decide("prepare-candidate", "MATERIAL_GRAPH_AFTER_OVERAGE", {
      retainIds: [current.id, ...retired.map(item => item.id)], discardIds: [], requiredFreeBytes,
      maximumCandidates: 1, maximumRetiredAfterPromotion: 1,
    });
  } catch { return result("blocked", "INVALID_OBSERVATIONS"); }
}
function protectedCopies(value) {
  return value?.runtimeCopiesIndependent === true && value?.pendingHandoffsSafe === true;
}
