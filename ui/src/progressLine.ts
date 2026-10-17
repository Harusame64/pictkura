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

/** React の `setStatus` と同じ形（値でも、前の値から作る関数でも受ける） */
export type SetStatus = (update: string | ((prev: string) => string)) => void;

/** 知らせの受け口から呼ぶ。待っていない間（返事の後に遅れて届いた知らせ等）は書かずに `false` */
export function writeProgress(
  line: { current: ProgressLine },
  setStatus: SetStatus,
  text: string,
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
  setStatus: SetStatus,
  op: () => Promise<T>,
): Promise<T> {
  // 呼び出しごとの持ち分。後から同じ行で始まった呼び出しの持ち分は、こちらの後片付けで畳まない（ゲート2）
  const own: ProgressLine = { active: true, text: null };
  line.current = own;
  try {
    return await op();
  } catch (e) {
    // 前の文へ戻さず空にする——前の文が隣の操作の古い進み具合なら、それこそ #68 の止まった行になる
    const mine = own.text;
    if (mine !== null) setStatus((prev) => (prev === mine ? "" : prev));
    throw e;
  } finally {
    own.active = false;
    if (line.current === own) line.current = idleLine();
  }
}
