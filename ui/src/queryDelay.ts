/**
 * 検索欄の打鍵から実クエリへ落とすまでの待ち時間（App.tsx のデバウンス）。
 *
 * `year:` / `年:` の値は4桁でしか年にならない（crates/pictkura-core/src/search.rs の `parse_year`）。
 * 読めない値は「何にも当たらない範囲」になるので、`year:2023` を1字ずつ打つと `year:202` の時点で
 * 0件の案内が一瞬出た（win2）。**打ちかけの年**（末尾の `year:` に数字が3桁まで）のあいだは長く待つ——
 * 普通の速さで打てば4桁目が先に来て、案内は出ない。手を止めたままなら、待ったあとで今の語のまま絞る
 * （0件で打ち間違いに気付ける、という search.rs の考えはそのまま）
 */
export const QUERY_DELAY_MS = 160;
export const QUERY_DELAY_PARTIAL_YEAR_MS = 900;

/** 末尾の語が打ちかけの `year:` / `年:`（数字が0〜3桁。全角の数字も数える）か */
export function endsWithPartialYear(input: string): boolean {
  return /(?:^|\s)(?:year|年):[0-9０-９]{0,3}$/iu.test(input);
}

export function queryDelayMs(input: string): number {
  return endsWithPartialYear(input) ? QUERY_DELAY_PARTIAL_YEAR_MS : QUERY_DELAY_MS;
}
