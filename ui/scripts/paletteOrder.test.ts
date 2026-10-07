/**
 * パレットの「（を）検索」と操作の並べ方（`src/paletteOrder.ts`。2026-10-07 利用者決定）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { actionMatches, orderSearchAndActions } from "../src/paletteOrder.ts";

const actions = ["すべての画像を表示", "再スキャン", "設定を開く", "Show all photos"];
const pick = (q: string) => actions.filter((a) => actionMatches(a, q));
const order = (q: string) => orderSearchAndActions(q.trim() ? `検索:${q}` : null, pick(q));

test("操作の名前に当たれば、操作が検索より上（win2 の実機: Enter で検索になっていた）", () => {
  assert.deepEqual(order("すべての画像"), ["すべての画像を表示", "検索:すべての画像"]);
  assert.deepEqual(order("画像"), ["すべての画像を表示", "検索:画像"], "部分一致でも上げる");
});

test("当たる操作が無ければ、検索が先頭のまま（たいていの検索語）", () => {
  assert.deepEqual(order("海"), ["検索:海"]);
  assert.deepEqual(order("沖縄 2019"), ["検索:沖縄 2019"]);
});

test("検索は消さない——操作が当たっても、すぐ下に残る", () => {
  const got = order("設定");
  assert.equal(got.at(-1), "検索:設定");
  assert.equal(got.length, 2);
});

test("大文字小文字を畳む・前後の空白は見ない", () => {
  assert.deepEqual(order("  show ALL "), ["Show all photos", "検索:  show ALL "]);
});

test("入力が空なら検索は出さず、操作は全部（並びはそのまま）", () => {
  assert.deepEqual(order(""), actions);
  assert.deepEqual(order("   "), actions);
});
