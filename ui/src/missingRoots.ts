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

/**
 * 左ペインのライブラリのフォルダに付ける印（dev #23）。
 *
 * - `missing`: 見つからない。いま効く手は「差し込む・外す」
 * - `temporary`: 一時フォルダの中にある（中の写真は今は開ける）
 * - `missingTemporary`: **両方**。一時フォルダの中で消えたのなら、「USBメモリを差し込んで」は
 *   的外れ——OS や他のアプリに消された見込みが高い、と言う（#154 のゲート2）
 */
export type RootMark = "missing" | "temporary" | "missingTemporary" | null;

export function rootMark(
  root: string,
  missing: ReadonlySet<string>,
  temporary: ReadonlySet<string>,
): RootMark {
  const gone = missing.has(root);
  const temp = temporary.has(root);
  if (gone && temp) return "missingTemporary";
  if (gone) return "missing";
  if (temp) return "temporary";
  return null;
}

/**
 * 印ごとの見た目。**1か所に書いて表で試す**——クラスと説明を別々の三項演算で選ぶと、
 * 片方だけ入れ替わっても型も試験も止めない（#154 のゲート2の変異がそれを通した）。
 * `cls` は `App.css`、`tip` は辞書の鍵（どれも `(path) => string`）
 */
export const ROOT_MARK_VIEW = {
  missing: { cls: "root-missing", tip: "rootMissingTip" },
  temporary: { cls: "root-temporary", tip: "rootTempTip" },
  missingTemporary: { cls: "root-missing", tip: "rootMissingTempTip" },
} as const satisfies Record<Exclude<RootMark, null>, { cls: string; tip: string }>;
