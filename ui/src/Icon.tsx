/**
 * 線で描いた小さなアイコン。色付きの絵文字（📷 💾 📂 🔍 🗑）は OS と書体で形も色も変わり、
 * 「AI で作った既定の感じ」の元になっていた（2026-11-15 利用者、配色 A 案の後半）。
 * 24 の格子に、線の太さを揃えて自前で描く（外から借りた図形ではない。設定の歯車・ビューアの ← と同じ流儀）。
 * 色は `currentColor`——置いた場所の文字色に従う。読み上げには出さない（隣の文字が名前を言う）。
 */
const PATHS = {
  // 虫眼鏡: 円と柄
  search: (
    <>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="M15 15l5 5" />
    </>
  ),
  // カメラ: 胴とレンズ、上のでっぱり
  camera: (
    <>
      <path d="M4 8h3.5l1.5-2.5h6L16.5 8H20v11H4z" />
      <circle cx="12" cy="13" r="3.5" />
    </>
  ),
  // 内蔵の記憶装置: 平たい箱と、灯り
  drive: (
    <>
      <rect x="3.5" y="7" width="17" height="10" rx="2" />
      <path d="M3.5 13h17" />
      <circle cx="16.5" cy="15" r="0.6" fill="currentColor" />
    </>
  ),
  // 外付け（USB・カード）: 差し込みの先と胴
  usb: (
    <>
      <rect x="7" y="9.5" width="10" height="11" rx="1.5" />
      <path d="M9 9.5V4h6v5.5M10.75 6.5v0.5M13.25 6.5v0.5" />
    </>
  ),
  // 開いたフォルダ
  folderOpen: (
    <>
      <path d="M3.5 18.5V6.5h5.5l2 2h7v3" />
      <path d="M3.5 18.5l2.5-7h15l-2.5 7z" />
    </>
  ),
  // ごみ箱: 蓋と、縦の筋 2 本
  trash: (
    <>
      <path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10.5 11v5.5M13.5 11v5.5" />
    </>
  ),
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      className={"icon" + (className ? ` ${className}` : "")}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}
