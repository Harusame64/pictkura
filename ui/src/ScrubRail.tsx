import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import { formatMonth, t } from "./i18n";
import { year } from "./i18n/plural.ts";
import { dayKeyAt, fractionAt, scrollTopFor, thumbOf } from "./scrubRailMath";

export type YearMarker = { year: number; rowIndex: number; top: number; label: boolean };

/** つまみの最小の高さ（帯に対する比）。これより小さいと掴めない */
const MIN_THUMB = 0.04;
/** これより動いたら「掴んで送る」。手ぶれで、押しただけの操作が送りに化けないように */
const DRAG_SLOP = 3;
/** 年の目盛りのこの距離（px）以内を押したら、その年の頭へ飛ぶ */
const SNAP_PX = 10;

/**
 * 一覧の帯（普通のスクロールバーの代わり。計算は `scrubRailMath.ts`）。
 *
 * - 帯のどこを押しても・掴んで動かしても、その比の所へ一覧が移る
 * - 動かしている間と、帯の上に指があるあいだは、その位置の年月を吹き出しで出す
 * - 年の目盛りの近くを押すと、その年の頭へ飛ぶ（前からの動き。目盛りは帯の一部で、上で掴んでも動かせる）
 * - ホイール・トラックパッド・キーは一覧そのものが受ける（帯は触らない）
 */
export function ScrubRail({
  scrollRef,
  rowTops,
  dayKeys,
  markers,
  onJumpToRow,
}: {
  scrollRef: RefObject<HTMLDivElement | null>;
  /** 各行の上端の累積（末尾は総高さ） */
  rowTops: readonly number[];
  /** 各行の日（`rowTops` と同じ並び、末尾の総高さの分は無い） */
  dayKeys: readonly number[];
  markers: readonly YearMarker[];
  onJumpToRow: (rowIndex: number) => void;
}) {
  const railRef = useRef<HTMLDivElement>(null);
  const total = rowTops[rowTops.length - 1] ?? 0;
  const [thumb, setThumb] = useState({ top: 0, height: 1 });
  const [viewport, setViewport] = useState(0);
  // 吹き出し: 帯の上の位置（px）と年月。指が帯の外で、掴んでもいないときは出さない
  const [bubble, setBubble] = useState<{ y: number; text: string } | null>(null);
  // 押している間の記録。動かしたかどうかで、離したときの意味（飛ぶ／送り終わり）が変わる
  const drag = useRef<{ startY: number; moved: boolean } | null>(null);
  const frame = useRef(0);

  // 一覧が動いたら（ホイールでも帯でも）つまみを合わせる
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const sync = () => {
      cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(() => {
        setViewport(el.clientHeight);
        setThumb(thumbOf(el.scrollTop, total, el.clientHeight, MIN_THUMB));
      });
    };
    sync();
    el.addEventListener("scroll", sync, { passive: true });
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", sync);
      ro.disconnect();
      cancelAnimationFrame(frame.current);
    };
  }, [scrollRef, total]);

  const textAt = useCallback(
    (fraction: number): string => {
      const key = dayKeyAt(fraction * total, rowTops, dayKeys);
      if (key === null) return "";
      return formatMonth(Math.floor(key / 10000), Math.floor(key / 100) % 100);
    },
    [total, rowTops, dayKeys],
  );

  /** 押すと飛ぶ先になる年の目盛り（点の近く）。吹き出しも同じ目盛りの年月を出す——比で引くと、目盛りの数 px 上は
   *  前の年の終わりで、「2023」の横に「2024年1月」と出た */
  const nearMarker = (y: number) => {
    const h = railRef.current?.getBoundingClientRect().height ?? 0;
    return markers.find((m) => Math.abs((m.top / 100) * h - y) <= SNAP_PX) ?? null;
  };
  const bubbleAt = (y: number, fraction: number, snap: boolean) => {
    const m = snap ? nearMarker(y) : null;
    const key = m ? dayKeys[m.rowIndex] : undefined;
    return {
      y,
      text:
        key !== undefined
          ? formatMonth(Math.floor(key / 10000), Math.floor(key / 100) % 100)
          : textAt(fraction),
    };
  };

  /** 帯の上の点（画面の y）を、比と帯の中の位置へ */
  const pointOf = (clientY: number) => {
    const rect = railRef.current?.getBoundingClientRect();
    if (!rect) return null;
    const y = Math.min(rect.height, Math.max(0, clientY - rect.top));
    return { y, fraction: fractionAt(y, rect.height) };
  };

  const moveTo = (fraction: number) => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = scrollTopFor(fraction, total, el.clientHeight);
  };

  // 中身が1画面に収まるなら、帯は要らない（動かす先が無い）
  if (!(total > viewport + 1) && viewport > 0) return null;

  return (
    <div
      ref={railRef}
      className={"scrub-rail" + (bubble ? " active" : "")}
      role="scrollbar"
      aria-orientation="vertical"
      aria-label={t.scrubRailLabel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(thumb.top * 100)}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        const p = pointOf(e.clientY);
        if (!p) return;
        // 押しただけ（動かさずに離す）なら「そこへ飛ぶ」、動かしたら「掴んで送る」。決めるのは動いたとき
        drag.current = { startY: p.y, moved: false };
        e.currentTarget.setPointerCapture(e.pointerId);
        setBubble(bubbleAt(p.y, p.fraction, true));
        e.preventDefault();
      }}
      onPointerMove={(e) => {
        const p = pointOf(e.clientY);
        if (!p) return;
        const d = drag.current;
        if (d) {
          if (!d.moved && Math.abs(p.y - d.startY) > DRAG_SLOP) d.moved = true;
          if (d.moved) moveTo(p.fraction);
        }
        // 掴んで送っている間は、指の所の年月（目盛りに吸い寄せない）
        setBubble(bubbleAt(p.y, p.fraction, !d?.moved));
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        if (!d || d.moved) return;
        const p = pointOf(e.clientY);
        if (!p) return;
        // 年の目盛りの近くを押したら、その年の頭へぴったり飛ぶ（比で送ると数 px ずれて前の年の終わりに着く）
        const near = nearMarker(p.y);
        if (near) onJumpToRow(near.rowIndex);
        else moveTo(p.fraction);
      }}
      onPointerCancel={() => {
        drag.current = null;
        setBubble(null);
      }}
      onPointerLeave={() => {
        if (!drag.current) setBubble(null);
      }}
      onLostPointerCapture={() => {
        drag.current = null;
      }}
    >
      <div
        className="scrub-thumb"
        style={{ top: `${thumb.top * 100}%`, height: `${thumb.height * 100}%` }}
      />
      {markers.length > 1 &&
        markers.map((m) => (
          // 目盛りは絵だけ（押すのは帯。近くを押すと、その年の頭へ飛ぶ）。ボタンにすると、ラベルの上で掴めなかった
          <span
            key={m.year}
            className={"scrub-tick" + (m.label ? " labeled" : "")}
            style={{ top: `${m.top}%` }}
          >
            {/* 年は地域の数字で（ar-SA ならアラビア・インド数字。`year()`） */}
            {m.label ? year(m.year) : ""}
          </span>
        ))}
      {bubble && bubble.text && (
        <div className="scrub-bubble" style={{ top: `${bubble.y}px` }} aria-hidden="true">
          {bubble.text}
        </div>
      )}
    </div>
  );
}
