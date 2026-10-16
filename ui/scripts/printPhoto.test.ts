/**
 * 印刷の紙面の CSS（ADR B-3）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { loadPrintFit, shapeOf, sheetCss } from "../src/printPhoto.ts";

test("the page has no margin, so WebView2 prints no header or footer", () => {
  for (const shape of ["landscape", "portrait", "square"] as const) {
    assert.match(sheetCss("contain", shape), /@page \{ margin: 0; \}/);
  }
});

test("the paper's orientation is left to the user (a size pins it in WebView2's preview)", () => {
  for (const shape of ["landscape", "portrait", "square"] as const) {
    assert.doesNotMatch(sheetCss("contain", shape), /size:/);
  }
});

test("the photo turns when the paper faces the other way, measured on the sheet, not the window", () => {
  const css = sheetCss("cover", "landscape");
  assert.match(css, /container-type: size;/);
  assert.match(css, /@container \(orientation: portrait\)/);
  assert.match(sheetCss("cover", "portrait"), /@container \(orientation: landscape\)/);
  // WebKit の印刷では vw・vh が窓の大きさになる（mac の実測）
  assert.doesNotMatch(css, /\d+v[wh]\b/);
  assert.match(css, /width: 100cqh;\s*height: 100cqw;/);
});

test("each fit is the object-fit value", () => {
  for (const fit of ["contain", "cover", "fill"] as const) {
    assert.match(sheetCss(fit, "landscape"), new RegExp(`object-fit: ${fit};`));
  }
});

test("without storage the fit falls back to fitting the page", () => {
  assert.equal(loadPrintFit(), "contain");
});

test("a square photo is never turned", () => {
  assert.equal(shapeOf(100, 100), "square");
  assert.equal(shapeOf(101, 100), "landscape");
  assert.equal(shapeOf(100, 101), "portrait");
  assert.doesNotMatch(sheetCss("contain", "square"), /@container|rotate/);
});
