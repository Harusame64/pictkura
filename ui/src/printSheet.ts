/**
 * 選んだ写真を1ページに何枚か並べて印刷する（pictkura-dev #55、2026-10-18 利用者の選択）。
 *
 * 1枚だけのとき（`printPhoto`）とは別の紙面。紙の大きさの測り方は同じ考え（`printPhoto.ts` の説明）:
 *
 * - **ページの高さは `height: 100%` で取る。** `html`・`body` を紙1枚の高さにし、ページの箱をそれぞれ
 *   `height: 100%`・`break-after: page` で積む。WebKit の印刷では `vh` も `@page { size }` も紙に従わない
 *   （Safari の印刷プレビューで、縦・横どちらの紙でも1ページずつ収まるのを確かめた。2026-10-18）
 * - **並びは紙の向きで変える**（2枚・6枚）。ページの箱を `container-type: size` にして `@container (orientation)`
 *   で読む——利用者が印刷の画面で向きを替えても組み直される
 * - 絵は**長辺を抑えたもの**（`print-<長辺>/<id>`）。原寸のままだと 45MP の写真1枚で 180MB を WebView が抱える
 */
import { convertFileSrc } from "@tauri-apps/api/core";

import type { PrintFit } from "./printPhoto";

/** 1ページに載せる枚数 */
export type PerPage = 1 | 2 | 4 | 6 | 9;
export const PER_PAGE: readonly PerPage[] = [1, 2, 4, 6, 9];

/** 並べて刷る1枚。`caption` は名前と撮影日（入れないときも作っておく） */
export type SheetItem = { id: number; caption: string };

export type SheetLayout = { perPage: PerPage; captions: boolean };

const LAYOUT_KEY = "pictkura.printSheet";
// 紙面の id は `printPhoto` と同じにしてある——どちらかが残した紙面を、次の印刷がどちらでも外す（紙面が2つ
// あると両方が刷られる）
const SHEET_ID = "print-sheet";
const STYLE_ID = "print-sheet-style";

/** 縦の紙での列×行。横の紙では入れ替える（正方形の並びは同じ） */
export function gridFor(perPage: PerPage): { cols: number; rows: number } {
  switch (perPage) {
    case 1:
      return { cols: 1, rows: 1 };
    case 2:
      return { cols: 1, rows: 2 };
    case 4:
      return { cols: 2, rows: 2 };
    case 6:
      return { cols: 2, rows: 3 };
    case 9:
      return { cols: 3, rows: 3 };
  }
}

/**
 * マスの絵の長辺（画素）。A4 を 300dpi で刷っても、マスの長辺はこれより短い（2枚のとき約 2480）。
 * 抑えないと WebView が全部の絵を原寸でほどいて抱える
 */
export function edgeFor(perPage: PerPage): number {
  return { 1: 2400, 2: 2000, 4: 1600, 6: 1400, 9: 1100 }[perPage];
}

/**
 * 一度に刷れる枚数。ほどいた絵（長辺×短辺×4 バイト、3:2 として）の合計がおよそ 600MB に収まる数。
 * 紙面は全部の絵をほどいてから印刷の画面へ渡すので、枚数がそのままメモリになる
 */
export function maxPhotosFor(perPage: PerPage): number {
  return { 1: 40, 2: 60, 4: 90, 6: 120, 9: 180 }[perPage];
}

/** `items` をページごとに分ける（並びはそのまま） */
export function pagesOf<T>(items: readonly T[], perPage: PerPage): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += perPage) out.push(items.slice(i, i + perPage));
  return out;
}

/** 並べて刷る絵の URL（撮影情報を落とし、長辺を `edge` に抑えた JPEG。`crates/pictkura-core/src/print.rs`） */
export const sheetSrc = (id: number, edge: number) =>
  convertFileSrc(`print-${edge}/${id}`, "media");

/** 前に選んだ並べ方。読めない・知らない値なら 4枚・名前なし */
export function loadSheetLayout(): SheetLayout {
  try {
    const v = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "null") as Partial<SheetLayout> | null;
    if (v && (PER_PAGE as readonly number[]).includes(v.perPage as number)) {
      return { perPage: v.perPage as PerPage, captions: v.captions === true };
    }
  } catch {
    // 保存先が使えない窓でも印刷はできる
  }
  return { perPage: 4, captions: false };
}

export function saveSheetLayout(layout: SheetLayout): void {
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
  } catch {
    // 覚えられなくても、今回の印刷には効いている
  }
}

