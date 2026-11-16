/**
 * 右クリックメニューで、キーを押したときに行く項目の番号（`ContextMenu.tsx`）。上下は端で回る。今どの項目にも
 * フォーカスが無い（`at` が -1。指がメニューの外へ出た後など）ときは、↓で先頭、↑で最後へ。動くキーでなければ `null`
 */
export function menuMoveIndex(key: string, at: number, n: number): number | null {
  if (n === 0) return null;
  if (key === "Home") return 0;
  if (key === "End") return n - 1;
  if (key === "ArrowDown") return at < 0 ? 0 : (at + 1) % n;
  if (key === "ArrowUp") return at < 0 ? n - 1 : (at - 1 + n) % n;
  return null;
}
