/**
 * 線で描いた小さなアイコン。色付きの絵文字（📷 💾 📂 🔍 🗑）は OS と書体で形も色も変わり、
 * 「AI で作った既定の感じ」の元になっていた（2026-11-15 利用者、配色 A 案の後半）。
 * 24 の格子に、線の太さを揃えて自前で描く（外から借りた図形ではない。設定の歯車・ビューアの ← と同じ流儀）。
 * 線は 1.8。**離れた図形どうしの縁は、線の太さ＋2（3.8）以上空ける**——18px に縮めると線は約 1.35px になり、
 * それより詰めると隣の線とくっついて1つに見える（packDays で利用者が気づいた）。囲んだ図形の中の空きも同じく 2 以上
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
      {/* 仕切りは上寄り。下の段に灯りが線と離れて入る（16px・1x の画素で、真ん中の仕切りでは上下に地の行が残らなかった。win2） */}
      <rect x="3.5" y="7" width="17" height="10" rx="2" />
      <path d="M3.5 10.5h17" />
      <circle cx="16.5" cy="14" r="0.9" fill="currentColor" stroke="none" />
    </>
  ),
  // 外付け（USB・カード）: 差し込みの先と胴
  usb: (
    <>
      <rect x="7" y="9.5" width="10" height="11" rx="1.5" />
      <path d="M9 9.5V4h6v5.5M10.75 6.5v0.5M13.25 6.5v0.5" />
    </>
  ),
  // ネットワークの場所: 地球（経線 1 本と緯線 1 本）
  network: (
    <>
      <circle cx="12" cy="12" r="8" />
      <path d="M4 12h16M12 4c2.5 2.4 2.5 13.6 0 16M12 4c-2.5 2.4-2.5 13.6 0 16" />
    </>
  ),
  // 光学ドライブ: 円盤と穴
  disc: (
    <>
      <circle cx="12" cy="12" r="8" />
      <circle cx="12" cy="12" r="2" />
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
  // 再スキャン: 回る矢
  refresh: (
    <>
      <path d="M19 12a7 7 0 1 1-2.05-4.95" />
      <path d="M19 4.5V8h-3.5" />
    </>
  ),
  // キーの一覧: 鍵盤
  keyboard: (
    <>
      <rect x="3" y="6.5" width="18" height="11" rx="2" />
      <path d="M7 10h.01M10.33 10h.01M13.67 10h.01M17 10h.01M8 14h8" />
    </>
  ),
  // すべての写真: 額と山と日
  image: (
    <>
      <rect x="3.5" y="5" width="17" height="14" rx="2" />
      <circle cx="9" cy="10" r="1.5" />
      <path d="M4 17.5l5-4.5 3.5 3 3-2.5 4.5 4" />
    </>
  ),
  // 日付・カレンダー: 綴じ輪と日の段
  calendar: (
    <>
      <rect x="4" y="5.5" width="16" height="14" rx="2" />
      <path d="M4 10h16M8.5 3.5v4M15.5 3.5v4" />
    </>
  ),
  // 送り出し: 受け皿から上へ出る矢
  outgoing: (
    <>
      <path d="M4.5 14v4a1.5 1.5 0 0 0 1.5 1.5h12a1.5 1.5 0 0 0 1.5-1.5v-4" />
      <path d="M12 15V4.5M8 8.5l4-4 4 4" />
    </>
  ),
  // 思い出: フィルムの駒
  film: (
    <>
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M8 4v16M16 4v16M4 8h4M4 12h4M4 16h4M16 8h4M16 12h4M16 16h4" />
    </>
  ),
  // 撮った場所: 地図の針
  pin: (
    <>
      <path d="M12 20.5s-6-5.5-6-10.5a6 6 0 0 1 12 0c0 5-6 10.5-6 10.5z" />
      <circle cx="12" cy="10" r="2" />
    </>
  ),
  // クラウドにだけ在る: 雲
  cloud: <path d="M7 18.5h10.5a4 4 0 0 0 .6-7.95A5.5 5.5 0 0 0 7.4 9.6 4.5 4.5 0 0 0 7 18.5z" />,
  // ===== ビューアの道具（写真の上に白で出る。20px で描く前提）=====
  // 星（お気に入り）。付いているときは .filled で塗る
  star: (
    <path d="M12 3.5l2.6 5.3 5.8.8-4.2 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.2-4.1 5.8-.8z" />
  ),
  // 旗（選別で選ぶ）
  flag: (
    <>
      <path d="M6 21V4" />
      <path d="M6 4.5h11l-2.5 4 2.5 4H6" />
    </>
  ),
  // ×（ボツの候補）
  close: <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />,
  // 撮影情報: 丸の中の i
  info: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 11v5.5" />
      <circle cx="12" cy="7.8" r="0.4" fill="currentColor" />
    </>
  ),
  // スライドショー: 再生と一時停止
  play: <path d="M8 5.5v13l10.5-6.5z" />,
  pause: <path d="M9 5.5v13M15 5.5v13" />,
  // 写しを取る: 重なった 2 枚
  copy: (
    <>
      <rect x="8.5" y="8.5" width="11" height="11" rx="2" />
      <path d="M15.5 5.5V5a1.5 1.5 0 0 0-1.5-1.5H6A1.5 1.5 0 0 0 4.5 5v8A1.5 1.5 0 0 0 6 14.5h.5" />
    </>
  ),
  // 書き出して保存: 下向きの矢と受け皿（フロッピーは使わない）
  save: (
    <>
      <path d="M12 4v11M7.5 10.5L12 15l4.5-4.5" />
      <path d="M4.5 15.5v3a1.5 1.5 0 0 0 1.5 1.5h12a1.5 1.5 0 0 0 1.5-1.5v-3" />
    </>
  ),
  // フォルダで見る: 閉じたフォルダ
  folder: <path d="M3.5 18.5V6.5h5.5l2 2h9.5v10z" />,
  // キーの一覧: 丸の中の ?
  help: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M9.6 9.6a2.5 2.5 0 1 1 3.4 2.3c-.6.3-1 .8-1 1.5v.6" />
      <circle cx="12" cy="16.6" r="0.4" fill="currentColor" />
    </>
  ),
  // 全画面に: 四隅が外へ。戻す: 四隅が内へ
  fullscreen: <path d="M4.5 9V4.5H9M15 4.5h4.5V9M19.5 15v4.5H15M9 19.5H4.5V15" />,
  fullscreenExit: <path d="M9 4.5V9H4.5M19.5 9H15V4.5M15 19.5V15h4.5M4.5 15H9v4.5" />,
  // 動画の繰り返し: 回る2本の矢。1本だけは真ん中に 1
  repeat: <path d="M16.5 3.5l3 3-3 3M4.5 11.5V10a3.5 3.5 0 0 1 3.5-3.5h11.5M7.5 20.5l-3-3 3-3M19.5 12.5V14a3.5 3.5 0 0 1-3.5 3.5H4.5" />,
  repeatOne: (
    <>
      <path d="M16.5 3.5l3 3-3 3M4.5 11.5V10a3.5 3.5 0 0 1 3.5-3.5h11.5M7.5 20.5l-3-3 3-3M19.5 12.5V14a3.5 3.5 0 0 1-3.5 3.5H4.5" />
      <path d="M11 10.6l1.2-.6v4" />
    </>
  ),
  // 一覧で見る: 4 つの升
  grid: (
    <>
      <rect x="4.5" y="4.5" width="6" height="6" rx="1" />
      <rect x="13.5" y="4.5" width="6" height="6" rx="1" />
      <rect x="4.5" y="13.5" width="6" height="6" rx="1" />
      <rect x="13.5" y="13.5" width="6" height="6" rx="1" />
    </>
  ),
  // 少ない日を並べる: 2 つの日が横に並ぶ——日付の短い線と、その下の写真（横長と縦長。形が違うので別の日に読める）。
  // 写真どうしの縁は 4.5 離す（線を引いた後の地は 2.7。4 では 100% の画面で地が 1 画素だけになり、ダークの押した
  // 状態で詰まって見えた。win2）。3 つ並べると、18px では線がくっつくか、細くして中の空きが
  // 潰れて棒に見えた（利用者・/code-review）
  packDays: (
    <>
      <path d="M3 6h4M15.5 6h3" />
      <rect x="3" y="10" width="8" height="8" rx="1" />
      <rect x="15.5" y="10" width="5.5" height="8" rx="1" />
    </>
  ),
  // 注意: 三角と !
  warning: (
    <>
      <path d="M12 4L21 19.5H3z" />
      <path d="M12 10v4.5" />
      <circle cx="12" cy="17" r="0.4" fill="currentColor" />
    </>
  ),
  // ビューアを閉じる（一覧へ戻る）矢。右→左の行では .flip-rtl で裏返す
  back: <path d="M19 12H5M11 6l-6 6 6 6" />,
  // 前・次（右→左の行では .flip-rtl で裏返す）
  chevronPrev: <path d="M14.5 5.5L8 12l6.5 6.5" />,
  chevronNext: <path d="M9.5 5.5L16 12l-6.5 6.5" />,
} as const;

export type IconName = keyof typeof PATHS;

/** ドライブの種類から絵を選ぶ（左の欄と取り込みの画面で同じにする）。ネットワークと光学は
 *  「取り込み元らしくない」ことを示す */
export function driveIconName(drive: { kind: string; removable: boolean }): IconName {
  if (drive.kind === "network") return "network";
  if (drive.kind === "optical") return "disc";
  return drive.removable ? "usb" : "drive";
}

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
