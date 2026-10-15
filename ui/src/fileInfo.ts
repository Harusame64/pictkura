/**
 * ファイルの基本情報の書式（`dev/adr.file-info-and-context-menu.md` の A）。
 *
 * **大きさは OS のファイル管理と同じ数え方にする**——エクスプローラーは 1024 で割って「KB・MB・GB」と書き、
 * Finder は 1000 で割る。同じファイルが pictkura と OS で違う数字に見えると、どちらかが間違っていると読まれる
 */

const UNITS = ["byte", "kilobyte", "megabyte", "gigabyte", "terabyte"] as const;

export const sizeBaseFor = (platform: string): 1000 | 1024 => (platform === "windows" ? 1024 : 1000);

/** 8.61 GB・512 KB・1 B のように。有効数字はおよそ3桁 */
export function formatBytes(bytes: number, base: 1000 | 1024, locale: string): string {
  let v = Math.max(0, bytes);
  let i = 0;
  while (v >= base && i < UNITS.length - 1) {
    v /= base;
    i += 1;
  }
  const digits = i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0;
  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit: UNITS[i],
    unitDisplay: "short",
    maximumFractionDigits: digits,
  }).format(v);
}

/** パスをフォルダと名前に分ける。区切りは `/` と `\` の両方（Windows のパスを mac で試験しても同じ） */
export function splitPath(path: string): { folder: string; name: string } {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  if (cut < 0) return { folder: "", name: path };
  // ルート直下（`C:\a.jpg`・`/a.jpg`）は区切りまで残す——「C:」だけでは別の意味になる
  const folder = path.slice(0, cut);
  return { folder: folder === "" || /^[A-Za-z]:$/.test(folder) ? path.slice(0, cut + 1) : folder, name: path.slice(cut + 1) };
}

/** 拡張子を大文字で（`IMG_1.cr3` → `CR3`）。無ければ空 */
export function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toUpperCase() : "";
}
