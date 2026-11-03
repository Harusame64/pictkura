/**
 * 画面の向き（右から左へ書く言語）。
 *
 * **向きは `<html dir>` が持つ**（main.tsx が言語から決めて付ける）。JS はそれを**その都度読む**——
 * 起動時に控えた値を持たないので、試験で `dir` を付け替えても追える。CSS は論理プロパティと
 * `[dir="rtl"]` で同じ属性に従う（scripts/logicalCss.test.ts）。`:dir()` は使わない——WebKit 16.4 より
 * 前（macOS 11・12 に残りうる）では効かず、鏡が半分だけになる
 */

/**
 * 右から左へ書く言語。アラビア語・ヘブライ語・ペルシア語・ウルドゥー語のほか、同じ文字を使う言語。
 * **文字の札（`-Arab` / `-Latn` など）が付いていれば、言語より札を信じる**——`pa-Arab` は右→左、
 * `sd-Deva`・`ar-Latn` は左→右。エンジンの `Intl.Locale` の textInfo には頼らない（WebView2 と WebKit で
 * 載っている版が違い、同じ言語で向きが割れうる）
 */
// iw・ji はヘブライ語・イディッシュ語の古いコード（Java 由来の環境がまだ返す）
// iw・ji はヘブライ語・イディッシュ語の古いコード（Java 由来の環境がまだ返す）。prs はダリー語（Windows が返す）
const RTL_LANGS = /^(ar|he|iw|fa|prs|ur|ckb|ps|sd|yi|ji|dv|ug|ks|syr|mzn|lrc|nqo|bal|pnb|azb)$/i;
const RTL_SCRIPTS = /^(Arab|Hebr|Thaa|Syrc|Nkoo|Adlm|Rohg|Mand|Samr)$/i;

export function dirOf(locale: string): "rtl" | "ltr" {
  const parts = locale.split(/[-_]/);
  // 文字の札は言語（と拡張言語 3 文字）の直後だけ。`-u-nu-latn` のような拡張の中の 4 文字は見ない
  let at = 1;
  while (at < parts.length && /^[A-Za-z]{3}$/.test(parts[at]) && at < 4) at++;
  const script = /^[A-Za-z]{4}$/.test(parts[at] ?? "") ? parts[at] : undefined;
  const rtl = script ? RTL_SCRIPTS.test(script) : RTL_LANGS.test(parts[0]);
  return rtl ? "rtl" : "ltr";
}

/** 横の向きの符号。左→右は 1、右→左は -1——CSS の `--inline-sign` と同じ。画面の座標を向きで裏返すときに掛ける */
export const inlineSign = (): 1 | -1 => (isRtl() ? -1 : 1);

/** いまの画面が右→左か */
export const isRtl = (): boolean =>
  typeof document !== "undefined" && document.documentElement.dir === "rtl";

/**
 * 画面の左右（矢印キー）を、読む順の前後へ。**右→左では左が「次」**——本の頁をめくる向きと同じ。
 * 送りのボタン（‹ ›）も論理の位置に置いてあるので、キーとボタンの向きは揃う
 */
export const stepOfArrow = (key: "ArrowLeft" | "ArrowRight", rtl: boolean): -1 | 1 =>
  (key === "ArrowRight") !== rtl ? 1 : -1;
