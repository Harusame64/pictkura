/**
 * 原寸が出なかった写真の帯の理由（`src/originalTrouble.ts`）。`npm --prefix ui test`。
 *
 * 「在る」と答えた原本でも黙らないこと（dev #23 の残り）を表で固定する。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { originalTrouble } from "../src/originalTrouble.ts";

test("無い・開けないは、そのまま名乗る", () => {
  assert.equal(originalTrouble("missing", false), "missing");
  assert.equal(originalTrouble("unreachable", false), "unreachable");
});

test("在ってクラウドにしか無いなら、まだダウンロードされていない", () => {
  assert.equal(originalTrouble("present", true), "notDownloaded");
});

test("在って手元にあるのに出なかったなら、表示できない（黙らない）", () => {
  assert.equal(originalTrouble("present", false), "notShown");
});
