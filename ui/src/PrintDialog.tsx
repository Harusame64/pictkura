/**
 * 右クリックの「印刷…」で、OS の印刷の画面より先に出す小さな画面（ADR B-3）。
 *
 * OS の印刷の画面には自分の選択肢を足せないので、載せ方（拡大／クロップ／用紙いっぱい）はここで選ぶ。
 * 前に選んだものを覚えておく
 */
import { useState } from "react";

import { t } from "./i18n";
import { loadPrintFit, PRINT_FITS, savePrintFit, type PrintFit } from "./printPhoto";
import { answerKey, useWindowEvent } from "./useWindowEvent";

const LABELS: Record<PrintFit, () => [string, string]> = {
  contain: () => [t.printFitContain, t.printFitContainHint],
  cover: () => [t.printFitCover, t.printFitCoverHint],
  fill: () => [t.printFitStretch, t.printFitStretchHint],
};

export function PrintDialog({
  onPrint,
  onClose,
}: {
  onPrint: (fit: PrintFit) => void;
  onClose: () => void;
}) {
  const [fit, setFit] = useState<PrintFit>(loadPrintFit);
  // 手前の幕として Esc を取る（`FileInfoDialog` と同じ。開くのはメニューを閉じた後）
  useWindowEvent("keydown", true, (e) => {
    if (e.key !== "Escape") return;
    if (!answerKey(e)) return;
    onClose();
  });
  const go = () => {
    savePrintFit(fit);
    onPrint(fit);
  };
  return (
    <div className="file-info-backdrop" onClick={onClose}>
      <div
        className="file-info-dialog print-dialog"
        role="dialog"
        aria-label={t.printTitle}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="print-dialog-title">{t.printTitle}</div>
        <div role="radiogroup" aria-label={t.printTitle}>
          {PRINT_FITS.map((f) => {
            const [label, hint] = LABELS[f]();
            return (
              <label key={f} className={`print-fit${fit === f ? " selected" : ""}`}>
                <input
                  type="radio"
                  name="print-fit"
                  checked={fit === f}
                  onChange={() => setFit(f)}
                />
                <span className={`print-fit-sample print-fit-sample-${f}`} aria-hidden="true">
                  <span />
                </span>
                <span className="print-fit-text">
                  <span className="print-fit-label">{label}</span>
                  <span className="print-fit-hint">{hint}</span>
                </span>
              </label>
            );
          })}
        </div>
        <p className="print-dialog-note">{t.printBorderless}</p>
        <div className="file-info-actions print-dialog-actions">
          <button onClick={onClose}>{t.confirmCancel}</button>
          <button className="primary" onClick={go} autoFocus>
            {t.printGo}
          </button>
        </div>
      </div>
    </div>
  );
}
