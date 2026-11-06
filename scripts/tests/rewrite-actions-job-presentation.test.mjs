import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";

const root = new URL("../../crates/rbit-web/", import.meta.url);
const read = path => readFile(new URL(path, root), "utf8").then(text => text.replaceAll("\r\n", "\n"));
async function expand(path) {
  let html = await read(`templates/${path}`);
  for (const match of [...html.matchAll(/{% include "(actions\/job\/[^\"]+)" %}/g)]) {
    html = html.replace(match[0], await expand(match[1]));
  }
  return html;
}

test("job presentation keeps the exact bounded line-rendering program and exposes real metadata", async () => {
  const html = await expand("actions/job.html");
  const body = await read("templates/actions/step_body.html");
  const lines = body.slice(body.indexOf('  <div class="overflow-x-auto">'), body.indexOf("  {% if log.truncated %}"));
  assert.equal(createHash("sha256").update(lines).digest("hex"),
    "6e800d00ecaa15bd0413ca345305f8cca69b76175ebb928c714a9dbe4484106b");
  assert.match(html, /<header class="box overflow-hidden">/);
  assert.match(html, /<dl /);
  assert.match(html, /<bdi>{{ labels }}<\/bdi>/);
  assert.match(html, /break-words text-\[13px\] font-medium">{{ step.name }}/);
  assert.doesNotMatch(html, /data-simulate|live-toggle|Copy visible|Search logs|All checks passed/);
});

test("presentation keeps bounded service reads, native links and stable step identity", async () => {
  const view = await read("src/views/actions/job.rs");
  assert.match(view, /JOB_PAGE_STEP_LINES: usize = 1000/);
  assert.match(view, /JOB_PAGE_STEP_BYTES: usize = 256 \* 1024/);
  const route = await read("src/routes/actions/jobs.rs");
  assert.equal(route.match(/actions\.job_log_detail\(/g)?.length, 1);
  assert.match(route, /detail\.steps\.iter\(\)\.zip\(read\.logs\)/);
  assert.match(route, /loaded\.ctx\.can_write_content\(\) && finished && detail\.run\.status\.is_done\(\)/);
  const html = await expand("actions/job.html");
  for (const contract of ['hx-trigger="every 3s"', 'hx-select="#job"', 'hx-swap="outerHTML"',
    'data-step="{{ step.number }}"', 'data-state="{{ step.look.key }}"', 'method="post" action="{{ rerun_url }}"',
    'href="{{ raw_url }}" hx-boost="false"']) assert.ok(html.includes(contract), contract);
  const body = await read("templates/actions/step_body.html");
  assert.match(body, /line\.html\|safe/);
  assert.match(body, /data-line="{{ line.number }}"/);
  assert.match(body, /log\.more_before/);
  assert.match(body, /log\.truncated/);
  assert.match(body, /log\.expired/);
});

test("both refreshed roots carry locale direction and plain localized labels", async () => {
  for (const name of ["job", "step_log"]) {
    const html = await read(`templates/actions/${name}.html`);
    assert.match(html, /lang="{{ page.locale.tag\(\) }}" dir="{{ page.locale.direction\(\).as_str\(\) }}"/);
    assert.match(html, /{% let text = self.labels\(\) %}/);
  }
  // Compare only production formatting: Rust equivalence assertions intentionally
  // materialize their reference strings inside the cfg(test) module.
  const text = (await read("src/views/actions/job_text.rs")).split("\n#[cfg(test)]\n", 1)[0];
  assert.doesNotMatch(text, /format!|\.to_string\(\)|\.to_owned\(\)/);
  const html = await expand("actions/job.html");
  assert.doesNotMatch(html, /(?:name|labels|look\.label|text\.[a-z_]+)\|safe/);
  const body = await read("templates/actions/step_body.html");
  for (const label of ["expired", "truncated", "no_output", "step_not_started", "earlier_lines", "show_more"]) {
    assert.ok(body.includes(`text.${label}`));
  }
});
