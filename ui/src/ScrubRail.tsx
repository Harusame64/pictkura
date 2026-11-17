import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from "react";

import { setRailActive } from "./scrollSpike"; // SPIKE
import { formatMonth, formatMonthOnly, t } from "./i18n";
import { year } from "./i18n/plural.ts";
import {
  dayKeyAt,
  fractionAt,
  landingTop,
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
/** 日付窓の1行の高さ（px） */
const MENU_STEP = 26;
/** 日付窓から横にこの距離（px）までは、指が外れても閉じない（帯との間を渡るため） */
const MENU_SLACK_PX = 24;
/** 日付窓の上下に見せる月の数（それぞれ） */
const DIAL_ROWS = 4;
/** 帯から写真の側へ出たとき、この距離（px）以内で止まったら日付窓を開く。それより遠くへ払ったら開かない */
const DIAL_OPEN_REACH_PX = 60;
/** 月のカードの高さ（px、およそ）。帯からはみ出さないように置くのに使う */
const CARD_HEIGHT = 236;
/** マウスの指が帯とカードの両方から離れて、これだけ経ったらカードを閉じる（ms）。渡る途中・一瞬のはみ出しでは閉じない */
const CARD_LEAVE_MS = 350;

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
  jumpTops,
}: {
  scrollRef: RefObject<HTMLDivElement | null>;
  /** 各行の上端の累積（末尾は総高さ） */
  rowTops: readonly number[];
  /** 各行の日（`rowTops` と同じ並び、末尾の総高さの分は無い） */
  dayKeys: readonly number[];
  onJumpToRow: (rowIndex: number) => void;
  /** 月（yyyymm）ごとの枚数。月の一覧で、押す先の月に添える */
  monthCounts: ReadonlyMap<number, number>;
  /**
   * 各行の、飛んだときに着く位置（`rowTops` と同じ並び）。少ない日を並べた行では、行の中の日の `rowTops` は行の途中を
   * 指すので、月へ飛ぶときはこちら（行の上端）に着く（`landingTop`）。省けば `rowTops` のまま
   */
  jumpTops?: readonly number[];
}) {
  const railRef = useRef<HTMLDivElement>(null);
  const total = rowTops[rowTops.length - 1] ?? 0;
  const [viewport, setViewport] = useState(0);
  // 指の所: 帯の上の位置（px）と、拡大鏡の中央の月（yyyymm）。指が帯の外で、掴んでもいないときは出さない
  const [bubble, setBubble] = useState<{ y: number; month: number | null } | null>(null);
  // 日付窓。y: 窓の高さ（帯の上の px）、month: 窓の中の月（yyyymm）。番号でなく月で持つ——開いている間に
  // 取り込みで月が増えると、番号では隣の月を指した（/code-review）
  const [menu, setMenu] = useState<{ y: number; month: number } | null>(null);
  // 日付窓を回し始めたときの指の高さと窓の月。指を 1 行分動かすと、月の列が 1 つ送られる（ダイヤルのように）
  // 起点は月（yyyymm）で持つ——番号で持つと、開いている間に取り込みで月が増えたとき、次の指の動きで別の月へ跳んだ（codex）
  const dialAnchor = useRef<{ py: number; month: number } | null>(null);
  const wheelAcc = useRef(0);
  const menuRef = useRef<HTMLDivElement>(null);
  // 月のカード（帯を押すと出る、その年の 12 か月のボタン）。y: 出した高さ（帯の上の px）、year: 見せている年
  const [card, setCard] = useState<{ y: number; year: number } | null>(null);
  // マウスの指が帯とカードの両方から離れたときの、閉じるまでの待ち（帯の pointerleave で始め、pointerenter で止める）
  const cardLeaveTimer = useRef<number | undefined>(undefined);
  const cancelCardLeave = () => {
    window.clearTimeout(cardLeaveTimer.current);
    cardLeaveTimer.current = undefined;
  };
  const cardRef = useRef<HTMLDivElement>(null);
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

  // 表示している年（一覧の上端の日の年）。年のラベルを太くし、詰まっていてもラベルを出す
  const currentKey = dayKeyAt(scrollTop + 1, rowTops, dayKeys);
  const currentYear = currentKey === null ? null : Math.floor(currentKey / 10000);

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
    const force = out.findIndex((m) => m.year === currentYear);
    const labels = chooseLabels(
      out.map((m) => m.top),
      spans,
      4,
      force >= 0 ? force : undefined,
    );
    return out.map((m, k) => ({ ...m, label: labels[k] }));
  }, [dayKeys, rowTops, map, currentYear]);

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
    setRailActive(false); // SPIKE（C）
  };
  useEffect(
    () => () => {
      window.clearTimeout(moveTimer.current);
      window.clearTimeout(cardLeaveTimer.current);
    },
    [],
  );

  // 日付窓が開いている間の指とキー。指の上下で列を回す（窓の外へ縦に出ても回し続ける——窓は 9 行分しかなく、
  // 中だけで数えると前後 4 か月にしか届かなかった。/code-review）。横に離れたら閉じる
  useEffect(() => {
    if (!menu) return;
    const onMove = (e: PointerEvent) => {
      const dial = menuRef.current?.getBoundingClientRect();
      const rail = railRef.current?.getBoundingClientRect();
      if (!dial || !rail) return;
      const onRail = e.clientX >= rail.left && e.clientX <= rail.right;
      const inBand =
        e.clientX >= dial.left - MENU_SLACK_PX &&
        e.clientX <= dial.right + MENU_SLACK_PX &&
        e.clientY >= rail.top &&
        e.clientY <= rail.bottom;
      if (onRail) return; // 帯へ戻る途中（帯の pointerleave／窓の pointerleave が閉じる）
      if (!inBand) {
        setMenu(null);
        return;
      }
      const a = dialAnchor.current;
      const now = starts.findIndex((x) => x.month === menu.month);
      const from = a ? starts.findIndex((x) => x.month === a.month) : -1;
      if (!a || from < 0) {
        dialAnchor.current = { py: e.clientY, month: menu.month };
        return;
      }
      const sel = Math.min(starts.length - 1, Math.max(0, from + Math.round((e.clientY - a.py) / MENU_STEP)));
      if (sel !== now && starts[sel]) setMenu({ ...menu, month: starts[sel].month });
    };
    // Esc は手前の幕が取る: 窓を閉じるだけで、下のグリッド（選択の解除）へは渡さない。捕獲の段で受けて止める。
    // ↑↓ で窓の月を送り、Enter で飛ぶ（利用者「上下カーソルで月を選ぶ」2026-11-15）。入力欄の中・変換中・
    // 修飾キー付きは触らない（検索欄の Enter やカーソル移動を奪わない。/code-review）
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const typing =
        !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
      if (typing || e.isComposing || e.altKey || e.ctrlKey || e.metaKey) return;
      if (e.key === "Escape") setMenu(null);
      else if (e.key === "ArrowUp") turnDial(-1);
      else if (e.key === "ArrowDown") turnDial(1);
      else if (e.key === "Enter") goToSel();
      else return;
      e.stopPropagation();
      e.preventDefault();
    };
    // 捕獲の段で受ける（窓の上の pointermove は、帯へ渡さないよう窓が止めるので、泡の段では届かない）
    window.addEventListener("pointermove", onMove, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [menu, starts]);

  // 月のカードは、カードと帯の外を押す・Esc で閉じる。マウスの指が帯とカードの両方から離れても閉じる（帯の
  // pointerleave。下）——帯を押して飛び、そのまま写真を見たい人（従来の使い方）の上にカードが残り、しかも消すための
  // 1回の押下が飲まれていた（利用者 2026-11-15）
  useEffect(() => {
    if (!card) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (t && (cardRef.current?.contains(t) || railRef.current?.contains(t))) return;
      // マウスの押下は、閉じるだけで通す（離れた時点で閉じるのが本筋。猶予の間に押した分を飲まない）。決めるのは
      // 押した道具（開いた道具ではない）——タッチで開いてトラックパッドで押す機械でも、押下を飲まない
      if (e.pointerType === "mouse") {
        setCard(null);
        return;
      }
      e.stopPropagation();
      e.preventDefault();
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      // 続く click も、この押下の分だけ飲む
      window.addEventListener("click", swallow, { capture: true, once: true });
      window.setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 600);
      setCard(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      e.preventDefault();
      setCard(null);
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [card]);
  /** 写真のある年（新しい順）と、月（yyyymm）→ 頭の位置 */
  const years = useMemo(() => [...new Set(starts.map((x) => Math.floor(x.month / 100)))], [starts]);
  const monthOffset = useMemo(() => new Map(starts.map((x) => [x.month, x.offset])), [starts]);

  // 拡大の寄せる側（帯の側）。左右は言語の向きで決まる。`<html dir>` を読む（main.tsx が言語から付ける）。
  // getComputedStyle は描くたびに書式を確定させ、スクロールの 1 コマごとに描き直すこの部品では重かった（/code-review）
  const rtl = document.documentElement.dir === "rtl";

  /**
   * 日付窓（腕時計の日付の小窓。利用者 2026-11-15）: 窓は止めた高さに固定で、月の列がその下を流れる。窓の中の月が
   * 押す先。指の上下・ホイール・↑↓ で列を回し、押すか Enter でその月の頭へ。列は年をまたいで続く
   */
  const turnDial = (step: number) => {
    setMenu((m) => {
      if (!m) return m;
      const i = starts.findIndex((x) => x.month === m.month);
      const n = Math.min(starts.length - 1, Math.max(0, i + step));
      return starts[n] ? { ...m, month: starts[n].month } : m;
    });
    // キーやホイールで回した後の指の動きは、今の窓の月から数え直す（古い起点から数えると、指を 1px 動かした
    // だけでキーで回した分が戻った。/code-review）
    dialAnchor.current = null;
  };
  // いまの窓の月（キーの受け口は window に付けたままなので、描いた時点の値を ref で渡す）
  const menuNow = useRef(menu);
  menuNow.current = menu;
  const goToSel = () => {
    const m = menuNow.current;
    const el = scrollRef.current;
    const x = m ? starts.find((s) => s.month === m.month) : undefined;
    if (x && el) {
      el.scrollTop = Math.min(landingTop(x.offset, rowTops, jumpTops), Math.max(0, total - el.clientHeight));
    }
    setMenu(null);
    dialAnchor.current = null;
  };
  const openDial = (y: number, month: number | null) => {
    if (month === null || !starts.some((x) => x.month === month)) return;
    dialAnchor.current = null;
    wheelAcc.current = 0;
    setMenu({ y, month });
  };

  // 日付窓の高さ。帯の上下の端からはみ出さないように留める（いちばん上の月で開くと、帯の上へ 13px 出た）
  const dialY = menu ? Math.min(Math.max(menu.y, MENU_STEP / 2), Math.max(MENU_STEP / 2, viewport - MENU_STEP / 2)) : 0;
  const sel = menu ? starts.findIndex((x) => x.month === menu.month) : -1;
  // 日付窓の箱も帯の中に留める（上の道具バーに被って押しを奪わないよう。/code-review）。窓は箱の中の dialY
  const dialBoxTop = Math.max(0, dialY - (DIAL_ROWS + 0.5) * MENU_STEP);
  const dialBoxBottom = Math.min(Math.max(viewport, dialY + MENU_STEP / 2), dialY + (DIAL_ROWS + 0.5) * MENU_STEP);
  const windowTop = dialY - MENU_STEP / 2 - dialBoxTop;

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
      className={
        "scrub-rail" + (bubble ? " active" : "") + (menu ? " dial-open" : "") + (dragFraction !== null ? " dragging" : "")
      }
      role="scrollbar"
      // キーボード（利用者「帯に対してのキーボードショートカット」2026-11-15。年へ飛ぶボタンが無くなったので、
      // キーだけで年・月を動けるようにする）: Tab で帯へ、↑↓ で月、PageUp/PageDown で年、Home/End で最新・最古、
      // Enter で日付窓を開く
      tabIndex={0}
      onKeyDown={(e) => {
        if (menu) return; // 日付窓が開いている間は、窓の受け口（↑↓・Enter・Esc）が取る
        // 帯そのものにいるときだけ。カードのボタンで押した Enter が泡でここへ来ると、ボタンの代わりに日付窓が開いた（codex）
        if (e.target !== e.currentTarget) return;
        const el = scrollRef.current;
        if (!el || starts.length === 0) return;
        const cur = Math.max(0, monthIndexAt(el.scrollTop + 1, starts));
        // 年の頭（`starts` の中で年の変わる番号。上が新しい年）
        const yearHeads = starts.flatMap((x, i) =>
          i === 0 || Math.floor(starts[i - 1].month / 100) !== Math.floor(x.month / 100) ? [i] : [],
        );
        const h = yearHeads.filter((i) => i <= cur).length - 1; // 今いる年の頭（yearHeads の番号）
        let target: number | null = null;
        // ↑ は、月の途中にいるならまず今の月の頭へ（PageUp と同じ）。↓・PageDown は、最後の月・年ではいちばん下へ
        // （頭へ戻すと「下へ」のキーで上へ動いた。/code-review）
        // 着地は行の上端（`landingTop`）。並べた行に同居する月は同じ所に着くので、実際に動く月まで送る——そのまま
        // 次の月へ送ると、同じ行の2つ目の月で位置が変わらず、↓が止まって見えた
        const land = (k: number) => landingTop(starts[k].offset, rowTops, jumpTops);
        if (e.key === "ArrowUp") {
          target = 0;
          for (let k = cur; k >= 0; k--) {
            if (land(k) < el.scrollTop - 1) {
              target = land(k);
              break;
            }
          }
        } else if (e.key === "ArrowDown") {
          target = total;
          for (let k = cur + 1; k < starts.length; k++) {
            if (land(k) > el.scrollTop + 1) {
              target = land(k);
              break;
            }
          }
        }
        else if (e.key === "PageUp") {
          // 新しい側の年の頭へ。今の年の途中にいるなら、まず今の年の頭へ
          const head = starts[yearHeads[h]].offset;
          target = head < el.scrollTop - 1 ? head : starts[yearHeads[Math.max(0, h - 1)]].offset;
        } else if (e.key === "PageDown") {
          target = h + 1 < yearHeads.length ? starts[yearHeads[h + 1]].offset : total;
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
            setRailActive(true); // SPIKE（C）
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
        // 押したら、その年の月のカードも出す（指の精度が要らない月の選び方。日付窓はマウスが得意な人には快適だが、
        // 指を止める精度が要る——トラックパッド・タッチ・手の震えでも確実に選べるように。利用者 2026-11-15）
        const ym = near ? Math.floor(dayKeys[near.rowIndex] / 100) : monthAt(p.fraction);
        if (ym !== null) {
          setMenu(null);
          setCard({ y: p.y, year: Math.floor(ym / 100) });
        }
      }}
      onPointerCancel={() => {
        drag.current = null;
        endDrag();
        setBubble(null);
      }}
      onPointerEnter={cancelCardLeave}
      onPointerLeave={(e) => {
        if (drag.current) return;
        // カードは帯の DOM の子なので、ここに来るのは指が帯とカードの両方から出たとき（窓の外へ出たときも）。
        // マウスなら、少し待って閉じる（渡る途中・一瞬のはみ出しは pointerenter が止める）。キーボードで操作している
        // 的がカードの中にある間は閉じない（押している最中に的が消えないように）
        if (card && e.pointerType === "mouse") {
          cancelCardLeave();
          cardLeaveTimer.current = window.setTimeout(() => {
            cardLeaveTimer.current = undefined;
            // 守るのはキーボードで来た的だけ（:focus-visible）。マウスで ‹ › を押してもボタンに的が乗るので、
            // 「的が中にある」だけで判定すると、年を送ってから写真へ戻ったときに閉じなかった（win2 の実測）
            const f = document.activeElement;
            if (f && cardRef.current?.contains(f) && f.matches(":focus-visible")) return;
            setCard(null);
          }, CARD_LEAVE_MS);
        }
        // 写真の側へ出たら、拡大鏡をその場に止めて、押せる一覧にする（帯の外側＝窓の端へ出たときは止めない）
        const rect = railRef.current?.getBoundingClientRect();
        // 写真の側へ出た、ただし帯のすぐ横まで（指を写真へ素早く払っただけで、日付窓が開かないよう。win2 の実測）
        const gap = rect ? (rtl ? e.clientX - rect.right : rect.left - e.clientX) : -1;
        const towardPhotos = gap > 0 && gap <= DIAL_OPEN_REACH_PX;
        if (towardPhotos && !afterDrag.current && !card && bubble) {
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
            className={
              "scrub-tick" +
              (m.label ? " labeled" : "") +
              (m.year === currentYear ? " current" : "")
            }
            style={{ top: edgeTop(m.top) }}
          >
            {/* 年は地域の数字で（ar-SA ならアラビア・インド数字。`year()`） */}
            {m.label ? year(m.year) : ""}
          </span>
        ))}
      {/* 指の所の横線（利用者の希望、2026-11-15）。帯から外れたら消える（掴んでいる間は外でも付いてくる） */}
      {bubble && <div className="scrub-cursor" style={{ top: `${bubble.y}px` }} aria-hidden="true" />}
      {card && (() => {
        const yi = years.indexOf(card.year);
        const top = Math.max(0, Math.min(card.y - CARD_HEIGHT / 2, viewport - CARD_HEIGHT));
        const go = (y: number) => setCard({ ...card, year: y });
        return (
          <div
            ref={cardRef}
            className="scrub-card"
            style={{ top: `${top}px` }}
            // カードの上の操作を、帯の「押す・掴む・拡大鏡」に渡さない
            onPointerDown={(e) => e.stopPropagation()}
            onPointerMove={(e) => e.stopPropagation()}
            onPointerUp={(e) => e.stopPropagation()}
            // pointerleave は止めない——止めると、カードから写真へ出たときに帯の pointerleave（拡大鏡を消す）まで
            // 届かず、拡大鏡が出たまま残った
            onWheel={(e) => {
              // ホイールで前後の年（下へ回すと古い年。一覧と同じ向き）
              const n = yi + (e.deltaY > 0 ? 1 : -1);
              if (e.deltaY !== 0 && years[n] !== undefined) go(years[n]);
            }}
            role="dialog"
            aria-label={year(card.year)}
          >
            <div className="scrub-card-head">
              <button
                className="scrub-card-step"
                disabled={yi <= 0}
                onClick={() => go(years[yi - 1])}
                aria-label={yi > 0 ? year(years[yi - 1]) : undefined}
              >
                {/* 新しい年（一覧の上の側） */}‹
              </button>
              <span className="scrub-card-year">{year(card.year)}</span>
              <button
                className="scrub-card-step"
                disabled={yi < 0 || yi >= years.length - 1}
                onClick={() => go(years[yi + 1])}
                aria-label={yi >= 0 && yi < years.length - 1 ? year(years[yi + 1]) : undefined}
              >
                ›
              </button>
            </div>
            <div className="scrub-card-grid">
              {/* 12 月から 1 月へ（一覧と同じく新しい月が先） */}
              {Array.from({ length: 12 }, (_, i) => 12 - i).map((m) => {
                const ym = card.year * 100 + m;
                const offset = monthOffset.get(ym);
                const n = monthCounts.get(ym);
                return (
                  <button
                    key={m}
                    className={"scrub-card-month" + (ym === hereMonth ? " here" : "")}
                    aria-current={ym === hereMonth ? "date" : undefined}
                    disabled={offset === undefined}
                    onClick={() => {
                      const el = scrollRef.current;
                      if (el && offset !== undefined)
                        el.scrollTop = Math.min(landingTop(offset, rowTops, jumpTops), Math.max(0, total - el.clientHeight));
                      setCard(null);
                    }}
                  >
                    <span>{monthOnly(ym)}</span>
                    {n !== undefined && <span className="scrub-card-count">{t.photosCount(n)}</span>}
                  </button>
                );
              })}
            </div>
          </div>
        );
      })()}
      {menu && sel >= 0 && (
        <div
          ref={menuRef}
          className="scrub-dial"
          style={{ top: `${dialBoxTop}px`, height: dialBoxBottom - dialBoxTop }}
          // 窓の上の操作を、帯の「押す・掴む・拡大鏡」に渡さない（回すのは window の受け口）
          onPointerDown={(e) => e.stopPropagation()}
          onPointerMove={(e) => e.stopPropagation()}
          onPointerUp={(e) => e.stopPropagation()}
          onWheel={(e) => {
            wheelAcc.current += e.deltaY;
            const step = Math.max(-1, Math.min(1, Math.trunc(wheelAcc.current / WHEEL_STEP_PX)));
            if (step !== 0) {
              wheelAcc.current = 0;
              turnDial(step);
            }
          }}
          // 押すと窓の中の月へ（腕時計の日付と同じく、選んでいるのは窓の中の1つ）
          onClick={goToSel}
          onPointerLeave={(e) => {
            e.stopPropagation();
            // 帯へ戻ったら、また拡大鏡で選び直せるように閉じる
            const r = railRef.current?.getBoundingClientRect();
            if (r && e.clientX >= r.left && e.clientX <= r.right) setMenu(null);
          }}
          role="listbox"
          aria-label={t.scrubRailLabel}
          aria-activedescendant={`scrub-dial-${menu.month}`}
        >
          {/* 窓（固定）。中に来た月が押す先 */}
          <div className="scrub-dial-window" style={{ top: windowTop, height: MENU_STEP }} aria-hidden="true" />
          <div className="scrub-dial-column" style={{ transform: `translateY(${windowTop - sel * MENU_STEP}px)` }}>
            {starts.slice(Math.max(0, sel - DIAL_ROWS - 1), sel + DIAL_ROWS + 2).map((x, i) => {
              const k = Math.max(0, sel - DIAL_ROWS - 1) + i;
              const d = Math.abs(k - sel);
              const prev = starts[k - 1];
              const yearTurns = !prev || Math.floor(prev.month / 100) !== Math.floor(x.month / 100);
              return (
                <div
                  key={x.month}
                  id={`scrub-dial-${x.month}`}
                  role="option"
                  aria-selected={d === 0}
                  className={"scrub-dial-item" + (d === 0 ? " target" : "")}
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
