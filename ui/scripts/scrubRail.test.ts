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
  landingTop,
} from "../src/scrubRailMath.ts";

test("a point on the rail maps to the same fraction of the content, held to the ends", () => {
  assert.equal(fractionAt(0, 800), 0);
  assert.equal(fractionAt(400, 800), 0.5);
  assert.equal(fractionAt(800, 800), 1);
  assert.equal(fractionAt(-30, 800), 0);
  assert.equal(fractionAt(900, 800), 1);
  assert.equal(fractionAt(10, 0), 0);
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

test("the year on screen always gets its label, and its crowded neighbours give way", () => {
  // 3 つの年が 1% ずつ詰まっている。真ん中を表示中に
  const tops = [10, 11, 12];
  const spans = [1, 1, 50];
  assert.deepEqual(chooseLabels(tops, spans, 4), [false, false, true]);
  assert.deepEqual(chooseLabels(tops, spans, 4, 1), [false, true, false]);
});

test("a month inside a packed row lands on the row's top, not part-way down it", () => {
  // 行 0（上端 0、高さ 224）に 3 日を並べ、行の高さを日の数で割ってずらしてある。行 1 は 224 から
  const tops = [0, 74.67, 149.33, 224, 400];
  const jumpTops = [0, 0, 0, 224, 400];
  assert.equal(landingTop(149.33, tops, jumpTops), 0);
  assert.equal(landingTop(74.67, tops, jumpTops), 0);
  assert.equal(landingTop(224, tops, jumpTops), 224);
  // 渡されなければ位置そのもの
  assert.equal(landingTop(149.33, tops), 149.33);
});

test("anchoring lands on a packed row's top, not part-way down it", () => {
  // 2 日が別々の行（見出し＋写真）→ 1000 から始まる 1 つの並べた行（高さ 264）へ
  const prevTops = [1000, 1040, 1224, 1264, 1448];
  const prevDays = [20251227, 20251227, 20251225, 20251225];
  const nextTops = [1000, 1000, 1264];
  const nextDays = [20251227, 20251225];
  // 2日目の見出しが上端にあった → 並べた行の頭へ（行の途中の 1112 ではない）
  assert.equal(anchoredScrollTop(1224, prevTops, prevDays, nextTops, nextDays), 1000);
  // 1日目の見出しが上端 → そのまま
  assert.equal(anchoredScrollTop(1000, prevTops, prevDays, nextTops, nextDays), 1000);
});

test("anchoring inside a packed row keeps the offset into the row", () => {
  // 並べた行（1000〜1264）の 200px 入った所。同じ上端の日は行の頭の日で見て、高さは行の高さ
  const tops = [1000, 1000, 1264];
  const days = [20251227, 20251225];
  assert.equal(anchoredScrollTop(1200, tops, days, tops, days), 1200);
  // 並べるのを切ると、頭の日（12/27）の 200px 入った所へ
  const offTops = [1000, 1040, 1224, 1264, 1448];
  const offDays = [20251227, 20251227, 20251225, 20251225];
  assert.equal(anchoredScrollTop(1200, tops, days, offTops, offDays), 1200);
});

// 末尾の「読み込み中」の束（-2）を見ている間に、読めた写真で上にいちばん古い日が増えても、束の中の同じ所に留まる
// （App.tsx の `dayRows` は、留め置きには束も含めて渡す。束を外すと束の外へ跳ばされた。PR #351 の codex）
test("an unread group at the end keeps its place when an older day appears above it", () => {
  const prevTops = [0, 100, 1000];
  const prevDays = [20250101, -2];
  const nextTops = [0, 100, 200, 1100];
  const nextDays = [20250101, 20190101, -2];
  assert.equal(anchoredScrollTop(500, prevTops, prevDays, nextTops, nextDays), 600);
  // 束を外して渡すと、いちばん古い日へ留めようとして束の外へ出る（この升が見張る形）
  assert.notEqual(anchoredScrollTop(500, [0, 100], [20250101], [0, 100, 200], [20250101, 20190101]), 600);
});
