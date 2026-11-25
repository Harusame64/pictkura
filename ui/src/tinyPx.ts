/**
 * spike #76: each photo's 4x4 colours (from `list_days`), drawn as the tile's ground until the picture is here.
 * Kept by id outside React: `media-updated` patches do not carry them, so a patch must not wipe them.
 */
const urls = new Map<number, string>();

function bmp4x4(hex: string): string {
  const px = new Uint8Array(48);
  for (let i = 0; i < 48; i++) px[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  const b = new Uint8Array(54 + 48);
  const v = new DataView(b.buffer);
  b[0] = 0x42;
  b[1] = 0x4d;
  v.setUint32(2, b.length, true);
  v.setUint32(10, 54, true);
  v.setUint32(14, 40, true);
  v.setInt32(18, 4, true);
  v.setInt32(22, 4, true);
  v.setUint16(26, 1, true);
  v.setUint16(28, 24, true);
  v.setUint32(34, 48, true);
  // BMP rows go bottom-up, pixels are BGR; 4 px * 3 = 12 bytes, already a multiple of 4
  for (let y = 0; y < 4; y++) {
    for (let x = 0; x < 4; x++) {
      const src = (y * 4 + x) * 3;
      const dst = 54 + (3 - y) * 12 + x * 3;
      b[dst] = px[src + 2];
      b[dst + 1] = px[src + 1];
      b[dst + 2] = px[src];
    }
  }
  let s = "";
  for (const c of b) s += String.fromCharCode(c);
  return `url(data:image/bmp;base64,${btoa(s)})`;
}

export function rememberTiny(id: number, hex: string | null | undefined) {
  if (!hex || hex.length !== 96 || urls.has(id)) return;
  urls.set(id, bmp4x4(hex));
}

export const tinyUrl = (id: number) => urls.get(id);
