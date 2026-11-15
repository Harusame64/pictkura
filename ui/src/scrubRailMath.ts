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

/** 比を、スクロール位置へ。底より先へは行けない（最後の画面の頭で止まる） */
export function scrollTopFor(fraction: number, total: number, viewport: number): number {
  const max = Math.max(0, total - viewport);
  return Math.min(max, Math.max(0, fraction * total));
}

/**
 * つまみ（いま見ている範囲）の、帯の上での位置と高さ（帯の高さに対する比）。
 * 小さすぎると掴めないので、高さは `minFraction` より小さくしない
 */
export function thumbOf(
  scrollTop: number,
  total: number,
  viewport: number,
  minFraction: number,
): { top: number; height: number } {
  if (!(total > 0)) return { top: 0, height: 1 };
  const height = Math.min(1, Math.max(minFraction, viewport / total));
  const top = Math.min(1 - height, Math.max(0, scrollTop / total));
  return { top, height };
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
