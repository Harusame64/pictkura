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
 * メニューを開く前にフォーカスを持っていた要素。項目が幕（情報・印刷）を開くと、後ろが inert になってから
 * メニューが閉じるので、メニューの側では戻せない。幕を閉じたときの戻り先として App がここを読む（ゲート2）
 */
export const menuOpener: { el: HTMLElement | null } = { el: null };

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

  // **開いたら先頭の項目へフォーカスを移し、閉じたら元へ戻す**。メニューとして名乗る（role="menu"）と、読み上げは
  // 上下の矢印で項目を動く前提で入ってくる——フォーカスが後ろに残ったままだと項目へ届かない（pictkura-dev #65 の
  // ゲート2）
  const isOpen = pos !== null;
  useLayoutEffect(() => {
    if (!isOpen) return;
    const before = document.activeElement;
    menuOpener.el = before instanceof HTMLElement && before !== document.body ? before : null;
    ref.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus({ preventScroll: true });
    return () => {
      // 後ろが inert になっていたら（項目が幕を開いた）戻さない——戻り先は幕が閉じるときに App が使う
      if (before instanceof HTMLElement && before.isConnected && !before.closest("[inert]")) {
        before.focus({ preventScroll: true });
      }
    };
  }, [isOpen]);
  /** 上下・Home・End で項目を動く（端で回る） */
  const onMenuKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    // Tab はメニューを閉じる（メニューは Tab で項目を回らない。開いたまま後ろへフォーカスが出ないように。ゲート2）
    if (e.key === "Tab") {
      onClose();
      return;
    }
    const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
    if (!keys.includes(e.key)) return;
    const list = [...(ref.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    if (list.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const next =
      e.key === "Home"
        ? 0
        : e.key === "End"
          ? list.length - 1
          : (at + (e.key === "ArrowDown" ? 1 : -1) + list.length) % list.length;
    list[next].focus({ preventScroll: true });
  };
  // スクロールやリサイズで位置がずれるくらいなら閉じる
  useWindowEvent("resize", pos !== null, () => onClose());

  if (!pos) return null;

  return (
    <div
      ref={ref}
      className="context-menu"
      // メニューとして見せる（UI Automation の Menu／MenuItem。pictkura-dev #65）
      role="menu"
      onKeyDown={onMenuKey}
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
