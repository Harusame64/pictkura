import { test } from "node:test";
import assert from "node:assert/strict";
import { rememberTiny, rgbaOfHex, tinyPixels } from "../src/tinyPx.ts";

const hexOf = (rgb: number[]) => rgb.map((v) => v.toString(16).padStart(2, "0")).join("");

test("the 4x4 RGB hex becomes RGBA in the same order, rows from the top", () => {
  const rgb = Array.from({ length: 48 }, (_, i) => i * 5);
  const px = rgbaOfHex(hexOf(rgb))!;
  assert.equal(px.length, 64);
  for (let i = 0; i < 16; i++) {
    assert.deepEqual([...px.slice(i * 4, i * 4 + 4)], [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2], 255]);
  }
});

test("other shapes are dropped, not drawn", () => {
  assert.equal(rgbaOfHex(null), null);
  assert.equal(rgbaOfHex(""), null);
  assert.equal(rgbaOfHex("ab".repeat(47)), null, "too short");
  assert.equal(rgbaOfHex("zz".repeat(48)), null, "not hex");
});

test("colours are kept by id and version: a newer version does not show the old colours", () => {
  rememberTiny(7, 1, hexOf(new Array(48).fill(10)));
  assert.equal(tinyPixels(7, 1)?.[0], 10);
  assert.equal(tinyPixels(7, 2), undefined, "rotated (rev 2): nothing until its own colours arrive");
  rememberTiny(7, 2, hexOf(new Array(48).fill(20)));
  assert.equal(tinyPixels(7, 2)?.[0], 20);
  // a later list with null does not wipe them
  rememberTiny(7, 2, null);
  assert.equal(tinyPixels(7, 2)?.[0], 20);
});

test("different colours for the same version replace the first ones (instant → final thumbnail)", () => {
  rememberTiny(8, 1, hexOf(new Array(48).fill(0)));
  const first = tinyPixels(8, 1);
  rememberTiny(8, 1, hexOf(new Array(48).fill(0)));
  assert.equal(tinyPixels(8, 1), first, "same colours: same array, the canvas is not redrawn");
  rememberTiny(8, 1, hexOf(new Array(48).fill(99)));
  assert.equal(tinyPixels(8, 1)?.[0], 99);
});
