/**
 * 確認ダイアログのキャンセルの語（`src/confirmLabels.ts`）。`npm --prefix ui test`。
 *
 * macOS の Esc は、OS の言語の「キャンセル」か英語の `Cancel` にしか割り当たらない
 * （#147 の実機）。この分岐は実機でしか見つからなかったので、表で固定する。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { cancelLabelFor } from "../src/confirmLabels.ts";

test("macOS はアプリの言語ではなく OS の言語のキャンセルを渡す", () => {
  // 日本語の Mac で、アプリだけドイツ語
  assert.equal(cancelLabelFor("macos", "Abbrechen", "キャンセル"), "キャンセル");
});

test("macOS で OS の言語の辞書が無ければ、何も渡さない（既定の Cancel）", () => {
  assert.equal(cancelLabelFor("macos", "Abbrechen", undefined), undefined);
});

test("Windows とその他はアプリの言語", () => {
  assert.equal(cancelLabelFor("windows", "Abbrechen", "キャンセル"), "Abbrechen");
  assert.equal(cancelLabelFor("other", "Abbrechen", "キャンセル"), "Abbrechen");
});
