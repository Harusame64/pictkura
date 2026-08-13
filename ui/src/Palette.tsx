import { useEffect, useMemo, useRef, useState } from "react";
import { type Camera, type DaySummary } from "./api";
import { formatDayKey, t } from "./i18n";

/** パレットの1候補。実行するとパレットは閉じる */
export type PaletteItem = {
  /** 種別ごとの見出し（グループ表示用） */
  group: string;
  label: string;
  hint?: string;
  icon: string;
  run: () => void;
};

export type PaletteProps = {
  open: boolean;
  onClose: () => void;
  /** タイムラインの骨組み（日付ジャンプの候補元） */
  summary: DaySummary[];
  cameras: Camera[];
  /** 日付ジャンプ */
  onJumpDay: (dayKey: number) => void;
  /** 検索を実行する（検索ボックスへ流し込む） */
  onSearch: (query: string) => void;
  /** アプリ操作（再スキャン等） */
  actions: PaletteItem[];
};

/** 候補の最大件数（種別ごと）。多すぎると選ぶのが遅くなる */
const MAX_PER_GROUP = 6;

/**
 * ⌘K / Ctrl+K のコマンドパレット。
 *
 * 「打ちながら決める」ための入口をひとつにまとめる:
 * 日付ジャンプ・カメラ絞り込み・全文検索・アプリ操作を同じ場所から実行できる。
 * 候補はすべて**すでにメモリにある骨組み（summary / cameras）**から作るので、
 * 打鍵ごとのIPCは発生しない（＝止まらない）。
 */
export default function Palette({
  open,
  onClose,
  summary,
  cameras,
  onJumpDay,
  onSearch,
  actions,
}: PaletteProps) {
  const [input, setInput] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // 開くたびに入力を空へ戻し、フォーカスを当てる
  useEffect(() => {
    if (!open) return;
    setInput("");
    setCursor(0);
    // マウント直後はまだ描画されていないので次フレームで当てる
    const t = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, [open]);

  const items = useMemo<PaletteItem[]>(() => {
    const q = input.trim();
    const lower = q.toLowerCase();
    const out: PaletteItem[] = [];

    // 1. 日付ジャンプ: 「2019」「2019年8」「8月11」等の部分一致
    if (q) {
      // 数字と年月日だけを含む入力を日付候補として扱う（"沖縄"では日付を出さない）
      const dateish = /^[0-9年月日/\-. ]+$/.test(q);
      if (dateish) {
        const digits = q.replace(/[^0-9]/g, "");
        for (const day of summary) {
          if (out.length >= MAX_PER_GROUP) break;
          const key = String(day.day_key);
          const label = formatDayKey(day.day_key);
          if (key.startsWith(digits) || label.includes(q)) {
            out.push({
              group: t.paletteGroupJumpDate,
              icon: "📅",
              label,
              hint: t.photosCount(day.count),
              run: () => onJumpDay(day.day_key),
            });
          }
        }
      }
    } else {
      // 入力前は直近の日を出す（「とりあえず開く」を最短にする）
      for (const day of summary.slice(0, 3)) {
        out.push({
          group: t.paletteGroupRecentDays,
          icon: "📅",
          label: formatDayKey(day.day_key),
          hint: t.photosCount(day.count),
          run: () => onJumpDay(day.day_key),
        });
      }
    }

    // 2. カメラで絞り込む
    const cameraHits = cameras
      .filter((c) => !q || c.name.toLowerCase().includes(lower))
      .slice(0, MAX_PER_GROUP);
    for (const cam of cameraHits) {
      out.push({
        group: t.paletteGroupCameras,
        icon: "📷",
        label: cam.name,
        hint: t.photosCount(cam.count),
        // 名前に空白を含むので引用符でくくる
        run: () => onSearch(`camera:"${cam.name}"`),
      });
    }

    // 3. 全文検索（入力があれば必ず候補に出す）
    if (q) {
      out.push({
        group: t.paletteGroupSearch,
        icon: "🔍",
        label: t.paletteSearchFor(q),
        hint: t.paletteSearchHint,
        run: () => onSearch(q),
      });
    }

    // 4. アプリ操作
    for (const action of actions) {
      if (!q || action.label.toLowerCase().includes(lower)) out.push(action);
    }
    return out;
  }, [input, summary, cameras, actions, onJumpDay, onSearch]);

  // 候補が入れ替わったら選択位置を範囲内へ収める
  useEffect(() => {
    setCursor((c) => Math.min(c, Math.max(0, items.length - 1)));
  }, [items.length]);

  // 選択中の候補を可視範囲へスクロールする
  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-idx="${cursor}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  if (!open) return null;

  const run = (item: PaletteItem | undefined) => {
    if (!item) return;
    onClose();
    item.run();
  };

  let lastGroup = "";
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder={t.paletteInput}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setCursor((c) => (items.length ? (c + 1) % items.length : 0));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setCursor((c) =>
                items.length ? (c - 1 + items.length) % items.length : 0,
              );
            } else if (e.key === "Enter") {
              e.preventDefault();
              run(items[cursor]);
            } else if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            }
          }}
        />
        <div className="palette-list" ref={listRef}>
          {items.length === 0 && (
            <div className="palette-empty">{t.paletteNoResults}</div>
          )}
          {items.map((item, i) => {
            const header = item.group !== lastGroup ? item.group : null;
            lastGroup = item.group;
            return (
              <div key={`${item.group}-${item.label}-${i}`}>
                {header && <div className="palette-group">{header}</div>}
                <div
                  data-idx={i}
                  className={"palette-item" + (i === cursor ? " active" : "")}
                  onMouseEnter={() => setCursor(i)}
                  onClick={() => run(item)}
                >
                  <span className="palette-icon">{item.icon}</span>
                  <span className="palette-label">{item.label}</span>
                  {item.hint && <span className="palette-hint">{item.hint}</span>}
                </div>
              </div>
            );
          })}
        </div>
        <div className="palette-foot">
          <kbd>↑</kbd>
          <kbd>↓</kbd> {t.paletteSelect}　<kbd>Enter</kbd> {t.paletteRun}　<kbd>Esc</kbd>{" "}
          {t.paletteCloseHint}
        </div>
      </div>
    </div>
  );
}
