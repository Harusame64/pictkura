/**
 * ファイルの基本情報の書式（`dev/adr.file-info-and-context-menu.md` の A）。
 *
 * 大きさの書式（`formatSize`）は送り出しの確認から移した（同じ数え方を2つ持たない。PR のゲート2）
 */

export const sizeBaseFor = (platform: string): 1000 | 1024 => (platform === "windows" ? 1024 : 1000);

/**
 * 大きさ（バイト）を「8.4 GB」の形に。送り出しの確認とファイルの基本情報（ADR の A）で使う。**OS のファイルの一覧と同じ数え方**にする
 * ——Finder は 1000 で割って小数1桁に四捨五入、エクスプローラー（`StrFormatByteSizeW`）は 1024 で割って
 * **有効3桁で切り捨て**（35.29 → 「35.2」、6.613 → 「6.61」。win の実機で突き合わせた）。食い違うと、
 * 利用者が見比べたときにどちらかが嘘に見える
 */
const fmts = new Map<string, [Intl.NumberFormat, Intl.NumberFormat]>();
/** 書式のロケールごとに1組だけ作る（描画のたびに `new Intl.NumberFormat` しない） */
function fmtsFor(locale: string): [Intl.NumberFormat, Intl.NumberFormat] {
  let f = fmts.get(locale);
  if (!f) {
    f = [
      new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }),
      new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }),
    ];
    fmts.set(locale, f);
  }
  return f;
}
export function formatSize(bytes: number, base: 1000 | 1024, locale: string): string {
  const [sizeFmt, sizeFmt2] = fmtsFor(locale);
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  // エクスプローラーは 1024 で割るが、**1,000 を超えたら次の単位へ上げる**（1,015 KB ではなく 0.99 MB。ゲート2）
  const step = base === 1024 ? 1000 : base;
  while (v >= step && i < units.length - 1) {
    v /= base;
    i++;
  }
  if (base === 1024) {
    // 有効3桁で切り捨て（エクスプローラーと同じ）。整数部が3桁ならそのまま
    const digits = v >= 100 ? 0 : v >= 10 ? 1 : 2;
    const f = 10 ** digits;
    const cut = Math.floor(v * f) / f;
    return `${(digits === 2 ? sizeFmt2 : sizeFmt).format(cut)} ${units[i]}`;
  }
  // 丸めると単位の境目に届くもの（999.96 KB → 「1,000 KB」）は、次の単位へ上げる（ゲート2）
  if (Number(v.toFixed(1)) >= base && i < units.length - 1) {
    v /= base;
    i++;
  }
  return `${sizeFmt.format(v)} ${units[i]}`;
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
