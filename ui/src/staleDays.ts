/**
 * 見えている日（とビューアの表示日）のうち、骨組みの枚数と中身の数がずれていて、まだ頼んでいない日を選ぶ（PR #352）。
 *
 * - 中身の無い日（見積もりの枠）は選ばない——そちらは「見えている枠を読む」道が読む
 * - 読み途中の日は選ばない——届いて描き直されたあと、もう一度ここで見る
 * - **同じ骨組みには1日1回まで**（`asked` に控える）。骨組みが古いまま中身が新しいと、ずれが解けず読み続けるので。
 *   骨組みが新しくなったら控えを空にする
 */
export type AskedFor<S> = { summary: S | null; days: Set<number> };

export function pickStaleDays<S>(
  summary: S,
  counts: ReadonlyMap<number, number>,
  dayItemCounts: ReadonlyMap<number, number>,
  visible: Iterable<number>,
  viewerDay: number | null,
  inflight: ReadonlySet<number>,
  asked: AskedFor<S>,
): number[] {
  if (asked.summary !== summary) {
    asked.summary = summary;
    asked.days = new Set();
  }
  const days = new Set(visible);
  if (viewerDay !== null) days.add(viewerDay);
  const out: number[] = [];
  for (const d of days) {
    const n = dayItemCounts.get(d);
    if (n === undefined || asked.days.has(d) || inflight.has(d)) continue;
    if (counts.get(d) !== n) {
      asked.days.add(d);
      out.push(d);
    }
  }
  return out;
}
