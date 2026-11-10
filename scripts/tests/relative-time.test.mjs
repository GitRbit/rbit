import assert from "node:assert/strict";
import test from "node:test";
import { age, relativeText, renderRelativeTimes } from "../../crates/rbit-web/assets/relative-time.js";

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const NOW = Date.parse("2026-11-08T12:00:00Z");
const ago = (seconds) => new Date(NOW - seconds * 1000).toISOString();

test("ages pick the server's units, years included", () => {
  assert.equal(age(30), null);
  assert.equal(age(-90), null, "a clock behind the commit is not the future");
  assert.deepEqual(age(5 * MINUTE + 30), [5, "minute"]);
  assert.deepEqual(age(3 * HOUR + 600), [3, "hour"]);
  assert.deepEqual(age(29 * DAY), [29, "day"]);
  assert.deepEqual(age(45 * DAY), [1, "month"]);
  assert.deepEqual(age(364 * DAY), [12, "month"]);
  assert.deepEqual(age(365 * DAY), [1, "year"]);
  assert.deepEqual(age(800 * DAY), [2, "year"]);
});

test("ages read as time ago in the page language, never as a date", () => {
  assert.equal(relativeText(ago(2 * HOUR), NOW, "en"), "2 hours ago");
  assert.equal(relativeText(ago(3 * DAY), NOW, "en"), "3 days ago");
  assert.equal(relativeText(ago(40 * DAY), NOW, "en"), "1 month ago");
  assert.equal(relativeText(ago(800 * DAY), NOW, "en"), "2 years ago");
  assert.equal(relativeText(ago(3 * HOUR), NOW, "de"), "vor 3 Stunden");
  assert.equal(relativeText(ago(2 * 365 * DAY), NOW, "de"), "vor 2 Jahren");
  assert.equal(relativeText("not a time", NOW, "en"), null);
});

test("only marked times are rewritten, from their datetime", () => {
  const time = (datetime, relative) => ({
    textContent: datetime.slice(0, 10),
    getAttribute: (name) => (name === "datetime" ? datetime : null),
    relative,
  });
  const marked = time(ago(3 * DAY), true);
  const plain = time(ago(3 * DAY), false);
  const root = {
    documentElement: { lang: "en" },
    querySelectorAll: (selector) => (selector === "time[data-relative]" ? [marked] : []),
  };
  renderRelativeTimes(root, NOW);
  assert.equal(marked.textContent, "3 days ago");
  assert.equal(plain.textContent, ago(3 * DAY).slice(0, 10));
});
