/**
 * 見つからないライブラリのフォルダの知らせ（dev #23）の**判定だけ**。
 *
 * 画面（`App.tsx`）から出したのは、**升を書くため**である——#146 の変異注入で、
 * 持ち越し・入れ子の合計を壊す変異が tsc 以外の何にも殺されなかった。
 * ここは React も `window` も持ち込まないので、`node --test` でそのまま読める
 * （`scripts/missingRoots.test.ts`）。
 */

export interface MissingRoot {
  root: string;
  /** DB だけで数えた配下の数（数えられなかったら 0） */
  count: number;
}

/**
 * 新しい答えと前の答えを合わせる。
 *
 * **答えが揃わなかったルート（確認中・見切った）だけ、前の答えを持ち越す**——
 * 「確認中」を「在る」と読んで、出ていた知らせを消さない。
 * **答えたルートは持ち越さない**——差し込んで「在る」と答えたフォルダを、隣の刺さった
 * フォルダのせいで「見つかりません」と言い続けない（#146 の2ゲート目）。
 */
export function mergeMissing(
  prev: readonly MissingRoot[],
  found: readonly MissingRoot[],
  unanswered: ReadonlySet<string>,
): MissingRoot[] {
  return [
    ...found,
    ...prev.filter(
      (m) => !found.some((f) => f.root === m.root) && unanswered.has(m.root),
    ),
  ];
}

/** 比べる前に綴りをそろえる（区切り・末尾。Windows は大小も） */
function comparable(path: string, windows: boolean): string {
  const s = path.replace(/\\/g, "/").replace(/\/+$/, "");
  return windows ? s.toLowerCase() : s;
}

/**
 * 合計の枚数。**入れ子のルートは外側に含まれている**（`count_by_prefix` は
 * 差し引かない）ので、別の見つからないルートの**配下**にあるものは足さない。
 * 配下かどうかは区切りまで見る——`/Volumes/SD 1` は `/Volumes/SD` の配下ではない。
 */
export function missingTotal(
  shown: readonly MissingRoot[],
  windows: boolean,
): number {
  return shown
    .filter(
      (m) =>
        !shown.some(
          (o) =>
            o.root !== m.root &&
            comparable(m.root, windows).startsWith(
              comparable(o.root, windows) + "/",
            ),
        ),
    )
    .reduce((sum, m) => sum + m.count, 0);
}

/** 「あとで」の鍵。**並べ替えてから作る**——答えの届く順で並びが変わっても、同じ顔ぶれなら同じ鍵 */
export function laterKey(shown: readonly MissingRoot[]): string {
  return shown
    .map((m) => m.root)
    .sort()
    .join("\u0000");
}
