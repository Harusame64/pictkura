/**
 * 辞書の検査。`npm --prefix ui test`。
 *
 * ## なぜ要るのか
 *
 * 辞書を守っているのは、いままで**キーの型だけ**だった（`Dict` を `ja.ts` から導出
 * しているので、抜けても余ってもコンパイルエラーになる）。**それ以外は何も見ていない。**
 * `${n}` を落とした訳も、桁区切りを通さない訳も、他言語から丸写しした訳も、
 * 型は通る。実際 2026-09-02 に測ったら、数を取る36キーのうち**36キー全部**が
 * 生の `${n}` を埋めていて、独語で12,000枚あると `12000 Fotos` と出ていた。
 *
 * **人のレビューは3回素通りしている**（PR #93 / #94 / #95）。機械の側にも同じ穴が
 * 空いていたので、ここで塞ぐ。
 *
 * ## なぜ依存を増やさないのか
 *
 * `index.ts` の冒頭の方針（ランタイムを増やさない）をテストにも通す。
 * **Node が `.ts` をそのまま読める**ので（22.18以降、型を剥がして実行）、
 * テストランナも変換器も要らない。`--experimental-` も要らない。
 *
 * その代わり **`src/i18n/` の中の相対 import には `.ts` を書いてある**
 * ——Node の ESM は拡張子を補わない。`tsconfig.json` の
 * `allowImportingTsExtensions` はそのため。
 *
 * ## 画面は動かさない
 *
 * ここが見るのは**辞書の中身だけ**で、`index.ts`（言語の選択・`Intl`）は読まない
 * ——`window` と `localStorage` が要るので、Nodeでは持ち込みが大きくなる。
 * `pickLocale()` の挙動は実機で確かめる側に置いたままにする（`plan.md` の
 * ルーティング実測の表）。
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { ja } from "../src/i18n/ja.ts";
import type { Dict } from "../src/i18n/ja.ts";
import { en } from "../src/i18n/en.ts";
import { de } from "../src/i18n/de.ts";
import { es } from "../src/i18n/es.ts";
import { AMERICAS_ONLY, AMERICAS_SWAPS, es419 } from "../src/i18n/es-419.ts";
import { zh } from "../src/i18n/zh.ts";
import { zhHant } from "../src/i18n/zh-hant.ts";
import { fr } from "../src/i18n/fr.ts";
import { it as itDict } from "../src/i18n/it.ts";
import { ru } from "../src/i18n/ru.ts";
import { pt } from "../src/i18n/pt.ts";
import { ko, koEuro } from "../src/i18n/ko.ts";
import { ar } from "../src/i18n/ar.ts";
import { he } from "../src/i18n/he.ts";
import { setNumberLocale } from "../src/i18n/plural.ts";
import { matchLocale } from "../src/i18n/match.ts";
import { firstWeekdayFromOs } from "../src/i18n/weekday.ts";
import { dirOf } from "../src/direction.ts";

type Any = Record<string, unknown>;

/**
 * 検査する辞書。**`index.ts` の `DICTS` と揃っていることを下のテストが見る**
 * （2026-09-02、ゲート2・3巡目）。ここへ足し忘れた言語は
 * **1つのテストにも掛からない**まま素通りするので、写しっぱなしにはしない。
 */
const DICTS: Record<string, Any> = {
  ja: ja as Any,
  en: en as Any,
  de: de as Any,
  es: es as Any,
  "es-419": es419 as Any,
  zh: zh as Any,
  "zh-hant": zhHant as Any,
  fr: fr as Any,
  it: itDict as Any,
  ru: ru as Any,
  pt: pt as Any,
  ko: ko as Any,
  ar: ar as Any,
  he: he as Any,
};

const KEYS = Object.keys(ja);

/**
 * 引数の型を `ja.ts` の**原文から**読む。
 *
 * 実行時のオブジェクトには型が残らないので、`number` を取るキーがどれかは
 * ソースを見るしかない。**`ja.ts` はキーの正**（ファイル冒頭にそう書いてある）なので、
 * ここを schema として読むのは筋が通っている。ついでに、
 * **関数なのに署名を読めないキーがあれば落ちる**（下の最初のテスト）。
 */
const SIGNATURES: Record<string, string[]> = (() => {
  const src = readFileSync(new URL("../src/i18n/ja.ts", import.meta.url), "utf8");
  const out: Record<string, string[]> = {};
  for (const m of src.matchAll(/^ {2}([A-Za-z][A-Za-z0-9]*): \(([^)]*)\) =>/gm)) {
    out[m[1]] = m[2]
      .split(",")
      .map((p) => p.split(":")[1]?.trim())
      .filter((t): t is string => Boolean(t));
  }
  return out;
})();

/** `index.ts` の原文。辞書の顔ぶれと、書式ロケールの注入を見るために読む */
const INDEX_SRC = readFileSync(new URL("../src/i18n/index.ts", import.meta.url), "utf8");

/** その言語で `n` を渡すキー（1つでも `number` を取るもの） */
const COUNT_KEYS = KEYS.filter((k) => SIGNATURES[k]?.includes("number"));

const call = (d: Any, key: string, args: unknown[]): string =>
  (d[key] as (...a: unknown[]) => string)(...args);

/** 署名どおりの引数を作る（`number` には数、`string` には文字列） */
const argsFor = (key: string, n: number, s: string): unknown[] =>
  (SIGNATURES[key] ?? []).map((t) => (t === "number" ? n : s));

test("関数のキーは、ja.ts から署名を読めること", () => {
  const unparsed = KEYS.filter(
    (k) => typeof (ja as Any)[k] === "function" && !SIGNATURES[k],
  );
  assert.deepEqual(
    unparsed,
    [],
    "ja.ts の書き方が変わって署名を読めなくなった（引数を複数行に折ったなど）。" +
      "この file の SIGNATURES の正規表現を直すこと",
  );
});

/**
 * **Rust が使う鍵が、辞書に在ること。**
 *
 * `src-tauri/src/errs.rs` の約束で、Rust 側は**辞書のキーそのもの**を
 * 失敗の頭に付けて返す（週の台紙の項目3）。鍵を足して辞書へ足し忘れると、
 * **画面に `errSourceUnreadable` という字がそのまま出る**——型もコンパイルも
 * 見ていない道なので、ここで見る。
 *
 * 探すのは `errs::code("...")` / `errs::coded("...", …)` と、`errs.rs` の
 * `fn code()` が返す文字列。**Rust を読むのはこの1本だけ**で、
 * 見つからなければ**テストごと失敗させる**（黙って0件を通さない）。
 */
