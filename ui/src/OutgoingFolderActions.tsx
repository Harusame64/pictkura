/**
 * 送り出しフォルダの「パスをコピー」「フォルダを開く」（pictkura-dev #40）。設定と送り出しの画面で**同じ部品**を使う
 * ——2か所に写すと、片方で直した不具合（遅れて届いた失敗・消し時計の重なり）がもう片方に残った（#205 のゲート2）。
 *
 * - 「開く」はパスを渡さない（バックエンドが設定から求める。`open_google_folder`）。だから**いまのフォルダにだけ**出す
 * - 失敗はこの部品の中に出す。消えた後・パスが変わった後に届いた結果は捨てる
 */
import { useEffect, useRef, useState } from "react";

import { openGoogleFolder } from "./api";
import { copyText } from "./copyText";
import { t } from "./i18n";
import { errText } from "./i18n/err.ts";

export function OutgoingFolderActions({
  path,
  canOpen,
  disabled = false,
}: {
  path: string;
  /** いまの送り出しフォルダか（前のドライブのフォルダは写すだけ） */
  canOpen: boolean;
  disabled?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  /** 押した回の番号。パスが変わる・消えると進む——それより前の回の結果は捨てる */
  const round = useRef(0);
  useEffect(() => {
    round.current += 1;
    setError(null);
    setCopied(false);
  }, [path]);
  useEffect(
    () => () => {
      round.current += 1;
      window.clearTimeout(timer.current);
    },
    [],
  );

  const copy = async () => {
    const mine = ++round.current;
    setError(null);
    const ok = await copyText(path);
    if (mine !== round.current) return;
    if (ok) {
      setCopied(true);
      // 続けて押したら前の消し時計は止める（早く消えないように）
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 1500);
    } else {
      setError(t.settingsGoogleCopyFailed);
    }
  };

  const open = () => {
    const mine = ++round.current;
    setError(null);
    // 眠った外付けでは数秒かかる。待つ間にもう一度押されても窓を増やさない
    setOpening(true);
    openGoogleFolder()
      .catch((e) => mine === round.current && setError(errText(e)))
      .finally(() => setOpening(false));
  };

  return (
    <>
      <button className="settings-dest-change" disabled={disabled} onClick={() => void copy()}>
        {copied ? t.settingsGoogleCopied : t.settingsGoogleCopyPath}
      </button>
      {canOpen && (
        <button className="settings-dest-change" disabled={disabled || opening} onClick={open}>
          {t.settingsGoogleOpenFolder}
        </button>
      )}
      {error && <span className="folder-actions-error">{error}</span>}
    </>
  );
}
