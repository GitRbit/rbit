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

test("job responsibility extraction preserves the exact existing template program", async () => {
  const html = await expand("actions/job.html");
  assert.equal(createHash("sha256").update(html).digest("hex"),
    "5d9ec9c2210fa1515b763d31911c362efb15637e62912b748a93cea66fb2bfa2");
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
