import { useLayoutEffect, useRef, useState } from "react";
import { answerKey, useWindowEvent } from "./useWindowEvent";

/** 右クリックメニューの1項目。`separator` の前に区切り線を引く */
export type MenuItem = {
  label: string;
  /** 危険な操作（削除）を赤字にする */
  danger?: boolean;
  /** この項目の前に区切り線を入れる */
  separator?: boolean;
  run: () => void;
};

export type MenuPos = { x: number; y: number };

/**
 * 右クリックのコンテキストメニュー。
 *
 * OSのネイティブメニューではなくDOMで描くのは、Webview内の座標・テーマ・
 * 多言語がそのまま使えて、プラットフォーム差の分岐を持たずに済むため。
 * 画面外にはみ出す位置なら内側へ寄せる。
 */
export default function ContextMenu({
  pos,
  items,
  onClose,
}: {
  pos: MenuPos | null;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [adjusted, setAdjusted] = useState<MenuPos | null>(pos);

  // 実寸を測ってから画面内へ収める（描画前に位置を確定させ、ちらつきを防ぐ）。
  // **寄せた位置を後から上書きしない**——以前は `useEffect(() => setAdjusted(pos))` がこの後に走り、
  // 寄せる前の位置へ戻していた。窓の下のほうで開くと「情報」「削除」が画面の外に出て押せなかった
  // （win の実機、2026-10-16。項目が増えて表に出た）。項目の数が変われば高さも変わるので測り直す
  useLayoutEffect(() => {
    if (!pos || !ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    const x = Math.min(pos.x, window.innerWidth - rect.width - 8);
    const y = Math.min(pos.y, window.innerHeight - rect.height - 8);
    setAdjusted({ x: Math.max(8, x), y: Math.max(8, y) });
  }, [pos, items.length]);

  // **受け口は [`useWindowEvent`] 越しに張る**——**張り替えると、その1回が落ちる**
  // （あちらの節に理由）。
  //
  // **クリックも同じである。** ここで閉じたいのは「次に起きた1回」ではなく
  // **いま押されたその1回**——タイルを押すと React の受け口が先に状態を変え、
  // その描き直しで**こちらの受け口が外れて、その押しが届かなくなる**
  // （メニューが残り、もう1回押す羽目になる）。**リサイズも揃えておく。**
  useWindowEvent("keydown", pos !== null, (e) => {
    if (e.key !== "Escape") return;
    // **同じ押しに二度答えない**（`answerKey` の項）
    if (!answerKey(e)) return;
    onClose();
  });
  useWindowEvent("click", pos !== null, () => onClose());
  // スクロールやリサイズで位置がずれるくらいなら閉じる
  useWindowEvent("resize", pos !== null, () => onClose());

  if (!pos) return null;

  return (
    <div
      ref={ref}
      className="context-menu"
      // メニューとして見せる（UI Automation の Menu／MenuItem。pictkura-dev #65）
      role="menu"
      style={{ left: adjusted?.x ?? pos.x, top: adjusted?.y ?? pos.y }}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) => (
        <div key={`${item.label}-${i}`} role="none">
          {item.separator && <div className="context-separator" role="separator" />}
          <button
            role="menuitem"
            className={"context-item" + (item.danger ? " danger" : "")}
            onClick={() => {
              onClose();
              item.run();
            }}
          >
            {item.label}
          </button>
        </div>
      ))}
    </div>
  );
}
