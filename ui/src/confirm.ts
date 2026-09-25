/**
 * 確認ダイアログ（ゴミ箱へ・移動・フォルダを外す）。**プラグインの `confirm` を import するのは
 * ここだけ**——直に呼ぶと既定の英語の Cancel / OK に戻る（#146 の実機）。決まりを
 * コメントではなく形にするため、`App.tsx` から外へ出した（#147 の2ゲート目）。
 */
import { confirm } from "@tauri-apps/plugin-dialog";

import { cancelLabelFor } from "./confirmLabels";
import { errText } from "./i18n/err.ts";
import { osT, t } from "./i18n";
import type { HostPlatform } from "./api";

/**
 * - **OK は押すと何が起きるかを言う**（`okLabel`）
 * - キャンセルの語は `cancelLabelFor`（macOS は OS の言語）
 * - **開けなかったら、知らせてから「取り消した」と同じに扱う**——黙って取り消しに倒すと、
 *   押しても何も起きないボタンになる（呼び側の `catch` に届いていた失敗を握り潰さない）
 */
export async function confirmAction(
  platform: HostPlatform,
  message: string,
  okLabel: string,
  onError: (message: string) => void,
): Promise<boolean> {
  try {
    return await confirm(message, {
      title: t.appName,
      kind: "warning",
      okLabel,
      cancelLabel: cancelLabelFor(platform, t.confirmCancel, osT?.confirmCancel),
    });
  } catch (e) {
    onError(errText(e));
    return false;
  }
}
