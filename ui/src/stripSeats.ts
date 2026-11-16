/**
 * ビューア下のフィルムストリップの台（App.css の `.viewer-strip`）の席の数と幅。
 *
 * 台の幅は「今の1枚を真ん中に、前後 r 席ずつ」で固定する——中身の数で幅を変えると、一覧の端の近くでは送るたびに
 * 台が伸び縮みして動いた（/code-review）。r は窓に丸ごと入る数まで: 入らない席を並べると、台の丸い角でコマが途中で
 * 切れた。端で写真の無い席は空けておく（「前後に何があるか」の距離を偽らない）
 */

/** コマの幅（App.css の `.strip-cell`）と、コマの間・台の内側の余白 */
export const STRIP_CELL = 56;
export const STRIP_GAP = 4;
export const STRIP_PAD = 6;
/** 台の外の、写真の側の左右の余白（台が窓や情報パネルの縁に触れない） */
export const STRIP_MARGIN = 16;

/** 前後の席の数（片側）。`maxRadius` が上限、窓が狭くても今の1枚と前後1席は残す */
export function stripRadiusFor(windowWidth: number, infoReserve: number, maxRadius: number): number {
  const inner = windowWidth - infoReserve - 2 * STRIP_MARGIN - 2 * STRIP_PAD;
  const seats = Math.floor((inner + STRIP_GAP) / (STRIP_CELL + STRIP_GAP));
  return Math.max(1, Math.min(maxRadius, Math.floor((seats - 1) / 2)));
}

/** 前後 r 席の台の幅（px。余白を含む） */
export function stripDockWidth(radius: number): number {
  const seats = 2 * radius + 1;
  return seats * STRIP_CELL + (seats - 1) * STRIP_GAP + 2 * STRIP_PAD;
}
