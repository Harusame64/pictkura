/**
 * ビューアの UI（道具バー・題名・← ・帯・前後の矢印）は、マウスを止めると消え、当たり判定も無くなる。
 * そのあいだに描かれていた場所を押すと、手は地に落ちてビューアが閉じる（dev #75、win の実測 2026-11-07）。
 * 押した所が消えている UI の上かを見て、そこなら閉じずに UI を出すだけにする
 */

export type Rect = { left: number; top: number; right: number; bottom: number };

/** 点が、どれか 1 つの箱の中（縁を含む）にあるか。大きさの無い箱（描かれていない要素）は数えない */
export function pointInAnyRect(x: number, y: number, rects: readonly Rect[]): boolean {
  return rects.some(
    (r) => r.right > r.left && r.bottom > r.top && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom,
  );
}

/** 消えるビューアの UI。App.css の `.viewer.idle …` と同じ並び */
export const IDLE_HIDDEN_UI = [".viewer-caption", ".viewer-tools", ".viewer-back", ".viewer-strip", ".viewer-nav"];