test("Rustが使う失敗の鍵が、辞書に在ること", () => {
  const lib = readFileSync(
    new URL("../../src-tauri/src/lib.rs", import.meta.url),
    "utf8",
  );
  const errs = readFileSync(
    new URL("../../src-tauri/src/errs.rs", import.meta.url),
    "utf8",
  );
  const used = new Set<string>();
  // **`errs.rs` は鍵しか持たない**（`Coded` の実装が並ぶだけ）ので、
  // `"err…"` を全部拾ってよい——`fn code() { "errDb" }` のような裸の返しも取れる
  for (const m of errs.matchAll(/"(err[A-Za-z0-9]+)"/g)) used.add(m[1]);
  // **`lib.rs` は絞る。** あちらには鍵でない `"err…"` が入りうる
  // （`"error"`・`"errno"` のような字。拾うとテストが**嘘の理由で落ちる**・ゲート2）。
  // 拾うのは**鍵として使われている形だけ**——`errs::code(…)` / `errs::coded(…)` と、
  // 定数に置いた鍵（`const X: &str = "err…"`）
  for (const m of lib.matchAll(/errs::coded?\(\s*"(err[A-Za-z0-9]+)"/g)) used.add(m[1]);
  for (const m of lib.matchAll(/:\s*&str\s*=\s*"(err[A-Za-z0-9]+)"/g)) used.add(m[1]);
  // **黙って0件を通さない。** 実際は20近く在るので、探し方が壊れたら気付ける
  assert.ok(used.size > 10, `Rust 側の鍵が ${used.size} 件しか読めていない`);
  for (const code of used) {
    assert.ok(
      KEYS.includes(code),
      `Rust が使う鍵 ${code} が ja.ts に無い（画面に鍵の字が出る）`,
    );
  }
});

test("どの辞書もキーが揃っていること", () => {
  // 型は既にこれを見ているが、**実体でも見る**——`Dict` に `as` を当てた瞬間に
  // 型の網は抜ける（`zh.ts` などは `: Dict` を付けているので効いているが、
  // 付け忘れは目で見ないと分からない）
  for (const [lang, d] of Object.entries(DICTS)) {
    assert.deepEqual(
      Object.keys(d).slice().sort(),
      KEYS.slice().sort(),
      `${lang} のキーが ja と違う`,
    );
  }
});

test("引数を落とした訳が無いこと", () => {
  // **訳しているうちに `${n}` ごと消える**のがいちばん静かな壊れ方で、
  // 型でもコンパイルでも出ない。引数を1つずつ動かして、出力が動くかを見る
  for (const [lang, d] of Object.entries(DICTS)) {
    for (const key of KEYS) {
      if (typeof d[key] !== "function") continue;
      const types = SIGNATURES[key] ?? [];
      const base = types.map((t, i) => (t === "number" ? 11 + i : `A${i}`));
      for (let i = 0; i < types.length; i++) {
        const moved = base.slice();
        moved[i] = types[i] === "number" ? 97 : "ZZZ";
        assert.notEqual(
          call(d, key, moved),
          call(d, key, base),
          `${lang}.${key} が ${i + 1} 番目の引数を使っていない（訳から落ちた？）`,
        );
      }
    }
  }
});

test("件数が桁区切りを通ること", () => {
  // **暦の年だけは通さない**——独語で `2.019` になる
  const YEARS = new Set(["jumpToYear"]);
  // **「生の数字が無いこと」では足りない**（2026-09-02、ゲート2・3巡目）。
  // 辞書が裸の `n.toLocaleString()` を呼ぶと、Nodeの既定ロケールで `1,234,567` になり、
  // `1234567` を含まないので素通りする——それは `plural.ts` の冒頭が塞いだはずの穴
  // （OSが `es-MX` なら同じ画面に `12,345` と `12.345` が並ぶ）そのもの。
  // **de-DE が出す綴りを含んでいること**まで見る
  const want = new Intl.NumberFormat("de-DE").format(1234567);
  setNumberLocale("de-DE");
  try {
    for (const [lang, d] of Object.entries(DICTS)) {
      for (const key of COUNT_KEYS) {
        if (YEARS.has(key)) continue;
        const out = call(d, key, argsFor(key, 1234567, "X"));
        assert.ok(
          out.includes(want),
          `${lang}.${key} が \`num(n)\` を通っていない（欲しいのは ${want}）: ${out}`,
        );
      }
    }
    // 逆も見る。年に桁区切りが付いたら、それはそれで壊れている
    for (const [lang, d] of Object.entries(DICTS)) {
      for (const key of YEARS) {
        assert.ok(
          call(d, key, argsFor(key, 2019, "X")).includes("2019"),
          `${lang}.${key} は暦の年なので桁区切りを付けないこと`,
        );
      }
    }
  } finally {
    setNumberLocale("en-US");
  }
});

/**
 * **1のときに語形が変わる「キーと引数の位置」**。単複のある言語だけ。
 *
 * 綴りは `キー:位置`（位置は `ja.ts` の署名で数えた `number` 引数の**通し番号ではなく
 * 引数そのものの番号**）。**位置まで持つ**のは、数を2つ以上取るキーで
 * 片方だけ単数形が抜けるのを拾うため（2026-09-02、ゲート2・3巡目）——
 * 全部の数に同じ値を渡していたころは `deletedSomeLeft(1, 1)` と `(2, 2)` が
 * 同じだけ動いてしまい、**2つ目が複数形のまま固まっていても通っていた**。
 *
 * これは「正しい訳の一覧」ではなく**いまの姿の写し**で、意味は2つ:
 *
 * - **減ったら落ちる**——文言を書き直した拍子に `one()` が外れると出る
 * - **数を取るキーを足したら落ちる**——足した人が、その言語で単数形が要るかを
 *   1度は考えることになる
 *
 * **載っていない ＝ 壊れている、ではない。** `photosCount`（数だけ）・
 * `importing`（`3/10` の分数）のように、変わらないのが正解のものがある。
 * 到達しないものもある（`wizardTruncated` は `TREE_LIMIT` でしか出ない。理由は `ja.ts` に）。
 */
const INFLECTS: Record<string, string[]> = {
  // ヘブライ語は1か、それ以外か（`one()`）。件数の報告は「ラベル: N」で書くので（`הועברו: 3`）、
  // 過去の動詞の性と数の一致が要らず、ここに無い。2 の形（双数）は下の升が見る
  he: [
    "burstTitle:0",
    "burstTitle:2",
    "burstTitleRawJpeg:0",
    "burstTitleRawJpeg:2",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "googleSendLeftOut:0",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSentLeftOut:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "printSheetTitle:0",
    "printSkipped:0",
    "printTooMany:0",
    "rejectGateConfirm:0",
    "rejectGateTitle:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "stackRawChipTitle:0",
  ],
  // アラビア語は6形（`arabic()`）。ここは 1 と 2 の差だけ。3〜10・11〜99・100〜 は下の升が見る。
  // 数えた物の一致を避けて「ラベル: N」と言う文（`المحدد: 3`）は 1 でも形が動かないので、ここに無い
  ar: [
    "bulkFavoriteDone:0",
    "bulkPickDone:0",
    "bulkUnfavoriteDone:0",
    "bulkUnpickDone:0",
    "burstTitle:0",
    "burstTitle:2",
    "burstTitleRawJpeg:0",
    "burstTitleRawJpeg:2",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedMissing:0",
    "deletedSomeLeft:0",
    "errTrashPartly:0",
    "exportDone:0",
    "googleSendLeftOut:0",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSentLeftOut:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "moveDone:0",
    "printSheetTitle:0",
    "printSkipped:0",
    "printTooMany:0",
    "rejectGateConfirm:0",
    "rejectGateTitle:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "stackRawChipTitle:0",
  ],
  // 葡語（pt-BR）は 1 だけ単数。0 は複数（`0 fotos`）——CLDR の pt は 0 も one だが、ブラジルの画面の書き方に合わせた（pt.ts の冒頭）
  pt: [
    "bgThumbs:0",
    "bgThumbsShort:0",
    "bulkFavoriteDone:0",
    "bulkPickDone:0",
    "bulkUnfavoriteDone:0",
    "bulkUnpickDone:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedMissing:0",
    "deletedMissing:1",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "drivesReturnedScanned:0",
    "drivesReturnedScanned:1",
    "errTrashPartly:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "googleRemoveFailed:0",
    "googleRemoved:0",
    "googleSendLeftOut:0",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSent:0",
    "googleSent:1",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "importDone:0",
    "importDone:1",
    "importGoogleFailed:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "moveDone:0",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "moveDone:4",
    "printSheetTitle:0",
    "printSkipped:0",
    "rejectGateTitle:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "selectedCount:0",
    "syncDone:0",
    "syncDone:1",
    "syncDone:2",
    "wizardEtaMinutes:0",
    "wizardEtaSeconds:0",
    "wizardHiddenCount:0",
    "wizardSelected:0",
  ],
  en: [
    "printSheetTitle:0",
    "printSkipped:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deletedSomeLeft:1",
    "moveConfirm:0",
    "exportDone:0",
    "moveDone:0",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSendLeftOut:0",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
  ],
  de: [
    "printSheetTitle:0",
    "printSkipped:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "andMore:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "wizardHiddenCount:0",
    "deleteConfirm:0",
    "deletedSomeLeft:1",
    "deletedMissing:1",
    "moveConfirm:0",
    "exportDone:0",
    "moveDone:0",
    "exportDone:1",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "exportDone:3",
    "moveDone:4",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "importGoogleFailed:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSendLeftOut:0",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "googleRemoveFailed:0",
  ],
  es: [
    "printSheetTitle:0",
    "printSkipped:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "importDone:0",
    "importDone:1",
    "drivesReturnedScanned:0",
    "drivesReturnedScanned:1",
    "syncDone:0",
    "syncDone:1",
    "syncDone:2",
    "wizardSelected:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "wizardHiddenCount:0",
    "wizardEtaSeconds:0",
    "wizardEtaMinutes:0",
    "wizardMoreFiles:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "deletedMissing:0",
    "deletedMissing:1",
    "selectedCount:0",
    "moveConfirm:0",
    "exportDone:0",
    "moveDone:0",
    "exportDone:1",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "exportDone:3",
    "moveDone:4",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bgThumbs:0",
    "bgThumbsShort:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "importGoogleFailed:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSendLeftOut:0",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "googleSent:1",
    "googleSent:0",
    "googleRemoved:0",
    "googleRemoveFailed:0",
  ],
  // **中南米版は本国と同じ活用**（差分は語彙だけで、単複の作りは変えていない）
  "es-419": [
    "printSheetTitle:0",
    "printSkipped:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "importDone:0",
    "importDone:1",
    "drivesReturnedScanned:0",
    "drivesReturnedScanned:1",
    "syncDone:0",
    "syncDone:1",
    "syncDone:2",
    "wizardSelected:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "wizardHiddenCount:0",
    "wizardEtaSeconds:0",
    "wizardEtaMinutes:0",
    "wizardMoreFiles:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "deletedMissing:0",
    "deletedMissing:1",
    "selectedCount:0",
    "moveConfirm:0",
    "exportDone:0",
    "moveDone:0",
    "exportDone:1",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "exportDone:3",
    "moveDone:4",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bgThumbs:0",
    "bgThumbsShort:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "importGoogleFailed:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSendLeftOut:0",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "googleSent:1",
    "googleSent:0",
    "googleRemoved:0",
    "googleRemoveFailed:0",
  ],
  // **0 も単数**（仏語の規則）。ここが見るのは 1 と 2 の差だけなので、0 の扱いは `fr.ts` の `un()` を見ること
  fr: [
    "andMore:0",
    "bulkFavoriteDone:0",
    "bulkPickDone:0",
    "bulkUnfavoriteDone:0",
    "bulkUnpickDone:0",
    "burstTitle:0",
    "burstTitle:2",
    "burstTitleRawJpeg:0",
    "burstTitleRawJpeg:2",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedMissing:0",
    "deletedMissing:1",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "drivesReturnedScanned:0",
    "drivesReturnedScanned:1",
    "errTrashPartly:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "googleRemoveFailed:0",
    "googleRemoved:0",
    "googleSendLeftOut:0",
    "googleSendSummary:0",
    "googleSendSummary:1",
    "googleSendSummary:2",
    "googleSent:0",
    "googleSent:1",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "importDone:0",
    "importDone:1",
    "importGoogle:0",
    "importGoogleFailed:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "moveDone:0",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "moveDone:4",
    "printSheetTitle:0",
    "printSkipped:0",
    "printTooMany:0",
    "rejectGateTitle:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "selectedCount:0",
    "showMore:0",
    "stackRawChipTitle:0",
    "syncDone:0",
    "syncDone:1",
    "syncDone:2",
    "wizardHiddenCount:0",
    "wizardSelected:0",
  ],
  // **`foto` / `video` / `file` は単複同形**。英語より少ないキーは名詞だけの文（`Stampa 2 foto`）で、動くのは冠詞・形容詞・分詞
  it: [
    "andMore:0",
    "bgThumbs:0",
    "bulkFavoriteDone:0",
    "bulkPickDone:0",
    "bulkUnfavoriteDone:0",
    "burstTitle:0",
    "burstTitleRawJpeg:0",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedMissing:0",
    "deletedMissing:1",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "drivesReturnedScanned:0",
    "drivesReturnedScanned:1",
    "errTrashPartly:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:2",
    "exportDone:3",
    "googleRemoveFailed:0",
    "googleRemoved:0",
    "googleSendLeftOut:0",
    "googleSendSummary:2",
    "googleSent:0",
    "googleSent:1",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "importDone:0",
    "importDone:1",
    "importFailed:0",
    "importGoogle:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "moveDone:0",
    "moveDone:1",
    "moveDone:2",
    "moveDone:3",
    "moveDone:4",
    "printSkipped:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "selectedCount:0",
    "showMore:0",
    "syncDone:0",
    "syncDone:1",
    "syncDone:2",
    "wizardEtaMinutes:0",
    "wizardEtaSeconds:0",
    "wizardHiddenCount:0",
    "wizardMoreFiles:0",
    "wizardSelected:0",
  ],
  // **露語は3形**（`oneFewMany`）。ここは 1 と 2 の差だけ。2 と 5 の差は下の `FEW_MANY`
  ru: [
    "bulkFavoriteDone:0",
    "bulkPickDone:0",
    "bulkUnfavoriteDone:0",
    "burstTitle:0",
    "burstTitle:2",
    "burstTitleRawJpeg:0",
    "burstTitleRawJpeg:2",
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "deleted:0",
    "deletedMissing:0",
    "deletedMissing:1",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "errTooManyIds:0",
    "errTrashPartly:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "googleRemoveFailed:0",
    "googleRemoved:0",
    "googleSendLeftOut:0",
    "googleSendSummary:2",
    "googleSent:0",
    "googleSent:1",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "importGoogleFailed:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveConfirm:0",
    "moveDone:0",
    "moveDone:2",
    "moveDone:3",
    "moveDone:4",
    "printSkipped:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "selectedCount:0",
    "stackRawChipTitle:0",
    "wizardHiddenCount:0",
    "wizardSelected:0",
  ],
};

/**
 * **単複を持たない言語**。`one()` を写してはいけない（英語の辞書を下敷きにすると入り込む）。
 *
 * `INFLECTS` と合わせて `DICTS` を覆い切る。**覆えていなければ下のテストが落ちる。**
 */
const NO_PLURAL = ["ja", "zh", "zh-hant", "ko"];

/**
 * **2〜4 と 5〜 で形が変わる位置**（露語の few と many）。`INFLECTS` は 1 と 2 しか比べないので、
 * 英語から `one()` を写して `5 файла`（正しくは `5 файлов`）と出るキーを見逃す（2026-11-03）。
 *
 * `INFLECTS.ru` にあってここに無いキーは、**2 と 5 で同じ形でよいもの**だけ:
 * `фото` が不変化で、変わるのが 1 のときの指示語（`это фото`）か、指し直す代名詞（`его` / `их`）だけの文。
 * `googleSendSummary` の関係節・`errTooManyIds` の属格も 2 と 5 で同じ
 */
const FEW_MANY: Record<string, string[]> = {
  ru: [
    "bulkPickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "burstTitle:0",
    "burstTitle:2",
    "burstTitleRawJpeg:0",
    "burstTitleRawJpeg:2",
    "deleted:0",
    "deletedMissing:0",
    "deletedMissing:1",
    "deletedSomeLeft:0",
    "deletedSomeLeft:1",
    "errTrashPartly:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "googleRemoveFailed:0",
    "googleRemoved:0",
    "googleSendLeftOut:0",
    "googleSent:0",
    "googleSent:1",
    "googleSentCloudOnly:0",
    "googleSentLeftOut:0",
    "importGoogleFailed:0",
    "itemsCount:0",
    "memoriesTitle:0",
    "moveDone:0",
    "moveDone:3",
    "moveDone:4",
    "printSkipped:0",
    "rootMissingNotice:1",
    "rootsMissingNotice:1",
    "selectedCount:0",
    "stackRawChipTitle:0",
    "wizardHiddenCount:0",
    "wizardSelected:0",
  ],
};

/**
 * **AppKit の「Cancel」の訳**（macOS 27 の `AppKit.framework/Resources/Common.loctable`、2026-11-03 に写した）。
 * macOS の確認画面は、題がこれと同じボタンにだけ Esc を割り当てる（`confirmLabels.ts`、#147）。
 * 露語の辞書が `Отмена` で、露語の Mac で Esc が効かなかった（ゲート2）。言語を足したら、ここに
 * `plutil -convert json -o - …/Common.loctable` の `<言語>.Cancel` を写す
 */
const APPKIT_CANCEL: Record<string, string> = {
  ja: "キャンセル",
  en: "Cancel",
  de: "Abbrechen",
  es: "Cancelar",
  "es-419": "Cancelar",
  zh: "取消",
  "zh-hant": "取消",
  fr: "Annuler",
  it: "Annulla",
  ru: "Отменить",
  pt: "Cancelar",
  ko: "취소",
  ar: "إلغاء",
  he: "ביטול",
};

test("確認画面のキャンセルの語は、AppKit の Cancel と同じであること（macOS の Esc）", () => {
  assert.deepEqual(Object.keys(APPKIT_CANCEL).sort(), Object.keys(DICTS).sort(), "辞書を足したら APPKIT_CANCEL にも写すこと");
  for (const [lang, word] of Object.entries(APPKIT_CANCEL)) {
    assert.equal(DICTS[lang].confirmCancel, word, `${lang}.confirmCancel が AppKit の Cancel と違う（macOS で Esc が効かない）`);
  }
});

test("3形の言語は、1の位と下2桁で形を決めること（11〜14 は 5〜、22 は 2〜4）", () => {
  // 2 と 5 の比べだけでは、手書きの `n < 5 ? few : many`（12 を few、22 を many にする）が通る。
  // 数の位置ごとに、11・12 が 5 と、22 が 2 と同じ形になることを見る（ほかの数は 2 に固定）
  for (const lang of Object.keys(FEW_MANY)) {
    const d = DICTS[lang];
    for (const key of COUNT_KEYS) {
      const types = SIGNATURES[key] ?? [];
      types.forEach((t, i) => {
        if (t !== "number") return;
        const at = (n: number) => shape(call(d, key, types.map((u, j) => (u !== "number" ? "X" : j === i ? n : 2))));
        assert.equal(at(11), at(5), `${lang}.${key} の ${i} 番目: 11 が 5〜 の形でない`);
        assert.equal(at(12), at(5), `${lang}.${key} の ${i} 番目: 12 が 5〜 の形でない`);
        assert.equal(at(22), at(2), `${lang}.${key} の ${i} 番目: 22 が 2〜4 の形でない`);
      });
    }
  }
});

/**
 * **21 が 1 と違う形でよい位置**（露語）。数詞の一致では 21 は 1 と同じ単数だが、
 * 指し直す代名詞・名詞（`его`/`их`、`оригинал`/`оригиналы`、`который`/`которые`、`миниатюру`/`миниатюры`）は
 * 1 のときだけ単数（`ru.ts` の `pron`）。`deleteConfirm` / `moveConfirm` は 1 のときだけ「это фото」と指す
 */
const REFERS_BACK: Record<string, string[]> = {
  ru: [
    "decoderHeifNotice:0",
    "decoderHeifNoticeMac:0",
    "decoderHeifNoticeOther:0",
    "deleteConfirm:0",
    "googleSendSummary:2",
    "googleSentCloudOnly:0",
    "moveConfirm:0",
    "moveDone:2",
    "moveDone:3",
    "printSkipped:0",
  ],
};

test("3形の言語は、21 を 1 と同じ形にすること（指し直す語の位置を除く）", () => {
  // 11/12/22 と 2/5 の比べでは、`n === 1 ? 単数 : …` と書いた単数（21 で `21 файлов` になる）が通る
  for (const [lang, allowed] of Object.entries(REFERS_BACK)) {
    const d = DICTS[lang];
    const differs: string[] = [];
    for (const key of COUNT_KEYS) {
      const types = SIGNATURES[key] ?? [];
      types.forEach((t, i) => {
        if (t !== "number") return;
        const at = (n: number) => shape(call(d, key, types.map((u, j) => (u !== "number" ? "X" : j === i ? n : 2))));
        if (at(21) !== at(1)) differs.push(`${key}:${i}`);
      });
    }
    assert.deepEqual(differs.sort(), allowed.slice().sort(), `${lang}: 21 と 1 の形が違う位置が変わった`);
  }
});

test("3形の言語は、2〜4 と 5〜 で形を変えること", () => {
  for (const [lang, expected] of Object.entries(FEW_MANY)) {
    assert.deepEqual(
      fewManyOf(DICTS[lang]).slice().sort(),
      expected.slice().sort(),
      `${lang} で 2 と 5 の形が変わる「キー:位置」が変わった。減ったなら one() を写していないか見ること`,
    );
  }
});

/**
 * 1のときだけ**言い回しを変える**キー。単複ではないので、`NO_PLURAL` の言語にもある。
 *
 * `deleteConfirm` / `moveConfirm` は1枚のときだけ「この写真を」と指示語に置き換えている
 * （`2枚の写真を` に対して `1枚の写真を` とは言わない、という日本語・中国語の言い回し）。
 * 英語も同じ形にしてある。
 */
const PHRASING = ["deleteConfirm", "moveConfirm"];

/** 数字そのものは伏せて、**語形だけ**を比べる */
const shape = (s: string) => s.replace(/[\d., ]+/g, "#");

/** 2 と 5 で形が変わる位置（露語の few と many）。ほかの数は 2 に固定する */
const fewManyOf = (d: Any): string[] => {
  const out: string[] = [];
  for (const key of COUNT_KEYS) {
    const types = SIGNATURES[key] ?? [];
    const few = call(d, key, types.map((t) => (t === "number" ? 2 : "X")));
    types.forEach((t, i) => {
      if (t !== "number") return;
      const many = types.map((u, j) => (u !== "number" ? "X" : j === i ? 5 : 2));
      if (shape(call(d, key, many)) !== shape(few)) out.push(`${key}:${i}`);
    });
  }
  return out;
};

/** その辞書で「1にすると語形が動く」引数の位置を、`キー:位置` で並べる */
const inflectionsOf = (d: Any): string[] => {
  const out: string[] = [];
  for (const key of COUNT_KEYS) {
    const types = SIGNATURES[key] ?? [];
    const plural = call(d, key, types.map((t) => (t === "number" ? 2 : "X")));
    types.forEach((t, i) => {
      if (t !== "number") return;
      const one = types.map((u, j) => (u !== "number" ? "X" : j === i ? 1 : 2));
      if (shape(call(d, key, one)) !== shape(plural)) out.push(`${key}:${i}`);
    });
  }
  return out;
};

test("どの辞書も、単複の扱いが決められていること", () => {
  // **足した言語が黙って検査の外へ出ない**（2026-09-02、ゲート2・3巡目）。
  // 下の2つのテストは表を回すので、表に載っていない言語は**1度も見られない**
  // ——`en.ts` を丸写しした7つ目の辞書が、全部通る状態になっていた
  const classified = [...Object.keys(INFLECTS), ...NO_PLURAL].sort();
  assert.deepEqual(
    Object.keys(DICTS).slice().sort(),
    classified,
    "辞書を足したら INFLECTS か NO_PLURAL のどちらかに入れること",
  );
  const compared = [...Object.keys(SAME_AS_EN), "en"].sort();
  assert.deepEqual(
    Object.keys(DICTS).slice().sort(),
    compared,
    "辞書を足したら SAME_AS_EN にも入れること（英語からの丸写しを見る表）",
  );
});

test("検査する辞書が index.ts の DICTS と揃っていること", () => {
  // この file は `index.ts` を import しない（`window` が要る）ので、
  // **顔ぶれだけ原文から読んで突き合わせる**。片方に足して片方を忘れると、
  // アプリには出るのに検査はされない言語ができる
  const decl = /const DICTS: Record<string, Dict> = \{([^}]*)\}/.exec(INDEX_SRC);
  assert.ok(decl, "index.ts の DICTS を読めなかった（綴りが変わった？）");
  const inIndex = decl[1]
    .split(",")
    .map((part) => part.split(":")[0].trim().replace(/^"|"$/g, ""))
    .filter(Boolean);
  assert.deepEqual(
    inIndex.slice().sort(),
    Object.keys(DICTS).slice().sort(),
    "index.ts の DICTS と、この file の DICTS が食い違っている",
  );
});

