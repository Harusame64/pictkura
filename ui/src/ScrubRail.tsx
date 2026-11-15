import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";

import { formatMonth, formatMonthOnly, t } from "./i18n";
import { year } from "./i18n/plural.ts";
import {
  dayKeyAt,
  fractionAt,
  monthDots,
  monthIndexAt,
  monthStarts,
  scrollTopFor,
  thumbOf,
} from "./scrubRailMath";

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
/** 拡大鏡に並べる月の数（指の月の前後それぞれ） */
const LOUPE_SPAN = 3;
/** 拡大鏡の月どうしの間隔（px） */
const LOUPE_STEP = 22;
/** 止めた一覧の1行の高さ（px）。押しやすい大きさ */
const MENU_STEP = 26;
/** 止めた一覧から、この距離（px）までは指が外れても閉じない（帯との間を渡るため） */
const MENU_SLACK_PX = 24;
/** 指からこの距離（px）までの月が大きくなる。真上は 1.6 倍 */
const DOCK_REACH_PX = 70;
const DOCK_GROW = 0.6;

/** 「2023年8月」（年をまたぐ所と、指の月） */
const monthLabel = (yyyymm: number) => formatMonth(Math.floor(yyyymm / 100), yyyymm % 100);
/** 「8月」（同じ年の前後の月） */
const monthOnly = (yyyymm: number) => formatMonthOnly(Math.floor(yyyymm / 100), yyyymm % 100);

