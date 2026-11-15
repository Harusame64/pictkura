/**
 * 一覧の帯の計算（`scrubRailMath.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  chooseLabels,
  dayKeyAt,
  anchoredScrollTop,
  fractionAt,
  makeRailMap,
  recencyFactor,
  monthDots,
  monthIndexAt,
  monthStarts,
  scrollTopFor,
  thumbOf,
} from "../src/scrubRailMath.ts";

test("a point on the rail maps to the same fraction of the content, held to the ends", () => {
  assert.equal(fractionAt(0, 800), 0);
  assert.equal(fractionAt(400, 800), 0.5);
  assert.equal(fractionAt(800, 800), 1);
  assert.equal(fractionAt(-30, 800), 0);
  assert.equal(fractionAt(900, 800), 1);
  assert.equal(fractionAt(10, 0), 0);
});

test("the scroll position stops at the last screen", () => {
  assert.equal(scrollTopFor(0.5, 10000, 800), 5000);
  // 底より先へは行かない
  assert.equal(scrollTopFor(1, 10000, 800), 9200);
  assert.equal(scrollTopFor(0.99, 10000, 800), 9200);
  // 1画面に収まるなら動かない
  assert.equal(scrollTopFor(0.5, 500, 800), 0);
});

test("the thumb shows the visible part and never shrinks below the minimum", () => {
  assert.deepEqual(thumbOf(0, 10000, 1000, 0.04), { top: 0, height: 0.1 });
  assert.deepEqual(thumbOf(5000, 10000, 1000, 0.04), { top: 0.5, height: 0.1 });
  // 長いライブラリでも掴める大きさ
  assert.deepEqual(thumbOf(0, 1_000_000, 1000, 0.04), { top: 0, height: 0.04 });
  // 底では帯からはみ出さない
  assert.deepEqual(thumbOf(999_000, 1_000_000, 1000, 0.04), { top: 0.96, height: 0.04 });
});

test("the day under a point is the row that starts at or above it", () => {
  const tops = [0, 40, 240, 280, 600];
  const days = [20260801, 20260801, 20250102, 20250102];
  assert.equal(dayKeyAt(0, tops, days), 20260801);
  assert.equal(dayKeyAt(239, tops, days), 20260801);
  assert.equal(dayKeyAt(240, tops, days), 20250102);
  assert.equal(dayKeyAt(10_000, tops, days), 20250102);
  assert.equal(dayKeyAt(5, [0], []), null);
});

test("month dots mark each new month, skip year changes, and thin out when crowded", () => {
  // 2026-08, 2026-08, 2026-07, 2026-06, 2025-12, 2025-11
  const days = [20260810, 20260801, 20260720, 20260601, 20251224, 20251101];
  const tops = [0, 100, 200, 300, 400, 500, 600];
  // 帯 600px: 1行 = 100px
  assert.deepEqual(
    monthDots(tops, days, 600, [0, 400 / 600], 6),
    [200 / 600, 300 / 600, 500 / 600],
  );
  // 帯が小さいと、近すぎる点は省く（1行 = 1px）
  assert.deepEqual(monthDots(tops, days, 6, [0, 400 / 600], 6), []);
});

test("when two year labels collide, the year with more photos keeps its label", () => {
  // 2106（3枚・ごく短い）の直下に 2026（長い）、その下に 2025
  const tops = [0, 0.005, 0.3];
  const spans = [0.005, 0.295, 0.7];
  assert.deepEqual(chooseLabels(tops, spans, 0.04), [false, true, true]);
  // 離れていれば全部
  assert.deepEqual(chooseLabels([0, 0.5], [0.5, 0.5], 0.04), [true, true]);
});

test("month starts list each month once, and a point finds the month it is in", () => {
  const days = [20260810, 20260801, 20260720, 20251224];
  const tops = [0, 100, 200, 300, 400];
  const starts = monthStarts(tops, days);
  assert.deepEqual(starts, [
    { month: 202608, offset: 0 },
    { month: 202607, offset: 200 },
    { month: 202512, offset: 300 },
  ]);
  assert.equal(monthIndexAt(150, starts), 0);
  assert.equal(monthIndexAt(200, starts), 1);
  assert.equal(monthIndexAt(999, starts), 2);
  assert.equal(monthIndexAt(5, []), -1);
});

test("the day at the top stays at the top when rows above change height", () => {
  // 前: 0801 の見積もり 1000px、0701 の見出し 40＋見積もり 1000
  const prevDays = [20260801, 20260801, 20260701, 20260701];
  const prevTops = [0, 40, 1040, 1080, 2080];
  // 後: 0801 の写真が実際は 300px
  const nextDays = [20260801, 20260801, 20260701, 20260701];
  const nextTops = [0, 40, 340, 380, 1380];
  // 0701 の頭から 50px 下を見ていた
  assert.equal(anchoredScrollTop(1090, prevTops, prevDays, nextTops, nextDays), 390);
  // 日が消えたら動かさない
  assert.equal(anchoredScrollTop(1090, prevTops, prevDays, [0, 40, 100], [20250101, 20250101]), null);
  // その日の中ほども、その日の頭からの距離を保つ
  assert.equal(anchoredScrollTop(1500, prevTops, prevDays, nextTops, nextDays), 800);
  // その日が縮んで届かなければ、その日の終わりに留める
  assert.equal(anchoredScrollTop(1500, prevTops, prevDays, [0, 40, 340, 380, 440], nextDays), 439);
});

test("the rail gives small months room, and maps back to the same place", () => {
  // 1か月目は 100px、2か月目は 900px
  const starts = [{ offset: 0 }, { offset: 100 }];
  const map = makeRailMap(starts, 1000, 0.5);
  // 量だけなら 10%。半分を等分に回すと 0.5*0.1 + 0.5*0.5 = 30%
  assert.ok(Math.abs(map.toRail(100) - 0.3) < 1e-9);
  assert.equal(map.toRail(0), 0);
  assert.ok(Math.abs(map.toRail(1000) - 1) < 1e-9);
  for (const o of [0, 50, 100, 400, 999]) {
    assert.ok(Math.abs(map.toContent(map.toRail(o)) - o) < 1e-6, String(o));
  }
  // alpha=1 なら量に比例（前の振る舞い）
  const prop = makeRailMap(starts, 1000, 1);
  assert.ok(Math.abs(prop.toRail(100) - 0.1) < 1e-9);
});

test("recent months get a wider share, and future-dated months only their photos' share", () => {
  const f = recencyFactor(202611, 24, 3);
  assert.equal(f(202611), 3);
  assert.equal(f(202412), 3);
  assert.equal(f(202411), 1);
  assert.equal(f(210602), 0);
  // 2 か月・同じ量: 新しい方（倍率 3）が等分の 3/4 を取る
  const starts = [
    { offset: 0, month: 202610 },
    { offset: 500, month: 201001 },
  ];
  const map = makeRailMap(starts, 1000, 0.5, f);
  // 0.5*0.5 + 0.5*(3/4) = 0.625
  assert.ok(Math.abs(map.toRail(500) - 0.625) < 1e-9);
});
