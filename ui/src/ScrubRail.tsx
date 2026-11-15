import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";

import { formatMonth, formatMonthOnly, t } from "./i18n";
import { year } from "./i18n/plural.ts";
import {
  dayKeyAt,
  fractionAt,
  monthDots,
  monthIndexAt,
  monthStarts,
  chooseLabels,
  makeRailMap,
  recencyFactor,
} from "./scrubRailMath";

/** 帯の上の位置（%）。下端の目盛りは、文字の半分が帯の外へはみ出さないよう、帯の中に留める */
const edgeTop = (percent: number) => `clamp(8px, ${percent}%, calc(100% - 8px))`;

export type YearMarker = { year: number; rowIndex: number; top: number; label: boolean };

/** これより動いたら「掴んで送る」。手ぶれで、押しただけの操作が送りに化けないように */
const DRAG_SLOP = 3;
/** 年の目盛りのこの距離（px）以内を押したら、その年の頭へ飛ぶ */
const SNAP_PX = 10;
/** 月の仕切りの点どうし・年の目盛りとの最小の間隔（px）。詰まる所は点を省く */
const MONTH_GAP_PX = 5;
/**
 * 帯の幅の割り当てで、写真の量に比例させる割合（残りは月の数で等分）。量だけだと、写真の少ない月・始まったばかりの
 * 今年が帯の上で幅を持たず、窓を縦に狭くすると 2026 と 2025 の間に月の点が置けなかった（利用者 2026-11-15）
 */
const RAIL_ALPHA = 0.5;
/**
 * 最近の月を広く取る（等分の重みを `RECENT_BOOST` 倍）。「いちばん使うのは最近」（利用者 2026-11-15）。
 * 今より先の日付の月は等分の重みを 0 にする——時計の狂った写真が 1 枚ずつ月を持つと、帯の上端を食った
 */
const RECENT_MONTHS = 24;
const RECENT_BOOST = 3;
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
/** 日付窓の上下に見せる月の数（それぞれ） */
const DIAL_ROWS = 4;
/**
 * ホイールでこれだけ回したら、日付窓の月を 1 つ送る。1 回の受け取りで送るのは 1 か月まで——マウスの 1 目盛り
 * （120）で 3 か月進み、細かく選べなかった（利用者「1 ずつ変わった方が良い」2026-11-15）。トラックパッドの
 * 細かい量はここまで溜めてから 1 か月
 */