test("書式ロケールが辞書へ渡されていること", () => {
  // `num()` は渡されるまで桁区切りを付けない（`plural.ts`）。**渡す1行が消えたり
  // `formatLocale` より前へ動いたりすると、静かに `12000 Fotos` へ戻る**
  // ——例外も型エラーも出ないので、ここで見る（2026-09-02、ゲート2・3巡目）
  const defined = INDEX_SRC.indexOf("export const formatLocale");
  const injected = INDEX_SRC.indexOf("setNumberLocale(formatLocale);");
  assert.ok(defined >= 0, "index.ts の formatLocale を読めなかった");
  assert.ok(injected >= 0, "index.ts が setNumberLocale(formatLocale) を呼んでいない");
  assert.ok(
    injected > defined,
    "setNumberLocale(formatLocale) が formatLocale の定義より前に居る（TDZで落ちる）",
  );
});

test("単数形を持つキーの顔ぶれが変わっていないこと", () => {
  for (const [lang, expected] of Object.entries(INFLECTS)) {
    assert.deepEqual(
      inflectionsOf(DICTS[lang]).slice().sort(),
      expected.slice().sort(),
      `${lang} で単数形を持つ「キー:位置」が変わった。増えたなら INFLECTS に足す。` +
        `減ったなら one() が外れていないか見ること`,
    );
  }
});

