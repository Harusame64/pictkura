/** 画面上の箱（`DOMRect` から取る。控えた後でレイアウトが動いても値は変わらない） */
export type Box = { left: number; top: number; width: number; height: number };

export function boxOf(r: DOMRect): Box {
  return { left: r.left, top: r.top, width: r.width, height: r.height };
}

/**
 * 開いたときに写真が置かれる箱。`.viewer-image` の収め方（拡大はしない・幅は `--viewer-fit-w`、高さは
 * `100vh - 80px`、情報パネルの分を除いた残りの中央）を写す。寸法は `servedSize`（届く絵の大きさ）
 */
export function viewerBox(
  [w, h]: [number, number],
  infoReserve: number,
  viewW = window.innerWidth,
  viewH = window.innerHeight,
): Box | null {
  if (!(w > 0 && h > 0)) return null;
  const areaW = viewW - infoReserve;
  const areaH = viewH;
  const k = Math.min(1, (areaW - 120) / w, (areaH - 80) / h);
  if (!(k > 0)) return null;
  const width = w * k;
  const height = h * k;
  return {
    left: (areaW - width) / 2,
    top: (areaH - height) / 2,
    width,
    height,
  };
}

