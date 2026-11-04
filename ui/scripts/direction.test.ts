/** 画面の向き（direction.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";

import { asciiDateTokens, asciiDigits, dirOf, hasRtlText, stepOfArrow } from "../src/direction.ts";

test("Arabic, Hebrew, Persian and Urdu read right to left", () => {
  for (const l of ["ar", "ar-EG", "he", "he-IL", "fa", "ur", "ckb", "ps", "yi", "iw", "ji", "ks", "prs", "fa-u-nu-latn", "pa-Arab", "az-Arab-IR", "pa-arab"])
    assert.equal(dirOf(l), "rtl", l);
  // 先頭が同じ文字でも別の言語（hr=クロアチア語、art=人工言語）は左→右
  for (const l of ["ja", "en", "de", "fr", "it", "ru", "zh-hant", "es-419", "hr", "art", "arn", "her", "pa", "pa-Guru", "az-Latn", "sr-Cyrl", "sd-Deva", "ar-Latn", "he-Latn", "en-u-nu-arab", "en-x-arab"])
    assert.equal(dirOf(l), "ltr", l);
});

test("the arrow keys step in reading order", () => {
  assert.equal(stepOfArrow("ArrowRight", false), 1);
  assert.equal(stepOfArrow("ArrowLeft", false), -1);
  // 右→左では左が次
  assert.equal(stepOfArrow("ArrowLeft", true), 1);
  assert.equal(stepOfArrow("ArrowRight", true), -1);
});

test("Arabic-Indic and Persian digits become ASCII, nothing else changes", () => {
  assert.equal(asciiDigits("٢٠١٩-٠٨"), "2019-08");
  assert.equal(asciiDigits("۱۴۰۳"), "1403");
  assert.equal(asciiDigits("year:٢٠٢٦ صور"), "year:2026 صور");
  assert.equal(asciiDigits("IMG_0001 写真"), "IMG_0001 写真");
});

test("right-to-left text is found in base and presentation forms", () => {
  assert.ok(hasRtlText("Shift + نقر"));
  assert.ok(hasRtlText("ﻧﻘﺮ")); // 表示形（U+FE70〜）
  assert.ok(hasRtlText("שלום"));
  assert.ok(!hasRtlText("⌘K"));
  assert.ok(!hasRtlText("Ctrl+,"));
});

test("only tokens made of digits and separators are turned into ASCII", () => {
  assert.equal(asciiDateTokens("folder:pk ٢٠٢٦-٠٨"), "folder:pk 2026-08");
  assert.equal(asciiDateTokens("year:٢٠٢٦"), "year:2026");
  // 名前に混ざった数字は名前として探す
  assert.equal(asciiDateTokens("DSC٠٠٠٨٩"), "DSC٠٠٠٨٩");
  assert.equal(asciiDateTokens("رحلة٢٠١٩"), "رحلة٢٠١٩");
  assert.equal(asciiDateTokens("IMG_1234 year:2019"), "IMG_1234 year:2019");
  // 名前を指す欄と引用符の中は字のまま（search.rs は folder: / camera: を名前として探す）
  assert.equal(asciiDateTokens("folder:٢٠١٩"), "folder:٢٠١٩");
  assert.equal(asciiDateTokens('folder:"trip ٢٠١٩ summer"'), 'folder:"trip ٢٠١٩ summer"');
  assert.equal(asciiDateTokens('"رحلة ٢٠١٩" ٢٠١٩'), '"رحلة ٢٠١٩" 2019');
  assert.equal(asciiDateTokens("年:٢٠١٩"), "年:2019");
  assert.ok(!hasRtlText("\ufeffCtrl+K"), "BOM is not right-to-left text");
});
