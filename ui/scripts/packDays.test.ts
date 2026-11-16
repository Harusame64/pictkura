/**
 * 少ない日を1行に並べる（`packDays.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { PACK_DAYS_DEFAULT, parsePackDays, planDayRows, type DayFit } from "../src/packDays.ts";

const d = (dayKey: number, width: number | null): DayFit => ({ dayKey, width });

test("small days share a row until the width runs out", () => {
  // 100 + 10 + 100 + 10 + 100 = 320 ≤ 330。4 日目は入らない
  const days = [d(20251217, 100), d(20250929, 100), d(20250727, 100), d(20250602, 100)];
  assert.deepEqual(planDayRows(days, 330, 10), [[0, 1, 2], [3]]);
});

test("the gap between days counts toward the width", () => {
  // 幅だけなら 300 で入るが、間（10）を足すと 310 > 300
  assert.deepEqual(planDayRows([d(20250102, 150), d(20250101, 150)], 300, 10), [[0], [1]]);
  assert.deepEqual(planDayRows([d(20250102, 145), d(20250101, 145)], 300, 10), [[0, 1]]);
});

test("months may be crossed, years may not", () => {
  const days = [d(20250304, 50), d(20250210, 50), d(20250105, 50), d(20241231, 50), d(20241120, 50)];
  assert.deepEqual(planDayRows(days, 1000, 10), [[0, 1, 2], [3, 4]]);
});

test("a day that does not fit, or is not loaded, stands alone and breaks the run", () => {
  const days = [d(20250510, 50), d(20250509, null), d(20250508, 50), d(20250507, 50), d(20250506, 2000)];
  assert.deepEqual(planDayRows(days, 1000, 10), [[0], [1], [2, 3], [4]]);
});

test("every day is placed exactly once and in order", () => {
  const days: DayFit[] = [];
  for (let i = 0; i < 200; i++) {
    const key = 20260000 - Math.floor(i / 7) * 10000 + 1231 - (i % 7);
    days.push(d(key, i % 5 === 0 ? null : 40 + ((i * 37) % 300)));
  }
  const flat = planDayRows(days, 900, 28).flat();
  assert.deepEqual(flat, days.map((_, i) => i));
});

test("the saved preference reads on/off; anything else is the default", () => {
  assert.equal(parsePackDays("on"), true);
  assert.equal(parsePackDays("off"), false);
  for (const raw of [null, "", "true", "1", "ON"]) assert.equal(parsePackDays(raw), PACK_DAYS_DEFAULT, String(raw));
});
