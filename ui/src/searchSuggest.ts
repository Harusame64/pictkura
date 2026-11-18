/**
 * 検索欄のキーと値の候補（2026-11-18 利用者「year とかはサジェストがあると失敗しづらい」）。
 *
 * - **キーは英語の綴りを出す**。検索の文法（`crates/pictkura-core/src/search.rs` の `parse_query`）は英語と日本語の
 *   綴りを受けるが、検索欄の案内は14言語すべて `year:2019` を例に出している。言語ごとの綴りは増やさず、説明だけを
 *   その言語で添える（呼ぶ側が `hint` を引く）
 * - **打った語がキーの頭に合うときだけ出す**（利用者の選択）。ふつうにファイル名で探す人の邪魔をしない
 * - 値を出すのは `year:`（ライブラリにある年）・`camera:`（カメラの一覧）・`kind:`（3種類）。`folder:` は数が
 *   多くなりうるのでキーだけ。`fav:` `pick:` は値の要らないキー
 * - 別名（`cam:` `type:` `年:` `カメラ:` `種類:`）で打っても値の候補を出す。差し込むときは打ったキーのまま
 */

/** 候補に出すキー（英語の綴り）。`hint` は呼ぶ側が言語の辞書から引く鍵 */
export const SEARCH_KEYS = ["year", "camera", "kind", "folder", "fav", "pick"] as const;
export type SearchKey = (typeof SEARCH_KEYS)[number];

/** 値の候補を出すキーの別名 → 正式な名前（`parse_query` と同じ綴り） */
const KEY_ALIASES: Record<string, SearchKey> = {
  year: "year",
  年: "year",
  camera: "camera",
  cam: "camera",
  カメラ: "camera",
  kind: "kind",
  type: "kind",
  種類: "kind",
};

/** `kind:` に書ける値（`MediaKind::parse` の英語の綴り） */
export const KIND_VALUES = ["photo", "raw", "video"] as const;

export type Suggestion =
  | { type: "key"; key: SearchKey; insert: string }
  | { type: "year"; year: number; count: number; insert: string }
  | { type: "camera"; name: string; count: number; insert: string }
  | { type: "kind"; kind: (typeof KIND_VALUES)[number]; insert: string };

/** カーソルのある語（空白で区切る。引用符の中の空白は区切りにしない＝`tokenize` と同じ） */
export function tokenAt(input: string, caret: number): { start: number; end: number; text: string } {
  let start = 0;
  let quoted = false;
  for (let i = 0; i < caret; i++) {
    const c = input[i];
    if (c === '"') quoted = !quoted;
    else if (!quoted && (c === " " || c === "　" || c === "\t")) start = i + 1;
  }
  // 引用符を開いたままカーソルがある（`cam:"iph beach`）なら、置き換えはカーソルまで——引用符が閉じていないので、
  // 後ろの語まで1つの語に見え、差し込むと消えた（PR #353 の /code-review）
  let end = caret;
  if (!quoted) {
    while (end < input.length && !/[\s　]/.test(input[end])) end++;
  }
  return { start, end, text: input.slice(start, caret) };
}

/** 候補を選ぶ。`token` はカーソルの手前までの語 */
export function suggest(
  token: string,
  data: { years: readonly { year: number; count: number }[]; cameras: readonly { name: string; count: number }[] },
  max = 8,
): Suggestion[] {
  const colon = token.indexOf(":");
  if (colon < 0) {
    const t = token.toLowerCase();
    if (t.length === 0 || t.startsWith('"')) return [];
    return SEARCH_KEYS.filter((k) => k.startsWith(t) && k !== t).map((key) => ({
      type: "key" as const,
      key,
      insert: `${key}:`,
    }));
  }
  const typedKey = token.slice(0, colon);
  const key = KEY_ALIASES[typedKey.toLowerCase()];
  // 全角の数字（`年:２０`）・アラビア・インド数字（`٢٠`）も、検索の文法が受けるので年の候補では ASCII に揃えて比べる
  const value = asciiDigits(token.slice(colon + 1).replace(/^"/, "")).toLowerCase();
  if (key === "year") {
    return data.years
      .filter((y) => String(y.year).startsWith(value) && String(y.year) !== value)
      .slice(0, max)
      .map((y) => ({ type: "year" as const, year: y.year, count: y.count, insert: `${typedKey}:${y.year} ` }));
  }
  if (key === "camera") {
    return data.cameras
      .filter((c) => c.name.toLowerCase().includes(value) && c.name.toLowerCase() !== value)
      .slice(0, max)
      .map((c) => ({
        type: "camera" as const,
        name: c.name,
        count: c.count,
        insert: `${typedKey}:${/\s/.test(c.name) ? `"${c.name}"` : c.name} `,
      }));
  }
  if (key === "kind") {
    return KIND_VALUES.filter((k) => k.startsWith(value) && k !== value).map((kind) => ({
      type: "kind" as const,
      kind,
      insert: `${typedKey}:${kind} `,
    }));
  }
  return [];
}

/** 全角数字・アラビア・インド数字・ペルシャ数字を ASCII の数字へ */
function asciiDigits(s: string): string {
  return s.replace(/[０-９٠-٩۰-۹]/g, (c) => {
    const code = c.charCodeAt(0);
    if (code >= 0xff10 && code <= 0xff19) return String(code - 0xff10);
    if (code >= 0x0660 && code <= 0x0669) return String(code - 0x0660);
    return String(code - 0x06f0);
  });
}

/**
 * カーソルの語が**値の無いキー**（`year:` `kind:` …）だけか。候補で値を選んでいる途中なので、検索に流さない——
 * 空の `year:` `kind:` は何にも当たらず、一覧が「該当なし」に一瞬なった（PR #353 の /code-review）
 */
export function isBareKey(token: string): boolean {
  if (!token.endsWith(":")) return false;
  const k = token.slice(0, -1).toLowerCase();
  // 値を取るキーだけ（`fav:` `pick:` は値が無くても正しい指定）
  return k in KEY_ALIASES || ["folder", "dir", "フォルダ"].includes(k);
}

/** 候補を差し込んだ後の入力とカーソルの位置（カーソルのある語を丸ごと置き換える） */
export function applySuggestion(input: string, caret: number, s: Suggestion): { value: string; caret: number } {
  const { start, end } = tokenAt(input, caret);
  // 後ろに語が続いていて、差し込みが空白で終わるなら、続きの空白と重ねない
  let rest = input.slice(end);
  if (s.insert.endsWith(" ") && rest.startsWith(" ")) rest = rest.slice(1);
  const value = input.slice(0, start) + s.insert + rest;
  return { value, caret: start + s.insert.length };
}

/** 一覧の骨組み（日ごと）から、年ごとの枚数（新しい年から）。日付ではない日（0 以下）は数えない */
export function yearsOf(summary: readonly { day_key: number; count: number }[]): { year: number; count: number }[] {
  const m = new Map<number, number>();
  for (const d of summary) {
    if (d.day_key <= 0) continue;
    const y = Math.floor(d.day_key / 10000);
    // `year:` が受ける年だけ（search.rs の MIN_YEAR〜MAX_YEAR）。外の年を出すと、選んでも0件になる
    if (y < 1800 || y > 9999) continue;
    m.set(y, (m.get(y) ?? 0) + d.count);
  }
  return [...m].sort((a, b) => b[0] - a[0]).map(([year, count]) => ({ year, count }));
}