/**
 * 一覧の帯（普通のスクロールバーの代わり。計算は `scrubRailMath.ts`）。
 *
 * - 帯のどこを押しても・掴んで動かしても、その比の所へ一覧が移る
 * - 帯の上に指があるあいだと掴んでいる間は、指の所に横線を引き、帯の横に拡大鏡を出す（指の月を中央に大きく、
 *   前後の月を小さく。押す場所は帯の上のまま——拡大鏡は見るだけ。利用者の「Dock みたいに」から 2026-11-15）
 * - 帯から写真の側へ指を出すと、拡大鏡がその場に止まり、その年の月を押せる一覧になる（2 段: 粗く帯で年へ、
 *   細かく一覧で月へ。帯の上では 1 か月が 1px に満たないことがあり、月を狙えなかった。利用者 2026-11-15）
 * - いま見ている位置には、現在地の札（年月）を常に出す。つまみ（見えている範囲の棒）は、3 万枚では 0.04% で意味が無かった
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
  // 指の所: 帯の上の位置（px）と、拡大鏡の中央の月（yyyymm）。指が帯の外で、掴んでもいないときは出さない
  const [bubble, setBubble] = useState<{ y: number; month: number | null } | null>(null);
  // 止めた拡大鏡（押せる月の一覧）。帯の上の位置（px）と、その年・指の月
  const [menu, setMenu] = useState<{ y: number; month: number } | null>(null);
  // 止めた一覧の上の指の高さ（帯の上の px）。Dock のように、指に近い月ほど大きく濃く描く
  const [menuPointerY, setMenuPointerY] = useState<number | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  // 現在地（一覧の上端の位置）
  const [scrollTop, setScrollTop] = useState(0);
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
        setScrollTop(el.scrollTop);
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

  const starts = useMemo(() => monthStarts(rowTops, dayKeys), [rowTops, dayKeys]);

  const monthAt = useCallback(
    (fraction: number): number | null => {
      const key = dayKeyAt(fraction * total, rowTops, dayKeys);
      return key === null ? null : Math.floor(key / 100);
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
    return { y, month: key !== undefined ? Math.floor(key / 100) : monthAt(fraction) };
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

  // 止めた一覧は、指が一覧からも帯からも離れたら閉じる（一覧へ入らずに別の所へ行ったときも残らないように）
  useEffect(() => {
    if (!menu) return;
    const onMove = (e: PointerEvent) => {
      const inside = (el: Element | null | undefined, pad: number) => {
        const r = el?.getBoundingClientRect();
        return (
          !!r &&
          e.clientX >= r.left - pad &&
          e.clientX <= r.right + pad &&
          e.clientY >= r.top - pad &&
          e.clientY <= r.bottom + pad
        );
      };
      if (!inside(menuRef.current, MENU_SLACK_PX) && !inside(railRef.current, 0)) setMenu(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  /** 止めた一覧の月の並び（その年の、写真のある月。上が新しい月） */
  const menuMonths = menu ? starts.filter((x) => Math.floor(x.month / 100) === Math.floor(menu.month / 100)) : [];
  const menuIndex = menu ? menuMonths.findIndex((x) => x.month === menu.month) : -1;
  // 一覧の上端（帯の上の px）。指の月を、止めたときの指の高さに置く。帯からはみ出さないように詰める
  const menuTop = menu
    ? Math.max(
        0,
        Math.min(
          viewport - (menuMonths.length + 1) * MENU_STEP,
          menu.y - (menuIndex + 1) * MENU_STEP - MENU_STEP / 2,
        ),
      )
    : 0;
  /** 一覧の k 番目の行（0 は年）の、指からの近さ（1＝真上、0＝遠い）。指がまだ一覧に入っていなければ止めた所から */
  const nearness = (k: number) => {
    const py = menuPointerY ?? menu?.y ?? 0;
    const cy = menuTop + k * MENU_STEP + MENU_STEP / 2;
    return Math.max(0, 1 - Math.abs(cy - py) / DOCK_REACH_PX);
  };
  const goTo = (offset: number) => {
    const el = scrollRef.current;
    if (el) el.scrollTop = offset;
    setMenu(null);
    setMenuPointerY(null);
  };

  // 拡大鏡: 指の月を中央に、前後の月を上下へ（上が新しい月。一覧と同じ向き）
  const loupe: { month: number; d: number }[] = [];
  if (bubble?.month != null) {
    const c = starts.findIndex((x) => x.month === bubble.month);
    const center = c >= 0 ? c : monthIndexAt(0, starts);
    for (let k = Math.max(0, center - LOUPE_SPAN); k <= Math.min(starts.length - 1, center + LOUPE_SPAN); k++) {
      loupe.push({ month: starts[k].month, d: k - center });
    }
  }

  // 現在地: 掴んで送っている間は指の所、それ以外は一覧の上端
  const hereFraction = dragFraction ?? (total > 0 ? scrollTop / total : 0);
  const hereMonth = monthAt(dragFraction ?? (total > 0 ? (scrollTop + 1) / total : 0));

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
      onPointerLeave={(e) => {
        if (drag.current) return;
        // 写真の側へ出たら、拡大鏡をその場に止めて、押せる一覧にする（帯の外側＝窓の端へ出たときは止めない）
        const rect = railRef.current?.getBoundingClientRect();
        const rtl = railRef.current ? getComputedStyle(railRef.current).direction === "rtl" : false;
        const towardPhotos = rect ? (rtl ? e.clientX > rect.right : e.clientX < rect.left) : false;
        if (towardPhotos && bubble?.month != null) {
          setMenuPointerY(null);
          setMenu({ y: bubble.y, month: bubble.month });
        }
        setBubble(null);
      }}
      onLostPointerCapture={() => {
        drag.current = null;
        endDrag();
      }}
    >
      {/* 現在地: 帯の溝の印と、帯の横の札（年月）。つまみの代わり */}
      <div className="scrub-here" style={{ top: edgeTop(hereFraction * 100) }} aria-hidden="true">
        {hereMonth !== null && <span className="scrub-here-label">{monthLabel(hereMonth)}</span>}
      </div>
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
      {menu && menuMonths.length > 0 && (
        <div
          ref={menuRef}
          className="scrub-menu"
          style={{ top: `${menuTop}px` }}
          // 一覧の上の操作を、帯の「押す・掴む・拡大鏡」に渡さない
          onPointerDown={(e) => e.stopPropagation()}
          onPointerMove={(e) => {
            e.stopPropagation();
            const r = railRef.current?.getBoundingClientRect();
            if (r) setMenuPointerY(e.clientY - r.top);
          }}
          onPointerUp={(e) => e.stopPropagation()}
          onPointerLeave={(e) => {
            e.stopPropagation();
            // 帯へ戻ったら、また拡大鏡で選び直せるように閉じる
            const r = railRef.current?.getBoundingClientRect();
            if (r && e.clientX >= r.left && e.clientX <= r.right) {
              setMenu(null);
              setMenuPointerY(null);
            }
          }}
        >
          {[{ month: -1, offset: menuMonths[0].offset }, ...menuMonths].map((x, k) => {
            const n = nearness(k);
            const isYear = x.month === -1;
            return (
              <button
                key={x.month}
                className={
                  (isYear ? "scrub-menu-year" : "scrub-menu-item") +
                  (!isYear && x.month === menu.month ? " here" : "") +
                  (n > 0.75 ? " near" : "")
                }
                style={{
                  height: MENU_STEP,
                  fontSize: `calc(var(--fs-sm) * ${1 + DOCK_GROW * n})`,
                  opacity: 0.55 + 0.45 * n,
                }}
                onClick={() => goTo(x.offset)}
              >
                {isYear ? year(Math.floor(menu.month / 100)) : monthOnly(x.month)}
              </button>
            );
          })}
        </div>
      )}
      {bubble && !menu && loupe.length > 0 && (
        <div className="scrub-loupe" style={{ top: `${bubble.y}px` }} aria-hidden="true">
          {loupe.map(({ month, d }) => {
            const center = loupe.find((x) => x.d === 0)?.month ?? month;
            const sameYear = Math.floor(month / 100) === Math.floor(center / 100);
            const a = Math.abs(d);
            return (
              <div
                key={month}
                className={"scrub-loupe-item" + (d === 0 ? " center" : "")}
                style={{
                  // 大きさは文字で変える（scale は寄せる側が左右の向きで変わり、右→左で逆へ縮んだ）
                  transform: `translateY(${d * LOUPE_STEP}px)`,
                  fontSize: `calc(var(--fs-sm) * ${1 - a * 0.12})`,
                  opacity: 1 - a * 0.22,
                }}
              >
                {d === 0 || !sameYear ? monthLabel(month) : monthOnly(month)}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
