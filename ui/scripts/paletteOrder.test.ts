/**
 * パレットの「（を）検索」と操作の並べ方（`src/paletteOrder.ts`。2026-10-07 利用者決定）。`npm --prefix ui test`。
 * 名前は実際の辞書（ja・en・zh）から取る——写しの名前で緑にしない
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { actionMatch, orderSearchAndActions } from "../src/paletteOrder.ts";
import { ja } from "../src/i18n/ja.ts";
import { en } from "../src/i18n/en.ts";
import { zh } from "../src/i18n/zh.ts";

const KEYS = [
  "importFromUsb",
  "rescan",
  "actionShowFavorites",
  "actionShowPicked",
  "actionShortcuts",
  "actionShowAll",
  "actionCalendar",
  "actionThumbnails",
] as const;
type Dict = typeof ja;
/** 画面の言語 `d` で、英語の名前を別名に持たせた操作（`App.tsx` の `paletteActions` と同じ組み方） */
const actionsIn = (d: Dict) => KEYS.map((k) => ({ label: d[k] as string, alias: en[k] as string }));
/** 検索の行は名前「検索」の操作の形で置き、並びを名前だけにして比べる */
const order = (d: Dict, q: string) =>
  orderSearchAndActions<{ label: string; alias?: string }>(
    q.trim() ? { label: "検索" } : null,
    actionsIn(d),
    (a) => actionMatch(a.label, q, a.alias),
  ).map((a) => a.label);

test("名前の頭に当たれば、操作が検索より上（win2 の実機: Enter で検索になっていた）", () => {
  assert.deepEqual(order(ja, "すべての画像"), [ja.actionShowAll, "検索"]);
  assert.deepEqual(order(ja, "再スキャン"), [ja.rescan, "検索"]);
});

test("名前の途中に当たるだけなら、検索が先頭。操作は消さずに検索の下（ゲート2）", () => {
  assert.deepEqual(order(ja, "画像"), ["検索", ja.actionShowAll], "Enter は検索");
  assert.deepEqual(order(en, "scan"), ["検索", en.rescan], "Rescan の途中");
  assert.deepEqual(order(zh, "收藏"), ["検索", zh.actionShowFavorites], "中国語の名詞は名前の途中にある");
  assert.deepEqual(order(ja, "表示"), [
    "検索",
    ja.actionShowFavorites,
    ja.actionShowPicked,
    ja.actionShowAll,
    ja.actionCalendar,
    ja.actionThumbnails,
  ]);
  assert.deepEqual(order(ja, "海"), ["検索"], "どこにも当たらない語は検索だけ");
});

test("英語の名前でも当たる——日本語・中国語の画面で、英語のまま打てる（利用者 10-07）", () => {
  assert.deepEqual(order(ja, "all"), [ja.actionShowAll, "検索"], "語の頭（Show all photos の all）");
  assert.deepEqual(order(ja, "show all"), [ja.actionShowAll, "検索"], "2語でも");
  assert.deepEqual(order(zh, "cal"), [zh.actionCalendar, "検索"]);
  assert.deepEqual(order(ja, "resc"), [ja.rescan, "検索"]);
});

test("当たる操作がいくつもあれば、全部を検索より上に（並びは操作の並びのまま）", () => {
  assert.deepEqual(order(en, "show"), [
    en.actionShowFavorites,
    en.actionShowPicked,
    en.actionShortcuts,
    en.actionShowAll,
    "検索",
  ]);
});

test("大文字小文字・前後と途中の空白は畳む", () => {
  assert.deepEqual(order(en, "  SHOW   all "), [en.actionShowAll, "検索"]);
});

test("入力が空なら検索は出さず、操作は全部", () => {
  assert.deepEqual(order(ja, ""), KEYS.map((k) => ja[k]));
  assert.deepEqual(order(ja, "   "), KEYS.map((k) => ja[k]));
});
