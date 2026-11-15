/**
 * 一覧の帯の計算（`scrubRailMath.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { dayKeyAt, fractionAt, scrollTopFor, thumbOf } from "../src/scrubRailMath.ts";

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
