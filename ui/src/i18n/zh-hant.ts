/**
 * 繁体字中国語の辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **台湾の用語（zh-Hant-TW）で書く**（2026-09-01 の判断）。繁体字も
 * 「どの繁体字か」を決めないと書けない——香港とは `影片` / `短片`、
 * `記憶卡` / `記憶咭` のように割れる。**香港版が要るようになったら
 * `zh-hant-hk.ts` を足せばよい**（このファイルは触らずに済む）。
 *
 * **コードは小文字の `zh-hant`。** `pickLocale()` はOSのタグを小文字にして引く
 * （`DICTS` の鍵が `zh-Hant` だとOSからは永久に当たらない）。OSが渡すのは
 * `zh-Hant-TW` や `zh-TW` だが、`pickLocale()` が書き言葉を補って後ろから
 * 1つずつ短くするので、**この1ファイルだけで台湾・香港・マカオに届く**。
 *
 * **簡体字の辞書（`zh.ts`）から機械的に変換していない。** 字を変えるだけでは
 * 別の言葉になる箇所がある:
 *
 * | 簡体字 | 繁体字（台湾） |
 * |---|---|
 * | `视频` | **`影片`** |
 * | `文件` / `文件夹` | **`檔案`** / **`資料夾`**（`文件` は台湾では「書類」） |
 * | `搜索` | **`搜尋`** |
 * | `默认` | **`預設`** |
 * | `设置` | **`設定`** |
 * | `应用` | **`應用程式`** |
 * | `单击` / `双击` | **`按一下`** / **`按兩下`** |
 * | `剪贴板` | **`剪貼簿`** |
 * | `窗口` / `屏幕` | **`視窗`** / **`螢幕`** |
 * | `驱动器` / `存储卡` | **`磁碟機`** / **`記憶卡`** |
 * | `向导` | **`精靈`** |
 *
 * **語は発明せず、既に台湾で使われているものへ合わせる**（他の辞書と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の繁体字版**——`留用` / `排除` / `取消標記`
 * - **⚑と✕の両方に出る文字列には `留用` を使わない**。`judgeUnflag` と `U` の行は
 *   中立の `標記`（簡体字・西語と同じ理由）
 * - **OSの用語は引いてくる**。`空白鍵`（Space）・`自動播放`（Windowsの自動再生）・
 *   `系統設定 → 隱私權與安全性`（macOS）・Microsoft Store の
 *   **`HEIF 影像延伸模組`** / **`HEVC 視訊延伸模組`**（`扩展` ではなく `延伸模組`）
 * - **引用符は 「 」**（簡体字の “ ” とは別。繁体字の組版はこちら）。
 *   漢字とラテン文字・数字の間は半角スペースを空ける
 *
 * **スライドショーもOSで割れる**——Windowsは `投影片放映`、macOSの写真.appは `幻燈片秀`。
 * ゴミ箱と同じ理由でWindows側に寄せた。**`幻燈片播放` は簡体字（`幻灯片放映`）を
 * 字だけ変えたもので、どちらのOSも使っていない**（ゲート2の指摘）。
 *
 * **ゴミ箱の呼び名はOSで割れる**——Windowsは `資源回收筒`、macOSは `垃圾桶`。
 * 簡体字辞書と同じ理由で**Windows側に寄せた**（配布の主戦場がWindows、
 * macOSの利用者にも通じる）。**分けるなら8キーと2行**——`menuDelete` /
 * `bulkDelete` / `deleteConfirm` / `deleted` / `deletedSomeLeft` /
 * `rejectGateTitle` / `rejectGateConfirm` / `rejectGateNote` と、
 * ショートカット一覧の X と右クリックの行（`grep 資源回收筒` で数える）。
 *
 * **見出しの語**は簡体字辞書と同じ分け方。`navPlaces` は `相簿`（`位置` はGPSの撮影地、
 * `圖庫` は `圖庫資料夾` と、`瀏覽` は `browse` の釦とぶつかる）、pictkura の蔵書は `圖庫資料夾`、写真.appの蔵書も
 * `圖庫` だが**必ず「照片」App 的 / 照片管理程式的 を付ける**。
 *
 * **金額は現地通貨へ。** `數十元` は台湾ドルの見当（HEVCの延伸模組はNT$30前後）。
 *
 * **「読めない」ではなく「読まない」**——`emptyRootIsPackage` /
 * `emptyManagedLibrary` / `emptyRootIsManagedLibrary` の3つは `刻意不讀取`。
 * 4つ目の `emptyPhotoLibrary` だけ「既定では」＝`預設不讀取`（他の辞書と同じ）。
 */
