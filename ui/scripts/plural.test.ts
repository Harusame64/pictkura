import { test } from "node:test";
import assert from "node:assert/strict";
import { arabic, oneFewMany } from "../src/i18n/plural.ts";

const form = (n: number) => oneFewMany(n, "one", "few", "many");

/** 手で書いた規則を、エンジンの答えと 0〜100000 の整数で突き合わせる */
function assertMatchesCldr(locale: string, fn: (n: number) => string) {
  const cldr = new Intl.PluralRules(locale);
  for (let n = 0; n <= 100000; n++) {
    if (fn(n) !== cldr.select(n)) assert.fail(`${locale} n=${n}: ${fn(n)} / CLDR ${cldr.select(n)}`);
  }
}

test("1／2〜4／5〜 の3形は、CLDR の ru と全部の整数で一致すること", () => {
  // 手で書いた規則の正しさは、エンジンの答えと突き合わせて決める（0〜100000）
  assertMatchesCldr("ru", form);
});

test("1／2〜4／5〜 の3形は、11〜14 を 5〜 の形にすること（下2桁を見る）", () => {
  // 1の位だけを見る誤り（11 を one、12 を few にする）は、ここで落ちる
  assert.deepEqual(
    [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111, 112, 1001, 12345].map(form),
    ["one", "few", "few", "many", "many", "many", "many", "one", "few", "many", "one", "many", "many", "one", "many"],
  );
  assert.equal(form(0), "many");
});

const ar = (n: number) =>
  arabic(n, { zero: "zero", one: "one", two: "two", few: "few", many: "many", other: "other" });

test("アラビア語の6形は、CLDR の ar と全部の整数で一致すること", () => {
  assertMatchesCldr("ar", ar);
});

test("アラビア語の6形は、小数を other にすること（CLDR と同じ）", () => {
  const cldr = new Intl.PluralRules("ar");
  for (const n of [0.5, 1.5, 2.5, 3.5, 10.5, 11.5, 99.5, 103.5]) assert.equal(ar(n), cldr.select(n), `n=${n}`);
});

test("アラビア語の6形は、下2桁で few と many を分け、100〜102 を other にすること", () => {
  // 0・1・2 を数そのもので見ずに下2桁で見る誤り（101 を one、102 を two にする）は、ここで落ちる
  assert.deepEqual(
    [0, 1, 2, 3, 10, 11, 99, 100, 101, 102, 103, 110, 111, 199, 200, 1000, 1001, 1002, 1003].map(ar),
    ["zero", "one", "two", "few", "few", "many", "many", "other", "other", "other", "few", "few", "many", "many", "other", "other", "other", "other", "few"],
  );
});
