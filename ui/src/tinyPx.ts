/**
 * 写真ごとの 4x4 の色（pictkura-dev #76）。一覧は、絵がまだ届いていないタイルの地にこれを引き伸ばして敷く
 * （`TinyCanvas`）。帯を速く動かす間、灰色の四角ではなく「ぼやけた写真」が並ぶ。
 *
 * 色を運ぶのは日の一覧（`listDays`）と、サムネイルが出来た知らせ（`media-updated`）。ほかの差分は運ばない
 * ので、項目には持たせず、**id と中身の版（`rev`）で覚える**——色の無い差分で項目が差し替わっても消えず、
 * 回して版が進んだ写真には古い向きの色を出さない（新しい版の色は、出来た知らせか次の日の一覧で届く）。
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

/**
 * 覚えておく色の数の上限。古い順に捨てる（捨てた色は、その日の一覧を読み直せばまた届く）。3万枚のライブラリでも
 * 全部覚えて数 MB
 */
const MAX_REMEMBERED = 60_000;

/**
 * 日の一覧で届いた色を覚える。読めない形・null は捨てる（前の色を消さない）。**同じ版でも違う色が届いたら
 * 差し替える**——即席のサムネイル（埋め込みの絵。上下に黒い帯があることも）の色が、本番の色に替わる
 * （PR #366 の /code-review）
 */
export function rememberTiny(id: number, rev: number, hex: string | null | undefined) {
  const px = rgbaOfHex(hex);
  if (!px) return;
  const key = keyOf(id, rev);
  const had = pixels.get(key);
  if (had && had.every((v, i) => v === px[i])) return;
  // 差し替えも入れ直す（Map の順を新しくして、古い順に捨てる対象から外す）
  pixels.delete(key);
  pixels.set(key, px);
  if (pixels.size > MAX_REMEMBERED) {
    const oldest = pixels.keys().next().value;
    if (oldest !== undefined) pixels.delete(oldest);
  }
}

/** 覚えた色（RGBA 64 バイト）。その版の色がまだ無ければ undefined */
export const tinyPixels = (id: number, rev: number) => pixels.get(keyOf(id, rev));

/**
 * 覚えた色を全部捨てる（設定の「サムネイルを作り直す」）。作り直しでは版が進まないので、捨てないと前の絵の
 * 色が出続ける（win2、PR #366）。作り直された写真の色は、出来た知らせと日の一覧でまた届く
 */
export function forgetAllTiny() {
  pixels.clear();
}
