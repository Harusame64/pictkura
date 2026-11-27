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
export const IDLE_HIDDEN_UI = [".viewer-caption", ".viewer-vbar", ".viewer-tools", ".viewer-back", ".viewer-strip", ".viewer-nav"];

/**
 * 押した所を見る相手。題名は横幅いっぱいの箱なので、その箱ではなく中身（文字）を見る——箱で見ると、下の帯の
 * どこを押しても閉じなくなる（ゲート2）。フィルムストリップは中身の幅の台なので台そのもの（台の余白やコマの間を
 * 押しても、見えている間と同じく閉じない。/code-review）。道具バーも並んだボタンを見る
 */
export const IDLE_HIT_TARGETS = [
  ".viewer-tools > *",
  ".viewer-back",
  ".viewer-nav",
  ".viewer-strip",
  ".viewer-caption > *",
  // 再生バーは押せる所だけ（帯と部品の塊）。箱の上の空きは動画のクリックへ素通しする（App.css の pointer-events）
  ".viewer-vbar .vbar-seek",
  ".viewer-vbar .vbar-group",
];
