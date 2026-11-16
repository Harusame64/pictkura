/**
 * 一覧で、写真の少ない日を1行に並べる（デザインレビュー: 1枚の日が縦に積まれ、右が大きく空いていた）。
 *
 * 並べるのは、その日の写真が基準の高さで1行に収まる日だけ。行の幅が尽きるまで横に並べ、**年では行を改める**
 * （右の帯で年を押すと、その年が行の頭から始まる）。月はまたいでよい——少ない日が続く時期は、たいてい月に
 * 1日ずつしかなく、同じ月に限ると、いちばん効いてほしい時期で1日も並ばなかった（mac の実機）。
 *
 * 並べるかどうかは見る人の好み（2026-11-16 利用者「画面に切り替えボタンをつけてユーザーの好み」）。
 * 保存先は `localStorage`（この端末だけの好み。`cellSize.ts` と同じ）。
 */
const KEY = "pictkura.packDays";

/** 既定は並べる */
export const PACK_DAYS_DEFAULT = true;

/** 保存した文字列を好みへ。"on"/"off" 以外は既定 */
export function parsePackDays(raw: string | null): boolean {
  if (raw === "on") return true;
  if (raw === "off") return false;
  return PACK_DAYS_DEFAULT;
}

export function loadPackDays(): boolean {
  try {
    return parsePackDays(localStorage.getItem(KEY));
  } catch {
    return PACK_DAYS_DEFAULT;
  }
}

export function savePackDays(on: boolean): void {
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {
    // 書けなくても、今の起動では効いている
  }
}

/** 並べる候補の日。`width` は、その日が1行に収まらない（または写真がまだ届いていない）なら `null` */
export type DayFit = { dayKey: number; width: number | null };

/**
 * 日の列（新しい順）を、行の単位へ分ける。各要素は日の番号の列で、長さ 2 以上なら並べた行、1 なら普通の形
 * （見出しの行＋写真の行）。`width` が `null` の日は必ず1日だけの単位になり、前後の並びを切る。
 */
export function planDayRows(days: readonly DayFit[], usable: number, dayGap: number): number[][] {
  const out: number[][] = [];
  let run: number[] = [];
  let runWidth = 0;
  let runYear = -1;
  const flush = () => {
    if (run.length > 0) out.push(run);
    run = [];
    runWidth = 0;
    runYear = -1;
  };
  days.forEach((day, i) => {
    if (day.width === null || day.width > usable) {
      flush();
      out.push([i]);
      return;
    }
    const year = Math.floor(day.dayKey / 10000);
    if (run.length > 0 && (year !== runYear || runWidth + dayGap + day.width > usable)) flush();
    runWidth += (run.length > 0 ? dayGap : 0) + day.width;
    run.push(i);
    runYear = year;
  });
  flush();
  return out;
}
