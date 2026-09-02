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
import { zh } from "../src/i18n/zh.ts";
import { zhHant } from "../src/i18n/zh-hant.ts";
import { setNumberLocale } from "../src/i18n/plural.ts";

type Any = Record<string, unknown>;

/** `index.ts` の `DICTS` と同じ並び。**言語を足したらここにも足す** */
const DICTS: Record<string, Any> = {
  ja: ja as Any,
  en: en as Any,
  de: de as Any,
  es: es as Any,
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
  setNumberLocale("de-DE");
  try {
    for (const [lang, d] of Object.entries(DICTS)) {
      for (const key of COUNT_KEYS) {
        if (YEARS.has(key)) continue;
        const out = call(d, key, argsFor(key, 1234567, "X"));
        assert.ok(
          !out.includes("1234567"),
          `${lang}.${key} が生の数を埋めている（\${n} ではなく \${num(n)} を通すこと）: ${out}`,
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
 * **1のときに語形が変わるキー**。単複のある言語（en / de / es）だけ。
 *
 * これは「正しい訳の一覧」ではなく**いまの姿の写し**で、意味は2つ:
 *
 * - **減ったら落ちる**——文言を書き直した拍子に `one()` が外れると出る
 * - **数を取るキーを足したら落ちる**——足した人が、その言語で単数形が要るかを
 *   1度は考えることになる
 *
 * **載っていない ＝ 壊れている、ではない。** `photosCount`（数だけ）・
 * `importing`（`3/10` の分数）・`speedDiff`（`1 copied` は英独西とも正しい）のように、
 * 数のあとに名詞が続かないキーは変わらないのが正しい。到達しないものもある
 * （`wizardTruncated` は `TREE_LIMIT` でしか出ない。理由は `ja.ts` に書いた）。
 */
const INFLECTS: Record<string, string[]> = {
  en: [
    "itemsCount",
    "memoriesTitle",
    "rejectGateTitle",
    "decoderHeifNotice",
    "decoderHeifNoticeMac",
    "decoderHeifNoticeOther",
    "deleteConfirm",
    "deletedSomeLeft",
    "moveConfirm",
    "exportDone",
    "bulkPickDone",
    "bulkUnpickDone",
    "bulkFavoriteDone",
    "bulkUnfavoriteDone",
    "speedUsnDirty",
    "speedPruned",
    "speedFull",
  ],
  de: [
    "itemsCount",
    "memoriesTitle",
    "rejectGateTitle",
    "andMore",
    "decoderHeifNotice",
    "decoderHeifNoticeMac",
    "decoderHeifNoticeOther",
    "wizardHiddenCount",
    "deleteConfirm",
    "deletedSomeLeft",
    "moveConfirm",
    "exportDone",
    "bulkPickDone",
    "bulkUnpickDone",
    "bulkFavoriteDone",
    "bulkUnfavoriteDone",
    "speedUsnDirty",
    // **`speedPruned` はここに居なくてよい**——`Ordner` は単複同形
    "speedFull",
  ],
  es: [
    "itemsCount",
    "memoriesTitle",
    "rejectGateTitle",
    "importDone",
    "syncDone",
    "wizardSelected",
    "decoderHeifNotice",
    "decoderHeifNoticeMac",
    "decoderHeifNoticeOther",
    "wizardHiddenCount",
    "wizardEtaSeconds",
    "wizardEtaMinutes",
    "wizardMoreFiles",
    "deleteConfirm",
    "deleted",
    "deletedSomeLeft",
    "selectedCount",
    "moveConfirm",
    "exportDone",
    "bulkPickDone",
    "bulkUnpickDone",
    "bulkFavoriteDone",
    "bulkUnfavoriteDone",
    "speedUsnDirty",
    "speedPruned",
    "speedFull",
    "speedDiff",
  ],
};

test("単数形を持つキーの顔ぶれが変わっていないこと", () => {
  // 数字そのものは伏せて、**語形だけ**を比べる
  const shape = (s: string) => s.replace(/[\d., ]+/g, "#");
  for (const [lang, expected] of Object.entries(INFLECTS)) {
    const d = DICTS[lang];
    const actual = COUNT_KEYS.filter(
      (k) => shape(call(d, k, argsFor(k, 1, "X"))) !== shape(call(d, k, argsFor(k, 2, "X"))),
    );
    assert.deepEqual(
      actual.slice().sort(),
      expected.slice().sort(),
      `${lang} で単数形を持つキーが変わった。増えたなら INFLECTS に足す。` +
        `減ったなら one() が外れていないか見ること`,
    );
  }
});

test("単複の無い言語が場合分けしていないこと", () => {
  // 日本語・中国語には単複が無い。**`one()` を写してはいけない**
  // （英語の辞書を下敷きに書くと入り込む）
  const shape = (s: string) => s.replace(/[\d., ]+/g, "#");
  for (const lang of ["ja", "zh", "zh-hant"]) {
    const d = DICTS[lang];
    for (const k of COUNT_KEYS) {
      // **`deleteConfirm` / `moveConfirm` は単複ではない**——1枚のときだけ
      // 「この写真を」と指示語に置き換えている（`2枚の写真を` に対して `1枚の写真を`
      // とは言わない、という日本語・中国語の言い回し）。英語も同じ形にしてある
      if (k === "deleteConfirm" || k === "moveConfirm") continue;
      assert.equal(
        shape(call(d, k, argsFor(k, 1, "X"))),
        shape(call(d, k, argsFor(k, 2, "X"))),
        `${lang}.${k} が1のときだけ語形を変えている（この言語に単複は無い）`,
      );
    }
  }
});

/**
 * **英語と同じ綴りでよいキー**。製品名・単位・記号・そのまま通じる語、
 * それに**数だけを出す関数**（`3` / `✕ 3` / `3+` に訳す余地は無い）。
 *
 * ここに無いのに英語と一致していたら、**訳し漏れ**の疑いがある。
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