/**
 * **仏語は 0 も単数**（`fr.ts` の `un()`）。上の顔ぶれの升は 1 と 2 しか比べないので、
 * `un()` を共通の `one()`（0 は複数）に書き換えても落ちない（2026-11-03、ゲート2・3巡目）。
 * 単数形を持つ位置ごとに、**0 が 2（複数）と同じ文にならない**ことを見る
 * （1 と比べないのは、1 だけ「cette photo」と数を言わないキーがあるため）
 */
test("仏語は 0 を単数で言うこと", () => {
  const digitless = (s: string) => s.replace(/\d+/g, "#");
  for (const pair of INFLECTS.fr) {
    const [key, at] = pair.split(":");
    const pos = Number(at);
    const make = (n: number) => {
      const args = argsFor(key, 2, "X");
      args[pos] = n;
      return call(DICTS.fr, key, args);
    };
    assert.notEqual(digitless(make(0)), digitless(make(2)), `fr.${key} の ${pos} 番目が 0 で複数になった`);
  }
});

/**
 * **葡語（pt-BR）は 0 を複数で言う**（`0 fotos`。`pt.ts` の冒頭）。CLDR の `pt` は 0 も単数なので、
 * 「CLDR に合わせる」直しや仏語の `un()` の持ち込みで `0 foto` に戻っても、1 と 2 しか比べない
 * 顔ぶれの升は落ちない。仏語の升の裏返しで、**0 が 2（複数）と同じ文になる**ことを見る
 */
