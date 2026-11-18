import { test } from "node:test";
import assert from "node:assert/strict";
import { applySuggestion, suggest, tokenAt, yearsOf } from "../src/searchSuggest.ts";

const data = {
  years: [
    { year: 2025, count: 3412 },
    { year: 2024, count: 5120 },
    { year: 2019, count: 20 },
  ],
  cameras: [
    { name: "Canon EOS R8", count: 12034 },
    { name: "iPhone 15 Pro", count: 8211 },
    { name: "X100V", count: 4 },
  ],
};

test("keys are offered only while the word is the start of a key", () => {
  assert.deepEqual(
    suggest("ca", data).map((s) => s.insert),
    ["camera:"],
  );
  assert.deepEqual(
    suggest("f", data).map((s) => s.insert),
    ["folder:", "fav:"],
  );
  assert.deepEqual(suggest("cat", data), [], "ファイル名の検索の邪魔をしない");
  assert.deepEqual(suggest("", data), [], "何も打っていなければ出さない");
  assert.deepEqual(suggest("year", data), [], "打ち切った（コロンの手前）キーは出さない");
  assert.deepEqual(suggest('"ca', data), [], "引用符の語はキーではない");
});

test("years, cameras and kinds are offered after their key, aliases included", () => {
  assert.deepEqual(
    suggest("year:20", data).map((s) => s.insert),
    ["year:2025 ", "year:2024 ", "year:2019 "],
  );
  assert.deepEqual(
    suggest("year:202", data).map((s) => s.insert),
    ["year:2025 ", "year:2024 "],
  );
  assert.deepEqual(suggest("year:2025", data), [], "打ち終えた値は出さない");
  assert.deepEqual(
    suggest("年:2019", data).map((s) => s.insert),
    [],
  );
  assert.deepEqual(
    suggest("年:201", data).map((s) => s.insert),
    ["年:2019 "],
    "別名は打ったキーのまま差し込む",
  );
  assert.deepEqual(
    suggest("camera:can", data).map((s) => s.insert),
    ['camera:"Canon EOS R8" '],
    "空白のある名前は引用符で囲む",
  );
  assert.deepEqual(
    suggest('cam:"iph', data).map((s) => s.insert),
    ['cam:"iPhone 15 Pro" '],
  );
  assert.deepEqual(
    suggest("camera:x1", data).map((s) => s.insert),
    ["camera:X100V "],
  );
  assert.deepEqual(
    suggest("kind:", data).map((s) => s.insert),
    ["kind:photo ", "kind:raw ", "kind:video "],
  );
  assert.deepEqual(
    suggest("type:v", data).map((s) => s.insert),
    ["type:video "],
  );
  assert.deepEqual(suggest("folder:a", data), [], "フォルダは値を出さない");
});

test("the token at the caret respects quotes", () => {
  assert.deepEqual(tokenAt("beach year:20", 13), { start: 6, end: 13, text: "year:20" });
  assert.deepEqual(tokenAt('camera:"Canon EOS', 17), { start: 0, end: 17, text: 'camera:"Canon EOS' });
  assert.deepEqual(tokenAt("a b", 1), { start: 0, end: 1, text: "a" });
});

test("applying a suggestion replaces the word at the caret and keeps the rest", () => {
  const s = suggest("year:20", data)[0];
  assert.deepEqual(applySuggestion("beach year:20", 13, s), { value: "beach year:2025 ", caret: 16 });
  const k = suggest("ca", data)[0];
  assert.deepEqual(applySuggestion("ca beach", 2, k), { value: "camera: beach", caret: 7 });
  const c = suggest("camera:can", data)[0];
  assert.deepEqual(applySuggestion("camera:can beach", 10, c), {
    value: 'camera:"Canon EOS R8" beach',
    caret: 22,
  });
});

test("years come from dated days, newest first", () => {
  assert.deepEqual(
    yearsOf([
      { day_key: 20250101, count: 2 },
      { day_key: 20250303, count: 3 },
      { day_key: 20190101, count: 1 },
      { day_key: -2, count: 99 },
    ]),
    [
      { year: 2025, count: 5 },
      { year: 2019, count: 1 },
    ],
  );
});
