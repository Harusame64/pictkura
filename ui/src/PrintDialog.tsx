/**
 * 右クリックの「印刷…」で、OS の印刷の画面より先に出す小さな画面（ADR B-3）。
 *
 * OS の印刷の画面には自分の選択肢を足せないので、載せ方（拡大／クロップ／用紙いっぱい）はここで選ぶ。
 * 前に選んだものを覚えておく。
 *
 * 選んだ写真をまとめて刷るとき（`sheetCount`。dev #55）は、1ページの枚数と、名前・撮影日を入れるかも選ぶ
 */
import { useState } from "react";

import { t } from "./i18n";
import { loadPrintFit, PRINT_FITS, savePrintFit, type PrintFit } from "./printPhoto";
import {
  loadSheetLayout,
  maxPhotosFor,
  PER_PAGE,
  saveSheetLayout,
  type SheetLayout,
} from "./printSheet";
import { answerKey, useWindowEvent } from "./useWindowEvent";

const LABELS: Record<PrintFit, () => [string, string]> = {
  contain: () => [t.printFitContain, t.printFitContainHint],
  cover: () => [t.printFitCover, t.printFitCoverHint],
  fill: () => [t.printFitStretch, t.printFitStretchHint],
};

export function PrintDialog({
  onPrint,
  onClose,
  sheetCount,
}: {
  /** `layout` はまとめて刷るときだけ渡る */
  onPrint: (fit: PrintFit, layout?: SheetLayout) => void;
  onClose: () => void;
  /** まとめて刷る写真の数（2枚以上）。無ければ1枚の印刷 */
  sheetCount?: number;
}) {
  const [fit, setFit] = useState<PrintFit>(loadPrintFit);
  const [layout, setLayout] = useState<SheetLayout>(loadSheetLayout);
  const sheet = sheetCount !== undefined;
  const tooMany = sheet && sheetCount > maxPhotosFor(layout.perPage);
  // 手前の幕として Esc を取る（`FileInfoDialog` と同じ。開くのはメニューを閉じた後）
  useWindowEvent("keydown", true, (e) => {
    if (e.key !== "Escape") return;
    if (!answerKey(e)) return;
    onClose();
  });
  const go = () => {
    if (tooMany) return;
    savePrintFit(fit);
    if (sheet) {
      saveSheetLayout(layout);
      onPrint(fit, layout);
    } else {
      onPrint(fit);
    }
  };
  return (
    <div className="file-info-backdrop" onClick={onClose}>
      <div
        className="file-info-dialog print-dialog"
        role="dialog"
        aria-label={t.printTitle}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="print-dialog-title">
          {sheet ? t.printSheetTitle(sheetCount) : t.printTitle}
        </div>
        {sheet && (
          <div className="print-sheet-options">
            <div className="print-per-page" role="radiogroup" aria-label={t.printPerPage}>
              <span className="print-per-page-label">{t.printPerPage}</span>
              {PER_PAGE.map((n) => (
                <button
                  key={n}
                  type="button"
                  role="radio"
                  aria-checked={layout.perPage === n}
                  className={layout.perPage === n ? "selected" : ""}
                  onClick={() => setLayout((l) => ({ ...l, perPage: n }))}
                >
                  {n}
                </button>
              ))}
            </div>
            <label className="print-captions">
              <input
                type="checkbox"
                checked={layout.captions}
                onChange={(e) => setLayout((l) => ({ ...l, captions: e.target.checked }))}
              />
              {t.printCaptions}
            </label>
          </div>
        )}
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
        {tooMany ? (
          <p className="print-dialog-note print-dialog-warn" role="alert">
            {t.printTooMany(maxPhotosFor(layout.perPage))}
          </p>
        ) : (
          !sheet && <p className="print-dialog-note">{t.printBorderless}</p>
        )}
        <div className="file-info-actions print-dialog-actions">
          <button onClick={onClose}>{t.confirmCancel}</button>
          <button className="primary" onClick={go} disabled={tooMany} autoFocus>
            {t.printGo}
          </button>
        </div>
      </div>
    </div>
  );
}