test("葡語は 0 を複数で言うこと", () => {
  let checked = 0;
  const digitless = (s: string) => s.replace(/\d+/g, "#");
  for (const pair of INFLECTS.pt) {
    const [key, at] = pair.split(":");
    const pos = Number(at);
    const make = (n: number) => {
      const args = argsFor(key, 2, "X");
      args[pos] = n;
      return call(DICTS.pt, key, args);
    };
    // 0 のときは節ごと出さない文がある（`exportDone` の「0 枚書き出した」）。数の数が合うものだけ比べる
    const hashes = (t: string) => (digitless(t).match(/#/g) ?? []).length;
    if (hashes(make(0)) !== hashes(make(2))) continue;
    checked++;
    assert.equal(digitless(make(0)), digitless(make(2)), `pt.${key} の ${pos} 番目が 0 で単数になった`);
  }
  // 飛ばしすぎて空振りしていないこと
  assert.ok(checked >= 40, `only ${checked}`);
});

/**
 * **アラビア語の6形は、下2桁で選ばれていること**（`arabic()` の規則が辞書の文まで届いているか）。
 * 1 と 2 しか比べない上の升では、`few`（3〜10）と `many`（11〜99）と `other`（100〜）の取り違えを見られない。
 * 単数形を持つ位置ごとに、**下2桁が同じ数は同じ形**（3 と 103、11 と 111、100 と 200）で、
 * **few と many は違う形**（3 と 11）であることを見る
 */
test("アラビア語は、数の形を下2桁で選ぶこと（3〜10・11〜99・100〜）", () => {
  let checked = 0;
  for (const pair of INFLECTS.ar) {
    const [key, at] = pair.split(":");
    const pos = Number(at);
    const make = (n: number) => {
      const args = argsFor(key, 2, "X");
      args[pos] = n;
      return shape(call(DICTS.ar, key, args));
    };
    // 0 で節ごと消える文は、数の数が変わるので飛ばす
    if (make(3).split("#").length !== make(11).split("#").length) continue;
    checked++;
    assert.equal(make(103), make(3), `ar.${key} の ${pos} 番目: 103 が 3〜10 の形でない`);
    assert.equal(make(111), make(11), `ar.${key} の ${pos} 番目: 111 が 11〜99 の形でない`);
    assert.equal(make(200), make(100), `ar.${key} の ${pos} 番目: 200 が 100 と違う形`);
    assert.notEqual(make(3), make(11), `ar.${key} の ${pos} 番目: 3 と 11 が同じ形（複数と単数の対格を分けていない）`);
  }
  assert.ok(checked >= 20, `only ${checked}`);
});

test("単複の無い言語が場合分けしていないこと", () => {
  for (const lang of NO_PLURAL) {
    const unexpected = inflectionsOf(DICTS[lang]).filter(
      (pair) => !PHRASING.includes(pair.split(":")[0]),
    );
    assert.deepEqual(
      unexpected,
      [],
      `${lang} が1のときだけ語形を変えている（この言語に単複は無い）`,
    );
  }
});

/**
 * **英語と同じ綴りでよいキー**。製品名・単位・記号・そのまま通じる語、
 * それに**数だけを出す関数**（`3` / `✕ 3` / `3+` に訳す余地は無い）。
 *
 * ここに無いのに英語と一致していたら、**訳し漏れ**の疑いがある。
 * **`DICTS` の全言語（英語自身を除く）を覆うこと**——覆えていなければ上のテストが落ちる。
 */
const SAME_AS_EN: Record<string, string[]> = {
  he: [
    "actualSizeBadge", // `1:1`
    "appName",
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "exifIso",
    "keyCtrl",
    "kindRaw",
    "listSeparator", // ヘブライ語の読点も `,`
    "photosCount", // 数だけ
    "rejectChip", // 記号と数だけ
    "wizardCapped", // 数だけ
  ],
  ar: [
    "actualSizeBadge", // `1:1`
    "appName",
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "exifIso",
    "keyCtrl",
    "kindRaw",
    "photosCount", // 数だけ
    "rejectChip", // 記号と数だけ
    "wizardCapped", // 数だけ
  ],
  ko: [
    "appName",
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "exifIso",
    "keyCtrl",
    "kindRaw",
    "listSeparator",
    "rejectChip", // 記号と数だけ
  ],
  pt: [
    "actualSizeBadge", // `1:1`
    "appName",
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "burstGapOption", // `0,5 s` など数と単位だけ
    "exifIso",
    "keyCtrl",
    "kindRaw",
    "listSeparator", // `, `
    "photosCount", // 数だけ
    "rejectChip", // 記号と数だけ
    "settingsManual", // `Manual` は葡語の語でもある
    "wizardCapped", // 数だけ
  ],
  de: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "burstGapOption", // `0,5 s` など数と単位だけ
    "appName",
    "kindRaw",
    "kindVideo", // 独語でも `Videos`
    "actualSizeBadge", // `1:1`
    "exifIso",
    "wizardTitle", // `Import` は独語の語でもある
    "fileInfoName", // `Name` は独語の語でもある
    "listSeparator", // `, `
    "photosCount", // 数だけ
    "rejectChip", // `✕ 3`
    "wizardCapped", // `3+`
  ],
  es: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "burstGapOption", // `0,5 s` など数と単位だけ
    "appName",
    "kindRaw",
    "keyCtrl",
    "actualSizeBadge",
    "exifIso",
    "listSeparator",
    "settingsManual", // `Manual` は西語の語でもある
    "photosCount",
    "rejectChip",
    "wizardCapped",
  ],
  "es-419": [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "burstGapOption", // `0,5 s` など数と単位だけ
    "appName",
    "kindRaw",
    "keyCtrl",
    "actualSizeBadge",
    "exifIso",
    "listSeparator",
    "settingsManual", // `Manual` は西語の語でもある
    "photosCount",
    "rejectChip",
    "wizardCapped",
    "kindVideo", // **中南米では `Videos`**（本国は `Vídeos`）。英語と同じ綴りになる
  ],
  ja: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "appName",
    "kindRaw",
    "exifIso",
    "keyCtrl",
    "rejectChip",
  ],
  zh: ["burstChipShort", "appName", "kindRaw", "keyCtrl", "actualSizeBadge", "exifIso", "rejectChip"],
  "zh-hant": ["burstChipShort", "appName", "kindRaw", "keyCtrl", "actualSizeBadge", "exifIso", "rejectChip"],
  fr: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "appName",
    "kindRaw",
    "keyCtrl",
    "actualSizeBadge",
    "exifIso",
    "listSeparator",
    "photosCount",
    "rejectChip",
    "wizardCapped",
    "viewThumbnails", // `Photos` は仏語の語でもある
    "kindPhoto", // 同上
    "extractFilter", // `Image`
    "paletteGroupActions", // `Actions`
    "wizardSources", // `Source`
    "wizardDestination", // `Destination`
    "settingsDestination", // 同上
    "fileInfoDimensions", // `Dimensions`
    "fileInfoType", // `Type`
  ],
  it: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "burstGapOption", // `0,5 s` など数と単位だけ
    "appName",
    "kindRaw",
    "keyCtrl",
    "actualSizeBadge",
    "exifIso",
    "fileInfoTitle", // `File` は伊語でも使う語
    "listSeparator",
    "photosCount",
    "rejectChip",
    "wizardCapped",
  ],
  ru: [
    "burstChipShort", // `▤ 12`: 記号と数だけ
    "appName",
    "kindRaw",
    "keyCtrl",
    "actualSizeBadge",
    "exifIso",
    "listSeparator",
    "photosCount",
    "rejectChip",
    "wizardCapped",
  ],
};

