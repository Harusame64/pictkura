/** 画面の向き（direction.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";

import { asciiDateTokens, asciiDigits, dirOf, hasRtlText, stepOfArrow, isolatesToEmbeddings } from "../src/direction.ts";

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

test("閉じの印を、中の最初の字の向きの埋め込みに置き換える（TaskDialog 用）", () => {
  // 左から右の字で始まるパス（アラビア語で終わっても、中ごと左から右）
  assert.equal(
    isolatesToEmbeddings("نقل \u2068C:\\Temp\\pk283 اختبار\u2069؟"),
    "نقل \u202aC:\\Temp\\pk283 اختبار\u202c؟",
  );
  // 右から左の字で始まる名前。数字は字ではないので飛ばす
  assert.equal(isolatesToEmbeddings("\u20682024 صور\u2069"), "\u202b2024 صور\u202c");
  // 補助面の右から左の字（Adlam）も右から左
  assert.equal(isolatesToEmbeddings("\u2068\u{1E900}x\u2069"), "\u202b\u{1E900}x\u202c");
  // 画面の言語に無い字種でも、右から左の範囲の字なら右から左（Phoenician・Mende Kikakui）
  assert.equal(isolatesToEmbeddings("\u2068\u{10900}\u2069"), "\u202b\u{10900}\u202c");
  assert.equal(isolatesToEmbeddings("\u2068\u{1E800}\u2069"), "\u202b\u{1E800}\u202c");
  // 補助面でも左から右の字（Brahmi）は左から右
  assert.equal(isolatesToEmbeddings("\u2068\u{11005}\u2069"), "\u202a\u{11005}\u202c");
  // アラビア・インド数字は字ではない（向きは後の字で決まる）
  assert.equal(isolatesToEmbeddings("\u2068٢٠٢٤ a\u2069"), "\u202a٢٠٢٤ a\u202c");
  // 字が無ければ左から右
  assert.equal(isolatesToEmbeddings("\u2068123\u2069"), "\u202a123\u202c");
  // 入れ子の中の字は外の向きを決めない
  assert.equal(
    isolatesToEmbeddings("\u2068\u2067ب\u2069a\u2069"),
    "\u202a\u202bب\u202ca\u202c",
  );
  // RLM・ALM は強い右から左、LRM は強い左から右
  assert.equal(isolatesToEmbeddings("\u2068\u200fabc\u2069"), "\u202b\u200fabc\u202c");
  assert.equal(isolatesToEmbeddings("\u2068\u200eب\u2069"), "\u202a\u200eب\u202c");
  // 対の無い PDI は捨てる（開いている埋め込みを閉じない）
  assert.equal(isolatesToEmbeddings("\u2069x"), "x");
  assert.equal(isolatesToEmbeddings("\u202bب\u2069x\u202c"), "\u202bبx\u202c");
  // 名前の中で開いたままの埋め込みは、PDI で一緒に閉じる（外の文へ漏らさない）
  assert.equal(isolatesToEmbeddings("\u2068a\u202bb\u2069c"), "\u202aa\u202bb\u202c\u202cc");
  // 名前の中の対の無い PDF は捨てる（こちらの埋め込みを早く閉じない）
  assert.equal(isolatesToEmbeddings("\u2068a\u202cb\u2069c"), "\u202aab\u202cc");
  // 閉じの外の埋め込みには触らない
  assert.equal(isolatesToEmbeddings("\u202ba\u202c"), "\u202ba\u202c");
  // 段落の切れ目より先の字で向きを決めない
  assert.equal(isolatesToEmbeddings("\u2068123\nب"), "\u202a123\nب");
  // LRI・RLI もそのまま対応する埋め込みへ。印の無い文は変えない
  assert.equal(isolatesToEmbeddings("\u2066a\u2069\u2067b\u2069"), "\u202aa\u202c\u202bb\u202c");
  assert.equal(isolatesToEmbeddings("abc"), "abc");
  assert.ok(!/[\u2066-\u2069]/u.test(isolatesToEmbeddings("\u2068x\u2069")));
});
