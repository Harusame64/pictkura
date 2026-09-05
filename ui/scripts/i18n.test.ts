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
import { en } from "../src/i18n/en.ts";
import { de } from "../src/i18n/de.ts";
import { es } from "../src/i18n/es.ts";
import { AMERICAS_ONLY, es419 } from "../src/i18n/es-419.ts";
import { zh } from "../src/i18n/zh.ts";
import { zhHant } from "../src/i18n/zh-hant.ts";
import { setNumberLocale } from "../src/i18n/plural.ts";
import { matchLocale } from "../src/i18n/match.ts";
import { firstWeekdayFromOs } from "../src/i18n/weekday.ts";

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

test("6つの辞書のキーが揃っていること", () => {
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
 * `importing`（`3/10` の分数）・`en.speedDiff`（`1 added` は英語として正しい）・
 * `de.speedPruned`（`Ordner` は単複同形）のように、変わらないのが正解のものがある。
 * 到達しないものもある（`wizardTruncated` は `TREE_LIMIT` でしか出ない。理由は `ja.ts` に）。
 */
const INFLECTS: Record<string, string[]> = {
  en: [
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
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "speedUsnDirty:0",
    "speedUsnDirty:1",
    "speedPruned:0",
    "speedFull:0",
  ],
  de: [
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
    "moveConfirm:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "speedUsnDirty:0",
    "speedFull:0",
  ],
  es: [
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "importDone:0",
    "importDone:1",
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
    "selectedCount:0",
    "moveConfirm:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "speedUsnDirty:0",
    "speedUsnDirty:1",
    "speedPruned:0",
    "speedFull:0",
    "speedDiff:0",
    "speedDiff:1",
    "speedDiff:2",
  ],
  // **中南米版は本国と同じ活用**（差分は語彙だけで、単複の作りは変えていない）
  "es-419": [
    "itemsCount:0",
    "memoriesTitle:0",
    "rejectGateTitle:0",
    "importDone:0",
    "importDone:1",
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
    "selectedCount:0",
    "moveConfirm:0",
    "exportDone:0",
    "exportDone:1",
    "exportDone:3",
    "bulkPickDone:0",
    "bulkUnpickDone:0",
    "bulkFavoriteDone:0",
    "bulkUnfavoriteDone:0",
    "speedUsnDirty:0",
    "speedUsnDirty:1",
    "speedPruned:0",
    "speedFull:0",
    "speedDiff:0",
    "speedDiff:1",
    "speedDiff:2",
  ],
};

/**
 * **単複を持たない言語**。`one()` を写してはいけない（英語の辞書を下敷きにすると入り込む）。
 *
 * `INFLECTS` と合わせて `DICTS` を覆い切る。**覆えていなければ下のテストが落ちる。**
 */
const NO_PLURAL = ["ja", "zh", "zh-hant"];

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
  de: [
    "appName",
    "kindRaw",
    "kindVideo", // 独語でも `Videos`
    "actualSizeBadge", // `1:1`
    "exifIso",
    "wizardTitle", // `Import` は独語の語でもある
    "listSeparator", // `, `
    "photosCount", // 数だけ
    "rejectChip", // `✕ 3`
    "wizardCapped", // `3+`
  ],
  es: [
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
    "appName",
    "kindRaw",
    "exifIso",
    "keyCtrl",
    "rejectChip",
  ],
  zh: ["appName", "kindRaw", "keyCtrl", "actualSizeBadge", "exifIso", "rejectChip"],
  "zh-hant": ["appName", "kindRaw", "keyCtrl", "actualSizeBadge", "exifIso", "rejectChip"],
};

test("英語からの丸写しが無いこと", () => {
  // **関数のキーも見る**（2026-09-02、ゲート2の指摘）。文字列だけを比べていたが、
  // **桁区切りが抜けていた36キーは全部が関数**で、いちばん写されやすいのもそちら。
  // 呼んで出力で比べれば、英語の `speedFull` をそのまま写した7つ目の辞書がここで止まる
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

/** 辞書の中の文字列を全部集める（入れ子の配列・オブジェクト・関数の出力まで） */
function everyString(d: Any): string[] {
  const out: string[] = [];
  const walk = (v: unknown) => {
    if (typeof v === "string") out.push(v);
    else if (Array.isArray(v)) v.forEach(walk);
    else if (v && typeof v === "object") Object.values(v).forEach(walk);
  };
  for (const k of KEYS) {
    const v = d[k];
    if (typeof v === "function") out.push(call(d, k, argsFor(k, 3, "X")));
    else walk(v);
  }
  return out;
}

test("中南米のスペイン語に、本国だけの語が残っていないこと", () => {
  // **`es-419.ts` は差分だけを持つ**ので、`es.ts` 側に新しく `vídeo` と書くと
  // 何もしなくてもこちらへ流れ込む。**そのときここが落ちる**のが、この試験の値打ち
  const strings = everyString(es419 as Any);
  const left = AMERICAS_ONLY.filter((w) => strings.some((s) => s.includes(w)));
  assert.deepEqual(
    left,
    [],
    "es-419 に本国だけの語が残っている。es-419.ts に差分を足すこと（es.ts は触らない）",
  );
  // **裏返しの確認**: 本国側にはその語がちゃんと在る（検査が生きている）
  const inSpain = AMERICAS_ONLY.filter((w) =>
    everyString(es as Any).some((s) => s.includes(w)),
  );
  assert.deepEqual(
    inSpain.length > 0,
    true,
    "本国の辞書からも消えている。AMERICAS_ONLY が古い（語を変えたなら一覧も直す）",
  );
});

test("辞書のコードの選び方（梯子）", () => {
  // `Object.hasOwn` は ES2022。この設定（ES2021）では使えない（`index.ts` の `hasDict` と同じ事情）
  const has = (c: string) => Object.prototype.hasOwnProperty.call(DICTS, c);
  const pick = (tag: string) => matchLocale([tag], has);

  assert.equal(pick("ja-JP"), "ja");
  assert.equal(pick("en-GB"), "en");
  assert.equal(pick("de-DE"), "de");

  // **スペイン語は地域で本国と中南米に割れる**（dev #17）
  assert.equal(pick("es-ES"), "es", "本国はそのまま");
  assert.equal(pick("es"), "es", "地域が分からなければ本国側");
  assert.equal(pick("es-419"), "es-419");
  assert.equal(pick("es-MX"), "es-419", "包含解決。ここが無いと本国の辞書に着く");
  assert.equal(pick("es-AR"), "es-419");
  assert.equal(pick("es-US"), "es-419");
  assert.equal(pick("es-Latn-MX"), "es-419", "書き言葉が挟まっても地域を見つける");

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
