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

