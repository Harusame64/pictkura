/** 画面上の箱（`DOMRect` から取る。控えた後でレイアウトが動いても値は変わらない） */
export type Box = { left: number; top: number; width: number; height: number };

export function boxOf(r: DOMRect): Box {
  return { left: r.left, top: r.top, width: r.width, height: r.height };
}

/**
 * ビューアが写真を収める倍率。CSS の `.viewer-image`（`max-width: var(--viewer-fit-w)` ＝ 幅 - 120 - 情報パネルの分、
 * `max-height: calc(100vh - 80px)`、拡大はしない）と同じ枠。表示倍率の分母とタイルから広がる先の箱が、ここ1か所を読む
 */
export function fitScaleFor(
  w: number,
  h: number,
  infoReserve: number,
  viewW: number,
  viewH: number,
): number {
  return Math.min(1, (viewW - 120 - infoReserve) / w, (viewH - 80) / h);
}

/**
 * 開いたときに写真が置かれる箱（情報パネルの分を除いた残りの中央）。寸法は届く絵の大きさ（実寸を知っていれば
 * それ、無ければ `servedSize` の見当）
 */
export function viewerBox(
  [w, h]: [number, number],
  infoReserve: number,
  viewW = window.innerWidth,
  viewH = window.innerHeight,
): Box | null {
  if (!(w > 0 && h > 0)) return null;
  const k = fitScaleFor(w, h, infoReserve, viewW, viewH);
  if (!(k > 0)) return null;
  const width = w * k;
  const height = h * k;
  return {
    left: (viewW - infoReserve - width) / 2,
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
