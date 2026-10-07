/**
 * パレットの「（を）検索」と、アプリ操作の並べ方（2026-10-07 利用者決定）。純関数。
 * `scripts/paletteOrder.test.ts` が見る。
 *
 * **打った語が操作の名前に当たったら、その操作を検索より上に出す。** パレットに操作の名前を
 * 打つ人はその操作を探している——「すべての画像」と打って Enter で検索になっていた（win2 の実機）。
 * 検索は消さずにすぐ下へ残す（↓ 1回で届く）。当たる操作が無ければ、今までどおり検索が先
 */

/** 操作の名前が、打った語に当たるか（大文字小文字を畳んだ部分一致。今までの絞り方と同じ） */
export function actionMatches(label: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  return q === "" || label.toLowerCase().includes(q);
}

/**
 * 検索の候補（`search`。入力が空なら `null`）と、名前で絞った操作（`matched`）を並べる。
 * 入力があって操作が当たれば操作が先、無ければ検索だけ
 */
export function orderSearchAndActions<T>(search: T | null, matched: readonly T[]): T[] {
  if (search === null) return [...matched];
  return matched.length > 0 ? [...matched, search] : [search];
}
