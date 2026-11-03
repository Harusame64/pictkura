import { test } from "node:test";
import assert from "node:assert/strict";
import { oneFewMany } from "../src/i18n/plural.ts";

const form = (n: number) => oneFewMany(n, "one", "few", "many");

test("1／2〜4／5〜 の3形は、CLDR の ru と全部の整数で一致すること", () => {
  // 手で書いた規則の正しさは、エンジンの答えと突き合わせて決める（0〜100000）
  const ru = new Intl.PluralRules("ru");
  for (let n = 0; n <= 100000; n++) {
    if (form(n) !== ru.select(n)) assert.fail(`n=${n}: ${form(n)} / CLDR ${ru.select(n)}`);
  }
});

test("1／2〜4／5〜 の3形は、11〜14 を 5〜 の形にすること（下2桁を見る）", () => {
  // 1の位だけを見る誤り（11 を one、12 を few にする）は、ここで落ちる
  assert.deepEqual(
    [1, 2, 4, 5, 11, 12, 14, 21, 22, 25, 101, 111, 112, 1001, 12345].map(form),
    ["one", "few", "few", "many", "many", "many", "many", "one", "few", "many", "one", "many", "many", "one", "many"],
  );
  assert.equal(form(0), "many");
});
