import type { KeyboardEvent } from "react";

/**
 * `div` や `span` で描いている「押せるもの」を、UI Automation と読み上げにボタンとして見せる（pictkura-dev #65）。
 *
 * 見た目と押したときの動き（`onClick`）は今のまま、役割（`role="button"`）と、キーボードで届く・押せること
 * （Tab で止まる・Enter と Space で押す）を足す。`<button>` へ置き換えないのは、既定の見た目と、中に置いた
 * ボタン（行の ✕ など）が入れ子にできないため。
 *
 * `focusable: false` は役割と名前だけを足す（一覧のタイルのように、数百枚を Tab で一つずつ止めたくないもの。
 * 一覧には自分のキー操作がある）
 */
export function pressable(
  onActivate: () => void,
  opts: { focusable?: boolean } = {},
): {
  role: "button";
  tabIndex?: number;
  onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
} {
  return {
    role: "button",
    tabIndex: opts.focusable === false ? undefined : 0,
    onKeyDown: (e) => {
      // 中の本物のボタン（行の ✕ 等）で押したキーは、そちらに任せる
      if (e.target !== e.currentTarget) return;
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onActivate();
      }
    },
  };
}