import { folderExample } from "./folderExample.ts";
import { num } from "./plural.ts";
import type { Dict } from "./ja.ts";

export const zhHant: Dict = {
  appName: "pictkura",
  viewThumbnails: "照片",
  viewCalendar: "行事曆",
  searchPlaceholder: "搜尋檔案、資料夾、相機、2019-08 或 year:2019",
  searchClear: "清除搜尋 (Esc)",
  commandPalette: "指令面板",
  importFromUsb: "從 USB 匯入",
  rescan: "重新掃描",
  size: "大小",
  /**
   * 一覧の件数。**数と単位を1つのキーにする**（2026-09-02、ゲート2の指摘）。
   * 呼ぶ側で `${formatNumber(n)} {t.itemsSuffix}` と組んでいたので、
   * **1枚に絞ると「1 items」「1 Objekte」**と出ていた——このPRが潰しに来た
   * `full scan (1 files)` と同じ壊れ方が、画面で一番目立つ数字に残っていた。
   * 単位だけのキーでは、どの言語も単数形を書けない。
   */
  itemsCount: (n: number) => `${num(n)} 項`,
  navPlaces: "相簿",
  navAllPhotos: "全部照片",
  navFavorites: "★ 最愛",
  navPicked: "⚑ 已留用",
  navKinds: "類型",
  kindPhoto: "照片",
  kindRaw: "RAW",
  kindVideo: "影片",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "鍵盤快速鍵 (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "顯示鍵盤快速鍵",
  shortcutGroups: [
    {
      title: "照片網格",
      keys: [
        ["Ctrl+K / ⌘K", "指令面板（跳到日期或相機、搜尋、匯入）"],
        ["Ctrl+A / ⌘A", "選取目前搜尋和篩選符合的全部照片"],
        ["Shift + 按一下", "選取從上次按的照片到這一張之間的全部照片"],
        ["Ctrl + 按一下", "加選或取消選取一張照片（macOS 上是 ⌘ + 按一下）"],
        ["按一下日期標題", "選取這一整天（再按一次取消）"],
        ["Esc", "結束選取"],
      ],
    },
    {
      title: "大圖檢視",
      keys: [
        ["← / →", "上一張 / 下一張"],
        ["P", "留用（加上 ⚑）。預設會接著跳到下一張"],
        ["X", "排除（✕）。關閉大圖時一併移到資源回收筒"],
        ["U", "取消這張照片的標記（清除 ⚑ 和 ✕）"],
        ["Ctrl+C / ⌘C", "把螢幕上的圖片複製到剪貼簿"],
        ["Ctrl+S / ⌘S", "把螢幕上的圖片儲存成檔案"],
        ["F", "最愛（★）的開關"],
        ["I", "拍攝資訊（相機、鏡頭、光圈、ISO、GPS）"],
        ["空白鍵", "投影片放映。影片則是播放 / 暫停"],
        ["1 / 0", "實際大小 100% / 符合視窗"],
        ["F11", "全螢幕"],
        ["Esc", "關閉"],
      ],
    },
    {
      title: "滑鼠（大圖檢視時）",
      keys: [
        ["按兩下", "實際大小 100% ⇔ 符合視窗"],
        ["滾輪", "放大 / 縮小"],
        ["拖曳", "放大後移動畫面"],
        ["按右鍵", "開啟 / 用其他程式開啟 / 在資料夾中顯示 / 移到資源回收筒"],
        ["按一下下方的底片條", "跳到那張照片"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "相機與媒體",
  navLibraryFolders: "圖庫資料夾",
  navDrives: "磁碟機",
  navAddFolder: "加入資料夾",
  add: "新增",
  browse: "瀏覽…",
  addFolderPlaceholder: folderExample("例如 ", "使用者名稱"),
  pickLibraryFolder: "選擇要加入圖庫的資料夾",
  showMore: (n: number) => `另外 ${num(n)} 台`,
  collapse: "收合",
  photosCount: (n: number) => `${num(n)} 張`,
  memoriesTitle: (years: number) => `${num(years)} 年前的今天`,
  viewerFavorite: "最愛 (F)",
  viewerPick: "留用 (P)",
  viewerUnpick: "取消留用 (U)",
  viewerPicked: "已留用的照片",
  judgeFav: "已加入最愛",
  judgeUnfav: "已從最愛移除",
  judgePick: "已留用",
  judgeUnflag: "已取消標記",
  viewerReject: "排除 (X)",
  viewerRejected: "已排除",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "查看已排除的照片",
  rejectGateTitle: (n: number) => `將 ${num(n)} 張照片移到資源回收筒`,
  rejectGateNote: "可以從資源回收筒還原（還原後也會回到清單中）",
  rejectGateRestore: "保留",
  rejectGateBack: "返回",
  rejectGateDiscard: "不刪除直接關閉",
  rejectGateConfirm: (n: number) => `將 ${num(n)} 張移到資源回收筒`,
  rejectGateTrashing: (done: number, total: number) =>
    `正在移動… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `有新版本 ${v}`,
  updateOpenPage: "開啟下載頁面",
  updateLater: "稍後再說",
  updateCheckNow: "檢查更新",
  updateChecking: "正在檢查…",
  updateUpToDate: "已是最新版本",
  updateFailed: "無法檢查",
  updateOnStart: "啟動時檢查新版本",
  updateOnStartNote:
    "會向 GitHub 查詢最新的版本號（每天一次）。不會傳送任何照片或檔名。關閉後，除了你按上面的「檢查更新」，本程式不會有任何網路連線。",
  viewerSlideshow: "投影片放映 (空白鍵)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `把這張圖片儲存成檔案 (${key})`,
  extractCopy: (key: string) => `把這張圖片複製到剪貼簿 (${key})`,
  extractSaveTitle: "儲存圖片",
  extractFilter: "影像",
  extractSaved: "已儲存",
  extractCopied: "已複製",
  extractFailed: "無法取出這張圖片",
  extractSameFile: "不能覆寫原檔案",
  viewerExif: "拍攝資訊 (I)",
  viewerFullscreen: "全螢幕 (F11)",
  viewerClose: "關閉 (Esc)",
  viewerPrev: "上一張 (←)",
  viewerNext: "下一張 (→)",
  viewerFitToScreen: "符合視窗 (0)",
  viewerActualSize: "實際大小 100% (1) —— 按兩下也可切換",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "程式內無法播放這種格式",
  videoMissing: "找不到這個檔案（似乎已被移動或刪除）",
  videoCloudOnly: "這部影片存放在雲端",
  videoCloudOnlyNote:
    "在程式內播放會先開始下載，下載完成前什麼也看不到。用預設的程式開啟，可以一邊看進度一邊取回。",
  videoFailed: "無法播放這部影片",
  videoOpenExternal: "用預設的程式開啟",
  videoCodecNote:
    "iPhone 等裝置拍的影片以 HEVC（H.265）錄製。播放需要作業系統的解碼器，在 Windows 上是 Microsoft Store 中需付費的延伸模組（數十元）。",
  videoCodecNoteMac:
    "macOS 本身就能播放 HEVC，所以這可能是它不支援的錄製格式。",
  videoCodecNoteOther:
    "這台裝置上可能沒有安裝支援這種錄製格式的解碼器。",
  videoCodecHelp: "查看 HEVC 視訊延伸模組（付費）",
  loading: "正在載入…",
  exifTitle: "拍攝資訊",
  exifCamera: "相機",
  exifLens: "鏡頭",
  exifAperture: "光圈",
  exifShutter: "快門",
  exifIso: "ISO",
  exifFocal: "焦距",
  exifLocation: "拍攝地點",
  exifNone: "沒有 EXIF 資訊",
  paletteInput: "日期、相機、關鍵字或指令…",
  paletteNoResults: "沒有符合的結果",
  paletteGroupJumpDate: "跳到日期",
  paletteGroupRecentDays: "最近的日期",
  paletteGroupCameras: "依相機篩選",
  paletteGroupSearch: "搜尋",
  paletteGroupActions: "操作",
  paletteSearchFor: (q: string) => `搜尋「${q}」`,
  paletteSearchHint: "檔名、資料夾、相機",
  paletteSelect: "選擇",
  paletteRun: "執行",
  paletteCloseHint: "關閉",
  actionShowFavorites: "只顯示最愛",
  actionShowPicked: "只顯示已留用的照片",
  actionShowAll: "顯示全部照片",
  actionCalendar: "行事曆檢視",
  actionThumbnails: "照片網格",
  indexBuilding: "🔍 正在建立搜尋索引… ",
  cameraScanning: "📷 正在讀取相機資訊… ",
  indexIncompleteWarning:
    "⚠ 搜尋索引的建立中斷了，搜尋結果可能不完整（下次啟動時接著建立）",
  indexProgressSuffix: "% —— 完成之前搜尋結果可能不完整",
  removeRoot: (path: string) => `把 ${path} 從圖庫中移除`,
  importFrom: (path: string) => `從 ${path} 匯入`,
  filterByCamera: (name: string) => `只顯示用 ${name} 拍的照片`,
  jumpToYear: (year: number) => `跳到 ${year} 年`,
  importing: (done: number, total: number) => `正在匯入… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `匯入完成：複製 ${num(copied)} 張，略過 ${num(skipped)} 張`,
  importFailed: (n: number) => `，失敗 ${num(n)} 張`,
  importIncomplete: " ⚠ 有資料夾無法讀取（請先不要清空記憶卡）",
  syncDone: (added: number, changed: number, removed: number) =>
    `新增 ${num(added)}，變更 ${num(changed)}，刪除 ${num(removed)}`,
  pickSource: "選擇要匯入的資料夾（USB / DCIM）",
  pickDestination: "選擇複製到的資料夾",
  // 取り込みウィザード（第5部 段階E）
  wizardTitle: "匯入",
  wizardSources: "匯入來源",
  wizardOtherFolder: "其他資料夾…",
  wizardRefresh: "重新偵測磁碟機",
  wizardRemovable: "卸除式",
  wizardNoDrives: "找不到磁碟機",
  emptyTitle: "還沒有照片",
  emptyTitleFailed: "無法顯示清單",
  emptyTitleChecking: "正在確認",
  emptyTitleStartupFailed: "啟動時的同步沒有完成",
  emptyStartupFailed:
    "啟動時執行的同步沒有全部完成。就算有照片，也可能還沒有進到清單裡。請按「重新掃描」。如果還是不行，請重新開啟程式。",
  emptyTitleMissing: "有些位置找不到了",
  emptyTitleUnreadable: "有些位置打不開",
  emptyNoRoots:
    "還沒有設定圖庫資料夾。請從記憶卡匯入，或者選擇一個存放照片的資料夾。",
  emptyMissing: (names: string) =>
    `找不到這些位置：${names}。如果是外接硬碟，請接好之後按「重新掃描」。`,
  emptyUnreadableMac: (names: string) =>
    `打不開這些位置：${names}。請在「系統設定 → 隱私權與安全性」中允許 pictkura 存取相應的資料夾（桌面、文件、外接硬碟等）。如果是網路上的資料夾，請確認連線正常之後按「重新掃描」。`,
  emptyUnreadableWin: (names: string) =>
    `打不開這些位置：${names}。請檢查資料夾的存取權限。如果是網路磁碟機，請確認連線正常之後按「重新掃描」。`,
  emptyUnreadableOther: (names: string) =>
    `打不開這些位置：${names}。請確認你有讀取權限，然後按「重新掃描」。`,
  listSeparator: "、",
  andMore: (n: number) => `另外 ${num(n)} 項`,
  emptyRootIsPackage:
    "圖庫資料夾裡指定的是「照片」App 的圖庫本身。pictkura 刻意不讀取它的內容，所以這裡永遠不會出現照片。請重新選擇一個存放照片的資料夾，或者從記憶卡匯入。",
  emptyPhotoLibrary:
    "除了「照片」App 的圖庫以外，沒有找到能處理的內容。pictkura 預設不讀取「照片」App 的內容（大部分原始檔在 iCloud 上，並不在這台 Mac 裡）。請從記憶卡匯入，或者選擇一個存放照片的資料夾。",
  emptyManagedLibrary:
    "除了照片管理程式的圖庫（「照片」App、iPhoto、Aperture 等）以外，沒有找到能處理的內容。pictkura 刻意不讀取它們的內容——就算建立了索引，之後的變更也無法反映到索引，索引會一直停在舊的狀態。請從記憶卡匯入，或者選擇一個存放照片的資料夾。",
  emptyRootIsManagedLibrary:
    "圖庫資料夾裡指定的是照片管理程式的圖庫本身（「照片」App、iPhoto、Aperture 等）。pictkura 刻意不讀取它的內容，所以這裡永遠不會出現照片。請重新選擇一個存放照片的資料夾，或者從記憶卡匯入。",
  emptyAllExcluded: (names: string) =>
    `找到的內容全都被排除設定略過了（例如 ${names}）。可以在設定資料夾裡的 pictkura.toml 中修改。`,
  emptyNothingHere:
    "還沒有找到 pictkura 能處理的照片。請從記憶卡匯入，或者選擇一個存放照片的資料夾。",
  calendarChecking: "正在確認…",
  emptyTitleStalled: "有些位置沒有回應",
  emptyStalled: (names: string) =>
    `這些位置沒有回應：${names}。如果其中有網路上的資料夾，請確認連線是否正常，然後按「重新掃描」。如果不再用它了，把它從圖庫資料夾中移除，其餘位置的結果就會顯示出來。`,
  emptyChecking:
    "還在查看資料夾。如果其中有網路上的資料夾，請確認連線正常之後按「重新掃描」。",
  emptyLoadFailed:
    "無法載入清單。原因顯示在上方的橫條裡。請按「重新掃描」，或者重新開啟程式。",
  wizardPickFolderHint: "在左邊選擇資料夾，這裡就會列出其中的照片",
  wizardNoImages: "這個資料夾裡沒有照片",
  wizardUnreadable: "無法讀取這個資料夾（可能已經被移除）",
  wizardCounting: "正在載入…",
  wizardSelectAll: "全選",
  wizardSelectNew: "只選未匯入的",
  wizardClearSelection: "取消選取",
  wizardSelected: (n: number) => `已選取 ${num(n)} 張`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "已匯入（複製的目的地裡有相同的檔案）",
  wizardDestination: "複製到",
  wizardChangeDestination: "變更",
  wizardStructure: "分類方式",
  wizardImportButton: (n: number) => `匯入 ${num(n)} 張`,
  wizardImportAll: "把這個資料夾整個匯入（包含下層資料夾）",
  wizardImportAllShort: "整個資料夾",
  wizardDeep: "包含下層資料夾",
  wizardDeepHint: "會把這個來源裡的照片全部找出來（不知道放在哪裡也沒關係）",
  wizardScanning: "正在尋找來源裡的照片…",
  wizardTruncated: (n: number) =>
    `數量太多，只顯示前 ${num(n)} 張。要全部匯入，請用「整個資料夾」`,
  wizardScanIncomplete: "⚠ 有資料夾無法讀取（可能有遺漏）",
  decoderHeifNotice: (n: number) =>
    `⚠ 有 ${num(n)} 張 HEIC/HEIF 照片在這台裝置上無法產生縮圖（開啟也顯示不出來）。除了免費的「HEIF 影像延伸模組」，還需要用來解碼像素的付費「HEVC 視訊延伸模組」（數十元）`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ 有 ${num(n)} 張 HEIC/HEIF 照片在這台裝置上無法產生縮圖（開啟也顯示不出來）`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ 有 ${num(n)} 張 HEIC/HEIF 照片在這台裝置上無法產生縮圖（開啟也顯示不出來）。這台裝置上可能沒有安裝支援 HEIC/HEVC 的解碼器`,
  decoderHeifHow: "HEIF 影像延伸模組（免費）",
  decoderHevcHow: "HEVC 視訊延伸模組（付費）",
  decoderNoticeDismiss: "不再顯示",
  wizardOfflineTitle:
    "這個檔案在雲端（這裡不顯示預覽，匯入時會下載）",
  wizardHideImported: "隱藏已匯入的",
  wizardAllImported: "沒有新的照片（這個資料夾裡的照片都已經匯入過了）",
  wizardHiddenCount: (n: number) => `已隱藏 ${num(n)} 張匯入過的照片`,
  wizardEtaSeconds: (n: number) => `剩餘約 ${num(n)} 秒`,
  wizardEtaMinutes: (n: number) => `剩餘約 ${num(n)} 分鐘`,
  wizardEtaCalculating: "正在估算剩餘時間…",
  wizardCapped: (n: number) => `${num(n)}+ 張`,
  wizardMoreFiles: (n: number) => `另外 ${num(n)} 張（捲動可以顯示）`,
  // ファイル操作
  menuOpen: "開啟",
  menuOpenWith: (name: string) => `用 ${name} 開啟`,
  menuOpenWithOther: "用其他程式開啟…",
  menuReveal: "在資料夾中顯示",
  menuDelete: "刪除（移到資源回收筒）",
  menuFavoriteOn: "加入最愛",
  menuFavoriteOff: "從最愛移除",
  pickEditor: "選擇用來編輯的程式",
  deleteConfirm: (n: number) =>
    n === 1
      ? "要把這張照片移到資源回收筒嗎？"
      : `要把這 ${num(n)} 張照片移到資源回收筒嗎？`,
  deleted: (n: number) => `已把 ${num(n)} 張移到資源回收筒`,
  deletedSomeLeft: (n: number, left: number) =>
    `已把 ${num(n)} 張移到資源回收筒（有 ${num(left)} 張沒有找到，保持原樣）`,
  // 複数選択と一括操作
  selectItem: "選取",
  selectedCount: (n: number) => `已選取 ${num(n)} 張`,
  selectAll: "全選",
  clearSelection: "取消選取 (Esc)",
  selectDay: "選取這一整天",
  bulkFavoriteOn: "加入最愛",
  bulkFavoriteOff: "從最愛移除",
  bulkDelete: "移到資源回收筒",
  bulkCopy: "複製到資料夾",
  bulkMove: "移動到資料夾",
  bulkViewer: "查看選取的照片",
  pickExportFolder: "選擇匯出到的資料夾",
  moveConfirm: (n: number) =>
    n === 1
      ? "要把這張照片移動到接下來選擇的資料夾嗎？它會離開原來的位置，也會從圖庫中移出（★ 和 ⚑ 的標記不會帶過去）。"
      : `要把這 ${num(n)} 張照片移動到接下來選擇的資料夾嗎？它們會離開原來的位置，也會從圖庫中移出（★ 和 ⚑ 的標記不會帶過去）。`,
  exporting: (done: number, total: number, name: string) =>
    `正在匯出… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [`已匯出 ${num(done)} 張`];
    if (skipped > 0) parts.push(`有 ${num(skipped)} 張已經存在`);
    if (failed > 0) parts.push(`有 ${num(failed)} 張失敗`);
    if (leftBehind > 0) parts.push(`有 ${num(leftBehind)} 張無法從原來的位置刪除`);
    return parts.join("，") + "。";
  },
  bulkPickOn: "留用",
  bulkPickOff: "取消留用",
  bulkPickDone: (n: number) => `已把 ${num(n)} 張標記為留用`,
  bulkUnpickDone: (n: number) => `已取消 ${num(n)} 張的留用標記`,
  bulkFavoriteDone: (n: number) => `已把 ${num(n)} 張加入最愛`,
  bulkUnfavoriteDone: (n: number) => `已把 ${num(n)} 張從最愛移除`,
  // 設定
  settings: "設定",
  close: "關閉",
  settingsTitle: "設定",
  settingsImportStructure: "匯入後的資料夾結構",
  settingsImportStructureNote:
    "從 USB 匯入的照片，要依拍攝日期如何分類。這會影響到幾千張照片，之後要改需要花費相當多的工夫。資料夾名稱裡帶日期時，不論哪種語言都一律依年月日的順序書寫——這樣依名稱排序就等於依時間排序。",
  settingsDestination: "複製到",
  settingsDestinationUnset: "（未設定：首次匯入時選擇）",
  settingsFlatExample: "IMG_0001.JPG（不分資料夾）",
  settingsCustomPattern: "自訂",
  settingsCustomPatternNote:
    "{year} {month} {day} 會換成日期。用 / 分出層級。不能使用的字元和指向上層資料夾的 .. 會自動去掉。",
  settingsCustomPatternResult: "產生的資料夾",
  settingsViewer: "全螢幕檢視照片時",
  settingsAutoAdvanceToggle: "用 P / U 判定之後，跳到下一張照片",
  settingsAutoAdvanceNote:
    "全螢幕時按 P 會給照片加上 ⚑ 標記（和 ★ 最愛是兩套），按 U 取消。開啟這個設定後會接著顯示下一張，挑選照片時一張只按一次鍵。關閉則停在同一張照片上。",
  settingsAutoplay: "插入 USB 隨身碟或 SD 記憶卡時",
  settingsAutoplayToggle: "在「自動播放」的選項中提供 pictkura",
  settingsAutoplayNote:
    "會把 pictkura 加到 Windows「自動播放」的選項裡。它不會自行啟動。請注意，這個選項本身的文字是日文。用安裝程式解除安裝時會自動取消註冊，但可攜版以及同一台電腦上其他使用者的註冊不在此列——那些情況下，請在刪除 pictkura 之前先關閉這裡。",
  settingsAbout: "關於",
  settingsAboutLicense: "以 MIT 授權發布。",
  settingsManual: "使用手冊",
  settingsOssLicenses: "使用的開放原始碼軟體",
  settingsDocNotBundled: "（開發版本中沒有附帶）",
  settingsLanguage: "語言",
  settingsLanguageSystem: "跟隨系統",
  settingsLanguageNote:
    "選好之後會重新載入畫面。複製本身會在背景繼續，但匯入精靈會關閉，看不到進度——所以請在匯入結束之後再切換。",
  settingsTheme: "主題",
  themeSystem: "跟隨系統",
  themeLight: "淺色",
  themeDark: "深色",
  settingsEditors: "用來編輯的程式",
  settingsEditorsNote: "會記住你在「用其他程式開啟…」裡選過的程式。",
  settingsForgetEditor: "從清單中移除",
  calendarEmpty: "沒有照片",
  // ⚡爆速メーター
  speedPrefix: (sec: string) => `⚡ ${sec} 秒完成啟動檢查 —— `,
  speedUsn: "USN 日誌差異：",
  speedUsnNoChange: "沒有變更，沒有走訪資料夾",
  speedUsnDirty: (records: number, dirs: number) =>
    `${num(records)} 筆日誌 → 只重新走訪了 ${num(dirs)} 個資料夾`,
  speedPruned: (skipped: number) => `剪枝掃描：略過了 ${num(skipped)} 個資料夾`,
  speedFull: (total: number) => `完整掃描（${num(total)} 個檔案）`,
  speedNoDiff: " —— 沒有變更",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` —— 新增 ${num(added)}、變更 ${num(changed)}、刪除 ${num(removed)}`,
};