test("英語からの丸写しが無いこと", () => {
  // **関数のキーも見る**（2026-09-02、ゲート2の指摘）。文字列だけを比べていたが、
  // **桁区切りが抜けていた36キーは全部が関数**で、いちばん写されやすいのもそちら。
  // 呼んで出力で比べれば、英語の関数をそのまま写した7つ目の辞書がここで止まる
  const e = en as Any;
  const rendered = (d: Any, k: string): string | null => {
    if (typeof e[k] === "string") return (e[k] as string).length > 1 ? (d[k] as string) : null;
    if (typeof e[k] !== "function") return null;
    return call(d, k, argsFor(k, 3, "X"));
  };
  for (const [lang, allowed] of Object.entries(SAME_AS_EN)) {
    const d = DICTS[lang];
    const same = KEYS.filter((k) => {
      const mine = rendered(d, k);
      return mine !== null && mine === rendered(e, k);
    });
    assert.deepEqual(
      same.slice().sort(),
      allowed.slice().sort(),
      `${lang} で英語と同じ文字列の顔ぶれが変わった。訳し忘れなら訳す。` +
        `製品名・単位・数だけの関数でよいなら SAME_AS_EN に理由を添えて足す`,
    );
  }
});

/**
 * **週の始まりだけは `Intl` に任せていない**ので、その受け取り口を見る。
 *
 * `index.ts` は `window` を読むのでここからは触れないが、`weekday.ts` は
 * 渡された値を検めるだけなので直に呼べる（それがあのファイルを分けた理由）。
 * 守るのは**原点**——Rustは `Date.getDay()`（0=日曜）に揃えて渡す約束で、
 * Win32 の原点（0=月曜）が素通しで来ると、**週の始まりが1日手前へずれる**。
 * **0〜6は7曜日ぜんぶが正しい値**なので、ここを狭めない
 * （`ar-EG` / `fa-IR` は土曜始まり。2026-09-04、ゲート2）。
 */
test("OSから来た週の始まりは、範囲の内側だけ通す", () => {
  for (let i = 0; i <= 6; i++) {
    assert.equal(firstWeekdayFromOs(i), i, `${i} は正しい曜日`);
  }
  // **0 を落とさない。** `if (raw)` で書くと日曜始まりの人だけ既定へ倒れる
  assert.equal(firstWeekdayFromOs(0), 0);

  // 取れなかった／置く前の版／Rustの居ないブラウザ
  assert.equal(firstWeekdayFromOs(undefined), null);
  assert.equal(firstWeekdayFromOs(null), null);
  // 範囲の外と、数でないもの。**文字列の "0" も通さない**——添字にすると
  // `weekdayLabels` が `NaN` を数えて曜日名が消える
  for (const bad of [-1, 7, 8, 1.5, NaN, Infinity, "0", "3", true, [3], {}]) {
    assert.equal(firstWeekdayFromOs(bad), null, `${String(bad)} は弾く`);
  }
});

/**
 * 辞書の中の文字列を全部集める（入れ子の配列・オブジェクト・関数の出力まで）。
 *
 * **関数は単数と複数の両方で呼ぶ**（ゲート2）。`n = 3` だけだと
 * `one(n, "vídeo", "videos")` のような**単数側にしか出ない語**を見落とす。
 */
function everyString(d: Any): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (typeof v === "string") out.push(v);
    else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") Object.values(v).forEach(walk);
  };
  for (const k of KEYS) {
    const v = d[k];
    if (typeof v === "function") {
      out.push(call(d, k, argsFor(k, 1, "X")), call(d, k, argsFor(k, 3, "X")));
    } else walk(v);
  }
  return out;
}

/**
 * `w` が `s` の中に**語の頭から**現れるか（大小は無視）。
 *
 * 部分一致だと語の中ほどでも当たる——`dañado`（壊れた）の中の `añad` を
 * 「本国だけの `añadir`」と読んで落ちた（dev #23 の帯を足したとき）。一覧は語幹と
 * 語句なので、前が文字でない位置だけを見る
 */
