/**
 * フィルムストリップの台の席（`stripSeats.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { STRIP_MARGIN, stripDockWidth, stripRadiusFor } from "../src/stripSeats.ts";

test("a wide window shows the full radius", () => {
  assert.equal(stripRadiusFor(2560, 0, 10), 10);
});

test("the dock never needs more room than the window gives it", () => {
  for (const w of [960, 1100, 1280, 1440, 1920]) {
    for (const reserve of [0, 276]) {
      const r = stripRadiusFor(w, reserve, 10);
      assert.ok(stripDockWidth(r) <= w - reserve - 2 * STRIP_MARGIN, `${w} ${reserve} r=${r}`);
      // 1 席増やすと入らない（入るだけ並べている）——上限に届いていないとき
      if (r < 10) assert.ok(stripDockWidth(r + 1) > w - reserve - 2 * STRIP_MARGIN, `${w} ${reserve} r+1`);
    }
  }
});

test("a very narrow window still keeps one seat on each side", () => {
  assert.equal(stripRadiusFor(200, 0, 10), 1);
});

test("dock width counts seats, gaps and padding", () => {
  // 3 席: 56*3 + 4*2 + 6*2
  assert.equal(stripDockWidth(1), 188);
});
