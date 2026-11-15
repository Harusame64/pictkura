import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";

import { formatMonth, t } from "./i18n";
import { year } from "./i18n/plural.ts";
import { dayKeyAt, fractionAt, monthDots, scrollTopFor, thumbOf } from "./scrubRailMath";

/** 帯の上の位置（%）。下端の目盛りは、文字の半分が帯の外へはみ出さないよう、帯の中に留める */
const edgeTop = (percent: number) => `clamp(8px, ${percent}%, calc(100% - 8px))`;

export type YearMarker = { year: number; rowIndex: number; top: number; label: boolean };

/** つまみの最小の高さ（px）。これより小さいと見えにくい（win2 の試作で 40px） */
const MIN_THUMB_PX = 40;
/** これより動いたら「掴んで送る」。手ぶれで、押しただけの操作が送りに化けないように */
const DRAG_SLOP = 3;
/** 年の目盛りのこの距離（px）以内を押したら、その年の頭へ飛ぶ */
const SNAP_PX = 10;
/** 月の仕切りの点どうし・年の目盛りとの最小の間隔（px）。詰まる所は点を省く */
const MONTH_GAP_PX = 7;
/**
 * 掴んで送っている間、一覧を動かす間隔（ms）。指が動くたびに動かすと、行き先の行を毎フレーム作り直して
 * カクついた（win2 の実測 2026-11-15、33,178 件: rAF p95 71ms）。間は帯の上だけ（つまみ・吹き出し・横線）が指に付いて動き、
 * 一覧はこの間隔で追いつく。離したときは必ず指の所へ動かす
 */
const DRAG_MOVE_MS = 120;

