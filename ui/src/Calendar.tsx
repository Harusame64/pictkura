import { useMemo } from "react";
import { splitDayKey, thumbSrcOf, type DaySummary } from "./api";
import {
  firstWeekday,
  formatDayKey,
  formatMonth,
  t,
  weekdayLabels,
} from "./i18n";

/** 月ごとのカレンダーカード。日セルにその日の代表サムネイルと枚数を出す。 */
interface MonthData {
  year: number;
  month: number; // 1-12
  days: Map<number, DaySummary>;
  total: number;
}

/** 日付サマリ（新しい日付順）から月カードを組み立てる。全件データは不要 */
function buildMonths(summary: DaySummary[]): MonthData[] {
  const map = new Map<number, MonthData>();
  for (const day of summary) {
    const [year, month, dayOfMonth] = splitDayKey(day.day_key);
    const key = year * 100 + month;
    let m = map.get(key);
    if (!m) {
      m = { year, month, days: new Map(), total: 0 };
      map.set(key, m);
    }
    m.total += day.count;
    m.days.set(dayOfMonth, day);
  }
  // 新しい月から順に
  return [...map.values()].sort(
    (a, b) => b.year * 12 + b.month - (a.year * 12 + a.month),
  );
}

export default function Calendar({
  summary,
  onOpenDay,
}: {
  summary: DaySummary[];
  /** 日セルのクリック: グリッド表示のその日へジャンプする */
  onOpenDay: (dayKey: number) => void;
}) {
  const months = useMemo(() => buildMonths(summary), [summary]);

  if (months.length === 0) {
    return <div className="calendar-empty">{t.calendarEmpty}</div>;
  }

  return (
    <div className="calendar">
      {months.map((m) => {
        // 1日が、週の何番目の枠に入るか。**週の始まりは言語で変わる**ので、
        // 曜日そのもの（0=日曜）から `firstWeekday` を引いて枠の位置へ直す
        const lead =
          (new Date(m.year, m.month - 1, 1).getDay() - firstWeekday + 7) % 7;
        const daysInMonth = new Date(m.year, m.month, 0).getDate();
        const cells: (number | null)[] = [
          ...Array.from({ length: lead }, () => null),
          ...Array.from({ length: daysInMonth }, (_, i) => i + 1),
        ];
        return (
          <section key={`${m.year}-${m.month}`} className="month-card">
            <h3 className="month-title">
              {formatMonth(m.year, m.month)}
              <span className="month-count">{t.photosCount(m.total)}</span>
            </h3>
            <div className="month-grid">
              {weekdayLabels.map((w, i) => {
                // **色は枠の位置ではなく曜日に付ける**。月曜始まりだと
                // 先頭が日曜ではなくなるので、`i === 0` で塗ると平日が赤くなる
                const weekday = (firstWeekday + i) % 7;
                return (
                  <div
                    key={i}
                    className={
                      "weekday" +
                      (weekday === 0 ? " sun" : weekday === 6 ? " sat" : "")
                    }
                  >
                    {w}
                  </div>
                );
              })}
              {cells.map((day, i) =>
                day === null ? (
                  <div key={`empty-${i}`} className="day-cell empty" />
                ) : (
                  (() => {
                    const data = m.days.get(day);
                    return (
                      <div
                        key={day}
                        className={"day-cell" + (data ? " has-photos" : "")}
                        onClick={() => data && onOpenDay(data.day_key)}
                        title={
                          data
                            ? `${formatDayKey(data.day_key)} ${t.photosCount(data.count)}`
                            : ""
                        }
                      >
                        {data && (
                          <img
                            className="day-thumb"
                            loading="lazy"
                            decoding="async"
                            src={thumbSrcOf(
                              data.cover_id,
                              data.cover_mtime_ms,
                              data.cover_thumb_state,
                            )}
                            alt=""
                          />
                        )}
                        <span className="day-number">{day}</span>
                        {data && data.count > 1 && (
                          <span className="day-count">{data.count}</span>
                        )}
                      </div>
                    );
                  })()
                ),
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}
