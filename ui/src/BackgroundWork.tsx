/**
 * 裏で走っている仕事を、ヘッダーの件数の隣に**邪魔にならない1行だけ**出す（pictkura-dev #64。2026-10-17 利用者の形）。
 * 走査（フォルダを足した・取り込み先・今すぐ同期・ドライブが戻った）→ 新しく入った写真の読み込み → サムネイルの自動生成、
 * の順に1つだけ言う。
 *
 * - **数えるのはバックエンド**（`background_status`）。写真の読み込みは**まだサムネイルの無い写真**だけを数えるので、
 *   起動のたびに列へ戻る寸法の無い RAW などは入らない。画面で「増えたかどうか」を当て推量しない（#64 のゲート2、3周）
 * - **続けて2回の問い合わせで何かの仕事を見たときだけ出す**——1回だけ捕まえた短い走査を出さない（#57 の決まり）
 * - 取り込みの面を開いている間は出さない（面が自分で進み具合を出している。幕の奥で二重に言わない）
 * - 問い合わせと状態は**この部品の中だけ**に持つ——数が毎秒変わっても、一覧ごと描き直させない（ゲート2）
 * - **サムネイルの自動生成**（設定、既定オフ）は押せる表示にする。押すと一時停止と設定への入口が開く（OneDrive の
 *   トレイの感じ。2026-11-18 利用者）。作り終えたら黙って消える
 */
import { useEffect, useRef, useState } from "react";

import { backgroundStatus, setAutoThumbnailsPaused } from "./api";
import { t } from "./i18n";
import { errText } from "./i18n/err.ts";

type Shown =
  | { kind: "scan" }
  | { kind: "reading"; left: number }
  | { kind: "auto"; left: number; paused: boolean }
  | null;

/** 小窓の幅と、窓の縁から空ける量（px） */
const POP_WIDTH = 300;
const POP_MARGIN = 8;

const same = (a: Shown, b: Shown) => JSON.stringify(a) === JSON.stringify(b);

