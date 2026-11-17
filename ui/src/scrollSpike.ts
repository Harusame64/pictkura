/**
 * SPIKE（A: 日の一覧をまとめて取る／C: 速く動かしている間はサムネイルを要求しない）。マージ前に作り直す。
 * 値は localStorage で変えられる（実機で最適値を探すため。変えたら再読み込み）:
 *   pk.spike.a          1 で日の一覧をまとめて取る（既定 0 = 今まで通り1日ずつ）
 *   pk.spike.c          1 で C を有効にする（既定 0）
 *   pk.spike.cThreshold この速さ（px/s）を超えたら「速い」（既定 3000）
 *   pk.spike.cSettleMs  速さが落ちて／スクロールが止まってから、何 ms で読み込みを戻すか（既定 120）
 *   pk.spike.cDays      1 なら「速い」間は日の一覧も取らない（既定 0）
 * 数は window.__spikeAC に積む（listCalls・listDays・fastOn・lastFastOffAt）
 */
function num(key: string, dflt: number): number {
  try {
    const v = localStorage.getItem(key);
    const n = v === null ? NaN : Number(v);
    return Number.isFinite(n) ? n : dflt;
  } catch {
    return dflt;
  }
}
export const SPIKE = {
  a: num("pk.spike.a", 0) === 1,
  c: num("pk.spike.c", 0) === 1,
  cThreshold: num("pk.spike.cThreshold", 3000),
  cSettleMs: num("pk.spike.cSettleMs", 120),
  cDays: num("pk.spike.cDays", 0) === 1,
  /** 1 でしきい値を実測から決める: 画面の高さ ÷ サムネイル1枚の読み込み時間 × cK */
  cAdaptive: num("pk.spike.cAdaptive", 0) === 1,
  cK: num("pk.spike.cK", 1),
  /** 帯を掴んでいる間の「止まった」とみなすまで（ms）。離したら待たずに戻す */
  cRailSettleMs: num("pk.spike.cRailSettleMs", 50),
};
/** サムネイル1枚の読み込み時間の移動平均（ms）。resource timing から */
let thumbLatencyMs = 80;
let railActive = false;
let railRelease: (() => void) | null = null;
try {
  new PerformanceObserver((list) => {
    for (const e of list.getEntries()) {
      if (!e.name.includes("thumb")) continue;
      const d = (e as PerformanceResourceTiming).duration;
      if (d > 0 && d < 5000) thumbLatencyMs = thumbLatencyMs * 0.9 + d * 0.1;
    }
  }).observe({ type: "resource", buffered: false });
} catch {
  /* 測れなければ既定のまま */
}
/** いまのしきい値（px/s） */
function threshold(el: HTMLElement): number {
  if (!SPIKE.cAdaptive) return SPIKE.cThreshold;
  const t = (el.clientHeight / (thumbLatencyMs / 1000)) * SPIKE.cK;
  return Math.min(40000, Math.max(2000, t));
}
/** 帯を掴んだ（true）・離した（false）。離したら待たずに読み込みを戻す */
export function setRailActive(on: boolean): void {
  railActive = on;
  if (!on && railRelease) railRelease();
}
export const spikeCounters = { listCalls: 0, listDays: 0, fastOn: 0, lastFastOffAt: 0, params: SPIKE, threshold: 0, latencyMs: 0 };
(window as unknown as { __spikeAC: typeof spikeCounters }).__spikeAC = spikeCounters;

/**
 * スクロールの速さを見て「速い」かを知らせる。速い→遅いは、速さが閾値の半分を下回るか、
 * スクロールが `cSettleMs` 止まったとき
 */
export function watchScrollSpeed(el: HTMLElement, onFast: (fast: boolean) => void): () => void {
  let lastTop = el.scrollTop;
  let lastT = performance.now();
  let v = 0;
  let fast = false;
  let timer: number | null = null;
  const set = (f: boolean) => {
    if (f === fast) return;
    fast = f;
    if (f) spikeCounters.fastOn++;
    else spikeCounters.lastFastOffAt = performance.now();
    onFast(f);
  };
  const onScroll = () => {
    const t = performance.now();
    const dt = Math.max(1, t - lastT);
    const inst = (Math.abs(el.scrollTop - lastTop) / dt) * 1000;
    v = v * 0.5 + inst * 0.5;
    lastTop = el.scrollTop;
    lastT = t;
    const th = threshold(el);
    spikeCounters.threshold = Math.round(th);
    spikeCounters.latencyMs = Math.round(thumbLatencyMs);
    if (v > th) set(true);
    // 閾値の半分より速い間は時計を巻き直す。遅くなったら巻き直さず、時計が切れたら戻す
    // （ゆっくり送り続けている間も読み込みが始まるように）
    if (v >= th / 2 || timer === null) {
      if (timer !== null) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        timer = null;
        v = 0;
        set(false);
      }, railActive ? SPIKE.cRailSettleMs : SPIKE.cSettleMs);
    }
  };
  railRelease = () => {
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
    v = 0;
    set(false);
  };
  el.addEventListener("scroll", onScroll, { passive: true });
  return () => {
    railRelease = null;
    el.removeEventListener("scroll", onScroll);
    if (timer !== null) window.clearTimeout(timer);
  };
}
