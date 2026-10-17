/**
 * 長い操作の進み具合をステータス行に出す、その「持ち分」（pictkura-dev #68）。
 *
 * ステータス行は1本しか無く、削除と書き出しが同時に走ることもある。進み具合は**操作の返事を待っている間だけ**
 * 書き、操作が転んだら**行がまだ自分の書いた文のときだけ**空にする——「移動中… (n / N)」を残さず、
 * 隣の操作の進み具合も消さない。
 */
export type ProgressLine = {
  /** 返事を待っている間だけ立つ。進みの知らせは、これが立っているときだけ行へ書く */
  active: boolean;
  /** この操作が最後に行へ書いた文 */
  text: string | null;
};

export const idleLine = (): ProgressLine => ({ active: false, text: null });

/** 知らせの受け口から呼ぶ。待っていない間（返事の後に遅れて届いた知らせ等）は書かずに `false` */
export function writeProgress(
  line: { current: ProgressLine },
  text: string,
  setStatus: (text: string) => void,
): boolean {
  if (!line.current.active) return false;
  line.current.text = text;
  setStatus(text);
  return true;
}

/**
 * `op` を待つ間だけ進み具合を書けるようにする。`op` が投げたら、行が自分の最後の文のままなら空にして投げ直す。
 * 成功のときは触らない——結果の文は呼び出し側が書く
 */
export async function withProgressLine<T>(
  line: { current: ProgressLine },
  setStatus: (update: (prev: string) => string) => void,
  op: () => Promise<T>,
): Promise<T> {
  line.current = { active: true, text: null };
  try {
    return await op();
  } catch (e) {
    const mine = line.current.text;
    if (mine !== null) setStatus((prev) => (prev === mine ? "" : prev));
    throw e;
  } finally {
    line.current = idleLine();
  }
}
