/**
 * 削除・移動の間に押された×の畳み方（pictkura-dev #69）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { quitAfterFileOp } from "../src/quitAfterFileOp.ts";

const base = { ok: true, pastGate: false, marks: [] as number[], gone: new Set<number>() };

test("an operation that failed or left files behind: stay", () => {
  assert.equal(quitAfterFileOp({ ...base, ok: false }), "stay");
  assert.equal(quitAfterFileOp({ ...base, ok: false, pastGate: true }), "stay");
});

test("× pressed over a gate is the second ×: close without looking at marks", () => {
  assert.equal(quitAfterFileOp({ ...base, pastGate: true, marks: [1] }), "destroy");
});

test("marks left besides the photos just removed: show the gate", () => {
  assert.equal(quitAfterFileOp({ ...base, marks: [1, 2], gone: new Set([1]) }), "gate");
});

test("no marks left: close", () => {
  assert.equal(quitAfterFileOp({ ...base, marks: [1, 2], gone: new Set([1, 2]) }), "destroy");
  assert.equal(quitAfterFileOp({ ...base }), "destroy");
});
