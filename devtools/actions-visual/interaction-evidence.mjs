// Fixed synthetic check identifiers only; never retain browser errors or page values.
const stages = new Set([
  "javascript.context", "javascript.page", "javascript.job-navigation",
  "javascript.search-navigation", "javascript.match-next", "javascript.match-feedback",
  "javascript.containment", "javascript.close",
  "approval.writer", "approval.reader", "deployment.reviewer", "deployment.writer",
  "logs.initial-lines", "logs.simulate-focus", "logs.scroll-setup", "logs.simulate-output",
  "logs.paused-lines", "logs.paused-focus", "logs.paused-scroll", "logs.follow-click",
  "logs.follow-count", "logs.follow-last-line", "logs.copy-click", "logs.copy-feedback",
  "logs.disclosure-focus", "logs.disclosure-space", "logs.disclosure-closed",
  "logs.disclosure-enter", "logs.disclosure-open",
  "nojs.context", "nojs.page", "nojs.filter-navigation", "nojs.branch-fill",
  "nojs.filter-submit", "nojs.branch-query", "nojs.job-navigation", "nojs.follow-hidden",
  "nojs.earlier-click", "nojs.earlier-start", "nojs.disclosure-focus",
  "nojs.disclosure-space", "nojs.disclosure-closed", "nojs.containment", "nojs.close",
]);
function code(error) {
  if (error?.code === "ERR_ASSERTION") return "ASSERTION_FAILED";
  const text = typeof error?.message === "string" ? error.message.slice(0, 32768) : "";
  if (error?.name === "TimeoutError" || /timeout \d+ms exceeded/i.test(text)) return "TIMEOUT";
  if (/target page, context or browser has been closed|session closed|target closed/i.test(text)) return "TARGET_CLOSED";
  if (/strict mode violation/i.test(text)) return "STRICT_SELECTOR";
  if (/net::ERR_ABORTED/i.test(text)) return "NAVIGATION_ABORTED";
  if (/protocol error/i.test(text)) return "PROTOCOL_ERROR";
  return "OPERATION_FAILED";
}
export class InteractionFailure extends Error {
  constructor(stage, error) {
    super("INTERACTION_CHECK_FAILED");
    this.evidence = Object.freeze({ stage: stages.has(stage) ? stage : "unknown", code: code(error) });
  }
}
export async function interactionStep(stage, operation) {
  try { return await operation(); }
  catch (error) { throw error instanceof InteractionFailure ? error : new InteractionFailure(stage, error); }
}
