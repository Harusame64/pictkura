/**
 * 原寸が出なかった写真の帯の理由（`src/originalTrouble.ts`）。`npm --prefix ui test`。
 *
 * 「在る」と答えた原本でも黙らないこと（dev #23 の残り）と、空の error では
 * 「壊れている」と言わないこと（#153 のゲート2）を表で固定する。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { originalTrouble, TROUBLE_TEXT } from "../src/originalTrouble.ts";
import { ja } from "../src/i18n/ja.ts";

test("無い・開けないは、error の形によらず名乗る", () => {
  for (const failedWithSrc of [true, false]) {
    assert.equal(originalTrouble("missing", false, failedWithSrc), "missing");
    assert.equal(originalTrouble("unreachable", false, failedWithSrc), "unreachable");
  }
});

test("無い・開けないとクラウドのみが同時なら、無い・開けないを先に言う", () => {
  assert.equal(originalTrouble("unreachable", true, true), "unreachable");
  assert.equal(originalTrouble("missing", true, true), "missing");
});

test("在ってクラウドにしか無いなら、まだダウンロードされていない", () => {
  assert.equal(originalTrouble("present", true, true), "notDownloaded");
});

test("在って手元にあるのに出なかったなら、表示できない（黙らない）", () => {
  assert.equal(originalTrouble("present", false, true), "notShown");
});

test("在るのに URL の無い error なら、何も名乗らない（本命の load が後で来る）", () => {
  assert.equal(originalTrouble("present", false, false), null);
  assert.equal(originalTrouble("present", true, false), null);
});

test("理由と帯の文言の鍵が取り違えられていない", () => {
  assert.deepEqual(TROUBLE_TEXT, {
    missing: "fileMissing",
    unreachable: "fileUnreachable",
    notDownloaded: "fileNotDownloaded",
    notShown: "fileNotShown",
  });
  // 鍵が辞書に在ること（綴りの取り違えは型でも止まるが、ここでも見る）
  for (const key of Object.values(TROUBLE_TEXT)) {
    assert.equal(typeof ja[key], "string", key);
  }
});
