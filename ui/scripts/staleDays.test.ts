import { test } from "node:test";
import assert from "node:assert/strict";
import { keepsWhileRefetching, pickStaleDays, type AskedFor } from "../src/staleDays.ts";

test("visible days whose count no longer matches are refetched once per summary", () => {
  const s1 = {};
  const asked: AskedFor<object> = { summary: null, days: new Set() };
  const counts = new Map([
    [20250101, 3],
    [20250102, 5],
    [20250103, 2],
  ]);
  const items = new Map([
    [20250101, 2], // ずれ
    [20250102, 5], // 合っている
    [20250104, 1], // 骨組みに無い（見えている日ではない）
  ]);
  // 20250103 は見えているが中身が無い（枠）→ 選ばない
  assert.deepEqual(pickStaleDays(s1, counts, items, [20250101, 20250102, 20250103], null, new Set(), asked), [
    20250101,
  ]);
  // 同じ骨組みでは二度頼まない（ずれが解けなくても読み続けない）
  assert.deepEqual(pickStaleDays(s1, counts, items, [20250101], null, new Set(), asked), []);
  // 骨組みが新しくなったら、もう一度
  assert.deepEqual(pickStaleDays({}, counts, items, [20250101], null, new Set(), asked), [20250101]);
});

test("a day being read is not asked for again, and the viewer's day counts as visible", () => {
  const s = {};
  const asked: AskedFor<object> = { summary: null, days: new Set() };
  const counts = new Map([
    [20250101, 3],
    [20250109, 4],
  ]);
  const items = new Map([
    [20250101, 2],
    [20250109, 1],
  ]);
  assert.deepEqual(pickStaleDays(s, counts, items, [20250101], 20250109, new Set([20250101]), asked), [20250109]);
  // 読み途中だった日は控えに残らないので、届いた後にまた見られる
  assert.deepEqual(pickStaleDays(s, counts, items, [20250101], null, new Set(), asked), [20250101]);
});

test("only the visible unread group that is still in the summary is kept while it is refetched", () => {
  const U = -2;
  const visible = new Set([U, 20250101]);
  assert.equal(keepsWhileRefetching(U, U, visible, true), true);
  assert.equal(keepsWhileRefetching(U, U, new Set([20250101]), true), false, "見えていない束は捨てる");
  assert.equal(keepsWhileRefetching(U, U, visible, false), false, "骨組みから消えた束は捨てる");
  assert.equal(keepsWhileRefetching(20250101, U, visible, true), false, "ほかの日は捨てる");
});
