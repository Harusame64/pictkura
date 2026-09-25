/**
 * 見つからないライブラリのフォルダの知らせ（dev #23）の判定。`npm --prefix ui test`。
 *
 * #146 の変異注入で、**持ち越しを「いつも」「決して」にする変異・入れ子を足す変異・
 * 区切りを見ない変異が、tsc 以外の何にも殺されなかった**。判定を `src/missingRoots.ts` へ
 * 出したのは、ここで升を書くためである。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  laterKey,
  mergeMissing,
  missingTotal,
  rootMark,
  ROOT_MARK_VIEW,
} from "../src/missingRoots.ts";
import { ja } from "../src/i18n/ja.ts";

const m = (root: string, count = 1) => ({ root, count });

test("答えたルートは持ち越さない（差し込んだ直後に「見つかりません」と言わない）", () => {
  // A は前の答えで無かった。今回 A は答えて「在る」（found に居ない）、B はまだ確認中
  const merged = mergeMissing([m("/A"), m("/B")], [], new Set(["/B"]));
  assert.deepEqual(
    merged.map((x) => x.root),
    ["/B"],
    "A を持ち越している——答えた相手まで持ち越す変異（いつも持ち越す）",
  );
});

test("答えの揃わなかったルートは持ち越す（確認中を「在る」と読まない）", () => {
  const merged = mergeMissing([m("/A", 4)], [], new Set(["/A"]));
  assert.deepEqual(merged, [m("/A", 4)], "確認中の A を落としている——決して持ち越さない変異");
});

test("新しい答えが前の答えより勝つ", () => {
  const merged = mergeMissing([m("/A", 4)], [m("/A", 9)], new Set(["/A"]));
  assert.deepEqual(merged, [m("/A", 9)]);
});

test("入れ子のルートは外側だけを足す", () => {
  const shown = [m("/Volumes/SD", 500), m("/Volumes/SD/DCIM", 500)];
  assert.equal(missingTotal(shown, false), 500);
});

test("名前が前方一致するだけの隣は入れ子ではない（区切りまで見る）", () => {
  // macOS は同名のボリュームを `SD 1` と名付ける
  const shown = [m("/Volumes/SD", 500), m("/Volumes/SD 1", 7)];
  assert.equal(missingTotal(shown, false), 507);
});

test("Windows では区切りと大小をそろえてから入れ子を見る", () => {
  const shown = [m("D:/Photos", 10), m("d:\\Photos\\2020", 3)];
  assert.equal(missingTotal(shown, true), 10);
  // macOS/Linux では大小は別のフォルダ
  assert.equal(missingTotal([m("/Photos", 10), m("/photos/2020", 3)], false), 13);
});

test("区切りで終わるルート（`/`・`D:\\`）の配下も入れ子と見る", () => {
  assert.equal(missingTotal([m("/", 10), m("/x", 3)], false), 10);
  assert.equal(missingTotal([m("D:\\", 10), m("D:\\DCIM", 3)], true), 10);
});

test("「あとで」の鍵は並び順によらない", () => {
  assert.equal(laterKey([m("/A"), m("/B")]), laterKey([m("/B"), m("/A")]));
  assert.notEqual(laterKey([m("/A")]), laterKey([m("/A"), m("/B")]));
});

test("左ペインの印: 両方・見つからない・一時フォルダ・どちらでもない", () => {
  const missing = new Set(["/a", "/both"]);
  const temporary = new Set(["/t", "/both"]);
  assert.equal(rootMark("/a", missing, temporary), "missing");
  assert.equal(rootMark("/t", missing, temporary), "temporary");
  assert.equal(rootMark("/both", missing, temporary), "missingTemporary");
  assert.equal(rootMark("/plain", missing, temporary), null);
});

test("印ごとの見た目（クラスと説明の鍵）が取り違えられていない", () => {
  assert.deepEqual(ROOT_MARK_VIEW, {
    missing: { cls: "root-missing", tip: "rootMissingTip" },
    temporary: { cls: "root-temporary", tip: "rootTempTip" },
    missingTemporary: { cls: "root-missing", tip: "rootMissingTempTip" },
  });
  for (const { tip } of Object.values(ROOT_MARK_VIEW)) {
    assert.equal(typeof ja[tip], "function", tip);
    assert.ok(ja[tip]("/p/x").includes("/p/x"), `${tip} はパスを出す`);
  }
});
