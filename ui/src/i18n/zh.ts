/**
 * 簡体字中国語の辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **中国大陸の簡体字（zh-Hans）で書く**（2026-09-01 の判断）。中国語も
 * 「どの中国語か」を決めないと書けない言語で、**この辞書が触る範囲でも割れる**——
 * `视频`（大陸）/ `影片`（台湾）、`默认` / `預設`、`文件夹` / `資料夾`、
 * `缩略图` / `縮圖`、`回收站` / `資源回收筒`。**繁体字は `zh-hant.ts` にある**
 * （同じPRで足した。辞書は言語ごとに1ファイルなので、こちらを触らずに済んでいる）。
 * **ゴミ箱の語もそちらは別**——Windowsの繁体字は `資源回收筒` で、`垃圾桶` はmacOSの語。
 *
 * **語は発明せず、既に中国語圏で使われているものへ合わせる**（独語・西語辞書と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の簡体字版に合わせる**——Adobeは3状態を
 *   `留用` (P) / `排除` (X) / `取消标记` (U) と呼ぶ。pictkura の P / X / U は
 *   Lightroom と同じ配列なので、中国の写真の人には**最初から通じる語**になる
 * - **ただし ⚑ と ✕ の両方に出る文字列には `留用` を使わない**。`judgeUnflag` は
 *   `flashViewer(on ? "reject" : "unflag")`（`App.tsx`）から**✕を取り消したときにも出る**
 *   ので、⚑限定の語だと「付いていない⚑を外した」と言うことになる。ここと `U` の
 *   ショートカット行は**両方を覆う `标记`** にしてある（日英独が `判定` / `judgement` /
 *   `Markierung` と中立語なのと同じ理由。西語の `marca` に当たる）
 * - **OSの用語は引いてくる**。`空格`（Space）・`自动播放`（Windowsの自動再生）・
 *   `系统设置 → 隐私与安全性`（macOS）・Microsoft Store の
 *   `HEIF 图像扩展` / `HEVC 视频扩展`。ここは訳ではなく**正解が1つある場所**
 * - **金額は元に直す**（`数百円` → "a few dollars" → `几元`）。英語辞書と同じ扱いで直訳しない
 * - **漢字とラテン文字・数字の間は半角スペースを空ける**（中国語の組版慣習）。
 *   引用符は “ ”、読点は `、`、句点は `。`（全角）
 *
 * **ゴミ箱の呼び名はOSで割れる**——Windows と Linux は `回收站`、macOSは `废纸篓`。
 * 辞書はプラットフォーム別に持てるキー（`videoCodecNoteMac` など）以外は1つしかないので、
 * **`回收站` に寄せた**。配布の主戦場がWindowsで（MSI / NSIS、DLもそちら）、
 * macOSの利用者にも `回收站` は通じる。**分けるなら8キーと2行**——`menuDelete` /
 * `bulkDelete` / `deleteConfirm` / **`deleted`** / **`deletedSomeLeft`** /
 * `rejectGateTitle` / `rejectGateConfirm` / `rejectGateNote` と、
 * ショートカット一覧の X と右クリックの行。**`grep 回收站` で数えること**
 * ——削除のたびに出る `deleted` 系を落とすと、いちばん読まれる文字列が残る（ゲート2の指摘）。
 *
 * **`打开` / `关掉` は繁体字と揃えない。** 繁体字では釦の語（`開啟` / `關閉`）へ寄せたが、
 * 大陸のUIでは設定を「打开／关掉」と言うのが普通で、直すと**かえって硬くなる**。
 * `没有装` → `没有安装` だけは両方で直した（こちらはどちらの地域でも書き言葉が上）。
 *
 * **見出しの語がぶつからないようにする**（ゲート2の指摘）。3つが近い意味を持つ:
 *
 * - `navPlaces` = **`相册`**。中身は「すべての画像 / ★ / ⚑」＝実体のない眺めで、
 *   写真アプリのサイドバーではこれを相册と呼ぶ。**使えない語が3つある**——
 *   `位置` はGPSの撮影地に読まれ（`exifLocation` がある）、`图库` は
 *   `navLibraryFolders`（`图库文件夹`）と、`浏览` は `browse`（`浏览…` の釦）と
 *   **同じサイドバーの中でぶつかる**（2巡のゲート2で1つずつ出た）
 * - `navLibraryFolders` = **`图库文件夹`**。pictkura が見ている本物のフォルダ
 * - **写真.appの蔵書も `图库`**。`资料库` ではない——macOSの簡体字版で
 *   `图库` がその語で（`资料库` はミュージック側）、`.photoslibrary` の既定名も `图库`。
 *   **必ず「“照片”App 的」「照片管理应用的」を付けて**、pictkura の `图库文件夹` と混ぜない
 *   （西語で `biblioteca` と `fototeca` を分けたのと同じ問題を、修飾で分けている）
 *
 * **「読めない」ではなく「読まない」**——`emptyRootIsPackage` /
 * `emptyManagedLibrary` / `emptyRootIsManagedLibrary` の3つは `有意不读取` で、
 * 故障ではなく決めごとだと言い切る。**3つまとめて動かすこと**。
 * 4つ目の `emptyPhotoLibrary` だけは日英独西とも「既定では」なので
 * `默认不读取` にしてある——ここを揃えてはいけない。
 */
