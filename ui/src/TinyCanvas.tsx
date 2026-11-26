import { useLayoutEffect, useRef } from "react";
import { tinyPixels } from "./tinyPx";

/**
 * spike #76 (canvas variant): a 4x4 canvas stretched over the tile. The browser scales it with smoothing, so it
 * reads as a blurred photo instead of the gradient variant's four colour bands. Sits under the <img>, which is
 * transparent until its picture arrives.
 */
export function TinyCanvas({ id }: { id: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const px = tinyPixels(id);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !px) return;
    el.getContext("2d")?.putImageData(px, 0, 0);
  }, [px]);
  if (!px) return null;
  return <canvas ref={ref} width={4} height={4} className="tiny-ground" aria-hidden />;
}
