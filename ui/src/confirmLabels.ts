/**
 * 確認ダイアログのキャンセルの語を決める（純関数。`scripts/confirmLabels.test.ts` が表で見る）。
 *
 * **macOS は OS の言語の語を渡す**——AppKit が Esc を割り当てるのは、題が OS の言語の
 * 「キャンセル」か英語の `Cancel` のボタンだけ。アプリだけドイツ語にした日本語の Mac で
 * `Abbrechen` を渡すと、Esc で閉じない（2026-09-25 実機、#147）。
 * OS の言語の辞書が無ければ `undefined`（＝プラグイン既定の英語 `Cancel`。これも Esc が効く）。
 *
 * **Windows とその他はアプリの言語**——Windows は `TDF_ALLOW_DIALOG_CANCELLATION` で、
 * 語によらず Esc が効く。
 *
 * この分岐は実機でしか見つからなかった。**`t` に寄せる「整理」をすると、緑のまま戻る**。
 */
export function cancelLabelFor(
  platform: "windows" | "macos" | "other",
  appCancel: string,
  osCancel: string | undefined,
): string | undefined {
  return platform === "macos" ? osCancel : appCancel;
}
