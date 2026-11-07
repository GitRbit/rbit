import assert from "node:assert/strict";
import test from "node:test";
import { fingerprint } from "../workspace/retention/handoff.mjs";

function metadata(size) {
  return {
    size,
    mtimeMs: 1,
    ino: 2,
    dev: 3,
    isFile: () => true,
    isSymbolicLink: () => false,
  };
}

test("a perpetually growing handoff stops at the initial size plus one byte", async () => {
  const initialSize = 64 * 1024 + 10;
  const requests = [];
  let total = 0;
  let closed = false;
  const io = {
    stat: async () => metadata(initialSize),
    openFile: async () => ({
      read: async (buffer, offset, requested) => {
        requests.push(requested);
        assert.equal(buffer.length, 64 * 1024);
        // This file never returns EOF: every read observes newly appended bytes.
        buffer.fill(97, offset, offset + requested);
        total += requested;
        return { bytesRead: requested };
      },
      stat: async () => metadata(total + 100),
      close: async () => { closed = true; },
    }),
  };
  await assert.rejects(fingerprint("fixture", io), /HANDOFF_CHANGED/);
  assert.deepEqual(requests, [64 * 1024, 11]);
  assert.equal(total, initialSize + 1);
  assert.equal(closed, true);
});

test("unsafe artifact sizes are refused before opening a handle", async () => {
  for (const size of [-1, NaN, Infinity, Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER + 1]) {
    let opened = false;
    await assert.rejects(fingerprint("fixture", {
      stat: async () => metadata(size),
      openFile: async () => { opened = true; },
    }), /HANDOFF_FILE_REFUSED/);
    assert.equal(opened, false);
  }
});

test("a shortened artifact cannot publish a partial fingerprint", async () => {
  let closed = false;
  await assert.rejects(fingerprint("fixture", {
    stat: async () => metadata(10),
    openFile: async () => ({
      read: async () => ({ bytesRead: 0 }),
      stat: async () => metadata(10),
      close: async () => { closed = true; },
    }),
  }), /HANDOFF_CHANGED/);
  assert.equal(closed, true);
});
