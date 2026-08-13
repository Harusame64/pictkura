import { useEffect, useLayoutEffect, useRef, useState } from "react";

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

  useEffect(() => setAdjusted(pos), [pos]);

  // 実寸を測ってから画面内へ収める（描画前に位置を確定させ、ちらつきを防ぐ）
  useLayoutEffect(() => {
    if (!pos || !ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    const x = Math.min(pos.x, window.innerWidth - rect.width - 8);
    const y = Math.min(pos.y, window.innerHeight - rect.height - 8);
    setAdjusted({ x: Math.max(8, x), y: Math.max(8, y) });
  }, [pos]);

  useEffect(() => {
    if (!pos) return;
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // スクロールやリサイズで位置がずれるくらいなら閉じる
    window.addEventListener("click", close);
    window.addEventListener("resize", close);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("resize", close);
      window.removeEventListener("keydown", onKey);
    };
  }, [pos, onClose]);

  if (!pos) return null;

  return (
    <div
      ref={ref}
      className="context-menu"
      style={{ left: adjusted?.x ?? pos.x, top: adjusted?.y ?? pos.y }}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) => (
        <div key={`${item.label}-${i}`}>
          {item.separator && <div className="context-separator" />}
          <button
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