function hasWordStart(s: string, w: string): boolean {
  const escaped = w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?<!\\p{L})${escaped}`, "iu").test(s);
}

test("本国だけの語は、語の頭から探す（語の中ほどは拾わない）", () => {
  assert.equal(hasWordStart("el archivo está dañado", "añad"), false);
  assert.equal(hasWordStart("Añade una carpeta", "añad"), true);
  assert.equal(hasWordStart("fotos añadidas", "añad"), true);
  assert.equal(hasWordStart("(añadir)", "añad"), true);
  assert.equal(hasWordStart("Vídeos", "vídeo"), true);
  assert.equal(hasWordStart("en Ajustes del Sistema →", "Ajustes del Sistema"), true);
  // 前がアクセント付きの文字でも「語の中ほど」（`[a-z]` や `\b` では見分けられない）
  assert.equal(hasWordStart("éañad", "añad"), false);
  // 語は字として探す（正規表現として読まない）
  assert.equal(hasWordStart("a.b", "a.b"), true);
  assert.equal(hasWordStart("axb", "a.b"), false);
});

/** `strings` の中に残っている本国だけの語（語の頭から探す） */
function spainOnlyIn(strings: readonly string[]): string[] {
  return AMERICAS_ONLY.filter((w) => strings.some((s) => hasWordStart(s, w)));
}

test("本国だけの語の探索は、植えた語を見つける（正の対照）", () => {
  // 緩めた照合（部分一致→語の頭）が、見つけるべきものまで見逃していないこと
  assert.deepEqual(spainOnlyIn(["Añade una carpeta", "el archivo está dañado"]), ["añad"]);
  assert.deepEqual(spainOnlyIn(["Abre los vídeos"]), ["vídeo"]);
  assert.deepEqual(spainOnlyIn(["el archivo está dañado"]), []);
});

test("中南米のスペイン語に、本国だけの語が残っていないこと", () => {
  // **`es-419.ts` は差分だけを持つ**ので、`es.ts` 側に新しく `vídeo` と書くと
  // 何もしなくてもこちらへ流れ込む。**そのときここが落ちる**のが、この試験の値打ち
  // **大小は無視する**（`Vídeos` も `vídeo` で拾いたい）
  const left = spainOnlyIn(everyString(es419 as Any));
  assert.deepEqual(
    left,
    [],
    "es-419 に本国だけの語が残っている。es-419.ts に差分を足すこと（es.ts は触らない）",
  );
  // **裏返しの確認は語ごとに**（ゲート2）。「1つでも在れば良し」だと、
  // 本国側で `Añadir` をやめたときに**その行だけ死んだ規則**になり、
  // 対応する差分が**意味の無い写し**になったまま気づけない
  const spainStrings = everyString(es as Any);
  const missing = AMERICAS_ONLY.filter(
    (w) => !spainStrings.some((s) => hasWordStart(s, w)),
  );
  assert.deepEqual(
    missing,
    [],
    "本国の辞書からこの語が消えている。AMERICAS_ONLY が古い（語を変えたなら一覧も直す）",
  );
  // **置き換えの表も死なせない**。ショートカット一覧を作り直すのに使うので、
  // 左の語が本国側から消えていたら、その規則はもう何も直していない
  const deadSwaps = AMERICAS_SWAPS.filter(
    ([spain]) => !everyString(es as Any).some((s) => s.includes(spain)),
  );
  assert.deepEqual(
    deadSwaps.map(([spain]) => spain),
    [],
    "AMERICAS_SWAPS の左が本国の辞書に無い（大小そのままで探している）",
  );
});

test("ショートカット一覧は、置き換えを当てただけの写しであること", () => {
  // **入れ子なので丸ごと持つしかない**（`es-419.ts` の注記）。放っておくと
  // **本国側だけ行が増えて、中南米の人は古い一覧を見続ける**——型もほかの試験も
  // 何も言わない（ゲート2）
  const swap = (s: string) =>
    AMERICAS_SWAPS.reduce((acc, [spain, americas]) => acc.split(spain).join(americas), s);
  const derived = (es as Any).shortcutGroups as { title: string; keys: [string, string][] }[];
  assert.deepEqual(
    (es419 as Any).shortcutGroups,
    derived.map((g) => ({
      title: swap(g.title),
      keys: g.keys.map(([k, v]) => [swap(k), swap(v)]),
    })),
    "es.ts のショートカット一覧と食い違っている。行を足したなら es-419.ts にも足すこと",
  );
});

test("辞書のコードの選び方（梯子）", () => {
  // `Object.hasOwn` は ES2022。この設定（ES2021）では使えない（`index.ts` の `hasDict` と同じ事情）
  const has = (c: string) => Object.prototype.hasOwnProperty.call(DICTS, c);
  const pick = (tag: string) => matchLocale([tag], has);

  assert.equal(pick("ja-JP"), "ja");
  assert.equal(pick("en-GB"), "en");
  assert.equal(pick("de-DE"), "de");
  // 仏語・伊語は地域を分けない（カナダ・スイス・アルジェリアも同じ辞書）
  assert.equal(pick("fr-FR"), "fr");
  assert.equal(pick("fr-CA"), "fr");
  assert.equal(pick("fr-DZ"), "fr");
  assert.equal(pick("it-IT"), "it");
  assert.equal(pick("it-CH"), "it");
  // 露語も地域で割らない（カザフスタンも本国の辞書。Rust の `os_dict::guess` と同じ答え）
  assert.equal(pick("ru-RU"), "ru");
  assert.equal(pick("ru-KZ"), "ru");
  // 葡語も地域で割らない。ポルトガルの人にもブラジルの言葉で出す（2026-11-04 利用者。Rust の guess と同じ）
  assert.equal(pick("pt-BR"), "pt");
  assert.equal(pick("pt-PT"), "pt");
  assert.equal(pick("pt-AO"), "pt");
  assert.equal(pick("ko-KR"), "ko");
  assert.equal(pick("ar-OM"), "ar");
  assert.equal(pick("ar-DZ"), "ar");
  assert.equal(pick("he-IL"), "he");
  assert.equal(pick("iw-IL"), "he");
  assert.equal(pick("iw"), "he");

  // **スペイン語は地域で本国と中南米に割れる**（dev #17）
  assert.equal(pick("es-ES"), "es", "本国はそのまま");
  assert.equal(pick("es"), "es", "地域が分からなければ本国側");
  assert.equal(pick("es-419"), "es-419");
  assert.equal(pick("es-MX"), "es-419", "包含解決。ここが無いと本国の辞書に着く");
  assert.equal(pick("es-AR"), "es-419");
  assert.equal(pick("es-US"), "es-419");
  assert.equal(pick("es-Latn-MX"), "es-419", "書き言葉が挟まっても地域を見つける");
  // **拡張は地域ではない**（ゲート1）。`-u-` の先の `nu` / `hc` は2文字だが国ではない
  assert.equal(pick("es-u-nu-latn"), "es");
  assert.equal(pick("es-u-hc-h23"), "es");
  assert.equal(pick("es-MX-u-nu-latn"), "es-419", "拡張の手前に地域があれば拾う");
  // **書き言葉が挟まった `419`**（ゲート1）。梯子は前から短くするので、
  // 2番目に `419` が無ければ挟み直さないと本国へ着く
  assert.equal(pick("es-Latn-419"), "es-419");

  // **中国語は書き言葉で割れる**（#19。ここは実機でしか確かめられなかった）
  assert.equal(pick("zh-Hans-CN"), "zh");
  assert.equal(pick("zh-CN"), "zh");
  assert.equal(pick("zh-Hant-TW"), "zh-hant");
  assert.equal(pick("zh-TW"), "zh-hant", "書き言葉を省いたタグに補う");
  assert.equal(pick("zh-HK"), "zh-hant");
  assert.equal(pick("zh-Hans-HK"), "zh", "`Hans` と書いてあれば地域より優先");
  assert.equal(pick("yue-Hant-HK"), "zh-hant", "広東語は繁体字の辞書で読める");
  assert.equal(pick("yue"), "zh-hant", "裸の広東語も繁体字へ");

  // 持っていない言語は `null`（呼び出し側が英語へ倒す）
  assert.equal(pick("th-TH"), null);
  assert.equal(matchLocale(["th-TH", "es-CL"], has), "es-419", "先に当たったものを採る");
  assert.equal(matchLocale([], has), null);
});

test("見つからないフォルダの知らせは、数えられなかった 0 を数字で言わないこと", () => {
  // 画面は数えられなかった数を 0 にする（`countMediaUnder(...).catch(() => 0)`）。
  // **「0 枚を開けません」と言わせない**——言うなら数を省く（#146 の変異注入 I7 が、
  // `count > 0` を `>= 0` にしても何も落ちないのを見せた）
  for (const [lang, d] of Object.entries(DICTS)) {
    for (const key of ["rootMissingNotice", "rootsMissingNotice"]) {
      const said = call(d, key, ["X", 0]);
      assert.ok(!/\d/.test(said), `${lang}.${key}(0) が数字を言っている: ${said}`);
    }
  }
});

test("見つからないフォルダの知らせは、1枚と2枚で単複がそろうこと", () => {
  // 1つの文に同じ数の `one()` が2つあると、片方が外れても「単数形を持つキーの顔ぶれ」は
  // 気づかない（`キー:位置` しか見ないため。#146 の変異注入 I3 が生き残った）。
  // **文の形そのものを見る**
  const cases: [string[], string, [string, string][]][] = [
    [["es", "es-419"], "rootMissingNotice", [["no se puede abrir la foto", "no se pueden abrir las 2 fotos"]]],
    [["es", "es-419"], "rootsMissingNotice", [["no se puede abrir 1 foto", "no se pueden abrir 2 fotos"]]],
    [["de"], "rootMissingNotice", [["1 Foto darin lässt", "2 Fotos darin lassen"]]],
    [["de"], "rootsMissingNotice", [["1 Foto lässt", "2 Fotos lassen"]]],
    [["en"], "rootMissingNotice", [["1 photo in it", "2 photos in it"]]],
    [["en"], "rootsMissingNotice", [["1 photo in them", "2 photos in them"]]],
    [["fr"], "rootMissingNotice", [["1 photo qu’il contient ne peut pas être ouverte", "2 photos qu’il contient ne peuvent pas être ouvertes"]]],
    [["fr"], "rootsMissingNotice", [["1 photo ne peut pas être ouverte", "2 photos ne peuvent pas être ouvertes"]]],
    [["it"], "rootMissingNotice", [["1 foto al suo interno non può essere aperta", "2 foto al suo interno non possono essere aperte"]]],
    [["it"], "rootsMissingNotice", [["1 foto non può essere aperta", "2 foto non possono essere aperte"]]],
    [["ru"], "rootMissingNotice", [["1 фото в ней недоступно", "2 фото в ней недоступны"]]],
    [["ru"], "rootsMissingNotice", [["1 фото недоступно", "2 фото недоступны"]]],
    [["pt"], "rootMissingNotice", [["1 foto nela não pode ser aberta", "2 fotos nela não podem ser abertas"]]],
    [["pt"], "rootsMissingNotice", [["1 foto nelas não pode ser aberta", "2 fotos nelas não podem ser abertas"]]],
  ];
  for (const [langs, key, pairs] of cases) {
    for (const lang of langs) {
      for (const [one, two] of pairs) {
        const s1 = call(DICTS[lang], key, ["X", 1]);
        const s2 = call(DICTS[lang], key, ["X", 2]);
        assert.ok(s1.includes(one), `${lang}.${key}(1): ${s1}`);
        assert.ok(s2.includes(two), `${lang}.${key}(2): ${s2}`);
      }
    }
  }
});

test("書き出し・移動の結果が、ライブラリから無くなった枚数を言うこと（pictkura-dev #33）", () => {
  // 選んだあとに再スキャンで行が消えた写真は運ばない。**数えずに落とすと「10枚を移動」が「8枚を移動しました」で
  // 終わり、2枚の行方が分からない**。全部の辞書で、その枚数が文に出ること、何も運ばなかったときに
  // 「0枚を書き出しました／移動しました」から始めないことを見る
  for (const [lang, d] of Object.entries(DICTS)) {
    const exportNone = call(d, "exportDone", [0, 0, 0, 0]);
    const exportMissing = call(d, "exportDone", [0, 0, 0, 7]);
    assert.match(exportMissing, /7/, `${lang}: exportDone が無くなった枚数を言わない`);
    assert.ok(
      !exportMissing.includes(exportNone.replace(/[。.]$/, "")),
      `${lang}: 何も書き出していないのに「0枚を書き出しました」から始めた: ${exportMissing}`,
    );
    assert.ok(!call(d, "exportDone", [3, 0, 0, 0]).includes("7"), `${lang}: 無くなった写真が無いのに数を言った`);
    // 書き出した枚数と無くなった枚数の両方を言う（ゲート2: 片方の場合しか見ていなかった）
    const both = call(d, "exportDone", [3, 0, 0, 7]);
    assert.ok(/3/.test(both) && /7/.test(both), `${lang}: 3枚書き出して7枚無くなったのに、両方を言わない: ${both}`);
    const moveBoth = call(d, "moveDone", [3, 0, 0, 0, 7]);
    assert.ok(/3/.test(moveBoth) && /7/.test(moveBoth), `${lang}: 3枚移動して7枚無くなったのに、両方を言わない: ${moveBoth}`);
    const moveNone = call(d, "moveDone", [0, 0, 0, 0, 0]);
    const moveMissing = call(d, "moveDone", [0, 0, 0, 0, 7]);
    assert.match(moveMissing, /7/, `${lang}: moveDone が無くなった枚数を言わない`);
    assert.ok(
      !moveMissing.includes(moveNone.replace(/[。.]$/, "")),
      `${lang}: 何も移動していないのに「0枚を移動しました」から始めた: ${moveMissing}`,
    );
    assert.notEqual(
      call(d, "moveDone", [8, 0, 0, 0, 2]),
      call(d, "moveDone", [8, 0, 0, 0, 0]),
      `${lang}: 8枚移動して2枚無くなったのに、8枚だけの文と同じ`,
    );
  }
});

test("the prev/next arrows point the way the keys step", () => {
  // 右→左の画面では ← が「次」（direction.ts の stepOfArrow）。← → は鏡像文字ではないので、
  // 右→左の辞書は自分で入れ替える（index.ts の手順）
  for (const [code, d] of Object.entries(DICTS)) {
    const rtl = dirOf(code) === "rtl";
    const next = String(d.viewerNext);
    const prev = String(d.viewerPrev);
    assert.ok(next.includes(rtl ? "←" : "→"), `${code}: viewerNext = ${next}`);
    assert.ok(prev.includes(rtl ? "→" : "←"), `${code}: viewerPrev = ${prev}`);
  }
});

test("韓国語の 으로/로 は、前の語の終わり（パッチム）で形を選び、読めなければ両方書くこと", () => {
  assert.equal(koEuro("사진"), "사진으로");
  assert.equal(koEuro("파일"), "파일로", "ㄹ で終わる語は 로");
  assert.equal(koEuro("포토샵"), "포토샵으로");
  assert.equal(koEuro("필름"), "필름으로", "ㅁ で終わる語は 으로");
  assert.equal(koEuro("“C:\\Temp\\사진”"), "“C:\\Temp\\사진”으로", "閉じの引用符は飛ばして読む");
  // 括弧書きの前の語で読む（助詞はそちらに付く）
  // 中の語と前の語で形が割れる組で見る（파일 なら 로、사진 なら 으로）
  assert.equal(koEuro("사진 (파일)"), "사진 (파일)으로");
  assert.equal(koEuro("“미리보기 (Preview)”"), "“미리보기 (Preview)”로", "引用符の内側の括弧書きも外す");
  assert.equal(koEuro("미리보기 (Preview)"), "미리보기 (Preview)로");
  // 読めない語（ラテン文字・数字で終わる名前）は両方書く。Windows 11 は 일레븐 と読まれる
  assert.equal(koEuro("Canon"), "Canon(으)로");
  assert.equal(koEuro("Canon EOS R5"), "Canon EOS R5(으)로");
  assert.equal(koEuro("Windows 11"), "Windows 11(으)로");
});

test("韓国語の辞書は、変数の直後に助詞を書き込んでいないこと", () => {
  // 助詞の形は前の語の終わりで変わるので、`${name}을` と書くと名前次第で誤る。変数の後ろには名詞を
  // 置く（`“${name}” 폴더를`）か、`euro()` を通す。数は助数詞（장・개）が後ろに付くので対象外
  const src = readFileSync(new URL("../src/i18n/ko.ts", import.meta.url), "utf8");
  // 後ろに続く字は問わない（`으로는`・`이라는`・`과의` も前の語の終わりで形が変わる）。閉じの括弧・引用符も越える
  const hits = [...src.matchAll(/\$\{(?!num\()[^}]*\}["”’)」』»\]]*(을|를|이|가|은|는|과|와|으로|로|라)/gu)].map((m) => m[0]);
  assert.deepEqual(hits, []);
});

test("右から左の辞書で差し込む名前の中の向きの印は外す（閉じが途中で切れない）", () => {
  for (const d of [ar, he]) {
    const out = d.rootRemoveConfirm("a\u2069ب\u2066c\u202bd\u2029e");
    assert.equal(out.match(/\u2068/gu)?.length, 1);
    assert.equal(out.match(/\u2069/gu)?.length, 1);
    assert.ok(out.includes("\u2068aبcd e\u2069"), out);
    assert.ok(!/[\u202a-\u202e\u2066\u2067\u2029]/u.test(out), out);
  }
});

test("ヘブライ語は年だけ双数で言うこと（שנתיים＝2年）", () => {
  assert.equal(he.memoriesTitle(1), "היום לפני שנה");
  assert.equal(he.memoriesTitle(2), "היום לפני שנתיים");
  assert.ok(he.memoriesTitle(3).includes("3 שנים"), he.memoriesTitle(3));
});

test("右から左の辞書は、名前の一覧を丸ごと閉じないこと（呼ぶ側が名前ごとに閉じる）", () => {
  // 丸ごと閉じると、閉じの向きが先頭の名前の字で決まり、右から読む人に2つ目の名前が先に来る（#284）。
  // しかも `isolate()` は値の中の向きの印を外すので、呼ぶ側が名前ごとに閉じた印まで消える
  const listed = "\u2068a\u2069, \u2068ב\u2069";
  // 右から左の辞書は `dirOf` で選ぶ（次の右から左の辞書を足したら、自動でここに入る）
  const rtlDicts = Object.entries(DICTS).filter(([code]) => dirOf(code) === "rtl");
  assert.ok(rtlDicts.length >= 2, rtlDicts.map(([c]) => c).join());
  for (const [, dict] of rtlDicts) {
    const d = dict as unknown as Dict;
    for (const out of [
      d.rootsMissingNotice(listed, 2),
      d.emptyMissing(listed),
      d.emptyUnreadableMac(listed),
      d.emptyUnreadableWin(listed),
      d.emptyUnreadableOther(listed),
      d.emptyAllExcluded(listed),
      d.emptyStalled(listed),
      d.googleSendFolders(listed),
    ]) {
      assert.ok(out.includes(listed), out);
      assert.equal(out.match(/\u2068/gu)?.length, 2, out);
    }
  }
});

test("右から左の辞書は、大きさ（`1.22 MB`）を閉じて差し込むこと", () => {
  for (const [, dict] of Object.entries(DICTS).filter(([code]) => dirOf(code) === "rtl")) {
    const d = dict as unknown as Dict;
    assert.ok(d.selectionTotal("1.22 MB").includes("\u20681.22 MB\u2069"), d.selectionTotal("1.22 MB"));
  }
});

/**
 * **Store の対応言語（MSIX のマニフェスト）と、画面の辞書が揃っていること**。Store が「対応言語」に出し、
 * 掲載ページを作れるのはマニフェストに書いた言語だけ——辞書を足してここを忘れると、アプリは話すのに
 * Store では見つからない（1.0.1 まで日英の2つだけだった。1.0.2 の準備で判明）
 */
test("MSIX のマニフェストが、辞書の言語をすべて宣言していること", () => {
  const manifest = readFileSync(new URL("../../src-tauri/windows/msix/AppxManifest.xml", import.meta.url), "utf8");
  // コメントの中の要素は宣言ではない（言語を一時的にコメントで外しても、ここが数えないように）。
  // 正規表現の置換では CodeQL が「取り除きが不完全」と止める（#285）ので、区切りを探して読み飛ばす
  let live = "";
  for (let at = 0; ; ) {
    const open = manifest.indexOf("<!--", at);
    if (open < 0) {
      live += manifest.slice(at);
      break;
    }
    live += manifest.slice(at, open);
    const close = manifest.indexOf("-->", open + 4);
    if (close < 0) break; // 閉じないコメントは、そこから先を宣言と数えない
    at = close + 3;
  }
  const declared = [...live.matchAll(/<Resource Language="([^"]+)"/g)].map((m) => m[1].toLowerCase());
  // 辞書のコード → Store の綴り（地域を付ける辞書だけ書く。ほかは同じ綴り）
  const storeTags: Record<string, string[]> = {
    ja: ["ja-jp"],
    en: ["en-us"],
    es: ["es-es"],
    zh: ["zh-hans"],
    "zh-hant": ["zh-hant"],
    pt: ["pt-br", "pt-pt"],
  };
  const expected = Object.keys(DICTS).flatMap((code) => storeTags[code] ?? [code]);
  assert.deepEqual(declared.slice().sort(), expected.slice().sort());
});
