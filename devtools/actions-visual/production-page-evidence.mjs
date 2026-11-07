// Only bounded observations of the three existing late page assertions are public.
const members = new Set(["overflow", "csp", "browser-faults"]);
const faults = new Set(["unexpected-request", "unexpected-websocket", "unexpected-service-worker", "unexpected-popup",
  "page-error", "console-error", "unexpected-language-redirect", "language-post-refused", "language-post-response-refused",
  "language-post-abort-failed", "language-post-dispose-failed", "other"]);
const refused = () => { throw Error("PAGE_DIAGNOSTIC_REFUSED"); };
const count = value => Number.isInteger(value) && value >= 0 && value <= 65535;
const exact = (value, keys) => value && typeof value === "object" && !Array.isArray(value)
  && Object.keys(value).sort().join(",") === keys;

function sanitize(member, actual) {
  if (member === "overflow") return typeof actual === "boolean" ? actual : "other";
  if (member === "csp") return count(actual) ? actual : actual === undefined ? "missing" : "other";
  if (!Array.isArray(actual)) return { count: "other", categories: ["other"], truncated: false };
  return { count: count(actual.length) ? actual.length : "over-limit",
    categories: [...new Set(actual.slice(0, 128).map(value => faults.has(value) ? value : "other"))].sort(),
    truncated: actual.length > 128 };
}
function valid(value) {
  if (!exact(value, "member,observed") || !members.has(value.member)) return false;
  const observed = value.observed;
  if (value.member === "overflow") return typeof observed === "boolean" || observed === "other";
  if (value.member === "csp") return count(observed) || observed === "missing" || observed === "other";
  return exact(observed, "categories,count,truncated") && (count(observed.count) || ["other", "over-limit"].includes(observed.count))
    && typeof observed.truncated === "boolean" && Array.isArray(observed.categories)
    && observed.categories.length <= faults.size && observed.categories.every(value => faults.has(value))
    && new Set(observed.categories).size === observed.categories.length;
}

/** Stage/record immediately before each original assertion, without changing it. */
export function refreshedPageProbe(stage, record) {
  return (member, actual) => {
    if (!members.has(member)) refused();
    const value = { member, observed: sanitize(member, actual) };
    if (!valid(value)) refused();
    stage(`language-refreshed-page-${member}`);
    record(value);
  };
}

/** One record per assertion, at most three; no raw browser fault payloads. */
export function pageRecord(previous = [], value) {
  if (!Array.isArray(previous) || previous.length > 3 || !previous.every(valid) || !valid(value)) refused();
  const result = previous.filter(item => item.member !== value.member);
  result.push(structuredClone(value));
  if (result.length > 3) refused();
  return result;
}
