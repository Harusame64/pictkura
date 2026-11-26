import { memo, useLayoutEffect, useRef } from "react";
import { tinyPixels } from "./tinyPx";

/**
 * タイルの地に敷く 4x4 の色（pictkura-dev #76、`tinyPx.ts`）。4x4 の canvas をタイルいっぱいに伸ばすと、
 * ブラウザがなめらかに補間するので、ぼやけた写真に見える（CSS のグラデーションでは 4 段の色の帯に見えた）。
 * 写真の `<img>` の下に居て、`<img>` は絵が届くまで透明なので、それまでこれが見える。色がまだ無ければ何も置かない
 */
export const TinyCanvas = memo(function TinyCanvas({ id, rev }: { id: number; rev: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const px = tinyPixels(id, rev);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !px) return;
    el.getContext("2d", { alpha: false })?.putImageData(new ImageData(px, 4, 4), 0, 0);
  }, [px]);
  if (!px) return null;
  return <canvas ref={ref} width={4} height={4} className="tiny-ground" aria-hidden />;
});
