/**
 * タイルの大きさの控え（`cellSize.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { CELL_SIZE_DEFAULT, parseCellSize } from "../src/cellSize.ts";

test("a saved size on the slider's grid is kept", () => {
  assert.equal(parseCellSize("100"), 100);
  assert.equal(parseCellSize("240"), 240);
  assert.equal(parseCellSize("320"), 320);
});

test("anything else falls back to the default", () => {
  for (const raw of [null, "", "110", "150.5", "0x80", " 200 ", "80", "340", "abc", "-120"]) {
    assert.equal(parseCellSize(raw), CELL_SIZE_DEFAULT, String(raw));
  }
});
