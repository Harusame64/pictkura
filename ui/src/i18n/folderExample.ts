/**
 * 「フォルダを追加」の入力例。**OSで綴りが違う**ので辞書に決め打てない
 * ——macOSに `D:` は無い。実測（2026-08-26）で、macOS版に
 * `例: D:\Pictures` がそのまま出ていた。
 *
 * `api.ts` の `isMac` と同じ判定をしているが、**あちらを import すると循環になる**
 * （`api.ts` はこの辞書の `locale` を使っている）。3つ目が要るときは
 * 判定だけを別のファイルへ出すこと。
 */
export function folderExample(prefix: string, user: string): string {
  const data = (navigator as Navigator & { userAgentData?: { platform?: string } })
    .userAgentData;
  const platform = data?.platform ?? navigator.userAgent;
  if (/mac/i.test(platform)) return `${prefix}/Users/${user}/Pictures`;
  if (/win/i.test(platform)) return `${prefix}D:\\Pictures`;
  return `${prefix}/home/${user}/Pictures`;
}
