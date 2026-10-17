/**
 * 右クリックの「その他のオプションを表示」——エクスプローラーと同じメニュー（ADR B-4。Windows だけ）。
 *
 * メニューを組むのに、間が空くと毎回 1 秒ほどかかる（シェルの拡張を読み直す。win の実測）。**待ちの合図は
 * 足さない**——その間は OS が処理中のカーソルを出し、メニューが出ると戻る。CSS で足した合図は、閉じた後も
 * マウスを動かすまで残った（#214 の win の実測）
 */
import { showShellMenu } from "./api";

/** 組んでいる間に重ねて押されたら受け付けない（メニューが2回続けて出る） */
let opening = false;

/** `ids` のメニューを窓の中の `x`・`y` に出し、閉じたら解ける */
export async function openShellMenu(ids: number[], x: number, y: number): Promise<void> {
  if (opening) return;
  opening = true;
  try {
    await showShellMenu(ids, x, y);
  } finally {
    opening = false;
  }
}
