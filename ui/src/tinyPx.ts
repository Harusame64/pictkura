/**
 * 写真ごとの 4x4 の色（pictkura-dev #76）。一覧は、絵がまだ届いていないタイルの地にこれを引き伸ばして敷く
 * （`TinyCanvas`）。帯を速く動かす間、灰色の四角ではなく「ぼやけた写真」が並ぶ。
 *
 * 色は日の一覧（`listDays`）だけが運ぶ。`media-updated` の差分は運ばないので、項目には持たせず、
 * **id と中身の版（`rev`）で覚える**——差分で項目が差し替わっても消えず、回して版が進んだ写真には
 * 古い向きの色を出さない（新しい版の色は、次に日の一覧を読んだときに届く）。
 *
 * 画像（`data:` の URL）にはしない: CSP の `img-src` に `data:` が無く、描かれない（win2 の spike）。
 */
const pixels = new Map<string, Uint8ClampedArray<ArrayBuffer>>();

const keyOf = (id: number, rev: number) => `${id}:${rev}`;

/** 96 文字の16進（4x4 の RGB、上の行から）を canvas に置ける RGBA（64 バイト）へ。形が違えば null */
export function rgbaOfHex(hex: string | null | undefined): Uint8ClampedArray<ArrayBuffer> | null {
  if (!hex || !/^[0-9a-f]{96}$/i.test(hex)) return null;
  const out = new Uint8ClampedArray(64);
  for (let i = 0; i < 16; i++) {
    out[i * 4] = parseInt(hex.slice(i * 6, i * 6 + 2), 16);
    out[i * 4 + 1] = parseInt(hex.slice(i * 6 + 2, i * 6 + 4), 16);
    out[i * 4 + 2] = parseInt(hex.slice(i * 6 + 4, i * 6 + 6), 16);
    out[i * 4 + 3] = 255;
  }
  return out;
}

/** 日の一覧で届いた色を覚える。読めない形は捨てる */
export function rememberTiny(id: number, rev: number, hex: string | null | undefined) {
  const key = keyOf(id, rev);
  if (pixels.has(key)) return;
  const px = rgbaOfHex(hex);
  if (px) pixels.set(key, px);
}

/** 覚えた色（RGBA 64 バイト）。その版の色がまだ無ければ undefined */
export const tinyPixels = (id: number, rev: number) => pixels.get(keyOf(id, rev));
