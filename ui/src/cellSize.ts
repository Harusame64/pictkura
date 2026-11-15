/**
 * タイルの大きさ（上の帯の「サイズ」）を起動をまたいで覚える。毎回 180 に戻っていた。
 * 保存先は `localStorage`（この端末だけの好み）。読めない・形が違う・つまみの刻みに乗らない値は既定へ——
 * 刻みの外の値（110 など）を通すと、つまみは 120 を指すのにタイルは 110 で並び、食い違った（/code-review）
 */
const KEY = "pictkura.cellSize";

export const CELL_SIZE_MIN = 100;
export const CELL_SIZE_MAX = 320;
export const CELL_SIZE_STEP = 20;
export const CELL_SIZE_DEFAULT = 180;

/** 保存した文字列を大きさへ。整数で、範囲の中で、刻みに乗っているものだけ */
export function parseCellSize(raw: string | null): number {
  if (raw === null || !/^\d+$/.test(raw)) return CELL_SIZE_DEFAULT;
  const v = Number(raw);
  if (v < CELL_SIZE_MIN || v > CELL_SIZE_MAX || (v - CELL_SIZE_MIN) % CELL_SIZE_STEP !== 0) {
    return CELL_SIZE_DEFAULT;
  }
  return v;
}

export function loadCellSize(): number {
  try {
    return parseCellSize(localStorage.getItem(KEY));
  } catch {
    // 保存先が使えない窓では既定
    return CELL_SIZE_DEFAULT;
  }
}

let last: number | null = null;

/** 変わったときだけ書く（つまみを動かす間、同じ値を何度も書かない） */
export function saveCellSize(v: number): void {
  if (v === last) return;
  last = v;
  try {
    localStorage.setItem(KEY, String(v));
  } catch {
    // 書けなくても、今の起動では効いている
  }
}
