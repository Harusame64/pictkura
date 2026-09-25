import { useEffect, useState } from "react";

import { isTemporaryFolder } from "./api";

/**
 * その場所が一時フォルダの中か（dev #30）。取り込みのウィンドウと設定の、コピー先の警告に使う。
 *
 * **開いているときだけ訊き、開くたびに訊き直す**（`active`）。どちらの画面も閉じていても
 * 組み立てられているので、場所だけで訊くと起動のたびに2本走り、眠った NAS を起こす。
 * しかも起動直後に3秒で見切られると、場所が変わらない限り二度と訊かず、警告が一度も
 * 出ない（#150 の2ゲート目）。
 * **場所が変わったら先に下ろす**——前の場所の答えを新しい場所の隣に出したままにしない。
 * 判定できなければ出さない。
 */
export function useIsTemporaryFolder(
  path: string | null,
  active: boolean,
): boolean {
  const [temporary, setTemporary] = useState(false);
  useEffect(() => {
    setTemporary(false);
    if (!active || !path) return;
    let cancelled = false;
    isTemporaryFolder(path)
      .then((yes) => {
        if (!cancelled) setTemporary(yes);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [path, active]);
  return temporary;
}

/**
 * ライブラリのフォルダのうち、一時フォルダの中にあるもの（dev #23）。左ペインの印に使う。
 *
 * #149 は**足すとき**に訊くだけで、**前から在るフォルダ**（確認が入る前に足した、
 * 設定ファイルを手で書いた）には何も言わなかった。**フォルダの一覧が変わったときだけ**
 * 訊く——一覧は滅多に変わらず、1本ずつ3秒で見切られる（`is_temporary_folder`）。
 * 前の答えは、新しい一覧の答えが揃うまで出したままにしない。判定できなければ出さない。
 */
export function useTemporaryRoots(roots: readonly string[]): ReadonlySet<string> {
  const [temporary, setTemporary] = useState<ReadonlySet<string>>(new Set());
  // 配列の同一性ではなく中身で決める（取り直すたびに新しい配列が来る）
  const key = roots.join("\n");
  useEffect(() => {
    setTemporary(new Set());
    const list = key === "" ? [] : key.split("\n");
    if (list.length === 0) return;
    let cancelled = false;
    Promise.all(
      list.map((r) =>
        isTemporaryFolder(r)
          .then((yes) => (yes ? r : null))
          .catch(() => null),
      ),
    ).then((found) => {
      if (!cancelled)
        setTemporary(new Set(found.filter((r): r is string => r !== null)));
    });
    return () => {
      cancelled = true;
    };
  }, [key]);
  return temporary;
}
