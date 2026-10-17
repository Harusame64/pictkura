/**
 * 右クリックの「その他のオプションを表示」——エクスプローラーと同じメニュー（ADR B-4。Windows だけ）。
 *
 * メニューを組むのに、間が空くと毎回 1 秒ほどかかる（シェルの拡張を読み直す。win の実測）。その間は何も
 * 出ないので、**カーソルで待ちを知らせる**。合図はメニューが出る直前（`shell-menu-shown`）に消す
 */
import { listen } from "@tauri-apps/api/event";

import { showShellMenu } from "./api";

/** 組んでいる間に重ねて押されたら受け付けない（メニューが2回続けて出る） */
let opening = false;

const CUE = "shell-menu-preparing";

/** `ids` のメニューを窓の中の `x`・`y` に出し、閉じたら解ける */
export async function openShellMenu(ids: number[], x: number, y: number): Promise<void> {
  if (opening) return;
  opening = true;
  const root = document.documentElement;
  root.classList.add(CUE);
  const unlisten = await listen("shell-menu-shown", () => root.classList.remove(CUE)).catch(
    () => undefined,
  );
  try {
    await showShellMenu(ids, x, y);
  } finally {
    opening = false;
    root.classList.remove(CUE);
    unlisten?.();
  }
}
