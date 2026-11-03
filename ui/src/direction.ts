/**
 * 画面の向き（右から左へ書く言語）。
 *
 * **向きは `<html dir>` が持つ**（main.tsx が言語から決めて付ける）。JS はそれを**その都度読む**——
 * 起動時に控えた値を持たないので、試験で `dir` を付け替えても追える。CSS は論理プロパティと
 * `:dir(rtl)` で同じ属性に従う（scripts/logicalCss.test.ts）。
 */

/** 右から左へ書く言語（アラビア語・ヘブライ語・ペルシア語・ウルドゥー語） */
export const RTL_LANG = /^(ar|he|fa|ur)(-|$)/;

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