/**
 * 一覧の帯（普通のスクロールバーの代わり。計算は `scrubRailMath.ts`）。
 *
 * - 帯のどこを押しても・掴んで動かしても、その比の所へ一覧が移る
 * - 動かしている間と、帯の上に指があるあいだは、その位置の年月を吹き出しで出す
 * - 年の目盛りの近くを押すと、その年の頭へ飛ぶ（前からの動き。目盛りは帯の一部で、上で掴んでも動かせる）
 * - 年と年の間には、月の仕切りの小さな点を置く（Google フォトのように。詰まる所は省く）
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
  // 掴んで送っている間の、つまみの仮の位置（指の所）。一覧が追いつくまで、つまみは指に付いて動く
  const [dragFraction, setDragFraction] = useState<number | null>(null);
  const moveTimer = useRef<number | undefined>(undefined);
  const lastMoveAt = useRef(0);
  const pendingFraction = useRef(0);

  // 一覧が動いたら（ホイールでも帯でも）つまみを合わせる
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const sync = () => {
      cancelAnimationFrame(frame.current);
      frame.current = requestAnimationFrame(() => {
        setViewport(el.clientHeight);
        setThumb(
          thumbOf(el.scrollTop, total, el.clientHeight, MIN_THUMB_PX / Math.max(1, el.clientHeight)),
        );
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

  // 月の仕切り。帯の高さ（≒一覧の見えている高さ）で間引くので、窓の大きさが変わったら置き直す
  const dots = useMemo(
    () => monthDots(rowTops, dayKeys, viewport, markers.map((m) => m.top / 100), MONTH_GAP_PX),
    [rowTops, dayKeys, viewport, markers],
  );

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
  //ラベルの出ている年だけを見て、いちばん近いものを選ぶ。点だけの年（詰まって省いた年）や先着順で拾うと、帯の下端に
  //古い年の目盛りが 20 本詰まった所で「2014」を押して 2016 年に着いた（win2 の実測）
  const nearMarker = (y: number) => {
    const h = railRef.current?.getBoundingClientRect().height ?? 0;
    let best: YearMarker | null = null;
    let bestD = SNAP_PX;
    for (const m of markers) {
      if (!m.label) continue;
      // 目盛りは帯の端で内側へ留めて描くので（`edgeTop`）、押す位置も同じ所で比べる
      const d = Math.abs(Math.min(h - 8, Math.max(8, (m.top / 100) * h)) - y);
      if (d <= bestD) {
        best = m;
        bestD = d;
      }
    }
    return best;
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
  /** 掴んで送っている間の移動。`DRAG_MOVE_MS` に1回まで、最後の指の位置へ（間引いた分は後追いで必ず動かす） */
  const moveWhileDragging = (fraction: number) => {
    pendingFraction.current = fraction;
    const wait = DRAG_MOVE_MS - (performance.now() - lastMoveAt.current);
    if (wait <= 0) {
      window.clearTimeout(moveTimer.current);
      moveTimer.current = undefined;
      lastMoveAt.current = performance.now();
      moveTo(fraction);
    } else if (moveTimer.current === undefined) {
      moveTimer.current = window.setTimeout(() => {
        moveTimer.current = undefined;
        lastMoveAt.current = performance.now();
        moveTo(pendingFraction.current);
      }, wait);
    }
  };
  const endDrag = () => {
    window.clearTimeout(moveTimer.current);
    moveTimer.current = undefined;
    setDragFraction(null);
  };
  useEffect(() => () => window.clearTimeout(moveTimer.current), []);

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
          if (d.moved) {
            moveWhileDragging(p.fraction);
            setDragFraction(p.fraction);
          }
        }
        // 掴んで送っている間は、指の所の年月（目盛りに吸い寄せない）
        setBubble(bubbleAt(p.y, p.fraction, !d?.moved));
      }}
      onPointerUp={(e) => {
        const d = drag.current;
        drag.current = null;
        const p = pointOf(e.clientY);
        if (d?.moved) {
          // 送り終わり: 間引いた分を残さず、指を離した所へ
          endDrag();
          if (p) moveTo(p.fraction);
          return;
        }
        if (!d || !p) return;
        // 年の目盛りの近くを押したら、その年の頭へぴったり飛ぶ（比で送ると数 px ずれて前の年の終わりに着く）
        const near = nearMarker(p.y);
        if (near) onJumpToRow(near.rowIndex);
        else moveTo(p.fraction);
      }}
      onPointerCancel={() => {
        drag.current = null;
        endDrag();
        setBubble(null);
      }}
      onPointerLeave={() => {
        if (!drag.current) setBubble(null);
      }}
      onLostPointerCapture={() => {
        drag.current = null;
        endDrag();
      }}
    >
      <div
        className="scrub-thumb"
        style={
          dragFraction === null
            ? { top: `${thumb.top * 100}%`, height: `${thumb.height * 100}%` }
            : {
                top: `${Math.min(1 - thumb.height, dragFraction) * 100}%`,
                height: `${thumb.height * 100}%`,
              }
        }
      />
      {dots.map((f) => (
        <span key={f} className="scrub-dot" style={{ top: edgeTop(f * 100) }} />
      ))}
      {markers.length > 1 &&
        markers.map((m) => (
          // 目盛りは絵だけ（押すのは帯。近くを押すと、その年の頭へ飛ぶ）。ボタンにすると、ラベルの上で掴めなかった
          <span
            key={m.year}
            className={"scrub-tick" + (m.label ? " labeled" : "")}
            style={{ top: edgeTop(m.top) }}
          >
            {/* 年は地域の数字で（ar-SA ならアラビア・インド数字。`year()`） */}
            {m.label ? year(m.year) : ""}
          </span>
        ))}
      {/* 指の所の横線（利用者の希望、2026-11-15）。帯から外れたら消える（掴んでいる間は外でも付いてくる） */}
      {bubble && <div className="scrub-cursor" style={{ top: `${bubble.y}px` }} aria-hidden="true" />}
      {bubble && bubble.text && (
        <div className="scrub-bubble" style={{ top: `${bubble.y}px` }} aria-hidden="true">
          {bubble.text}
        </div>
      )}
    </div>
  );
}
