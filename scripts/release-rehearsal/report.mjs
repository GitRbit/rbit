// Timed steps and the rehearsal report (JSON for tools, Markdown for people).
import { writeFile } from "node:fs/promises";

export class Report {
  constructor(meta) {
    this.meta = { ...meta, started: new Date().toISOString() };
    this.steps = [];
  }

  /** Runs `task`, recording its duration, its checks and whether it threw. */
  async step(name, task) {
    const started = performance.now();
    const entry = { name, ok: true, seconds: 0, checks: [], notes: [] };
    this.steps.push(entry);
    process.stdout.write(`▸ ${name}\n`);
    try {
      const result = await task(entry);
      if (result?.results) entry.checks.push(...result.results);
      return result;
    } catch (error) {
      entry.ok = false;
      entry.error = error.message.split("\n").slice(0, 12).join("\n");
      return undefined;
    } finally {
      entry.seconds = Math.round((performance.now() - started) / 100) / 10;
      if (entry.checks.some((check) => !check.ok && !check.expectedLoss)) entry.ok = false;
      const failed = entry.checks.filter((check) => !check.ok);
      process.stdout.write(`  ${entry.ok ? "ok" : "FAILED"} in ${entry.seconds} s${failed.length ? `, ${failed.length} check(s) failed` : ""}${entry.error ? `: ${entry.error.split("\n")[0]}` : ""}\n`);
    }
  }

  get ok() {
    return this.steps.every((step) => step.ok);
  }

  markdown() {
    const lines = [`# rbit release rehearsal (${this.meta.mode})`, ""];
    for (const [key, value] of Object.entries(this.meta)) lines.push(`- ${key}: \`${value}\``);
    lines.push("", "| Step | Result | Seconds |", "| --- | --- | ---: |");
    for (const step of this.steps) lines.push(`| ${step.name} | ${step.ok ? "passed" : "**failed**"} | ${step.seconds} |`);
    for (const step of this.steps) {
      if (!step.checks.length && !step.notes.length && !step.error) continue;
      lines.push("", `## ${step.name}`, "");
      if (step.error) lines.push(`Error: ${step.error}`, "");
      for (const note of step.notes) lines.push(`- ${note}`);
      for (const check of step.checks) {
        const mark = check.ok ? "✓" : check.expectedLoss ? "lost" : "✗";
        lines.push(`- ${mark} ${check.name}${check.detail ? `: ${String(check.detail).replaceAll("\n", " ")}` : ""}`);
      }
    }
    return `${lines.join("\n")}\n`;
  }

  async write(jsonPath, markdownPath) {
    await writeFile(jsonPath, `${JSON.stringify({ meta: this.meta, ok: this.ok, steps: this.steps }, null, 2)}\n`);
    await writeFile(markdownPath, this.markdown());
  }
}
