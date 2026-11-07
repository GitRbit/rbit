// Fixed diagnostic vocabulary: no URLs, header text, bodies or error details escape.
const phases = new Set(["spanish", "refreshed"]);
const kinds = Object.freeze({ status: "status", "content-language": "locale", url: "boolean",
  "document-language": "locale", "selected-language": "locale", "document-marker": "boolean" });
const locales = new Set(["en", "es", "en-XA", "ar-XB"]);
const keys = "expected,member,observed,phase";
const refused = () => { throw Error("LANGUAGE_DIAGNOSTIC_REFUSED"); };

function safeValue(kind, value) {
  if (kind === "status") return Number.isInteger(value) && value >= 100 && value <= 599 ? value : "other";
  if (kind === "boolean") return typeof value === "boolean" ? value : "other";
  return locales.has(value) ? value : value === null ? "missing" : "other";
}

function valid(value) {
  if (!value || typeof value !== "object" || Object.keys(value).sort().join(",") !== keys
    || !phases.has(value.phase) || !Object.hasOwn(kinds, value.member)) return false;
  const kind = kinds[value.member];
  const observed = value.observed === "other" || (kind === "locale" && value.observed === "missing")
    || safeValue(kind, value.observed) === value.observed;
  const expected = kind === "status" ? value.expected === 200 : kind === "boolean"
    ? value.expected === true : value.expected === "en" || value.expected === "es";
  return observed && expected;
}

/** Record before the original assertion, so a failing member retains bounded evidence. */
export function languageProbe(phase, stage, record) {
  if (!phases.has(phase)) refused();
  return (member, actual, expected) => {
    if (!Object.hasOwn(kinds, member)) refused();
    const value = { phase, member, observed: safeValue(kinds[member], actual), expected };
    if (!valid(value)) refused();
    stage(`language-${phase}-explore-${member}`);
    record(Object.freeze(value));
  };
}

/** The report retains at most six members from the latest fixed navigation phase. */
export function languageRecord(previous = [], value) {
  if (!Array.isArray(previous) || previous.length > 6 || !previous.every(valid) || !valid(value)) refused();
  const next = previous.filter(item => item.phase === value.phase && item.member !== value.member);
  next.push(Object.freeze({ ...value }));
  if (next.length > 6) refused();
  return next;
}
