/**
 * 並べて印刷の紙面（pictkura-dev #55）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  edgeFor,
  gridFor,
  loadSheetLayout,
  maxPhotosFor,
  pagesOf,
  PER_PAGE,
  printSides,
  sheetCss,
} from "../src/printSheet.ts";

test("each count fills its grid exactly", () => {
  for (const n of PER_PAGE) {
    const { cols, rows } = gridFor(n);
    assert.equal(cols * rows, n);
  }
});

test("a landscape sheet swaps columns and rows", () => {
  const css = sheetCss("contain", { perPage: 6, captions: false });
  assert.match(css, /grid-template-columns: repeat\(2, minmax\(0, 1fr\)\); grid-template-rows: repeat\(3,/);
  assert.match(
    css,
    /@container \(orientation: landscape\) \{\s*#print-sheet \.sheet-grid \{ grid-template-columns: repeat\(3, minmax\(0, 1fr\)\); grid-template-rows: repeat\(2,/,
  );
});

test("pages break after each page but the last, and the page has no margin", () => {
  const css = sheetCss("cover", { perPage: 4, captions: true });
  assert.match(css, /@page \{ margin: 0; \}/);
  assert.match(css, /\.sheet-page \{[^}]*break-after: page;/);
  assert.match(css, /\.sheet-page:last-child \{ break-after: auto; \}/);
  assert.match(css, /object-fit: cover;/);
  // 1枚の紙面と違い、ページを積むので html・body を隠さない
  assert.doesNotMatch(css, /overflow: hidden !important/);
});

test("pagesOf keeps the order and leaves the remainder on the last page", () => {
  assert.deepEqual(pagesOf([1, 2, 3, 4, 5], 2), [[1, 2], [3, 4], [5]]);
  assert.deepEqual(pagesOf([], 4), []);
});

test("smaller cells get smaller pictures and allow more photos", () => {
  for (let i = 1; i < PER_PAGE.length; i++) {
    assert.ok(edgeFor(PER_PAGE[i]) < edgeFor(PER_PAGE[i - 1]));
    assert.ok(maxPhotosFor(PER_PAGE[i]) > maxPhotosFor(PER_PAGE[i - 1]));
  }
  // ほどいた絵の合計が、いちばん重い正方形の写真でも 650MB に収まる
  for (const n of PER_PAGE) {
    const e = edgeFor(n);
    const bytes = maxPhotosFor(n) * e * e * 4;
    assert.ok(bytes <= 650e6, `${n}: ${bytes}`);
  }
});

test("without storage the layout falls back to 4 per page, no captions", () => {
  assert.deepEqual(loadSheetLayout(), { perPage: 4, captions: false });
});

const info = (id: number, shot_key: number, is_raw: boolean, taken_at_ms: number | null) => ({
  id,
  shot_key,
  is_raw,
  taken_at_ms,
});

test("printSides keeps the side the viewer shows for each RAW+JPEG pair", () => {
  const xs = [info(1, 10, false, 5000), info(2, 10, true, 5000), info(3, 20, false, 6000)];
  assert.deepEqual(printSides(xs, "jpeg").map((x) => x.id), [1, 3]);
  assert.deepEqual(printSides(xs, "raw").map((x) => x.id), [2, 3]);
  assert.deepEqual(printSides(xs, "both").map((x) => x.id), [1, 2, 3]);
  assert.deepEqual(printSides(xs, null).map((x) => x.id), [1, 2, 3]);
});

test("printSides pairs only files whose real capture time matches", () => {
  // 撮影日時が分からない・ずれている同じ名前の RAW と JPEG は、一覧でも組にならない
  const xs = [
    info(1, 10, false, null),
    info(2, 10, true, null),
    info(3, 20, false, 6000),
    info(4, 20, true, 7000),
  ];
  assert.deepEqual(printSides(xs, "jpeg").map((x) => x.id), [1, 2, 3, 4]);
});
