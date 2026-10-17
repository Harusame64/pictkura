import { useLayoutEffect, useRef } from "react";

import type { Box } from "./viewerZoomBox";

/**
 * グリッドとビューアの間の動き（pictkura-dev #52）。開くときは押したマスから写真の位置へ広がり、閉じるときは
 * いまの写真のマスへ縮んで戻る。
 *
 * - `to` が `null` の間は `from` に止めて描く（閉じるとき、戻る先のマスを測るまで写真を消さない）
 * - 箱は `overflow: hidden` で、中の絵は `object-fit: cover`。マスは写真を切り抜いて見せ、ビューアは写真の
 *   縦横比の箱に収めるので、箱の形が変わるにつれて切り抜きがほどける
 */
export type Zoom = {
  kind: "open" | "close";
  src: string;
  from: Box;
  to: Box | null;
  /** 戻る先が見つからなかった（閉じる）。その場で薄れて消える */
  lost?: boolean;
};

/** 動きの長さ（ms）。送りの動き（手で 250）より短く、押した手に遅れない程度 */
export const ZOOM_MS = 220;
const EASE = "cubic-bezier(0.2, 0, 0, 1)";

/** 動きを減らす設定の OS では動かさない（送りの動きと同じ判定） */
export function zoomAllowed(): boolean {
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

function frame(b: Box): Keyframe {
  return {
    left: `${b.left}px`,
    top: `${b.top}px`,
    width: `${b.width}px`,
    height: `${b.height}px`,
  };
}

export function ViewerZoom({
  zoom,
  onDone,
}: {
  zoom: Zoom;
  onDone: () => void;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const backRef = useRef<HTMLDivElement>(null);
  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  useLayoutEffect(() => {
    const box = boxRef.current;
    const back = backRef.current;
    if (!box || !back) return;
    if (zoom.lost) {
      const a = box.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: ZOOM_MS,
        easing: "ease-out",
        fill: "forwards",
      });
      back.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: ZOOM_MS,
        fill: "forwards",
      });
      a.onfinish = () => doneRef.current();
      return () => a.cancel();
    }
    if (!zoom.to) return;
    const a = box.animate([frame(zoom.from), frame(zoom.to)], {
      duration: ZOOM_MS,
      easing: EASE,
      fill: "forwards",
    });
    // 開くときの地はビューアの側が薄れて入ってくる。ここで敷くのは閉じるときだけ
    if (zoom.kind === "close") {
      back.animate([{ opacity: 1 }, { opacity: 0 }], {
        duration: ZOOM_MS,
        easing: "ease-out",
        fill: "forwards",
      });
    }
    a.onfinish = () => doneRef.current();
    return () => a.cancel();
  }, [zoom]);

  return (
    <div className="viewer-zoom" aria-hidden>
      {zoom.kind === "close" && <div className="viewer-zoom-back" ref={backRef} />}
      {zoom.kind === "open" && <div ref={backRef} />}
      <div
        className="viewer-zoom-box"
        ref={boxRef}
        style={{
          left: zoom.from.left,
          top: zoom.from.top,
          width: zoom.from.width,
          height: zoom.from.height,
        }}
      >
        <img src={zoom.src} alt="" draggable={false} />
      </div>
    </div>
  );
}