const WHEEL_STEP_PX = 50;

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
  onJumpToRow,
  monthCounts,
}: {
  scrollRef: RefObject<HTMLDivElement | null>;
  /** 各行の上端の累積（末尾は総高さ） */
  rowTops: readonly number[];
  /** 各行の日（`rowTops` と同じ並び、末尾の総高さの分は無い） */
  dayKeys: readonly number[];
  onJumpToRow: (rowIndex: number) => void;
  /** 月（yyyymm）ごとの枚数。月の一覧で、押す先の月に添える */
  monthCounts: ReadonlyMap<number, number>;
}) {
  const railRef = useRef<HTMLDivElement>(null);
  const total = rowTops[rowTops.length - 1] ?? 0;
  const [viewport, setViewport] = useState(0);
  // 指の所: 帯の上の位置（px）と、拡大鏡の中央の月（yyyymm）。指が帯の外で、掴んでもいないときは出さない
  const [bubble, setBubble] = useState<{ y: number; month: number | null } | null>(null);
  // 止めた拡大鏡（押せる月の一覧）。帯の上の位置（px）と、その年・指の月
  // y: 日付窓の高さ（帯の上の px）、sel: 窓の中の月（`starts` の番号）、here: 帯で指していた月
  const [menu, setMenu] = useState<{ y: number; sel: number; here: number } | null>(null);
  // 日付窓を回し始めたときの指の高さと窓の月。指を 1 行分動かすと、月の列が 1 つ送られる（ダイヤルのように）
  const dialAnchor = useRef<{ py: number; sel: number } | null>(null);
  const wheelAcc = useRef(0);
  const menuRef = useRef<HTMLDivElement>(null);
  // 現在地（一覧の上端の位置）
  const [scrollTop, setScrollTop] = useState(0);
  // 押している間の記録。動かしたかどうかで、離したときの意味（飛ぶ／送り終わり）が変わる
  const drag = useRef<{ startY: number; moved: boolean } | null>(null);
  // 掴んで送り終えた直後。その後の pointerleave で月の一覧を開かない（帯に指が戻るまで）
  const afterDrag = useRef(false);
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
  }, [scrollRef]);

  const starts = useMemo(() => monthStarts(rowTops, dayKeys), [rowTops, dayKeys]);
  // 帯と中身の対応（`makeRailMap`）。写真の少ない月・今年にも幅を回す
  const map = useMemo(() => {
    const now = new Date();
    const nowMonth = now.getFullYear() * 100 + now.getMonth() + 1;
    return makeRailMap(starts, total, RAIL_ALPHA, recencyFactor(nowMonth, RECENT_MONTHS, RECENT_BOOST));
  }, [starts, total]);

  // 年の目盛り: 年の変わる行ごとに1つ。ラベルは 4% 以上離れているときだけ、重なったら写真の多い年を残す
  const markers = useMemo((): YearMarker[] => {
    const out: YearMarker[] = [];
    let lastYear = -1;
    for (let i = 0; i < dayKeys.length; i++) {
      const y = Math.floor(dayKeys[i] / 10000);
      if (y === lastYear) continue;
      lastYear = y;
      out.push({ year: y, rowIndex: i, top: map.toRail(rowTops[i]) * 100, label: false });
    }
    const spans = out.map((m, k) => (k + 1 < out.length ? out[k + 1].top : 100) - m.top);
    const labels = chooseLabels(
      out.map((m) => m.top),
      spans,
      4,
    );
    return out.map((m, k) => ({ ...m, label: labels[k] }));
  }, [dayKeys, rowTops, map]);

  // 月の仕切り。帯の高さ（≒一覧の見えている高さ）で間引くので、窓の大きさが変わったら置き直す。
  // 位置は帯の対応（`map`）で引く——中身の位置を対応で写した上端を渡す
  const dots = useMemo(() => {
    const mapped = rowTops.map((t) => map.toRail(t) * total);
    return monthDots(mapped, dayKeys, viewport, markers.map((m) => m.top / 100), MONTH_GAP_PX);
  }, [rowTops, dayKeys, viewport, markers, map, total]);

  const monthAt = useCallback(
    (fraction: number): number | null => {
      const key = dayKeyAt(map.toContent(fraction), rowTops, dayKeys);
      return key === null ? null : Math.floor(key / 100);
    },
    [map, rowTops, dayKeys],
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
    el.scrollTop = Math.min(Math.max(0, total - el.clientHeight), map.toContent(fraction));
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
    // Esc は手前の幕が取る: 一覧を閉じるだけで、一覧の下のグリッド（選択の解除）へは渡さない。
    // 捕獲の段で受けて止める（同じ window の泡の段の受け口より先に走る）
    // ↑↓ で窓の月を送り、Enter で飛ぶ（利用者「上下カーソルで月を選ぶ」2026-11-15）
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(null);
      else if (e.key === "ArrowUp") turnDial(-1);
      else if (e.key === "ArrowDown") turnDial(1);
      else if (e.key === "Enter") goToSel();
      else return;
      e.stopPropagation();
      e.preventDefault();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [menu]);

  // 拡大の寄せる側（帯の側）。左右は言語の向きで決まる。`<html dir>` を読む（main.tsx が言語から付ける）。
  // getComputedStyle は描くたびに書式を確定させ、スクロールの 1 コマごとに描き直すこの部品では重かった（/code-review）
  const rtl = document.documentElement.dir === "rtl";

  /**
   * 日付窓（腕時計の日付の小窓。利用者 2026-11-15）: 窓は止めた高さに固定で、月の列がその下を流れる。窓の中の月が
   * 押す先。指の上下・ホイール・↑↓ で列を回し、押すか Enter でその月の頭へ。列は年をまたいで続く
   */
  const turnDial = (step: number) =>
    setMenu((m) => (m ? { ...m, sel: Math.min(starts.length - 1, Math.max(0, m.sel + step)) } : m));
  // いまの窓の月（キーの受け口は window に付けたままなので、描いた時点の値を ref で渡す）
  const menuNow = useRef(menu);
  menuNow.current = menu;
  const goToSel = () => {
    const m = menuNow.current;
    const el = scrollRef.current;
    if (m && el && starts[m.sel]) {
      el.scrollTop = Math.min(starts[m.sel].offset, Math.max(0, total - el.clientHeight));
    }
    setMenu(null);
    dialAnchor.current = null;
  };
  const openDial = (y: number, month: number | null) => {
    const sel = month !== null ? starts.findIndex((x) => x.month === month) : -1;
    if (sel < 0) return;
    dialAnchor.current = null;
    wheelAcc.current = 0;
    setMenu({ y, sel, here: month as number });
  };

  // 日付窓の高さ。帯の上下の端からはみ出さないように留める（いちばん上の月で開くと、帯の上へ 13px 出た）
  const dialY = menu ? Math.min(Math.max(menu.y, MENU_STEP / 2), Math.max(MENU_STEP / 2, viewport - MENU_STEP / 2)) : 0;

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
  const hereFraction = dragFraction ?? map.toRail(scrollTop);
  const hereKey = dragFraction === null ? dayKeyAt(scrollTop + 1, rowTops, dayKeys) : null;
  const hereMonth = dragFraction !== null ? monthAt(dragFraction) : hereKey === null ? null : Math.floor(hereKey / 100);

  // 中身が1画面に収まるなら、帯は要らない（動かす先が無い）
  if (!(total > viewport + 1) && viewport > 0) return null;

  return (
    <div
      ref={railRef}
      className={"scrub-rail" + (bubble ? " active" : "")}
      role="scrollbar"
      // キーボード（利用者「帯に対してのキーボードショートカット」2026-11-15。年へ飛ぶボタンが無くなったので、
      // キーだけで年・月を動けるようにする）: Tab で帯へ、↑↓ で月、PageUp/PageDown で年、Home/End で最新・最古、
      // Enter で日付窓を開く
      tabIndex={0}
      onKeyDown={(e) => {
        if (menu) return; // 日付窓が開いている間は、窓の受け口（↑↓・Enter・Esc）が取る
        const el = scrollRef.current;
        if (!el || starts.length === 0) return;
        const cur = Math.max(0, monthIndexAt(el.scrollTop + 1, starts));
        // 年の頭（`starts` の中で年の変わる番号。上が新しい年）
        const yearHeads = starts.flatMap((x, i) =>
          i === 0 || Math.floor(starts[i - 1].month / 100) !== Math.floor(x.month / 100) ? [i] : [],
        );
        const h = yearHeads.filter((i) => i <= cur).length - 1; // 今いる年の頭（yearHeads の番号）
        let target: number | null = null;
        if (e.key === "ArrowUp") target = starts[Math.max(0, cur - 1)].offset;
        else if (e.key === "ArrowDown") target = starts[Math.min(starts.length - 1, cur + 1)].offset;
        else if (e.key === "PageUp") {
          // 新しい側の年の頭へ。今の年の途中にいるなら、まず今の年の頭へ
          const head = starts[yearHeads[h]].offset;
          target = head < el.scrollTop - 1 ? head : starts[yearHeads[Math.max(0, h - 1)]].offset;
        } else if (e.key === "PageDown") {
          target = starts[yearHeads[Math.min(yearHeads.length - 1, h + 1)]].offset;
        } else if (e.key === "Home") target = 0;
        else if (e.key === "End") target = total;
        else if (e.key === "Enter") {
          openDial(hereFraction * viewport, hereMonth);
        } else return;
        e.preventDefault();
        e.stopPropagation();
        if (target !== null) el.scrollTop = Math.min(target, Math.max(0, total - el.clientHeight));
      }}
      aria-orientation="vertical"
      aria-label={t.scrubRailLabel}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(hereFraction * 100)}
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
        if (!drag.current) afterDrag.current = false;
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
          // 送り終わり: 間引いた分を残さず、指を離した所へ。写真の上で離したときに、続く pointerleave で
          // 月の一覧が開かないよう、指の所の表示もここで消す（/code-review）
          endDrag();
          afterDrag.current = true;
          setBubble(null);
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
        const towardPhotos = rect ? (rtl ? e.clientX > rect.right : e.clientX < rect.left) : false;
        if (towardPhotos && !afterDrag.current && bubble) {
          // 月が引けていなければ（起動直後、帯の元データが揃う前に指が乗った）、いまの元データで引き直す
          const month = bubble.month ?? (rect ? monthAt(bubble.y / rect.height) : null);
          openDial(bubble.y, month);
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
      {menu && starts[menu.sel] && (
        <div
          ref={menuRef}
          className="scrub-dial"
          style={{ top: `${dialY - (DIAL_ROWS + 0.5) * MENU_STEP}px`, height: (DIAL_ROWS * 2 + 1) * MENU_STEP }}
          // 窓の上の操作を、帯の「押す・掴む・拡大鏡」に渡さない
          onPointerDown={(e) => e.stopPropagation()}
          onPointerMove={(e) => {
            e.stopPropagation();
            const a = dialAnchor.current;
            if (!a) {
              dialAnchor.current = { py: e.clientY, sel: menu.sel };
              return;
            }
            const sel = Math.min(
              starts.length - 1,
              Math.max(0, a.sel + Math.round((e.clientY - a.py) / MENU_STEP)),
            );
            if (sel !== menu.sel) setMenu({ ...menu, sel });
          }}
          onPointerUp={(e) => e.stopPropagation()}
          onWheel={(e) => {
            wheelAcc.current += e.deltaY;
            const step = Math.max(-1, Math.min(1, Math.trunc(wheelAcc.current / WHEEL_STEP_PX)));
            if (step !== 0) {
              wheelAcc.current = 0;
              turnDial(step);
              // ホイールで回した後の指の動きは、今の窓の月から数え直す
              dialAnchor.current = { py: e.clientY, sel: Math.min(starts.length - 1, Math.max(0, menu.sel + step)) };
            }
          }}
          onClick={goToSel}
          onPointerLeave={(e) => {
            e.stopPropagation();
            // 帯へ戻ったら、また拡大鏡で選び直せるように閉じる
            const r = railRef.current?.getBoundingClientRect();
            if (r && e.clientX >= r.left && e.clientX <= r.right) setMenu(null);
          }}
          role="listbox"
          aria-label={t.scrubRailLabel}
          aria-activedescendant={`scrub-dial-${starts[menu.sel].month}`}
        >
          {/* 窓（固定）。中に来た月が押す先 */}
          <div className="scrub-dial-window" style={{ top: DIAL_ROWS * MENU_STEP, height: MENU_STEP }} aria-hidden="true" />
          <div
            className="scrub-dial-column"
            style={{ transform: `translateY(${(DIAL_ROWS - menu.sel) * MENU_STEP}px)` }}
          >
            {starts
              .slice(Math.max(0, menu.sel - DIAL_ROWS - 1), menu.sel + DIAL_ROWS + 2)
              .map((x) => {
                const k = starts.indexOf(x);
                const d = Math.abs(k - menu.sel);
                const prev = starts[k - 1];
                const yearTurns = !prev || Math.floor(prev.month / 100) !== Math.floor(x.month / 100);
                return (
                  <div
                    key={x.month}
                    id={`scrub-dial-${x.month}`}
                    role="option"
                    aria-selected={d === 0}
                    className={
                      // 帯で指していた月の輪は外した（利用者「年月の左の○が気になる」2026-11-15）
                      "scrub-dial-item" + (d === 0 ? " target" : "")
                    }
                    style={{
                      top: k * MENU_STEP,
                      height: MENU_STEP,
                      opacity: d === 0 ? 1 : Math.max(0.15, 0.7 - d * 0.14),
                      transform: d === 0 ? "scale(1.15)" : undefined,
                      transformOrigin: rtl ? "left center" : "right center",
                    }}
                  >
                    {d === 0 || yearTurns ? monthLabel(x.month) : monthOnly(x.month)}
                    {d === 0 && monthCounts.has(x.month) && (
                      <span className="scrub-menu-count">{t.photosCount(monthCounts.get(x.month) ?? 0)}</span>
                    )}
                  </div>
                );
              })}
          </div>
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
