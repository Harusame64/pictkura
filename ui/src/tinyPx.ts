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

export function rememberTiny(id: number, hex: string | null | undefined) {
  if (!hex || hex.length !== 96 || grounds.has(id)) return;
  grounds.set(id, ground(hex));
}

export const tinyGround = (id: number) => grounds.get(id);