import { folderExample } from "./folderExample";
import type { Dict } from "./ja";

export const zh: Dict = {
  appName: "pictkura",
  viewThumbnails: "照片",
  viewCalendar: "日历",
  searchPlaceholder: "搜索文件、文件夹、相机、2019-08 或 year:2019",
  searchClear: "清除搜索 (Esc)",
  commandPalette: "命令面板",
  importFromUsb: "从 USB 导入",
  rescan: "重新扫描",
  size: "大小",
  itemsSuffix: "项",
  navPlaces: "相册",
  navAllPhotos: "全部照片",
  navFavorites: "★ 收藏",
  navPicked: "⚑ 已留用",
  navKinds: "类型",
  kindPhoto: "照片",
  kindRaw: "RAW",
  kindVideo: "视频",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "键盘快捷键 (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "显示键盘快捷键",
  shortcutGroups: [
    {
      title: "照片网格",
      keys: [
        ["Ctrl+K / ⌘K", "命令面板（跳转到日期或相机、搜索、导入）"],
        ["Ctrl+A / ⌘A", "选中当前搜索和筛选匹配的全部照片"],
        ["Shift + 单击", "选中从上次单击的照片到这一张之间的全部照片"],
        ["Ctrl + 单击", "增选或取消选中一张照片（macOS 上是 ⌘ + 单击）"],
        ["单击日期标题", "选中这一整天（再次单击取消）"],
        ["Esc", "退出选择"],
      ],
    },
    {
      title: "大图查看",
      keys: [
        ["← / →", "上一张 / 下一张"],
        ["P", "留用（加上 ⚑）。默认会接着跳到下一张"],
        ["X", "排除（✕）。关闭大图时一并移到回收站"],
        ["U", "取消这张照片的标记（清除 ⚑ 和 ✕）"],
        ["Ctrl+C / ⌘C", "把屏幕上的图片复制到剪贴板"],
        ["Ctrl+S / ⌘S", "把屏幕上的图片保存为文件"],
        ["F", "收藏（★）的开关"],
        ["I", "拍摄信息（相机、镜头、光圈、ISO、GPS）"],
        ["空格", "幻灯片放映。视频则是播放 / 暂停"],
        ["1 / 0", "实际大小 100% / 适合窗口"],
        ["F11", "全屏"],
        ["Esc", "关闭"],
      ],
    },
    {
      title: "鼠标（大图查看时）",
      keys: [
        ["双击", "实际大小 100% ⇔ 适合窗口"],
        ["滚轮", "放大 / 缩小"],
        ["拖动", "放大后移动画面"],
        ["右键单击", "打开 / 用其他应用打开 / 在文件夹中显示 / 移到回收站"],
        ["单击下方的胶片条", "跳到那张照片"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "相机与媒体",
  navLibraryFolders: "图库文件夹",
  navDrives: "驱动器",
  navAddFolder: "添加文件夹",
  add: "添加",
  browse: "浏览…",
  addFolderPlaceholder: folderExample("例如 ", "用户名"),
  pickLibraryFolder: "选择要添加到图库的文件夹",
  showMore: (n: number) => `另外 ${n} 台`,
  collapse: "收起",
  photosCount: (n: number) => `${n} 张`,
  memoriesTitle: (years: number) => `${years} 年前的今天`,
  viewerFavorite: "收藏 (F)",
  viewerPick: "留用 (P)",
  viewerUnpick: "取消留用 (U)",
  viewerPicked: "已留用的照片",
  judgeFav: "已收藏",
  judgeUnfav: "已取消收藏",
  judgePick: "已留用",
  judgeUnflag: "已取消标记",
  viewerReject: "排除 (X)",
  viewerRejected: "已排除",
  rejectChip: (n: number) => `✕ ${n}`,
  rejectChipTitle: "查看已排除的照片",
  rejectGateTitle: (n: number) => `将 ${n} 张照片移到回收站`,
  rejectGateNote: "可以从回收站还原（还原后也会回到列表中）",
  rejectGateRestore: "保留",
  rejectGateBack: "返回",
  rejectGateDiscard: "不删除直接关闭",
  rejectGateConfirm: (n: number) => `将 ${n} 张移到回收站`,
  rejectGateTrashing: (done: number, total: number) =>
    `正在移动… (${done} / ${total})`,
  updateFound: (v: string) => `有新版本 ${v}`,
  updateOpenPage: "打开下载页面",
  updateLater: "以后再说",
  updateCheckNow: "检查更新",
  updateChecking: "正在检查…",
  updateUpToDate: "已是最新版本",
  updateFailed: "无法检查",
  updateOnStart: "启动时检查新版本",
  updateOnStartNote:
    "会向 GitHub 查询最新的版本号（每天一次）。不会发送任何照片或文件名。关闭后，除了你按上面的“检查更新”，本应用不会进行任何联网通信。",
  viewerSlideshow: "幻灯片放映 (空格)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `把这张图片保存为文件 (${key})`,
  extractCopy: (key: string) => `把这张图片复制到剪贴板 (${key})`,
  extractSaveTitle: "保存图片",
  extractFilter: "图像",
  extractSaved: "已保存",
  extractCopied: "已复制",
  extractFailed: "无法取出这张图片",
  extractSameFile: "不能覆盖原文件",
  viewerExif: "拍摄信息 (I)",
  viewerFullscreen: "全屏 (F11)",
  viewerClose: "关闭 (Esc)",
  viewerPrev: "上一张 (←)",
  viewerNext: "下一张 (→)",
  viewerFitToScreen: "适合窗口 (0)",
  viewerActualSize: "实际大小 100% (1) —— 双击也可切换",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "应用内无法播放这种格式",
  videoMissing: "找不到这个文件（似乎已被移动或删除）",
  videoCloudOnly: "这个视频存放在云端",
  videoCloudOnlyNote:
    "在应用内播放会先开始下载，下载完成前什么也看不到。用默认应用打开，可以一边看进度一边取回。",
  videoFailed: "无法播放这个视频",
  videoOpenExternal: "用默认应用打开",
  videoCodecNote:
    "iPhone 等设备拍的视频用 HEVC（H.265）录制。播放需要操作系统的解码器，在 Windows 上是 Microsoft Store 里的付费扩展（几元）。",
  videoCodecNoteMac:
    "macOS 本身就能播放 HEVC，所以这可能是它不支持的录制格式。",
  videoCodecNoteOther:
    "这台设备上可能没有安装支持这种录制格式的解码器。",
  videoCodecHelp: "查看 HEVC 视频扩展（付费）",
  loading: "正在加载…",
  exifTitle: "拍摄信息",
  exifCamera: "相机",
  exifLens: "镜头",
  exifAperture: "光圈",
  exifShutter: "快门",
  exifIso: "ISO",
  exifFocal: "焦距",
  exifLocation: "拍摄地点",
  exifNone: "没有 EXIF 信息",
  paletteInput: "日期、相机、关键词或命令…",
  paletteNoResults: "没有匹配的结果",
  paletteGroupJumpDate: "跳转到日期",
  paletteGroupRecentDays: "最近的日期",
  paletteGroupCameras: "按相机筛选",
  paletteGroupSearch: "搜索",
  paletteGroupActions: "操作",
  paletteSearchFor: (q: string) => `搜索“${q}”`,
  paletteSearchHint: "文件名、文件夹、相机",
  paletteSelect: "选择",
  paletteRun: "执行",
  paletteCloseHint: "关闭",
  actionShowFavorites: "只显示收藏",
  actionShowPicked: "只显示已留用的照片",
  actionShowAll: "显示全部照片",
  actionCalendar: "日历视图",
  actionThumbnails: "照片网格",
  indexBuilding: "🔍 正在建立搜索索引… ",
  cameraScanning: "📷 正在读取相机信息… ",
  indexIncompleteWarning:
    "⚠ 搜索索引的建立中断了，搜索结果可能不完整（下次启动时接着建立）",
  indexProgressSuffix: "% —— 完成之前搜索结果可能不完整",
  removeRoot: (path: string) => `把 ${path} 从图库中移除`,
  importFrom: (path: string) => `从 ${path} 导入`,
  filterByCamera: (name: string) => `只显示用 ${name} 拍的照片`,
  jumpToYear: (year: number) => `跳转到 ${year} 年`,
  importing: (done: number, total: number) => `正在导入… ${done}/${total}`,
  importDone: (copied: number, skipped: number) =>
    `导入完成：复制 ${copied} 张，跳过 ${skipped} 张`,
  importFailed: (n: number) => `，失败 ${n} 张`,
  importIncomplete: " ⚠ 有文件夹无法读取（请先不要清空存储卡）",
  syncDone: (added: number, changed: number, removed: number) =>
    `新增 ${added}，变更 ${changed}，删除 ${removed}`,
  pickSource: "选择要导入的文件夹（USB / DCIM）",
  pickDestination: "选择复制到的文件夹",
  // 取り込みウィザード（第5部 段階E）
  wizardTitle: "导入",
  wizardSources: "导入来源",
  wizardOtherFolder: "其他文件夹…",
  wizardRefresh: "重新检测驱动器",
  wizardRemovable: "可移动",
  wizardNoDrives: "找不到驱动器",
  emptyTitle: "还没有照片",
  emptyTitleFailed: "无法显示列表",
  emptyTitleChecking: "正在确认",
  emptyTitleStartupFailed: "启动时的同步没有完成",
  emptyStartupFailed:
    "启动时运行的同步没有全部完成。即使有照片，也可能还没有进入列表。请按“重新扫描”。如果还是不行，请重新打开应用。",
  emptyTitleMissing: "有些位置找不到了",
  emptyTitleUnreadable: "有些位置打不开",
  emptyNoRoots:
    "还没有设置图库文件夹。请从存储卡导入，或者选择一个存放照片的文件夹。",
  emptyMissing: (names: string) =>
    `找不到这些位置：${names}。如果是外置硬盘，请连接好之后按“重新扫描”。`,
  emptyUnreadableMac: (names: string) =>
    `打不开这些位置：${names}。请在“系统设置 → 隐私与安全性”中允许 pictkura 访问相应的文件夹（桌面、文稿、外置硬盘等）。如果是网络上的文件夹，请确认连接正常之后按“重新扫描”。`,
  emptyUnreadableWin: (names: string) =>
    `打不开这些位置：${names}。请检查文件夹的访问权限。如果是网络驱动器，请确认连接正常之后按“重新扫描”。`,
  emptyUnreadableOther: (names: string) =>
    `打不开这些位置：${names}。请确认你有读取权限，然后按“重新扫描”。`,
  listSeparator: "、",
  andMore: (n: number) => `另外 ${n} 项`,
  emptyRootIsPackage:
    "图库文件夹里指定的是“照片”App 的图库本身。pictkura 有意不读取它的内容，所以这里永远不会出现照片。请重新选择一个存放照片的文件夹，或者从存储卡导入。",
  emptyPhotoLibrary:
    "除了“照片”App 的图库以外，没有找到能处理的内容。pictkura 默认不读取“照片”App 的内容（大部分原图在 iCloud 上，并不在这台 Mac 里）。请从存储卡导入，或者选择一个存放照片的文件夹。",
  emptyManagedLibrary:
    "除了照片管理应用的图库（“照片”App、iPhoto、Aperture 等）以外，没有找到能处理的内容。pictkura 有意不读取它们的内容——就算建立了索引，之后的改动也无法反映到索引，索引会一直停在旧的状态。请从存储卡导入，或者选择一个存放照片的文件夹。",
  emptyRootIsManagedLibrary:
    "图库文件夹里指定的是照片管理应用的图库本身（“照片”App、iPhoto、Aperture 等）。pictkura 有意不读取它的内容，所以这里永远不会出现照片。请重新选择一个存放照片的文件夹，或者从存储卡导入。",
  emptyAllExcluded: (names: string) =>
    `找到的内容都被排除设置跳过了（例如 ${names}）。可以在设置文件夹里的 pictkura.toml 中修改。`,
  emptyNothingHere:
    "还没有找到 pictkura 能处理的照片。请从存储卡导入，或者选择一个存放照片的文件夹。",
  calendarChecking: "正在确认…",
  emptyTitleStalled: "有些位置没有响应",
  emptyStalled: (names: string) =>
    `这些位置没有响应：${names}。如果其中有网络上的文件夹，请确认连接是否正常，然后按“重新扫描”。如果不再用它了，把它从图库文件夹中移除，其余位置的结果就会显示出来。`,
  emptyChecking:
    "还在查看文件夹。如果其中有网络上的文件夹，请确认连接正常之后按“重新扫描”。",
  emptyLoadFailed:
    "无法加载列表。原因显示在上方的横条里。请按“重新扫描”，或者重新打开应用。",
  wizardPickFolderHint: "在左侧选择文件夹，这里就会列出其中的照片",
  wizardNoImages: "这个文件夹里没有照片",
  wizardUnreadable: "无法读取这个文件夹（可能已经被拔出）",
  wizardCounting: "正在加载…",
  wizardSelectAll: "全选",
  wizardSelectNew: "只选未导入的",
  wizardClearSelection: "取消选择",
  wizardSelected: (n: number) => `已选择 ${n} 张`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "已导入（复制目标里有相同的文件）",
  wizardDestination: "复制到",
  wizardChangeDestination: "更改",
  wizardStructure: "归类方式",
  wizardImportButton: (n: number) => `导入 ${n} 张`,
  wizardImportAll: "把这个文件夹整个导入（包含下级文件夹）",
  wizardImportAllShort: "整个文件夹",
  wizardDeep: "包含下级文件夹",
  wizardDeepHint: "会把这个来源里的照片全部找出来（不知道放在哪里也没关系）",
  wizardScanning: "正在查找来源里的照片…",
  wizardTruncated: (n: number) =>
    `数量太多，只显示前 ${n} 张。要全部导入，请用“整个文件夹”`,
  wizardScanIncomplete: "⚠ 有文件夹无法读取（可能有遗漏）",
  decoderHeifNotice: (n: string) =>
    `⚠ 有 ${n} 张 HEIC/HEIF 照片在这台设备上无法生成缩略图（打开也显示不出来）。除了免费的“HEIF 图像扩展”，还需要用来解码像素的付费“HEVC 视频扩展”（几元）`,
  decoderHeifNoticeMac: (n: string) =>
    `⚠ 有 ${n} 张 HEIC/HEIF 照片在这台设备上无法生成缩略图（打开也显示不出来）`,
  decoderHeifNoticeOther: (n: string) =>
    `⚠ 有 ${n} 张 HEIC/HEIF 照片在这台设备上无法生成缩略图（打开也显示不出来）。这台设备上可能没有安装支持 HEIC/HEVC 的解码器`,
  decoderHeifHow: "HEIF 图像扩展（免费）",
  decoderHevcHow: "HEVC 视频扩展（付费）",
  decoderNoticeDismiss: "不再显示",
  wizardOfflineTitle:
    "这个文件在云端（这里不显示预览，导入时会下载）",
  wizardHideImported: "隐藏已导入的",
  wizardAllImported: "没有新的照片（这个文件夹里的照片都已经导入过了）",
  wizardHiddenCount: (n: number) => `已隐藏 ${n} 张导入过的照片`,
  wizardCopying: "正在导入",
  wizardEtaSeconds: (n: number) => `剩余约 ${n} 秒`,
  wizardEtaMinutes: (n: number) => `剩余约 ${n} 分钟`,
  wizardEtaCalculating: "正在估算剩余时间…",
  wizardCapped: (n: number) => `${n}+ 张`,
  wizardMoreFiles: (n: number) => `另外 ${n} 张（滚动可以显示）`,
  // ファイル操作
  menuOpen: "打开",
  menuOpenWith: (name: string) => `用 ${name} 打开`,
  menuOpenWithOther: "用其他应用打开…",
  menuReveal: "在文件夹中显示",
  menuDelete: "删除（移到回收站）",
  menuFavoriteOn: "添加到收藏",
  menuFavoriteOff: "取消收藏",
  pickEditor: "选择用来编辑的应用",
  deleteConfirm: (n: number) =>
    n === 1
      ? "要把这张照片移到回收站吗？"
      : `要把这 ${n} 张照片移到回收站吗？`,
  deleted: (n: number) => `已把 ${n} 张移到回收站`,
  deletedSomeLeft: (n: number, left: number) =>
    `已把 ${n} 张移到回收站（有 ${left} 张没有找到，保持原样）`,
  // 複数選択と一括操作
  selectItem: "选择",
  selectedCount: (n: number) => `已选择 ${n} 张`,
  selectAll: "全选",
  clearSelection: "取消选择 (Esc)",
  selectDay: "选中这一整天",
  bulkFavoriteOn: "添加到收藏",
  bulkFavoriteOff: "取消收藏",
  bulkDelete: "移到回收站",
  bulkCopy: "复制到文件夹",
  bulkMove: "移动到文件夹",
  bulkViewer: "查看选中的照片",
  pickExportFolder: "选择导出到的文件夹",
  moveConfirm: (n: number) =>
    n === 1
      ? "要把这张照片移动到接下来选择的文件夹吗？它会离开原来的位置，也会从图库中移出（★ 和 ⚑ 的标记不会带过去）。"
      : `要把这 ${n} 张照片移动到接下来选择的文件夹吗？它们会离开原来的位置，也会从图库中移出（★ 和 ⚑ 的标记不会带过去）。`,
  exporting: (done: number, total: number, name: string) =>
    `正在导出… ${done}/${total} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [`已导出 ${done} 张`];
    if (skipped > 0) parts.push(`有 ${skipped} 张已经存在`);
    if (failed > 0) parts.push(`有 ${failed} 张失败`);
    if (leftBehind > 0) parts.push(`有 ${leftBehind} 张无法从原来的位置删除`);
    return parts.join("，") + "。";
  },
  bulkPickOn: "留用",
  bulkPickOff: "取消留用",
  bulkPickDone: (n: number) => `已把 ${n} 张标记为留用`,
  bulkUnpickDone: (n: number) => `已取消 ${n} 张的留用标记`,
  bulkFavoriteDone: (n: number) => `已把 ${n} 张添加到收藏`,
  bulkUnfavoriteDone: (n: number) => `已取消 ${n} 张的收藏`,
  // 設定
  settings: "设置",
  close: "关闭",
  settingsTitle: "设置",
  settingsImportStructure: "导入后的文件夹结构",
  settingsImportStructureNote:
    "从 USB 导入的照片，按拍摄日期怎样归类。这会影响到几千张照片，以后再改要费相当大的工夫。文件夹名里带日期时，不论哪种语言都按年月日的顺序写——这样按名称排序就等于按时间排序。",
  settingsDestination: "复制到",
  settingsDestinationUnset: "（未设置：首次导入时选择）",
  settingsFlatExample: "IMG_0001.JPG（不分文件夹）",
  settingsCustomPattern: "自定义",
  settingsCustomPatternNote:
    "{year} {month} {day} 会替换成日期。用 / 分出层级。不能用的字符和指向上级文件夹的 .. 会自动去掉。",
  settingsCustomPatternResult: "生成的文件夹",
  settingsViewer: "全屏查看照片时",
  settingsAutoAdvanceToggle: "用 P / U 判定之后，跳到下一张照片",
  settingsAutoAdvanceNote:
    "全屏时按 P 会给照片加上 ⚑ 标记（和 ★ 收藏是两套），按 U 取消。打开这个设置后会接着显示下一张，挑选照片时一张只按一次键。关掉则停在同一张照片上。",
  settingsAutoplay: "插入 U 盘或 SD 卡时",
  settingsAutoplayToggle: "在“自动播放”的选项里提供 pictkura",
  settingsAutoplayNote:
    "会把 pictkura 加到 Windows“自动播放”的选项里。它不会自行启动。请注意，这个选项本身的文字是日语。用安装程序卸载时会自动取消注册，但便携版以及同一台电脑上其他用户的注册不在此列——那些情况下，请在删除 pictkura 之前先关掉这里。",
  settingsAbout: "关于",
  settingsAboutLicense: "以 MIT 许可证发布。",
  settingsManual: "使用手册",
  settingsOssLicenses: "使用的开源软件",
  settingsDocNotBundled: "（开发版本里没有附带）",
  settingsLanguage: "语言",
  settingsLanguageSystem: "跟随系统",
  settingsLanguageNote:
    "选好之后会重新加载界面。复制本身会在后台继续，但导入向导会关闭，看不到进度——所以请在导入结束之后再切换。",
  settingsTheme: "主题",
  themeSystem: "跟随系统",
  themeLight: "浅色",
  themeDark: "深色",
  settingsEditors: "用来编辑的应用",
  settingsEditorsNote: "会记住你在“用其他应用打开…”里选过的应用。",
  settingsForgetEditor: "从列表中移除",
  calendarEmpty: "没有照片",
  // ⚡爆速メーター
  speedPrefix: (sec: string) => `⚡ ${sec} 秒完成启动检查 —— `,
  speedUsn: "USN 日志差分：",
  speedUsnNoChange: "没有变更，没有遍历文件夹",
  speedUsnDirty: (records: number, dirs: number) =>
    `${records} 条日志 → 只重新遍历了 ${dirs} 个文件夹`,
  speedPruned: (skipped: number) => `剪枝扫描：跳过了 ${skipped} 个文件夹`,
  speedFull: (total: number) => `全量扫描（${total} 个文件）`,
  speedNoDiff: " —— 没有变更",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` —— 新增 ${added}、变更 ${changed}、删除 ${removed}`,
};
