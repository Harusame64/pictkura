/** 画面上の箱（`DOMRect` から取る。控えた後でレイアウトが動いても値は変わらない） */
export type Box = { left: number; top: number; width: number; height: number };

export function boxOf(r: DOMRect): Box {
  return { left: r.left, top: r.top, width: r.width, height: r.height };
}

/** 写真のまわりに空ける幅（左右の合計・上下の合計）。CSS の `.viewer` の `--viewer-pad-x` / `--viewer-pad-y` と同じ値 */
export const VIEWER_PAD = { x: 120, y: 80 };
/** 全画面では空けない（道具は写真の上に重なり、マウスを止めると消える）。CSS の `.viewer.fullscreen` と同じ */
export const VIEWER_PAD_FULLSCREEN = { x: 0, y: 0 };
/** 全画面の動画は字幕の帯（`--caption-h`）の高さだけ下を空ける。CSS の `.viewer.fullscreen .viewer-video` と同じ */
export const VIEWER_PAD_FULLSCREEN_VIDEO = { x: 0, y: 44 };

/** ビューアの今の空き。全画面か・動画かで決まる（CSS の `.viewer.fullscreen` と `.viewer.fullscreen .viewer-video`） */
export function viewerPad(fullscreen: boolean, video: boolean): { x: number; y: number } {
  // spike: 窓でも全画面と同じく余白なし（利用者 11-27「ウィンドウ一杯に」）。VIEWER_PAD は比べるために残す
  void fullscreen;
  return video ? VIEWER_PAD_FULLSCREEN_VIDEO : VIEWER_PAD_FULLSCREEN;
}

/**
 * ビューアが写真を収める倍率。CSS の `.viewer-image`（`max-width: var(--viewer-fit-w)` ＝ 幅 - 左右の空き - 情報パネルの分、
 * `max-height: var(--viewer-fit-h)` ＝ 高さ - 上下の空き、拡大はしない）と同じ枠。表示倍率の分母とタイルから広がる先の箱が、ここ1か所を読む
 */
export function fitScaleFor(
  w: number,
  h: number,
  infoReserve: number,
  viewW: number,
  viewH: number,
  pad: { x: number; y: number } = VIEWER_PAD,
): number {
  return Math.min(1, (viewW - pad.x - infoReserve) / w, (viewH - pad.y) / h);
}

/**
 * 開いたときに写真が置かれる箱（情報パネルの分を除いた残りの中央）。寸法は届く絵の大きさ（実寸を知っていれば
 * それ、無ければ `servedSize` の見当）。情報パネルは行の終わりの側に開くので、右→左（`rtl`）では左に在る
 */
export function viewerBox(
  [w, h]: [number, number],
  infoReserve: number,
  // 省けなくしてある——省いた呼び出しが右→左で黙ってずれた箱を返さないように（呼ぶ側は `isRtl()`）
  rtl: boolean,
  viewW = window.innerWidth,
  viewH = window.innerHeight,
  pad: { x: number; y: number } = VIEWER_PAD,
): Box | null {
  if (!(w > 0 && h > 0)) return null;
  const k = fitScaleFor(w, h, infoReserve, viewW, viewH, pad);
  if (!(k > 0)) return null;
  const width = w * k;
  const height = h * k;
  return {
    left: (rtl ? infoReserve : 0) + (viewW - infoReserve - width) / 2,
    top: (viewH - height) / 2,
    width,
    height,
  };
}

/**
 * 要素の箱のうち、`object-fit: contain` で絵が実際に描かれている部分。下敷き（`.viewer-thumb`）は幅と高さの上限が
 * 別々に効くので、箱が絵より横長・縦長になることがある——その箱から縮めると、最初の1コマで絵が切り抜かれて跳ねる
 */
export function containBox(b: Box, natW: number, natH: number): Box {
  if (!(natW > 0 && natH > 0 && b.width > 0 && b.height > 0)) return b;
  const k = Math.min(b.width / natW, b.height / natH);
  const width = natW * k;
  const height = natH * k;
  return {
    left: b.left + (b.width - width) / 2,
    top: b.top + (b.height - height) / 2,
    width,
    height,
  };
}
