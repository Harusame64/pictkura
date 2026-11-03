/**
 * 画面の向き（右から左へ書く言語）。
 *
 * **向きは `<html dir>` が持つ**（main.tsx が言語から決めて付ける）。JS はそれを**その都度読む**——
 * 起動時に控えた値を持たないので、試験で `dir` を付け替えても追える。CSS は論理プロパティと
 * `[dir="rtl"]` で同じ属性に従う（scripts/logicalCss.test.ts）。`:dir()` は使わない——WebKit 16.4 より
 * 前（macOS 11・12 に残りうる）では効かず、鏡が半分だけになる
 */

/**
 * 右から左へ書く言語。アラビア語・ヘブライ語・ペルシア語・ウルドゥー語のほか、同じ文字を使う言語と、
 * 文字の札（`-Arab` / `-Hebr` など）で右→左の文字を名乗るもの。**エンジンの `Intl.Locale` の
 * textInfo には頼らない**——WebView2 と WebKit で載っている版が違い、同じ言語で向きが割れうる
 */
export const RTL_LANG = /^(ar|he|fa|ur|ckb|ps|sd|yi|dv|ug)(-|$)|-(Arab|Hebr|Thaa|Syrc|Nkoo|Adlm|Rohg)(-|$)/i;

export const dirOf = (locale: string): "rtl" | "ltr" => (RTL_LANG.test(locale) ? "rtl" : "ltr");

/** いまの画面が右→左か */
export const isRtl = (): boolean =>
  typeof document !== "undefined" && document.documentElement.dir === "rtl";

/**
 * 画面の左右（矢印キー）を、読む順の前後へ。**右→左では左が「次」**——本の頁をめくる向きと同じ。
 * 送りのボタン（‹ ›）も論理の位置に置いてあるので、キーとボタンの向きは揃う
 */
export const stepOfArrow = (key: "ArrowLeft" | "ArrowRight", rtl: boolean): -1 | 1 =>
  (key === "ArrowRight") !== rtl ? 1 : -1;
