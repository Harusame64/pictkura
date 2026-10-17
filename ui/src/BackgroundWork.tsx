/**
 * 裏で走っている仕事を、ヘッダーの件数の隣に**邪魔にならない1行だけ**出す（pictkura-dev #64。2026-10-17 利用者の形）。
 * 走査（フォルダを足した・取り込み先・今すぐ同期・ドライブが戻った）→ 新しく入った写真の読み込み、の順に1つだけ言う。
 *
 * - **数えるのはバックエンド**（`background_status`）。写真の読み込みは**まだサムネイルの無い写真**だけを数えるので、
 *   起動のたびに列へ戻る寸法の無い RAW などは入らない。画面で「増えたかどうか」を当て推量しない（#64 のゲート2、3周）
 * - **続けて2回の問い合わせで何かの仕事を見たときだけ出す**——1回だけ捕まえた短い走査を出さない（#57 の決まり）
 * - 取り込みの面を開いている間は出さない（面が自分で進み具合を出している。幕の奥で二重に言わない）
 * - 問い合わせと状態は**この部品の中だけ**に持つ——数が毎秒変わっても、一覧ごと描き直させない（ゲート2）
 */
import { useEffect, useRef, useState } from "react";

import { backgroundStatus } from "./api";
import { t } from "./i18n";

type Shown = { kind: "scan" } | { kind: "reading"; left: number } | null;

const same = (a: Shown, b: Shown) =>
  a === b ||
  (a !== null &&
    b !== null &&
    a.kind === b.kind &&
    (a.kind === "scan" || (b.kind === "reading" && a.left === b.left)));

export function BackgroundWork({ quiet }: { quiet: boolean }) {
  const [shown, setShown] = useState<Shown>(null);
  const quietRef = useRef(quiet);
  useEffect(() => {
    quietRef.current = quiet;
  }, [quiet]);

  // 何か動いている間は1秒ごと、無い間は3秒ごと、窓が隠れている間は5秒ごと。1回の問い合わせは数と旗を読むだけ
  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    let streak = 0;
    const tick = async () => {
      let next: Shown = null;
      try {
        const s = await backgroundStatus();
        if (!alive) return;
        if (s.scanning) next = { kind: "scan" };
        else if (s.thumbs_pending > 0) next = { kind: "reading", left: s.thumbs_pending };
      } catch {
        // 取れなかった回は何も無かったことにする（出しっぱなしにしない）
      }
      // 種類が移っても数え直さない（走査 → 読み込みの間で一度消えて見えないように）
      streak = next === null ? 0 : streak + 1;
      const show = streak >= 2 && !quietRef.current ? next : null;
      setShown((prev) => (same(prev, show) ? prev : show));
      if (alive) timer = window.setTimeout(tick, document.hidden ? 5000 : next !== null ? 1000 : 3000);
    };
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  if (shown === null || quiet) return null;
  const label = shown.kind === "scan" ? t.bgScanning : t.bgThumbs(shown.left);
  // 読み上げには載せない（数が毎秒変わる）
  return (
    <span className="bg-work" title={label}>
      <span className="bg-spinner" aria-hidden />
      <span className="bg-text">{label}</span>
    </span>
  );
}