/** 紙面の CSS */
export function sheetCss(fit: PrintFit, layout: SheetLayout): string {
  const { cols, rows } = gridFor(layout.perPage);
  const grid = (c: number, r: number) =>
    `grid-template-columns: repeat(${c}, minmax(0, 1fr)); grid-template-rows: repeat(${r}, minmax(0, 1fr));`;
  return `
@page { margin: 0; }
#${SHEET_ID} { display: none; }
@media print {
  html, body {
    margin: 0 !important;
    padding: 0 !important;
    height: 100% !important;
    background: #fff !important;
  }
  body > *:not(#${SHEET_ID}) { display: none !important; }
  #${SHEET_ID} { display: block; height: 100%; }
  #${SHEET_ID} .sheet-page {
    position: relative;
    height: 100%;
    overflow: hidden;
    break-after: page;
    break-inside: avoid;
    container-type: size;
  }
  #${SHEET_ID} .sheet-page:last-child { break-after: auto; }
  #${SHEET_ID} .sheet-grid {
    position: absolute;
    inset: 5cqmin;
    display: grid;
    gap: 3cqmin;
    ${grid(cols, rows)}
  }
  @container (orientation: landscape) {
    #${SHEET_ID} .sheet-grid { ${grid(rows, cols)} }
  }
  #${SHEET_ID} figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  #${SHEET_ID} img {
    flex: 1;
    min-height: 0;
    width: 100%;
    object-fit: ${fit};
  }
  #${SHEET_ID} figcaption {
    padding-top: 1cqmin;
    font: 8pt sans-serif;
    color: #333;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
}`;
}

function clearSheet(): void {
  document.getElementById(SHEET_ID)?.remove();
  document.getElementById(STYLE_ID)?.remove();
}

/** 読み込み中に重ねて押されたら受け付けない（`printPhoto` と同じ理由） */
let preparing = false;

/**
 * 並べて印刷の画面を出す。絵は4枚ずつ並行して読み、`onProgress(読めた数, 全数)` で知らせる。
 * 読めなかった写真（動画・消えたファイル等）は飛ばして詰め、その数を返す。1枚も読めなければ
 * 辞書の鍵 `errPrint` を名に持つ Error を投げる
 */
export async function printSheet(
  items: readonly SheetItem[],
  fit: PrintFit,
  layout: SheetLayout,
  onProgress: (done: number, total: number) => void,
): Promise<{ skipped: number }> {
  if (preparing) return { skipped: 0 };
  clearSheet();
  preparing = true;
  document.documentElement.classList.add("print-preparing");
  const edge = edgeFor(layout.perPage);
  const loaded: (HTMLImageElement | null)[] = new Array(items.length).fill(null);
  let done = 0;
  onProgress(0, items.length);
  try {
    let next = 0;
    const worker = async () => {
      while (next < items.length) {
        const i = next++;
        const img = new Image();
        img.decoding = "sync";
        img.src = sheetSrc(items[i].id, edge);
        try {
          await img.decode();
          loaded[i] = img;
        } catch {
          // 飛ばして詰める
        }
        onProgress(++done, items.length);
      }
    };
    await Promise.all(Array.from({ length: Math.min(4, items.length) }, worker));
  } finally {
    preparing = false;
    document.documentElement.classList.remove("print-preparing");
  }
  const ok = items.flatMap((it, i) => (loaded[i] ? [{ it, img: loaded[i] }] : []));
  if (ok.length === 0) throw new Error("errPrint");

  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = sheetCss(fit, layout);
  const sheet = document.createElement("div");
  sheet.id = SHEET_ID;
  for (const page of pagesOf(ok, layout.perPage)) {
    const pageEl = document.createElement("div");
    pageEl.className = "sheet-page";
    const grid = document.createElement("div");
    grid.className = "sheet-grid";
    for (const { it, img } of page) {
      const fig = document.createElement("figure");
      img.alt = "";
      fig.appendChild(img);
      if (layout.captions) {
        const cap = document.createElement("figcaption");
        cap.textContent = it.caption;
        fig.appendChild(cap);
      }
      grid.appendChild(fig);
    }
    pageEl.appendChild(grid);
    sheet.appendChild(pageEl);
  }
  document.head.appendChild(style);
  document.body.appendChild(sheet);
  window.addEventListener("afterprint", clearSheet, { once: true });
  try {
    // macOS は Promise（Tauri の差し替え）、Windows は undefined（閉じるまで戻らない）
    await (window.print as () => unknown)();
  } catch (e) {
    window.removeEventListener("afterprint", clearSheet);
    clearSheet();
    throw e;
  }
  return { skipped: items.length - ok.length };
}
