import { useLayoutEffect, useRef, useState } from "react";
import { answerKey, useWindowEvent } from "./useWindowEvent";
import { isRtl } from "./direction";
import { menuMoveIndex } from "./menuMove";

/**
 * 直前の操作がキーボードだったか（メニューを開いたときに、フォーカスのある行を最初から示すかを決める）。
 * Shift+F10・メニューキーで開いた・Enter で入れ子を開いたときは示す、マウスで開いたときは示さない（OS のメニューと
 * 同じ）。捕まえ口は1度だけ張る
 */
let lastInputWasKey = false;
if (typeof window !== "undefined") {
  window.addEventListener("keydown", () => (lastInputWasKey = true), { capture: true });
  window.addEventListener("pointerdown", () => (lastInputWasKey = false), { capture: true });
}

/** 右クリックメニューの1項目。`separator` の前に区切り線を引く */
export type MenuItem = {
  /** 試験と自動操作の手がかり（`id="pk-menu-<id>"`・`data-testid="menu-<id>"`）。表示の言語で変わらない名前（pictkura-dev #65） */
  id?: string;
  /** 入り切りのある項目（★・⚑）の今の状態。あれば menuitemcheckbox として aria-checked で言う（表示の言葉に頼らない） */
  checked?: boolean;
  label: string;
  /** 危険な操作（削除）を赤字にする */
  danger?: boolean;
  /** この項目の前に区切り線を入れる */
  separator?: boolean;
  /** 右端に添えるショートカットの表示（「Ctrl+1」など）。読み上げには aria-keyshortcuts で渡す */
  hint?: string;
  /** aria-keyshortcuts の値（「Control+1」など）。hint と一緒に */
  keys?: string;
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
    // 右→左の画面では、カーソルの左へ開く（メニューは行の始まりの側から読む）
    const x = Math.min(isRtl() ? pos.x - rect.width : pos.x, window.innerWidth - rect.width - 8);
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

  // **開いたらメニューの中へフォーカスを移し（キーで開いたら先頭の項目、マウスならメニューそのもの）、閉じたら元へ戻す**。メニューとして名乗る（role="menu"）と、読み上げは
  // 上下の矢印で項目を動く前提で入ってくる——フォーカスが後ろに残ったままだと項目へ届かない（pictkura-dev #65 の
  // ゲート2）
  const isOpen = pos !== null;
  /**
   * いまキーで動いているか。キーなら、フォーカスのある行をホバーと同じ地で示し、止まった指の下のホバーは消す
   * （`.context-menu.keys`）。マウスなら、指の下の行だけを光らせる——どちらでも光る行は1つ。開くと先頭へフォーカスを
   * 移すので、ブラウザ既定の輪が「開く」に毎回付いていた（ダークでは暗い面に暗い輪で、どこにいるか分からなかった。
   * win2）。開いたときの値は、直前の操作がキーボードだったか
   */
  const [keys, setKeys] = useState(false);
  useLayoutEffect(() => {
    if (!isOpen) return;
    const byKey = lastInputWasKey;
    setKeys(byKey);
    const before = document.activeElement;
    // キーで開いたら先頭の項目へ（読み上げはここから入る）。マウスで開いたら、メニューそのものへ——光らないまま
    // 先頭に乗せると、最初の ↓ が2番目へ飛んで見えた（win2）。メニューで ↓ を受けて先頭から動ける
    (byKey ? ref.current?.querySelector<HTMLElement>('[role^="menuitem"]') : ref.current)?.focus({
      preventScroll: true,
    });
    return () => {
      // 後ろが inert になっていたら（項目が幕を開いた）戻さない——幕が閉じるときに App が戻す
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
    const list = [...(ref.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]') ?? [])];
    const at = list.indexOf(document.activeElement as HTMLElement);
    const next = menuMoveIndex(e.key, at, list.length);
    if (next === null) return;
    setKeys(true);
    e.preventDefault();
    e.stopPropagation();
    list[next].focus({ preventScroll: true });
  };
  // スクロールやリサイズで位置がずれるくらいなら閉じる
  useWindowEvent("resize", pos !== null, () => onClose());

  if (!pos) return null;

  return (
    <div
      ref={ref}
      className={"context-menu" + (keys ? " keys" : "")}
      // メニューとして見せる（UI Automation の Menu／MenuItem。pictkura-dev #65）
      role="menu"
      id="pk-context-menu"
      data-testid="context-menu"
      onKeyDown={onMenuKey}
      // 指が項目から外れたら、フォーカスはメニューそのものへ（光っていない行が Enter で動かないように。/code-review）。
      // 矢印はメニューで受けるので、そこから続けて動ける
      tabIndex={-1}
      style={{ left: adjusted?.x ?? pos.x, top: adjusted?.y ?? pos.y }}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) => (
        <div key={item.id ?? `${item.label}-${i}`} role="none">
          {item.separator && <div className="context-separator" role="separator" />}
          <button
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            id={item.id ? `pk-menu-${item.id}` : undefined}
            data-testid={item.id ? `menu-${item.id}` : undefined}
            className={"context-item" + (item.danger ? " danger" : "") + (item.hint ? " has-hint" : "")}
            aria-keyshortcuts={item.keys}
            // 指の下の行へフォーカスも移す——光る行がいつも1つになり、マウスからキーへ移っても続きから動ける
            onMouseMove={(e) => {
              if (keys) setKeys(false);
              if (document.activeElement !== e.currentTarget) e.currentTarget.focus({ preventScroll: true });
            }}
            onMouseLeave={(e) => {
              if (!keys && document.activeElement === e.currentTarget) ref.current?.focus({ preventScroll: true });
            }}
            onClick={() => {
              onClose();
              item.run();
            }}
          >
            {item.hint ? (
              <>
                <span className="context-label">{item.label}</span>
                <kbd className="context-hint" aria-hidden="true">
                  {item.hint}
                </kbd>
              </>
            ) : (
              item.label
            )}
          </button>
        </div>
      ))}
    </div>
  );
}
