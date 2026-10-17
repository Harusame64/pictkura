/**
 * 削除・移動の間に押された×の畳み方（pictkura-dev #69）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { quitAfterFileOps } from "../src/quitAfterFileOps.ts";

const base = { quitAsked: true, failed: false, pastGate: false, marks: [] as number[], gone: new Set<number>() };

test("no × pressed: stay", () => {
  assert.equal(quitAfterFileOps({ ...base, quitAsked: false }), "stay");
});

test("an operation failed or left photos behind: stay, even if the last one succeeded", () => {
  assert.equal(quitAfterFileOps({ ...base, failed: true }), "stay");
});

test("× pressed over a gate counts as the second ×: close without looking at marks", () => {
  assert.equal(quitAfterFileOps({ ...base, pastGate: true, marks: [1] }), "destroy");
  // ただし転んだ回は閉じない
  assert.equal(quitAfterFileOps({ ...base, pastGate: true, failed: true }), "stay");
});

test("marks left besides the ones just removed: show the gate", () => {
  assert.equal(quitAfterFileOps({ ...base, marks: [1, 2], gone: new Set([1]) }), "gate");
});

test("every mark was among the removed photos: close", () => {
  assert.equal(quitAfterFileOps({ ...base, marks: [1, 2], gone: new Set([1, 2]) }), "destroy");
  assert.equal(quitAfterFileOps({ ...base }), "destroy");
});
