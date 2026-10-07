/**
 * パレットの「（を）検索」と、アプリ操作の並べ方（2026-10-07 利用者決定）。純関数。
 * `scripts/paletteOrder.test.ts` が見る。
 *
 * **打った語が操作の名前の頭（語の頭）に当たったら、その操作を検索より上に出す。** パレットに操作の
 * 名前を打つ人はその操作を探している——「すべての画像」と打って Enter で検索になっていた（win2 の実機）。
 * 頭に限るのは、名前の途中に当たる普通の検索語（「画像」「scan」）で Enter が操作を走らせないため
 * （ゲート2。再スキャン・取り込みのような重い操作もある）。検索は消さずに当たった操作の下へ残す。
 *
 * **英語の名前でも当たる**（`alias`）。IME で変換する言語（日本語・中国語）では、英語のまま打てば
 * 変換と確定の手間が要らない（2026-10-07 利用者）。英語の名前は動詞から始まる（"Show all photos"）
 * ので、頭は**語ごと**に見る（"all" で当たる）
 */

/** `text` のどこかの語の頭から、`q`（小文字・前後の空白なし）が続くか */
function fromAWordStart(text: string, q: string): boolean {
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  return words.some((_, i) => words.slice(i).join(" ").startsWith(q));
}

/** 操作が、打った語に当たるか。名前（今の言語）と英語の名前（`alias`）の、語の頭で見る */
export function actionMatches(label: string, query: string, alias?: string): boolean {
  const q = query.trim().toLowerCase().replace(/\s+/g, " ");
  if (q === "") return true;
  return fromAWordStart(label, q) || (alias !== undefined && fromAWordStart(alias, q));
}

/**
 * 検索の候補（`search`。入力が空なら `null`）と、名前で絞った操作（`matched`）を並べる。
 * 入力があって操作が当たれば操作が先、無ければ検索だけ
 */
export function orderSearchAndActions<T>(search: T | null, matched: readonly T[]): T[] {
  if (search === null) return [...matched];
  return matched.length > 0 ? [...matched, search] : [search];
}
