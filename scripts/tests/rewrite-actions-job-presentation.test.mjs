import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const root = new URL("../../crates/rbit-web/", import.meta.url);
const read = path => readFile(new URL(path, root), "utf8").then(text => text.replaceAll("\r\n", "\n"));
async function expand(path) {
  let html = await read(`templates/${path}`);
  for (const match of [...html.matchAll(/{% include "(actions\/job\/[^\"]+)" %}/g)]) {
    html = html.replace(match[0], await expand(match[1]));
  }
  return html;
}

test("job presentation renders the log console as one LTR machine island and exposes real metadata", async () => {
  const html = await expand("actions/job.html");
  const body = await read("templates/actions/step_body.html");
  assert.equal(body.split('<div class="log-console" dir="ltr" data-log="{{ step.key }}"').length - 1, 1,
    "only the log console has the exact required LTR boundary");
  assert.equal(body.match(/\|safe/g)?.length, 1, "the console's escaped rows are the only trusted HTML");
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
  for (const contract of ['data-live-url="{{ self_url }}/logs/live"', 'data-live-state="{{ live_state }}"',
    'data-step="{{ step.key }}"', 'data-state="{{ step.look.key }}"', 'method="post" action="{{ rerun_url }}"',
    'href="{{ raw_url }}" hx-boost="false"']) assert.ok(html.includes(contract), contract);
  const body = await read("templates/actions/step_body.html");
  assert.match(body, /log\.html\|safe/);
  const rows = await read("src/views/actions/log_lines.rs");
  assert.match(rows, /step-\{\}-line-\{number\}/, "rows are #step-<key>-line-<n>");
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
  const route = await read("src/routes/actions/jobs.rs");
  const presentation = route.slice(route.indexOf("pub async fn show("), route.indexOf("pub async fn raw("));
  assert.equal(presentation.match(/locale: RequestLocale/g)?.length, 2);
  assert.equal(presentation.match(/page\.with_locale\(locale\.0\.locale\)/g)?.length, 2);
  assert.equal(presentation.match(/Ok\(locale\.response\(view\.into_response\(\)\)\)/g)?.length, 2);
  assert.match(presentation, /return Ok\(axum::response::Redirect::to\(&job_url\)\.into_response\(\)\)/);
  assert.doesNotMatch(route.slice(route.indexOf("pub async fn raw(")), /RequestLocale|locale\.response|with_locale/);
});

test("the log toolbar is hidden until its script runs and speaks only catalog text", async () => {
  const toolbar = await read("templates/actions/job/toolbar.html");
  assert.match(toolbar, /data-log-tools [^>]*hidden>/);
  for (const hook of ["data-log-search", 'data-log-match="previous"', 'data-log-match="next"', "data-log-times",
    'data-log-expand="open"', 'data-log-expand="close"', "data-log-status", "data-log-count"]) assert.ok(toolbar.includes(hook), hook);
  const visible = toolbar.replace(/{[%#{][\s\S]*?[%#}]}/g, "").replace(/<[^>]*>/g, "").trim();
  assert.equal(visible, "", "every label comes from the catalog");
  const script = await readFile(new URL("assets/actions-log-tools.js", root), "utf8");
  assert.doesNotMatch(script, /innerHTML/, "matches are marked as text nodes, never parsed");
  assert.match(await read("templates/actions/job/steps.html"), /{% include "actions\/job\/toolbar.html" %}/);
});
