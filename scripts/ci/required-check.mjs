import { appendFile } from "node:fs/promises";
import { evaluateResults, summaryMarkdown, annotations } from "./required-results.mjs";

const report = evaluateResults(process.env.CI_NEEDS);
try {
  if (!process.env.GITHUB_STEP_SUMMARY) throw new Error("Job summary output is unavailable");
  await appendFile(process.env.GITHUB_STEP_SUMMARY, summaryMarkdown(report, process.env));
  for (const annotation of annotations(report)) console.log(annotation);
  console.log(`CI required: ${report.passed ? "passed" : "blocked"} (${report.rows.filter((row) => row.result === "success").length}/${report.rows.length}).`);
  if (!report.passed) process.exitCode = 1;
} catch {
  console.error("::error title=CI required summary::Unable to publish the required-check summary.");
  process.exitCode = 1;
}
