/**
 * 一覧の右端（行の終わりの側）の帯の計算。帯は普通のスクロールバーの代わりで、年の目盛りを載せ、掴んで動かせる
 * （2026-11-15 利用者の選択。スクロールバーの上に年のラベルが重なり、掴もうとして別の年へ飛んでいた）。
 *
 * **帯の位置と中身の位置は、同じ比で対応させる**——帯の上から f の所は、中身の上から f×総高さ。
 * 年の目盛りも同じ比で置いているので（`yearMarkers`）、目盛りを掴んで離すとその年の頭へ着く。
 */

/** 帯の上の点（帯の上端からの距離）を、中身の位置（0〜1 の比）へ。帯の外へ出た指は端に留める */
export function fractionAt(y: number, railHeight: number): number {
  if (!(railHeight > 0)) return 0;
  return Math.min(1, Math.max(0, y / railHeight));
}

/**
 * 中身の位置（上端からの距離）にある日。`tops` は各行の上端の累積（末尾は総高さ）、`dayKeys` は各行の日。
 * 行の間を二分探索する。行が無ければ `null`
 */
export function dayKeyAt(offset: number, tops: readonly number[], dayKeys: readonly number[]): number | null {
  const n = dayKeys.length;
  if (n === 0) return null;
  let lo = 0;
  let hi = n - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (tops[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return dayKeys[lo];
}

/**
 * 月の仕切り（帯の上の小さな点）。月が変わる行ごとに1つ、帯の上での位置（0〜1 の比）を返す。
 * 詰まりすぎると帯が点で埋まるので、前に置いた点・年の目盛りから `minGapPx` 未満なら置かない。
 * 年の変わり目の月は年の目盛りがあるので置かない
 */
export function monthDots(
  tops: readonly number[],
  dayKeys: readonly number[],
  railPx: number,
  yearFractions: readonly number[],
  minGapPx: number,
): number[] {
  const total = tops[dayKeys.length] ?? 0;
  if (!(total > 0) || !(railPx > 0)) return [];
  const out: number[] = [];
  const years = [...yearFractions].sort((a, b) => a - b);
  let lastPx = -Infinity;
  let lastMonth = -1;
  for (let i = 0; i < dayKeys.length; i++) {
    const month = Math.floor(dayKeys[i] / 100);
    if (month === lastMonth) continue;
    const newYear = lastMonth !== -1 && Math.floor(month / 100) !== Math.floor(lastMonth / 100);
    const first = lastMonth === -1;
    lastMonth = month;
    if (first || newYear) continue;
    const f = tops[i] / total;
    const px = f * railPx;
    if (px - lastPx < minGapPx) continue;
    if (years.some((y) => Math.abs(y * railPx - px) < minGapPx)) continue;
    out.push(f);
    lastPx = px;
  }
  return out;
}

/**
 * どの年にラベルを出すか。ラベルどうしが `minGap`（帯の比）より近いときは、**写真の多い（帯の上で長い）年を残す**。
 * 上から先着順で決めると、外れ値の年（日付の読めない数枚が 2106 年に並ぶ等）が一番上に来たとき、今年のラベルを
 * 奪った（win2 の実測 2026-11-15: 「2106」の直下が「2025」で、2026 が出なかった）
 */
export function chooseLabels(tops: readonly number[], spans: readonly number[], minGap: number): boolean[] {
  const order = tops.map((_, i) => i).sort((a, b) => spans[b] - spans[a] || a - b);
  const chosen: number[] = [];
  const out = tops.map(() => false);
  for (const i of order) {
    if (chosen.some((j) => Math.abs(tops[j] - tops[i]) < minGap)) continue;
    chosen.push(i);
    out[i] = true;
  }
  return out;
}

/** 月の頭の並び（`yyyymm` と中身の上端からの位置）。上から順（新しい月から）。拡大鏡が指の前後の月を引く */
export function monthStarts(
  tops: readonly number[],
  dayKeys: readonly number[],
): { month: number; offset: number }[] {
  const out: { month: number; offset: number }[] = [];
  let last = -1;
  for (let i = 0; i < dayKeys.length; i++) {
    const month = Math.floor(dayKeys[i] / 100);
    if (month === last) continue;
    last = month;
    out.push({ month, offset: tops[i] });
  }
  return out;
}

/** 位置 `offset` を含む月の、`starts` の中の番号（その位置より上で、いちばん近い月の頭）。無ければ -1 */
export function monthIndexAt(offset: number, starts: readonly { offset: number }[]): number {
  let lo = 0;
  let hi = starts.length - 1;
  if (hi < 0) return -1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid].offset <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/**
 * 行の高さが変わった（見積もりの行が実際の写真に置き換わった等）あとも、画面の上端に同じ日が来るスクロール位置。
 * 上端にあった日の頭からの距離を保つ（その日の高さを超えるなら、その日の終わりに留める）。
 * その日が新しい並びに無ければ `null`（動かさない）。
 *
 * 置き換わるたびに上の行の高さが変わり、上端の写真がずれていった——帯で 2022 年へ飛ぶと 1,000px 手前に着き、
 * 3 秒かけて落ち着いた（win2 の実測 2026-11-15）。帯の横線と現在地の印も、そのずれの分だけ離れた
 */
export function anchoredScrollTop(
  scrollTop: number,
  prevTops: readonly number[],
  prevDays: readonly number[],
  nextTops: readonly number[],
  nextDays: readonly number[],
): number | null {
  if (prevDays.length === 0 || nextDays.length === 0) return null;
  const i = indexAt(scrollTop, prevTops, prevDays.length);
  const day = prevDays[i];
  let p = i;
  while (p > 0 && prevDays[p - 1] === day) p--;
  const delta = scrollTop - prevTops[p];
  const j = nextDays.indexOf(day);
  if (j < 0) return null;
  let k = j;
  while (k + 1 < nextDays.length && nextDays[k + 1] === day) k++;
  const dayHeight = nextTops[k + 1] - nextTops[j];
  return nextTops[j] + Math.min(delta, Math.max(0, dayHeight - 1));
}

function indexAt(offset: number, tops: readonly number[], n: number): number {
  let lo = 0;
  let hi = n - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (tops[mid] <= offset) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/**
 * 帯の上の位置と、中身の位置の対応。**写真の量だけに比例させない**——月ごとに、量の比（`alpha`）と
 * 月の数で等分した比（`1 - alpha`）を混ぜた幅を割り当てる。
 *
 * 量だけに比例させると、写真の少ない月や、始まったばかりの今年が帯の上でほとんど幅を持たず、窓を縦に狭くすると
 * 2026 と 2025 の間に月の点が1つも置けなかった（利用者 2026-11-15）。月の頭どうしの間は直線でつなぐので、
 * 行き来しても同じ位置へ戻る
 */
export type RailMap = {
  /** 中身の位置（上端からの距離）→ 帯の上の比（0〜1） */
  toRail: (offset: number) => number;
  /** 帯の上の比 → 中身の位置 */
  toContent: (fraction: number) => number;
};

export function makeRailMap(
  starts: readonly { offset: number; month?: number }[],
  total: number,
  alpha: number,
  /**
   * 月ごとの等分の重みに掛ける倍率（`yyyymm` から）。最近の月を広く（いちばん使うのは最近）、
   * 今より先の月（時計の狂った写真・日付の読めない写真）は 0 にして量の分だけにする。省けば全部 1
   */
  factor?: (month: number) => number,
): RailMap {
  const k = starts.length;
  if (k === 0 || !(total > 0)) {
    return {
      toRail: (o) => (total > 0 ? Math.min(1, Math.max(0, o / total)) : 0),
      toContent: (f) => Math.min(1, Math.max(0, f)) * total,
    };
  }
  const len = starts.map((s, i) => (i + 1 < k ? starts[i + 1].offset : total) - s.offset);
  const f = starts.map((s) => (factor && s.month !== undefined ? factor(s.month) : 1));
  const fSum = f.reduce((a, b) => a + b, 0) || 1;
  const weight = len.map((l, i) => alpha * (l / total) + (1 - alpha) * (f[i] / fSum));
  const sum = weight.reduce((a, b) => a + b, 0);
  const railStart: number[] = [];
  let acc = 0;
  for (let i = 0; i < k; i++) {
    railStart.push(acc / sum);
    acc += weight[i];
  }
  const w = weight.map((x) => x / sum);
  const find = (arr: readonly number[], v: number) => {
    let lo = 0;
    let hi = arr.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (arr[mid] <= v) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  const offs = starts.map((s) => s.offset);
  return {
    toRail: (o) => {
      if (o <= offs[0]) return railStart[0] * (o / Math.max(1, offs[0]));
      const i = find(offs, o);
      const within = len[i] > 0 ? Math.min(1, (o - offs[i]) / len[i]) : 0;
      return Math.min(1, railStart[i] + w[i] * within);
    },
    toContent: (f) => {
      const c = Math.min(1, Math.max(0, f));
      const i = find(railStart, c);
      const within = w[i] > 0 ? Math.min(1, (c - railStart[i]) / w[i]) : 0;
      return offs[i] + within * len[i];
    },
  };
}

/**
 * 等分の重みの倍率（`makeRailMap` の `factor`）。今月から数えて `recent` か月以内は `boost` 倍、
 * 今月より先（未来の日付）は 0、それ以外は 1
 */
export function recencyFactor(nowMonth: number, recent: number, boost: number): (month: number) => number {
  const index = (m: number) => Math.floor(m / 100) * 12 + (m % 100) - 1;
  const now = index(nowMonth);
  return (month) => {
    const age = now - index(month);
    if (age < 0) return 0;
    return age < recent ? boost : 1;
  };
}
