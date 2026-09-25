import type { Presence } from "./api";

/**
 * 写真の原寸が出なかったとき、ビューアの帯で名乗る理由（dev #23）。
 * 純関数。`scripts/originalTrouble.test.ts` が表で見る。
 *
 * - `missing` / `unreachable`: 原本が無い／開けない（#145）。**在っても読み取りを
 *   断られたファイル**も `unreachable` で来る（Rust の `Presence::of_file`）
 * - `notDownloaded`: 在るが**クラウドにしか実体が無い**。配信口は開いた時点で取り寄せを
 *   試みるので、まだクラウドのままなら取り寄せられなかった
 * - `notShown`: 在って開けるのに出せなかった——壊れている、pictkura が読めない形式
 *   （プレビューを持たない RAW、中身の違うファイル）
 *
 * **「在る」を「問題なし」と読まない。** #145 の版は `present` で黙っていたので、
 * 権限の無い1枚も壊れた1枚も、壊れた画像の印とファイル名だけになっていた（実機）。
 */
export type OriginalTrouble =
  | "missing"
  | "unreachable"
  | "notDownloaded"
  | "notShown";

/**
 * **在ると分かった原本を「壊れている」と言ってよいのは、本物の失敗のときだけ。**
 * `<img>` の error は `currentSrc` が空のまま来ることがあり、その後で本命の load が
 * 成功する（App.tsx の onLoad の注記）。その空の error で「表示できません」を出すと、
 * HEIC の詰め直し（0.6〜1秒）の間、正常な写真に誤った帯が乗る（#153 のゲート2）。
 * `failedWithSrc` は、**いまの絵の URL を持った error** を受けたか。
 *
 * 無い・開けないは原本を確かめた事実なので、error の形によらず名乗る。
 *
 * **向きの選択**: 無い・開けないとクラウドのみが同時に立つとき（同期アプリが止まって
 * stat が断られたのに、属性はクラウドのまま、など）は**無い・開けないを先に言う**。
 * 実測はしていない——言い換えるなら表の行ごと直す。
 */
export function originalTrouble(
  presence: Presence,
  cloudOnly: boolean,
  failedWithSrc: boolean,
): OriginalTrouble | null {
  if (presence !== "present") return presence;
  if (!failedWithSrc) return null;
  return cloudOnly ? "notDownloaded" : "notShown";
}

/** 帯の文言の鍵。**理由から文言までを表で固定する**（取り違えは型では止まらない） */
export const TROUBLE_TEXT = {
  missing: "fileMissing",
  unreachable: "fileUnreachable",
  notDownloaded: "fileNotDownloaded",
  notShown: "fileNotShown",
} as const satisfies Record<OriginalTrouble, string>;
