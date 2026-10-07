/**
 * パレットの「（を）検索」と、アプリ操作の並べ方（2026-10-07 利用者決定）。純関数。
 * `scripts/paletteOrder.test.ts` が見る。
 *
 * **打った語が操作の名前の頭（語の頭）に当たったら、その操作を検索より上に出す。** パレットに操作の
 * 名前を打つ人はその操作を探している——「すべての画像」と打って Enter で検索になっていた（win2 の実機）。
 * 頭に限るのは、名前の途中に当たる普通の検索語（「画像」「scan」）で Enter が操作を走らせないため
 * （ゲート2。再スキャン・取り込みのような重い操作もある）。検索は消さずに当たった操作の下へ残す。
 * **途中に当たる操作は消さない**——検索の下に今までどおり並べる（「表示」「收藏」で操作が見つからなく
 * ならないように。ゲート2）
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

/**
 * 操作が、打った語にどう当たるか。`start`＝名前（今の言語）か英語の名前（`alias`）の語の頭、
 * `inside`＝どちらかの途中（今までの部分一致）、`null`＝当たらない。入力が空なら全部 `start`
 */
export function actionMatch(
  label: string,
  query: string,
  alias?: string,
): "start" | "inside" | null {
  const q = query.trim().toLowerCase().replace(/\s+/g, " ");
  if (q === "") return "start";
  if (fromAWordStart(label, q) || (alias !== undefined && fromAWordStart(alias, q))) return "start";
  const inside = (text: string) => text.toLowerCase().replace(/\s+/g, " ").includes(q);
  if (inside(label) || (alias !== undefined && inside(alias))) return "inside";
  return null;
}

/**
 * 検索の候補（`search`。入力が空なら `null`）と操作を並べる: 語の頭に当たった操作 → 検索 →
 * 途中に当たった操作。操作どうしの並びは元のまま
 */
export function orderSearchAndActions<T>(
  search: T | null,
  actions: readonly T[],
  matchOf: (action: T) => "start" | "inside" | null,
): T[] {
  const start = actions.filter((a) => matchOf(a) === "start");
  const inside = actions.filter((a) => matchOf(a) === "inside");
  return search === null ? [...start, ...inside] : [...start, search, ...inside];
}
