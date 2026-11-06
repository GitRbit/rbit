// Preserve compiler-tree uncertainty through observer and report failures. The exact
// original error must reach leased(), whose instanceof check retains the lane.
import { UncertainProcessTreeError } from "../workspace/lease.mjs";

export async function preserveUncertainty(primary, stage, operation) {
  if (primary instanceof UncertainProcessTreeError) primary.retainControl = true;
  try { return await operation(); }
  catch (secondary) {
    if (!(primary instanceof UncertainProcessTreeError)) throw secondary;
    if (secondary === primary) throw primary;
    primary.secondaryFailures ??= [];
    // Error messages may contain private paths or process arguments: retain types only.
    primary.secondaryFailures.push({ stage, errorType: secondary?.name ?? "Error" });
    if (secondary?.observerReport) primary.observerReport = secondary.observerReport;
    throw primary;
  }
}
