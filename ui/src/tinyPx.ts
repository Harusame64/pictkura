/**
 * spike #76: each photo's 4x4 colours (from `list_days`), drawn as the tile's ground until the picture is here.
 * Kept by id outside React: `media-updated` patches do not carry them, so a patch must not wipe them.
 *
 * Drawn as CSS gradients, not an image: the app's CSP (`img-src 'self' media: …`) refuses `data:` images, so a
 * data-URL BMP was never painted (win2, 2026-11-26: 98 violations in 1.5 s). Four rows, each smooth left to
 * right; rows step top to bottom.
 */
export type TinyGround = { backgroundImage: string; backgroundSize: string; backgroundPosition: string };

const grounds = new Map<number, TinyGround>();

const ROW_POS = "0 0, 0 33.333%, 0 66.667%, 0 100%";

function ground(hex: string): TinyGround {
  const rows: string[] = [];
  for (let y = 0; y < 4; y++) {
    const stops: string[] = [];
    for (let x = 0; x < 4; x++) {
      const i = (y * 4 + x) * 6;
      stops.push(`#${hex.slice(i, i + 6)}`);
    }
    rows.push(`linear-gradient(to right, ${stops.join(", ")})`);
  }
  return { backgroundImage: rows.join(", "), backgroundSize: "100% 25%", backgroundPosition: ROW_POS };
}

const hexes = new Map<number, string>();

export function rememberTiny(id: number, hex: string | null | undefined) {
  if (!hex || hex.length !== 96 || hexes.has(id)) return;
  hexes.set(id, hex);
  grounds.set(id, ground(hex));
}

export const tinyGround = (id: number) => grounds.get(id);

/** canvas variant: the raw 4x4 RGBA, for a 4x4 <canvas> the browser scales up smoothly (a real blur) */
const pixels = new Map<number, ImageData>();

export function tinyPixels(id: number): ImageData | undefined {
  const hit = pixels.get(id);
  if (hit) return hit;
  const hex = hexes.get(id);
  if (!hex) return undefined;
  const d = new ImageData(4, 4);
  for (let i = 0; i < 16; i++) {
    d.data[i * 4] = parseInt(hex.slice(i * 6, i * 6 + 2), 16);
    d.data[i * 4 + 1] = parseInt(hex.slice(i * 6 + 2, i * 6 + 4), 16);
    d.data[i * 4 + 2] = parseInt(hex.slice(i * 6 + 4, i * 6 + 6), 16);
    d.data[i * 4 + 3] = 255;
  }
  pixels.set(id, d);
  return d;
}
