/**
 * 一覧の重ね（`src/stacks.ts`、dev #32）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  closeOverStacks,
  countPhotos,
  filesOf,
  stackMembersIndex,
  stacksOfDay,
  type Stackable,
} from "../src/stacks.ts";

/** id・組の鍵・RAW か・撮影日時。名前は読みやすさのため（関数は見ない） */
const f = (id: number, shot_key: number, is_raw: boolean, name = "", taken_at_ms = 1000) =>
  ({ id, shot_key, is_raw, name, taken_at_ms }) as Stackable & { name: string };

const leads = (stacks: ReturnType<typeof stacksOfDay>) =>
  stacks.map((s) => s.cover.lead.id);
const members = (stacks: ReturnType<typeof stacksOfDay>) =>
  stacks.map((s) => filesOf(s).map((x) => x.id));

test("RAW+JPEG の組は1枚に重ね、表紙は JPEG", () => {
  const day = [
    f(1, 10, true, "IMG_0001.CR3"),
    f(2, 10, false, "IMG_0001.JPG"),
    f(3, 20, false, "IMG_0002.JPG"),
  ];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [2, 3]);
  assert.deepEqual(members(stacks), [[1, 2], [3]]);
});

test("重ねの位置は組の最初の1件が居た位置（並びを崩さない）", () => {
  // 最初の1件が RAW（表紙ではない）でも、位置はその RAW のところ
  const day = [f(5, 1, false), f(2, 10, true), f(6, 2, false), f(1, 10, false)];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [5, 1, 6]);
  assert.deepEqual(members(stacks), [[5], [2, 1], [6]]);
});

test("名前が同じでも撮影日時が違えば別の写真（カメラ2台の IMG_0001）", () => {
  const day = [
    f(1, 10, true, "IMG_0001.CR3 (本体A)", 1000),
    f(2, 10, false, "IMG_0001.JPG (本体B)", 5000),
  ];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("RAW 同士（CR3 と書き出した DNG）は RAW+JPEG ではないので重ねない", () => {
  const day = [f(1, 10, true, "IMG_0001.CR3"), f(2, 10, true, "IMG_0001.DNG")];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("RAW を含まない同名の組（Live Photos の HEIC+MOV）は重ねない", () => {
  const day = [f(1, 10, false, "IMG_0001.HEIC"), f(2, 10, false, "IMG_0001.MOV")];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("RAW だけの組は1枚のまま、表紙は先頭（JPEG が無い）", () => {
  const day = [f(1, 10, true), f(2, 20, true)];
  assert.deepEqual(leads(stacksOfDay(day, { rawJpeg: true })), [1, 2]);
});

test("RAW と JPEG の2枚以上が同じ鍵でも1枚に（RAW+JPEG+HEIF 等）", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 10, false)];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [2]);
  assert.deepEqual(members(stacks), [[1, 2, 3]]);
});

test("設定で切ったら、1件1枚（重ねない）", () => {
  const day = [f(1, 10, true), f(2, 10, false)];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: false })), [[1], [2]]);
});

test("空の日は空", () => {
  assert.deepEqual(stacksOfDay([], { rawJpeg: true }), []);
});

test("範囲選択の閉包: 組のどれか1つが入っていれば、組ぜんぶを入れる", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 20, false), f(4, 30, true), f(5, 30, false)];
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  // 重なっていない id は索引に入れない
  assert.equal(index.has(3), false);
  // 範囲が組 (1,2) の途中の 2 から始まり、組 (4,5) の途中の 4 で終わった
  assert.deepEqual([...closeOverStacks([2, 3, 4], index)].sort(), [1, 2, 3, 4, 5]);
  // 索引に無い id（読み込んでいない日）はそのまま通す
  assert.deepEqual([...closeOverStacks([99], index)], [99]);
});

test("見えているタイルの枚数: 重ねの組は1枚と数える", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 20, false)];
  const loaded = new Set([1, 2, 3]);
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  assert.equal(countPhotos([1, 2, 3], index, loaded), 2);
  assert.equal(countPhotos([1, 2], index, loaded), 1);
  assert.equal(countPhotos([2], index, loaded), 1, "組の片方だけでも1枚");
  assert.equal(countPhotos([3], index, loaded), 1);
  assert.equal(countPhotos([], index, loaded), 0);
  // 重ねを切ったら、ファイルの数そのもの
  const off = stackMembersIndex(stacksOfDay(day, { rawJpeg: false }));
  assert.equal(countPhotos([1, 2, 3], off, loaded), 3);
});

test("読み込んでいない日の id が混ざったら、枚数は数えられない（null）", () => {
  const day = [f(1, 10, true), f(2, 10, false)];
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  assert.equal(countPhotos([1, 2, 99], index, new Set([1, 2])), null);
});
