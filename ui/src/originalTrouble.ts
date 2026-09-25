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

export function originalTrouble(
  presence: Presence,
  cloudOnly: boolean,
): OriginalTrouble {
  if (presence !== "present") return presence;
  return cloudOnly ? "notDownloaded" : "notShown";
}