export function BackgroundWork({
  quiet,
  onOpenSettings,
  onError,
}: {
  quiet: boolean;
  onOpenSettings: () => void;
  onError: (message: string) => void;
}) {
  const [shown, setShown] = useState<Shown>(null);
  const [popOpen, setPopOpen] = useState(false);
  /** 小窓の置き場（窓の座標）。ボタンの端に揃え、窓からはみ出さないよう寄せる（win2 の実機: 幅 1280 で右へ 71px、
   *  右から左で左へ 44px はみ出て本文が切れた） */
  const [popAt, setPopAt] = useState<{ left: number; top: number } | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const openPop = () => {
    const r = buttonRef.current?.getBoundingClientRect();
    if (r) {
      const rtl = document.documentElement.dir === "rtl";
      const vw = document.documentElement.clientWidth;
      const want = rtl ? r.right - POP_WIDTH : r.left;
      setPopAt({
        left: Math.max(POP_MARGIN, Math.min(want, vw - POP_WIDTH - POP_MARGIN)),
        top: r.bottom + 6,
      });
    }
    setPopOpen(true);
  };
  const quietRef = useRef(quiet);
  const wrapRef = useRef<HTMLSpanElement>(null);
  /** 押した直後の一時停止・再開。次の問い合わせが追いつくまで、押した側の見た目を出す */
  const [pausedNow, setPausedNow] = useState<boolean | null>(null);
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
        const auto = s.auto_thumbs;
        if (s.scanning) next = { kind: "scan" };
        else if (s.thumbs_pending > 0) next = { kind: "reading", left: s.thumbs_pending };
        // 一時停止中は数に依らず出す——消えると「再開」の口が無くなる（PR #348 の /code-review）
        else if (auto && ((auto.working && auto.left > 0) || auto.paused))
          next = { kind: "auto", left: auto.left, paused: auto.paused };
      } catch {
        // 取れなかった回は何も無かったことにする（出しっぱなしにしない）
      }
      // 種類が移っても数え直さない（走査 → 読み込みの間で一度消えて見えないように）
      streak = next === null ? 0 : streak + 1;
      // 一時停止中は 1 回で出す（押した結果がすぐ見えるように）
      const ready = streak >= 2 || (next?.kind === "auto" && next.paused);
      const show = ready && !quietRef.current ? next : null;
      setShown((prev) => (same(prev, show) ? prev : show));
      // 押した側の見た目は、知らせが追いついてから外す（先に外すと古い状態へ一瞬戻る）
      const serverPaused = next?.kind === "auto" ? next.paused : null;
      setPausedNow((p) => (serverPaused === null || p === serverPaused ? null : p));
      if (alive) timer = window.setTimeout(tick, document.hidden ? 5000 : next !== null ? 1000 : 3000);
    };
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  const autoShown = shown?.kind === "auto" && !quiet;
  // 自動生成の表示が消えたら、開いていた小窓も畳む
  useEffect(() => {
    if (!autoShown) setPopOpen(false);
  }, [autoShown]);

  // 小窓の外を押す・Esc で畳む。**Esc は手前の幕が取る**——ここで止めて、奥（一覧の選択の解除など）へ渡さない
  useEffect(() => {
    if (!popOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      setPopOpen(false);
    };
    const onDown = (e: PointerEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setPopOpen(false);
    };
    // 窓の大きさが変わったら畳む（置き場は開いたときの窓の座標で決めている）
    const onResize = () => setPopOpen(false);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("resize", onResize);
    };
  }, [popOpen]);

  if (shown === null || quiet) return null;

  if (shown.kind === "auto") {
    const paused = pausedNow ?? shown.paused;
    const label = paused ? t.bgAutoThumbsPaused(shown.left) : t.bgAutoThumbs(shown.left);
    const short = paused ? t.bgAutoThumbsPausedShort(shown.left) : t.bgAutoThumbsShort(shown.left);
    const togglePause = async () => {
      const next = !paused;
      setPausedNow(next);
      try {
        await setAutoThumbnailsPaused(next);
      } catch (e) {
        setPausedNow(null);
        onError(errText(e));
      }
    };
    return (
      <span className="bg-auto" ref={wrapRef}>
        <button
          type="button"
          className={"bg-work bg-auto-button" + (popOpen ? " open" : "")}
          title={label}
          aria-expanded={popOpen}
          aria-haspopup="dialog"
          ref={buttonRef}
          onClick={() => (popOpen ? setPopOpen(false) : openPop())}
        >
          {paused ? (
            <svg className="bg-auto-icon" width="14" height="14" viewBox="0 0 16 16" aria-hidden>
              <rect x="4" y="3" width="2.4" height="10" rx="0.8" fill="currentColor" />
              <rect x="9.6" y="3" width="2.4" height="10" rx="0.8" fill="currentColor" />
            </svg>
          ) : (
            <svg className="bg-auto-icon" width="14" height="14" viewBox="0 0 16 16" aria-hidden>
              <rect x="2" y="3" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
              <path
                d="M5 14h8.5a.5.5 0 0 0 .5-.5V6"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.4"
                strokeLinecap="round"
              />
            </svg>
          )}
          <span className="bg-text">{short}</span>
        </button>
        {popOpen && (
          <div
            className="bg-auto-pop"
            role="dialog"
            aria-label={t.settingsAutoThumbsToggle}
            style={popAt ? { left: popAt.left, top: popAt.top, width: POP_WIDTH } : undefined}
          >
            <div className="bg-auto-pop-title">
              {paused ? t.autoThumbsPausedHeading : t.autoThumbsHeading}
            </div>
            <p className="bg-auto-pop-body">{t.autoThumbsBody(shown.left)}</p>
            <div className="bg-auto-pop-actions">
              <button type="button" className="settings-plain-button" onClick={togglePause}>
                {paused ? t.autoThumbsResume : t.autoThumbsPause}
              </button>
              <button
                type="button"
                className="bg-auto-pop-link"
                onClick={() => {
                  setPopOpen(false);
                  onOpenSettings();
                }}
              >
                {t.autoThumbsOpenSettings}
              </button>
            </div>
          </div>
        )}
      </span>
    );
  }

  const label = shown.kind === "scan" ? t.bgScanning : t.bgThumbs(shown.left);
  // ヘッダーには短い形だけ（全文は title）。全文を並べると、出ている間ふつうの幅でも検索欄が半分ほどに縮んだ
  // （win の実機。2026-10-17 利用者の選択）。読み上げには載せない（数が毎秒変わる）
  const short = shown.kind === "scan" ? t.bgScanningShort : t.bgThumbsShort(shown.left);
  return (
    <span className="bg-work" title={label}>
      <span className="bg-spinner" aria-hidden />
      <span className="bg-text">{short}</span>
    </span>
  );
}
