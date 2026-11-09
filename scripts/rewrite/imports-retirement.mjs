/** Retired import policy must not return; see docs/design/rewrite/repositories-imports-acceptance.md. */
import { code, withoutTestModules } from "./notifications-retirement.mjs";

// Admission and lifecycle rules that moved to rbit-repositories must not be redefined
// beside their adapters in the services import modules.
const POLICY = new RegExp([
  String.raw`\bconst\s+(?:MAX_TOKEN_BYTES|MAX_DESCRIPTION_CHARS|INTERRUPTED|STOPPED)\b`,
  "the token is not valid", "importing is disabled on this instance",
  "the repository is no longer available to the importing user",
  String.raw`\btruncate_chars\(`,
].join("|"), "u");
const ADAPTERS = /^crates\/rbit-services\/src\/imports(?:\.rs|\/(?:ports|startup)\.rs)$/u;
// What the copy does with each item moved too; the job keeps only stores, client and Git.
const COPY_POLICY = new RegExp([
  String.raw`\bconst\s+(?:SAVE_EVERY|DEFAULT_LABEL_COLOR|MAX_LABEL_NAME|MAX_LABEL_DESCRIPTION|MAX_MILESTONE_TITLE)\b`,
  String.raw`\bfn\s+(?:is_full_id|non_empty|attributed_text)\b`, String.raw`\bMAX_IMPORT_WARNINGS\b`, String.raw`\bself\.unsaved\b`,
  "Originally by", "its name is not valid here", "its commits were not available", String.raw`\(untitled\)`,
  String.raw`\(no text\)`, "info/lfs/objects/batch", "larger than this instance allows", "already exists here", "could not be imported", "arrived with", "branches are unknown", String.raw`\btruncate_chars\(`,
].join("|"), "u");
const JOB = "crates/rbit-services/src/imports/job.rs";
// The shared models are re-exported by core, never defined there again.
const CORE_MODELS = /\b(?:fn\s+(?:clean_message|truncate_chars|truncate_bytes|label_color)|struct\s+(?:ImportOptions|ImportCounts)|enum\s+(?:ImportPhase|ImportStatus))\b/u;
const CORE = "crates/rbit-core/src/import.rs";

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkImportsRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    const text = withoutTestModules(code(source));
    if (ADAPTERS.test(path) && POLICY.test(text)) {
      violations.push(`${path}: import admission and lifecycle policy belongs to rbit-repositories`);
    }
    if (path === JOB && COPY_POLICY.test(text)) violations.push(`${path}: import copy policy belongs to rbit-repositories`);
    if (path === CORE && CORE_MODELS.test(text)) violations.push(`${path}: import models are re-exported from rbit-repositories`);
  }
  return violations;
}
