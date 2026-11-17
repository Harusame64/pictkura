/** 一覧のサムネイルの部品（src/gridThumbs.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";
import { BlankWaiters, paintable, RecentThumbs } from "../src/gridThumbs.ts";

test("最近出した絵は覚え、上限を超えたら古いものから忘れる。触れたものは新しい側へ回す", () => {
  const r = new RecentThumbs(2);
  r.add("a");
  r.add("b");
  assert.equal(r.touch("a"), "a"); // a を新しい側へ
  r.add("c"); // b が押し出される
  assert.equal(r.touch("b"), undefined);
  assert.equal(r.touch("a"), "a");
  assert.equal(r.touch("c"), "c");
});

test("問い直して出た絵は、出した URL（回数付き）で覚える。作り直したタイルは素の URL を読み直さない", () => {
  const r = new RecentThumbs();
  r.add("t?v=1&grid=1", "t?v=1&grid=1&ask=2");
  assert.equal(r.touch("t?v=1&grid=1"), "t?v=1&grid=1&ask=2");
});

test("paintable は読めた絵で、「サムネイルが無い」の 1x1 だけを外す（幅1の細長い絵は出す）", () => {
  assert.equal(paintable({ complete: true, naturalWidth: 512, naturalHeight: 341 }), true);
  assert.equal(paintable({ complete: true, naturalWidth: 1, naturalHeight: 512 }), true);
  assert.equal(paintable({ complete: true, naturalWidth: 1, naturalHeight: 1 }), false);
  assert.equal(paintable({ complete: true, naturalWidth: 0, naturalHeight: 0 }), false);
  assert.equal(paintable({ complete: false, naturalWidth: 512, naturalHeight: 341 }), false);
});

test("空のタイルは、その写真の知らせで1回だけ起こされる（ほかの写真・やめたタイルは起こさない）", () => {
  const w = new BlankWaiters();
  const woke: string[] = [];
  w.wait(5, () => woke.push("a"));
  w.wait(5, () => woke.push("b")); // 同じ写真のタイルが2枚
  const stop = w.wait(6, () => woke.push("c"));
  stop(); // 6 のタイルは外れた
  w.notify([7]);
  assert.deepEqual(woke, []);
  w.notify([5, 6]);
  assert.deepEqual(woke, ["a", "b"]);
  w.notify([5]); // 起こしたら外れる。問い直して空ならタイルがまた待つ
  assert.deepEqual(woke, ["a", "b"]);
});
