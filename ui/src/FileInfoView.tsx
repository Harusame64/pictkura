/**
 * ファイルの基本情報の表示（`dev/adr.file-info-and-context-menu.md` の A）。ビューアのパネル・右クリックの「情報」の
 * 両方で**同じ行の並び**を使う。値は DB のもの（`get_file_info`）——ディスクに触らない
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { listen } from "@tauri-apps/api/event";

import { getFilesInfo, type FileInfo } from "./api";
import { extensionOf, formatSize, sizeBaseFor, splitPath } from "./fileInfo";
import {
  formatDateTime,
  formatDuration,
  formatLocale,
  formatNumber,
  t,
} from "./i18n";
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
    [t.fileInfoSize, formatSize(info.size, base, formatLocale)],
    [
      t.fileInfoTaken,
      info.taken_at_ms === null
        ? t.fileInfoTakenUnknown
        : formatDateTime(info.taken_at_ms),
    ],
    [t.fileInfoModified, formatDateTime(info.mtime_ms)],
  ];
  if (info.width !== null && info.height !== null) {
    rows.push([
      t.fileInfoDimensions,
      `${formatNumber(info.width)} × ${formatNumber(info.height)}`,
    ]);
  }
  if (info.duration_ms !== null)
    rows.push([t.fileInfoDuration, formatDuration(info.duration_ms)]);
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
 * 開いている間に記録が変わったら進む数（聞き直しの合図）。抽出が寸法を埋める・走査が大きさや日時を直す・
 * 外で消される——`media-updated`（その id）と `library-updated`。続けて来る知らせは 300ms まとめる。
 * ファイルの情報と選択の合計の両方で使う（PR の codex が2回、同じ形を別の場所で挙げた）
 */
export function useChangedCount(ids: Iterable<number> | null): number {
  const [count, setCount] = useState(0);
  // 全選択は数万件になる。並びの身元が同じ間は組み直さない（選択の Set は変わったときだけ作り直される）
  const key = useMemo(() => (ids === null ? "" : [...ids].join(",")), [ids]);
  useEffect(() => {
    if (key === "") return;
    const wanted = new Set(key.split(",").map(Number));
    let timer: number | undefined;
    const bump = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setCount((n) => n + 1), 300);
    };
    const offs = [
      listen<{ id: number }>("media-updated", (ev) => {
        if (wanted.has(ev.payload.id)) bump();
      }),
      listen("library-updated", bump),
    ];
    return () => {
      window.clearTimeout(timer);
      for (const off of offs) void off.then((f) => f()).catch(() => {});
    };
  }, [key]);
  return count;
}

/**
 * id の並びを**まとめて1回で**問い合わせる（連写は何百ファイルにもなる。PR の codex）。
 *
 * - **消えた・変わった後に届いた答えは捨てる**（`ids` が変わったら番号を進める）
 * - 開いている間に記録が変わったら聞き直す（[`useChangedCount`]）
 * - 失敗はその理由、DB に無い id は「見つからない」を、その id の行の代わりに出す（黙って欠けない）
 */
export function useFileInfos(ids: readonly number[] | null) {
  const [infos, setInfos] = useState<Map<number, FileInfo | string>>(new Map());
  const round = useRef(0);
  const key = ids === null ? "" : ids.join(",");
  const stale = useChangedCount(ids);
  useEffect(() => {
    const mine = ++round.current;
    if (ids === null) {
      setInfos(new Map());
      return;
    }
    const list = [...ids];
    getFilesInfo(list)
      .then((got) => {
        if (mine !== round.current) return;
        const m = new Map<number, FileInfo | string>(
          list.map((id) => [id, t.errNotFound]),
        );
        for (const info of got) m.set(info.id, info);
        setInfos(m);
      })
      .catch((e) => {
        if (mine !== round.current) return;
        const reason = errText(e);
        setInfos(new Map(list.map((id) => [id, reason])));
      });
    // `ids` の中身は `key` で見る（配列の身元が描くたびに変わっても問い合わせ直さない）
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, stale]);
  return infos;
}

/** 右クリックの「情報」。重ねのタイルはファイルごとに節を並べる（RAW と JPEG の大きさが両方見える） */
export function FileInfoDialog({
  ids,
  onClose,
}: {
  ids: readonly number[];
  onClose: () => void;
}) {
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
        {/* 名前の順に並べる（組の RAW と JPEG が組ごとに逆の順で並んでいた。win の実機、2026-10-16）。
            答えが届くまでは渡された順 */}
        {[...ids]
          .sort((a, b) => {
            const pa = infos.get(a);
            const pb = infos.get(b);
            return typeof pa === "object" && typeof pb === "object"
              ? pa.path.localeCompare(pb.path)
              : 0;
          })
          .map((id) => {
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
