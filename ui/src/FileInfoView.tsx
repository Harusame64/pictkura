/**
 * ファイルの基本情報の表示（`dev/adr.file-info-and-context-menu.md` の A）。ビューアのパネル・右クリックの「情報」の
 * 両方で**同じ行の並び**を使う。値は DB のもの（`get_file_info`）——ディスクに触らない
 */
import { useEffect, useRef, useState } from "react";

import { getFileInfo, type FileInfo } from "./api";
import { extensionOf, formatBytes, sizeBaseFor, splitPath } from "./fileInfo";
import { formatDateTime, formatDuration, formatLocale, formatNumber, t } from "./i18n";
import { errText } from "./i18n/err.ts";
import { usePlatform } from "./usePlatform";
import { answerKey, useWindowEvent } from "./useWindowEvent";

/** 1ファイルぶんの行。寸法が未読・読めない（null）なら欄ごと出さない */
export function FileInfoRows({ info }: { info: FileInfo }) {
  const base = sizeBaseFor(usePlatform());
  const { folder, name } = splitPath(info.path);
  const rows: [string, string][] = [
    [t.fileInfoName, name],
    [t.fileInfoFolder, folder],
    [t.fileInfoSize, formatBytes(info.size, base, formatLocale)],
    [t.fileInfoTaken, info.taken_at_ms === null ? t.fileInfoTakenUnknown : formatDateTime(info.taken_at_ms)],
    [t.fileInfoModified, formatDateTime(info.mtime_ms)],
  ];
  if (info.width !== null && info.height !== null) {
    rows.push([t.fileInfoDimensions, `${formatNumber(info.width)} × ${formatNumber(info.height)}`]);
  }
  if (info.duration_ms !== null) rows.push([t.fileInfoDuration, formatDuration(info.duration_ms)]);
  const ext = extensionOf(name);
  if (ext) rows.push([t.fileInfoType, ext]);
  return (
    <>
      {rows.map(([label, value]) => (
        <div className="exif-row" key={label}>
          <span className="exif-key">{label}</span>
          <span className="exif-value file-info-value">{value}</span>
        </div>
      ))}
    </>
  );
}

/**
 * id を1つずつ問い合わせる。**消えた・変わった後に届いた答えは捨てる**（`ids` が変わったら番号を進める）。
 * 1件でも失敗したら、その id の行の代わりに理由を出す（黙って欠けない）
 */
export function useFileInfos(ids: readonly number[] | null) {
  const [infos, setInfos] = useState<Map<number, FileInfo | string>>(new Map());
  const round = useRef(0);
  const key = ids === null ? "" : ids.join(",");
  useEffect(() => {
    const mine = ++round.current;
    setInfos(new Map());
    if (ids === null) return;
    for (const id of ids) {
      getFileInfo(id)
        .then((info) => info as FileInfo | string)
        .catch((e) => errText(e))
        .then((v) => {
          if (mine !== round.current) return;
          setInfos((m) => new Map(m).set(id, v));
        });
    }
    // `ids` の中身は `key` で見る（配列の身元が描くたびに変わっても問い合わせ直さない）
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return infos;
}

/** 右クリックの「情報」。重ねのタイルはファイルごとに節を並べる（RAW と JPEG の大きさが両方見える） */
export function FileInfoDialog({ ids, onClose }: { ids: readonly number[]; onClose: () => void }) {
  const infos = useFileInfos(ids);
  // 手前の幕として Esc を取る（`useWindowEvent` の項。開くのはメニューを閉じた後で、上に幕は生まれない）
  useWindowEvent("keydown", true, (e) => {
    if (e.key !== "Escape") return;
    if (!answerKey(e)) return;
    onClose();
  });
  return (
    <div className="file-info-backdrop" onClick={onClose}>
      <div
        className="file-info-dialog"
        role="dialog"
        aria-label={t.fileInfoTitle}
        onClick={(e) => e.stopPropagation()}
      >
        {ids.map((id) => {
          const v = infos.get(id);
          return (
            <section className="file-info-section" key={id}>
              {v === undefined ? (
                <div className="exif-row">{t.loading}</div>
              ) : typeof v === "string" ? (
                <div className="exif-row file-info-error">{v}</div>
              ) : (
                <FileInfoRows info={v} />
              )}
            </section>
          );
        })}
        <div className="file-info-actions">
          <button onClick={onClose} autoFocus>
            {t.close}
          </button>
        </div>
      </div>
    </div>
  );
}
