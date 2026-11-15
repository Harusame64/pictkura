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
