/**
 * UI文字列の多言語対応。
 *
 * 方針:
 * - ランタイムを増やさない（i18nライブラリを入れない）。辞書は素のオブジェクトで、
 *   バンドルに乗るのは選ばれた言語ではなく全言語だが、UI文字列は数KBなので
 *   起動時間には響かない（＝分割ロードの複雑さを買う理由がない）。
 * - 言語の追加は**辞書を1ブロック足し、`DICTS` と `LOCALES` に1行ずつ**。
 *   キーの型は日本語辞書から導出しているので、追加言語でキーの抜けがあれば
 *   コンパイルエラーになる。**`LOCALES` への追加を忘れると、OSの言語が
 *   それだったときは出るのに設定からは選べない**という半端な状態になる。
 * - **取扱説明書も忘れずに**。`docs/manual.<言語>.html` を置けば配布物には
 *   グロブで入る（CIが「リポジトリにある分が全部入っているか」を見る）が、
 *   どれを開くかは `Settings.tsx` の `manualDoc` が決めている。
 * - 日付・数値の書式は辞書に持たず `Intl` に任せる（全ロケールが無料で正しくなる）。
 * - 話者数の多い言語を優先。RTL（アラビア語等）はレイアウトの論理プロパティ化が
 *   済んでから追加する。
 */

/**
 * 「フォルダを追加」の入力例。**OSで綴りが違う**ので辞書に決め打てない
 * ——macOSに `D:` は無い。実測（2026-08-26）で、macOS版に
 * `例: D:\Pictures` がそのまま出ていた。
 *
 * `api.ts` の `isMac` と同じ判定をしているが、**あちらを import すると循環になる**
 * （`api.ts` はこの辞書の `locale` を使っている）。3つ目が要るときは
 * 判定だけを別のファイルへ出すこと。
 */
function folderExample(prefix: string, user: string): string {
  const data = (navigator as Navigator & { userAgentData?: { platform?: string } })
    .userAgentData;
  const platform = data?.platform ?? navigator.userAgent;
  if (/mac/i.test(platform)) return `${prefix}/Users/${user}/Pictures`;
  if (/win/i.test(platform)) return `${prefix}D:\\Pictures`;
  return `${prefix}/home/${user}/Pictures`;
}

/** 日本語辞書。これがキーの正（他言語はこの形に合わせる） */
const ja = {
  appName: "pictkura",
  viewThumbnails: "サムネイル",
  viewCalendar: "カレンダー",
  searchPlaceholder: "検索（ファイル名・フォルダ・カメラ・2019年8月・year:2019）",
  searchClear: "検索を消す (Esc)",
  commandPalette: "コマンドパレット",
  importFromUsb: "USBから取り込み",
  rescan: "再スキャン",
  size: "サイズ",
  itemsSuffix: "件",
  navPlaces: "画像の場所",
  navAllPhotos: "すべての画像",
  navFavorites: "★ お気に入り",
  navPicked: "⚑ 選別で選んだもの",
  // 種類の絞り込み（画像・RAW・動画）。押すと絞り、もう一度押すと外れる
  navKinds: "種類",
  kindPhoto: "画像",
  kindRaw: "RAW",
  kindVideo: "動画",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "ショートカット一覧 (?)",
  actionShortcuts: "ショートカット一覧を出す",
  shortcutGroups: [
    {
      title: "一覧",
      keys: [
        ["Ctrl+K / ⌘K", "コマンドパレット（日付・カメラへ飛ぶ、検索、取り込み）"],
        ["Ctrl+A / ⌘A", "いまの検索と絞り込みに一致するもの全部を選ぶ"],
        ["Shift + クリック", "最後に押したタイルからここまでをまとめて選ぶ"],
        ["Ctrl + クリック", "1枚を選ぶ・外す（macOSは ⌘ + クリック）"],
        ["日付の見出しを押す", "その日を全部選ぶ（もう一度で解除）"],
        ["Esc", "選択をやめる"],
      ],
    },
    {
      title: "写真を大きく見る",
      keys: [
        ["← / →", "前後の写真へ"],
        ["P", "選ぶ（⚑ を付ける）。既定では続けて次の写真へ"],
        ["X", "ボツの候補にする（✕）。閉じるときにまとめてゴミ箱へ"],
        ["U", "この写真への判定を取り消す（⚑ と ✕ を外す）"],
        ["F", "お気に入り（★）の付け外し"],
        ["I", "撮影情報（カメラ・レンズ・絞り・ISO・GPS）"],
        ["Space", "スライドショー。動画のときは再生／一時停止"],
        ["1 / 0", "等倍100% ／ 画面に合わせる"],
        ["F11", "全画面"],
        ["Esc", "閉じる"],
      ],
    },
    {
      title: "マウス（写真を大きく見ているとき）",
      keys: [
        ["ダブルクリック", "等倍100% ⇔ 画面に合わせる"],
        ["ホイール", "拡大・縮小"],
        ["ドラッグ", "拡大中に位置を動かす"],
        ["右クリック", "開く／他のアプリで開く／場所を表示／ゴミ箱へ"],
        ["下の帯をクリック", "前後の写真へ飛ぶ"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "カメラとメディア",
  navLibraryFolders: "ライブラリのフォルダ",
  navDrives: "ドライブ",
  navAddFolder: "フォルダを追加",
  add: "追加",
  browse: "参照…",
  addFolderPlaceholder: folderExample("例: ", "ユーザー名"),
  pickLibraryFolder: "ライブラリに追加するフォルダを選択",
  showMore: (n: number) => `他${n}台`,
  collapse: "閉じる",
  photosCount: (n: number) => `${n}枚`,
  memoriesTitle: (years: number) => `${years}年前の今日`,
  viewerFavorite: "お気に入り (F)",
  viewerPick: "選ぶ (P)",
  viewerUnpick: "選ぶのをやめる (U)",
  viewerPicked: "選別で選んだ写真",
  // 判定したことを**絵の上で**知らせる（2026-08-19の利用者指摘。道具の帯は
  // 1.8秒で消えるうえ、自動送りが効いていると付けた相手はもう画面に居ない）
  judgeFav: "お気に入り",
  judgeUnfav: "お気に入りを外した",
  judgePick: "選んだ",
  judgeUnflag: "判定を取り消した",
  // ボツの候補（0.2 ③）。印を付けるだけで、ファイルは閉じるときまで動かない
  viewerReject: "ボツにする (X)",
  viewerRejected: "ボツの候補",
  rejectChip: (n: number) => `✕ ${n}`,
  rejectChipTitle: "ボツの候補を確かめる",
  rejectGateTitle: (n: number) =>
    n === 1 ? "1枚をゴミ箱へ移動します" : `${n}枚をゴミ箱へ移動します`,
  rejectGateNote: "ゴミ箱から戻せます（戻すと一覧にも戻ります）",
  rejectGateRestore: "戻す",
  rejectGateBack: "戻る",
  rejectGateDiscard: "入れずに閉じる",
  rejectGateConfirm: (n: number) => `${n}枚をゴミ箱へ`,
  rejectGateTrashing: (done: number, total: number) =>
    `移動中… (${done} / ${total})`,
  // 新しいバージョンの確認（0.2）。**このアプリで唯一の外向き通信**なので、
  // 何を送っていないかまで書く
  updateFound: (v: string) => `新しいバージョン ${v} が出ています`,
  updateOpenPage: "ダウンロードページを開く",
  updateLater: "あとで",
  updateCheckNow: "更新を確認",
  updateChecking: "確認中…",
  updateUpToDate: "最新です",
  updateFailed: "確認できませんでした",
  updateOnStart: "起動時に新しいバージョンを確認する",
  updateOnStartNote:
    "GitHubに最新のバージョンを聞きに行きます（1日1回）。写真もファイル名も送りません。切ると、上の「更新を確認」を押したとき以外は一切通信しません",
  viewerSlideshow: "スライドショー (Space)",
  viewerExif: "撮影情報 (I)",
  viewerFullscreen: "フルスクリーン (F11)",
  viewerClose: "閉じる (Esc)",
  viewerPrev: "前へ (←)",
  viewerNext: "次へ (→)",
  viewerFitToScreen: "画面に合わせる (0)",
  viewerActualSize: "等倍100%で表示 (1)　※ダブルクリックでも切替",
  actualSizeBadge: "等倍",
  // 動画（第9部）
  videoUnsupported: "この形式はアプリ内で再生できません",
  videoMissing: "このファイルが見つかりません（移動または削除されたようです）",
  videoCloudOnly: "この動画はクラウドにあります",
  videoCloudOnlyNote:
    "アプリ内で再生するとダウンロードが始まり、終わるまで何も映りません。既定のアプリで開くと、進み具合を見ながら取り寄せられます。",
  videoFailed: "この動画を再生できませんでした",
  videoOpenExternal: "既定のアプリで開く",
  videoCodecNote:
    "iPhoneなどの動画はHEVC（H.265）で記録されています。再生にはOSのデコーダが必要で、Windowsでは有料の拡張機能（数百円）になります。",
  /** macOS。**買わせる話をしない**——OSがHEVCを最初から再生できる */
  videoCodecNoteMac:
    "この動画は表示に使っているブラウザ部品が再生できませんでした。macOSはHEVCを最初から再生できるので、対応していない記録方式である可能性が高いです。「既定のアプリで開く」から再生してください。",
  /**
   * それ以外（Linux）。**デコーダがあるとは言い切らない**——
   * ディストリによってはHEVCのデコーダが入っていない（`video.rs` のとおり同梱しない）
   */
  videoCodecNoteOther:
    "この動画は表示に使っているブラウザ部品が再生できませんでした。この記録方式に対応するデコーダが環境に無いのかもしれません。「既定のアプリで開く」から再生してください。",
  videoCodecHelp: "HEVC拡張機能を見る（有料）",
  loading: "読み込み中…",
  exifTitle: "撮影情報",
  exifCamera: "カメラ",
  exifLens: "レンズ",
  exifAperture: "絞り",
  exifShutter: "シャッター",
  exifIso: "ISO",
  exifFocal: "焦点距離",
  exifLocation: "撮影地",
  exifNone: "EXIF情報がありません",
  paletteInput: "日付・カメラ・キーワード・操作…",
  paletteNoResults: "候補がありません",
  paletteGroupJumpDate: "日付へジャンプ",
  paletteGroupRecentDays: "最近の日",
  paletteGroupCameras: "カメラで絞り込む",
  paletteGroupSearch: "検索",
  paletteGroupActions: "操作",
  paletteSearchFor: (q: string) => `「${q}」を検索`,
  paletteSearchHint: "ファイル名・フォルダ・カメラ",
  paletteSelect: "選択",
  paletteRun: "実行",
  paletteCloseHint: "閉じる",
  actionShowFavorites: "お気に入りだけ表示",
  actionShowPicked: "選別で選んだものだけ表示",
  actionShowAll: "すべての画像を表示",
  actionCalendar: "カレンダー表示",
  actionThumbnails: "サムネイル表示",
  indexBuilding: "🔍 検索インデックスを作成中… ",
  cameraScanning: "📷 カメラ情報を読み取り中… ",
  indexIncompleteWarning:
    "⚠ 検索インデックスの作成を中断しました ／ 検索結果が一部欠けています（次の起動で続きから作り直します）",
  indexProgressSuffix: "％ ／ 完了までは検索結果が一部欠けます",
  removeRoot: (path: string) => `${path} をライブラリから外す`,
  importFrom: (path: string) => `${path} から取り込む`,
  filterByCamera: (name: string) => `${name} で撮った写真だけを表示`,
  jumpToYear: (year: number) => `${year}年へ`,
  importing: (done: number, total: number) => `取り込み中… ${done}/${total}`,
  importDone: (copied: number, skipped: number) =>
    `取り込み完了: コピー${copied} スキップ${skipped}`,
  importFailed: (n: number) => ` 失敗${n}`,
  importIncomplete: " ⚠読み取れないフォルダあり（カードを消去しないでください）",
  syncDone: (added: number, changed: number, removed: number) =>
    `追加${added} 変更${changed} 削除${removed}`,
  pickSource: "取り込み元フォルダ（USB/DCIM）を選択",
  pickDestination: "コピー先フォルダを選択",
  // 取り込みウィザード（第5部 段階E）
  wizardTitle: "取り込み",
  wizardSources: "取り込み元",
  wizardOtherFolder: "他のフォルダ…",
  wizardRefresh: "ドライブを再検出",
  wizardRemovable: "リムーバブル",
  wizardNoDrives: "ドライブが見つかりません",
  wizardPickFolderHint: "左からフォルダを選ぶと、この中の画像が並びます",
  wizardNoImages: "このフォルダに画像はありません",
  wizardUnreadable: "フォルダを読めませんでした（取り外された可能性があります）",
  wizardCounting: "読み込み中…",
  wizardSelectAll: "すべて選択",
  wizardSelectNew: "未取り込みだけ選択",
  wizardClearSelection: "選択を解除",
  wizardSelected: (n: number) => `${n}枚を選択中`,
  wizardImportedBadge: "済",
  wizardImportedTitle: "取り込み済み（コピー先に同じファイルがあります）",
  wizardDestination: "コピー先",
  wizardChangeDestination: "変更",
  wizardStructure: "振り分け",
  wizardImportButton: (n: number) => `${n}枚を取り込む`,
  wizardImportAll: "このフォルダを丸ごと取り込む（下の階層も含む）",
  wizardImportAllShort: "フォルダごと",
  wizardDeep: "下の階層も含める",
  wizardDeepHint: "メディアの中を全部さらって並べます（どこに入っているか分からなくてもOK）",
  wizardScanning: "メディアの中を探しています…",
  wizardTruncated: (n: number) =>
    `多すぎるため先頭${n}枚だけ表示しています。全部入れるなら「フォルダごと」をどうぞ`,
  wizardScanIncomplete: "⚠読み取れないフォルダがありました（取りこぼしの可能性があります）",
  decoderHeifNotice: (n: number) =>
    `⚠ HEIC/HEIF ${n.toLocaleString()}枚は、この環境では絵を作れません。無料のHEIF拡張機能に加え、画素の展開に有料のHEVC拡張機能（数百円）が要ります`,
  /** Windows以外。**買わせる話をしない**——macOSはImageIOが最初から読める */
  decoderHeifNoticeOther: (n: number) =>
    `⚠ HEIC/HEIF ${n.toLocaleString()}枚は、この環境では絵を作れませんでした`,
  decoderHeifHow: "HEIF拡張機能（無料）",
  decoderHevcHow: "HEVC拡張機能（有料）",
  decoderNoticeDismiss: "今後表示しない",
  wizardOfflineTitle:
    "クラウド上のファイルです（この場では絵を出しません。取り込むとダウンロードされます）",
  wizardHideImported: "取り込み済みを隠す",
  wizardAllImported: "新しい写真はありません（このフォルダはすべて取り込み済みです）",
  wizardHiddenCount: (n: number) => `取り込み済み${n}枚は隠しています`,
  wizardCopying: "取り込み中",
  wizardEtaSeconds: (n: number) => `残り約${n}秒`,
  wizardEtaMinutes: (n: number) => `残り約${n}分`,
  wizardEtaCalculating: "残り時間を見積もっています…",
  wizardCapped: (n: number) => `${n}+枚`,
  wizardMoreFiles: (n: number) => `ほか${n}枚（スクロールで表示）`,
  // ファイル操作
  menuOpen: "開く",
  menuOpenWith: (name: string) => `${name} で開く`,
  menuOpenWithOther: "他のアプリで開く…",
  menuReveal: "ファイルの場所を開く",
  menuDelete: "削除（ゴミ箱へ）",
  menuFavoriteOn: "お気に入りに追加",
  menuFavoriteOff: "お気に入りを外す",
  pickEditor: "編集に使うアプリを選択",
  deleteConfirm: (n: number) =>
    n === 1
      ? "この写真をゴミ箱へ移動しますか？"
      : `${n}枚の写真をゴミ箱へ移動しますか？`,
  deleted: (n: number) => `${n}枚をゴミ箱へ移動しました`,
  // 関所に並べた数より少なかったとき。**黙って減らさない**（ゲート2の指摘）
  deletedSomeLeft: (n: number, left: number) =>
    `${n}枚をゴミ箱へ移動しました（${left}枚は見つからず残しました）`,
  // 複数選択と一括操作
  selectItem: "選択",
  selectedCount: (n: number) => `${n}枚を選択中`,
  selectAll: "すべて選択",
  clearSelection: "選択を解除 (Esc)",
  selectDay: "この日をまとめて選ぶ",
  bulkFavoriteOn: "お気に入りに追加",
  bulkFavoriteOff: "お気に入りを外す",
  bulkDelete: "ゴミ箱へ",
  bulkCopy: "フォルダへコピー",
  bulkMove: "フォルダへ移動",
  bulkViewer: "選んだぶんを見る",
  pickExportFolder: "書き出し先のフォルダを選ぶ",
  moveConfirm: (n: number) =>
    n === 1
      ? "この写真を、このあと選ぶフォルダへ移動しますか？ 元の場所からは無くなり、ライブラリからも外れます（★と⚑の印は引き継がれません）。"
      : `${n}枚の写真を、このあと選ぶフォルダへ移動しますか？ 元の場所からは無くなり、ライブラリからも外れます（★と⚑の印は引き継がれません）。`,
  exporting: (done: number, total: number, name: string) =>
    `書き出し中… ${done}/${total} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [`${done}枚を書き出しました`];
    if (skipped > 0) parts.push(`${skipped}枚は同じものが既にありました`);
    if (failed > 0) parts.push(`${failed}枚は失敗しました`);
    if (leftBehind > 0) parts.push(`${leftBehind}枚は元を消せませんでした`);
    return parts.join("。");
  },
  bulkPickOn: "選ぶ",
  bulkPickOff: "選ぶのをやめる",
  bulkPickDone: (n: number) => `${n}枚を選別で選びました`,
  bulkUnpickDone: (n: number) => `${n}枚の選別の印を外しました`,
  bulkFavoriteDone: (n: number) => `${n}枚をお気に入りに追加しました`,
  bulkUnfavoriteDone: (n: number) => `${n}枚のお気に入りを外しました`,
  // 設定
  settings: "設定",
  close: "閉じる",
  settingsTitle: "設定",
  settingsImportStructure: "取り込み先のフォルダ構成",
  settingsImportStructureNote:
    "USBから取り込んだ写真を、撮影日でどう振り分けるか。数千枚に効くので後から変えにくい設定です。フォルダ名に日付を入れるときは、言語を問わず年月日の順にします——名前順に並べたときに時系列になるためです。",
  settingsDestination: "コピー先",
  settingsDestinationUnset: "（未設定：初回の取り込み時に選びます）",
  settingsFlatExample: "IMG_0001.JPG（振り分けない）",
  settingsCustomPattern: "自分で決める",
  settingsCustomPatternNote:
    "{year} {month} {day} が日付に置き換わります。/ で階層になります。使えない文字や上の階層への移動（..）は自動で落とします。",
  settingsCustomPatternResult: "できるフォルダ",
  settingsViewer: "写真を大きく見るとき",
  settingsAutoAdvanceToggle: "P / U で判定したら、次の写真へ進む",
  settingsAutoAdvanceNote:
    "全画面表示で P を押すと ⚑ の印が付き（★ お気に入りとは別の棚です）、U で外れます。この設定を入れておくと、続けて次の写真が出ます（選ぶ作業が1枚1操作で進みます）。切ると、その写真に留まります。",
  settingsAutoplay: "USBやSDカードを挿したとき",
  settingsAutoplayToggle: "「pictkura で写真を取り込む」を候補に出す",
  settingsAutoplayNote:
    "Windowsの「自動再生」の選択肢に並びます。勝手に起動することはありません。インストーラで消す場合はアンインストール時に自動で解除しますが、持ち歩き版と、同じPCの他の利用者のぶんは残ります——その場合は、消す前にここを切ってください。",
  settingsAbout: "このアプリについて",
  settingsAboutLicense: "MIT ライセンスで配布しています。",
  settingsManual: "取扱説明書",
  settingsOssLicenses: "使っているオープンソース",
  settingsDocNotBundled: "（開発中の実行では同梱されていません）",
  settingsLanguage: "言語",
  settingsLanguageSystem: "OSに合わせる",
  settingsLanguageNote:
    "選ぶと画面を読み込み直します。コピーそのものは裏で続きますが、取り込みウィザードは閉じて進み具合が見えなくなるので、取り込み中は終わってから切り替えてください。",
  settingsTheme: "テーマ",
  themeSystem: "システムに合わせる",
  themeLight: "ライト",
  themeDark: "ダーク",
  settingsEditors: "編集に使うアプリ",
  settingsEditorsNote: "「他のアプリで開く…」で選んだアプリを覚えています。",
  settingsForgetEditor: "一覧から外す",
  calendarEmpty: "写真がありません",
  weekdays: ["日", "月", "火", "水", "木", "金", "土"],
  // ⚡爆速メーター
  speedPrefix: (sec: string) => `⚡ ${sec}秒で起動チェック — `,
  speedUsn: "USNジャーナル差分: ",
  speedUsnNoChange: "変更ゼロ、フォルダ走査なし",
  speedUsnDirty: (records: number, dirs: number) =>
    `${records}件のログ → ${dirs}フォルダだけ再走査`,
  speedPruned: (skipped: number) => `枝刈りスキャン: ${skipped}フォルダをスキップ`,
  speedFull: (total: number) => `フルスキャン（${total}件）`,
  speedNoDiff: " ／ 変更なし",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` ／ 追加${added}・変更${changed}・削除${removed}`,
};

/** 辞書の形。追加言語はこの型を満たす必要がある（キーの抜けはコンパイルエラー） */
export type Dict = typeof ja;

const en: Dict = {
  appName: "pictkura",
  viewThumbnails: "Photos",
  viewCalendar: "Calendar",
  searchPlaceholder: "Search files, folders, cameras, 2019-08, or year:2019",
  searchClear: "Clear search (Esc)",
  commandPalette: "Command palette",
  importFromUsb: "Import from USB",
  rescan: "Rescan",
  size: "Size",
  itemsSuffix: "items",
  navPlaces: "Places",
  navAllPhotos: "All photos",
  navFavorites: "★ Favorites",
  navPicked: "⚑ Picked",
  navKinds: "Kind",
  kindPhoto: "Photos",
  kindRaw: "RAW",
  kindVideo: "Videos",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Keyboard shortcuts (?)",
  actionShortcuts: "Show keyboard shortcuts",
  shortcutGroups: [
    {
      title: "Grid",
      keys: [
        ["Ctrl+K / ⌘K", "Command palette (jump to a date or camera, search, import)"],
        ["Ctrl+A / ⌘A", "Select everything the current search and filter match"],
        ["Shift + click", "Select everything between the last tile you clicked and this one"],
        ["Ctrl + click", "Add or remove one photo (⌘ + click on macOS)"],
        ["Click a date heading", "Select that whole day (click again to clear)"],
        ["Esc", "Stop selecting"],
      ],
    },
    {
      title: "Viewer",
      keys: [
        ["← / →", "Previous / next photo"],
        ["P", "Pick it (⚑). By default this moves on to the next photo"],
        ["X", "Reject it (✕). Rejected photos go to the trash when you close the viewer"],
        ["U", "Undo the judgement on this photo (clear ⚑ and ✕)"],
        ["F", "Toggle favorite (★)"],
        ["I", "Capture details (camera, lens, aperture, ISO, GPS)"],
        ["Space", "Slideshow. On a video, play / pause"],
        ["1 / 0", "Actual size 100% / fit to screen"],
        ["F11", "Full screen"],
        ["Esc", "Close"],
      ],
    },
    {
      title: "Mouse (in the viewer)",
      keys: [
        ["Double-click", "Actual size 100% ⇔ fit to screen"],
        ["Wheel", "Zoom in / out"],
        ["Drag", "Move around while zoomed in"],
        ["Right-click", "Open / open with / show in folder / move to trash"],
        ["Click the strip", "Jump to that photo"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Cameras & media",
  navLibraryFolders: "Library folders",
  navDrives: "Drives",
  navAddFolder: "Add a folder",
  add: "Add",
  browse: "Browse…",
  addFolderPlaceholder: folderExample("e.g. ", "you"),
  pickLibraryFolder: "Choose a folder to add to the library",
  showMore: (n: number) => `${n} more`,
  collapse: "Show less",
  photosCount: (n: number) => `${n}`,
  memoriesTitle: (years: number) =>
    years === 1 ? "1 year ago today" : `${years} years ago today`,
  viewerFavorite: "Favorite (F)",
  viewerPick: "Pick (P)",
  viewerUnpick: "Unpick (U)",
  viewerPicked: "Picked photo",
  judgeFav: "Favorite",
  judgeUnfav: "Favorite removed",
  judgePick: "Picked",
  judgeUnflag: "Judgement cleared",
  viewerReject: "Reject (X)",
  viewerRejected: "Rejected",
  rejectChip: (n: number) => `✕ ${n}`,
  rejectChipTitle: "Review the rejected photos",
  rejectGateTitle: (n: number) =>
    n === 1 ? "Move 1 photo to the trash" : `Move ${n} photos to the trash`,
  rejectGateNote: "You can restore them from the trash (they come back to the library, too).",
  rejectGateRestore: "Keep",
  rejectGateBack: "Back",
  rejectGateDiscard: "Close without deleting",
  rejectGateConfirm: (n: number) => (n === 1 ? "Move 1 to trash" : `Move ${n} to trash`),
  rejectGateTrashing: (done: number, total: number) =>
    `Moving… (${done} / ${total})`,
  updateFound: (v: string) => `Version ${v} is available`,
  updateOpenPage: "Open the download page",
  updateLater: "Later",
  updateCheckNow: "Check for updates",
  updateChecking: "Checking…",
  updateUpToDate: "You are up to date",
  updateFailed: "Could not check",
  updateOnStart: "Check for updates at startup",
  updateOnStartNote:
    "Asks GitHub for the latest version name (once a day). No photos or file names are sent. Turn it off and nothing leaves this machine except when you press “Check for updates”.",
  viewerSlideshow: "Slideshow (Space)",
  viewerExif: "Photo info (I)",
  viewerFullscreen: "Full screen (F11)",
  viewerClose: "Close (Esc)",
  viewerPrev: "Previous (←)",
  viewerNext: "Next (→)",
  viewerFitToScreen: "Fit to screen (0)",
  viewerActualSize: "Actual size, 100% (1) — or double-click",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "This format cannot be played in the app",
  videoMissing: "This file is missing (it looks moved or deleted)",
  videoCloudOnly: "This video lives in the cloud",
  videoCloudOnlyNote:
    "Playing it here starts a download and shows nothing until it finishes. Opening it in the default app lets you watch the download progress.",
  videoFailed: "Could not play this video",
  videoOpenExternal: "Open in default app",
  videoCodecNote:
    "Videos from iPhones and similar cameras use HEVC (H.265). Playback needs an OS decoder; on Windows that is a paid extension (a few dollars).",
  videoCodecNoteMac:
    "The browser engine used for display could not play this video. macOS decodes HEVC out of the box, so the recording format is most likely one it does not handle. Open it in your default player instead.",
  videoCodecNoteOther:
    "The browser engine used for display could not play this video. Your system may not have a decoder for this recording format. Open it in your default player instead.",
  videoCodecHelp: "Get the HEVC extension (paid)",
  loading: "Loading…",
  exifTitle: "Photo info",
  exifCamera: "Camera",
  exifLens: "Lens",
  exifAperture: "Aperture",
  exifShutter: "Shutter",
  exifIso: "ISO",
  exifFocal: "Focal length",
  exifLocation: "Location",
  exifNone: "No EXIF data",
  paletteInput: "Date, camera, keyword, or command…",
  paletteNoResults: "No results",
  paletteGroupJumpDate: "Jump to date",
  paletteGroupRecentDays: "Recent days",
  paletteGroupCameras: "Filter by camera",
  paletteGroupSearch: "Search",
  paletteGroupActions: "Actions",
  paletteSearchFor: (q: string) => `Search for “${q}”`,
  paletteSearchHint: "File name, folder, camera",
  paletteSelect: "Select",
  paletteRun: "Run",
  paletteCloseHint: "Close",
  actionShowFavorites: "Show favorites only",
  actionShowPicked: "Show picked only",
  actionShowAll: "Show all photos",
  actionCalendar: "Calendar view",
  actionThumbnails: "Photo grid",
  indexBuilding: "🔍 Building the search index… ",
  cameraScanning: "📷 Reading camera info… ",
  indexIncompleteWarning:
    "⚠ Search indexing was interrupted — results may be incomplete (it resumes on next launch)",
  indexProgressSuffix: "% — results may be incomplete until this finishes",
  removeRoot: (path: string) => `Remove ${path} from the library`,
  importFrom: (path: string) => `Import from ${path}`,
  filterByCamera: (name: string) => `Show only photos taken with ${name}`,
  jumpToYear: (year: number) => `Jump to ${year}`,
  importing: (done: number, total: number) => `Importing… ${done}/${total}`,
  importDone: (copied: number, skipped: number) =>
    `Import finished: ${copied} copied, ${skipped} skipped`,
  importFailed: (n: number) => `, ${n} failed`,
  importIncomplete: " ⚠ Some folders could not be read — do not erase the card yet",
  syncDone: (added: number, changed: number, removed: number) =>
    `${added} added, ${changed} changed, ${removed} removed`,
  pickSource: "Choose the folder to import from (USB / DCIM)",
  pickDestination: "Choose the destination folder",
  wizardTitle: "Import",
  wizardSources: "Source",
  wizardOtherFolder: "Other folder…",
  wizardRefresh: "Rescan drives",
  wizardRemovable: "Removable",
  wizardNoDrives: "No drives found",
  wizardPickFolderHint: "Pick a folder on the left to see the photos in it",
  wizardNoImages: "No photos in this folder",
  wizardUnreadable: "Could not read this folder (it may have been removed)",
  wizardCounting: "Loading…",
  wizardSelectAll: "Select all",
  wizardSelectNew: "Select new only",
  wizardClearSelection: "Clear selection",
  wizardSelected: (n: number) => `${n} selected`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Already imported (the same file exists in the destination)",
  wizardDestination: "Destination",
  wizardChangeDestination: "Change",
  wizardStructure: "Filing",
  wizardImportButton: (n: number) => `Import ${n}`,
  wizardImportAll: "Import this whole folder (including subfolders)",
  wizardImportAllShort: "Whole folder",
  wizardDeep: "Include subfolders",
  wizardDeepHint: "Sweeps the whole media so you do not have to know where the photos are",
  wizardScanning: "Looking through the media…",
  wizardTruncated: (n: number) =>
    `Showing the first ${n} only. Use "Whole folder" to import everything`,
  wizardScanIncomplete: "⚠ Some folders could not be read (photos may be missing)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${n.toLocaleString()} HEIC/HEIF photos cannot be shown here. They need the free HEIF extension plus the paid HEVC extension (a few dollars) that decodes the pixels`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${n.toLocaleString()} HEIC/HEIF photos could not be rendered here`,
  decoderHeifHow: "HEIF extension (free)",
  decoderHevcHow: "HEVC extension (paid)",
  decoderNoticeDismiss: "Don't show again",
  wizardOfflineTitle:
    "This file lives in the cloud (no preview here; importing will download it)",
  wizardHideImported: "Hide already imported",
  wizardAllImported: "Nothing new here (everything in this folder is already imported)",
  wizardHiddenCount: (n: number) => `${n} already-imported hidden`,
  wizardCopying: "Importing",
  wizardEtaSeconds: (n: number) => `about ${n}s left`,
  wizardEtaMinutes: (n: number) => `about ${n} min left`,
  wizardEtaCalculating: "estimating time left…",
  wizardCapped: (n: number) => `${n}+`,
  wizardMoreFiles: (n: number) => `${n} more (scroll to load)`,
  menuOpen: "Open",
  menuOpenWith: (name: string) => `Open with ${name}`,
  menuOpenWithOther: "Open with another app…",
  menuReveal: "Show in folder",
  menuDelete: "Delete (move to trash)",
  menuFavoriteOn: "Add to favorites",
  menuFavoriteOff: "Remove from favorites",
  pickEditor: "Choose an app to edit with",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Move this photo to the trash?"
      : `Move ${n} photos to the trash?`,
  deleted: (n: number) => `Moved ${n} to the trash`,
  deletedSomeLeft: (n: number, left: number) =>
    `Moved ${n} to the trash (${left} could not be found and were left alone)`,
  // 複数選択と一括操作
  selectItem: "Select",
  selectedCount: (n: number) => (n === 1 ? "1 selected" : `${n} selected`),
  selectAll: "Select all",
  clearSelection: "Clear selection (Esc)",
  selectDay: "Select this whole day",
  bulkFavoriteOn: "Add to favorites",
  bulkFavoriteOff: "Remove from favorites",
  bulkDelete: "Move to trash",
  bulkCopy: "Copy to folder",
  bulkMove: "Move to folder",
  bulkViewer: "View the selection",
  pickExportFolder: "Choose a folder to export to",
  moveConfirm: (n: number) =>
    n === 1
      ? "Move this photo to a folder you pick next? It leaves its current place and leaves the library (★ and ⚑ marks are not carried over)."
      : `Move ${n} photos to a folder you pick next? They leave their current place and leave the library (★ and ⚑ marks are not carried over).`,
  exporting: (done: number, total: number, name: string) =>
    `Exporting… ${done}/${total} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [done === 1 ? "Exported 1 photo" : `Exported ${done} photos`];
    if (skipped > 0) parts.push(`${skipped} already there`);
    if (failed > 0) parts.push(`${failed} failed`);
    if (leftBehind > 0) parts.push(`${leftBehind} could not be removed from the original place`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Pick",
  bulkPickOff: "Unpick",
  bulkPickDone: (n: number) =>
    n === 1 ? "1 photo picked" : `${n} photos picked`,
  bulkUnpickDone: (n: number) =>
    n === 1 ? "1 photo unpicked" : `${n} photos unpicked`,
  bulkFavoriteDone: (n: number) =>
    n === 1 ? "1 photo added to favorites" : `${n} photos added to favorites`,
  bulkUnfavoriteDone: (n: number) =>
    n === 1
      ? "1 photo removed from favorites"
      : `${n} photos removed from favorites`,
  settings: "Settings",
  close: "Close",
  settingsTitle: "Settings",
  settingsImportStructure: "Import folder structure",
  settingsImportStructureNote:
    "How imported photos are filed by capture date. This affects thousands of files, so it is hard to change later. Where a folder name carries a date, it is written year-first in every language, so that sorting by name sorts by time.",
  settingsDestination: "Destination",
  settingsDestinationUnset: "(not set — you'll choose it on the first import)",
  settingsFlatExample: "IMG_0001.JPG (no subfolders)",
  settingsCustomPattern: "Custom",
  settingsCustomPatternNote:
    "{year} {month} {day} are replaced with the date. Use / for nesting. Unusable characters and moves to a parent folder (..) are dropped automatically.",
  settingsCustomPatternResult: "Resulting folder",
  settingsViewer: "When you view a photo full screen",
  settingsAutoAdvanceToggle: "Move to the next photo after P / U",
  settingsAutoAdvanceNote:
    "In full screen, P flags the photo with ⚑ (a separate shelf from ★ favorites) and U clears it. With this on, the next photo follows right away, so picking takes one key per photo. With it off, you stay on the same photo.",
  settingsAutoplay: "When you insert a USB drive or SD card",
  settingsAutoplayToggle: "Offer pictkura in the AutoPlay choices",
  settingsAutoplayNote:
    "Adds pictkura to the Windows AutoPlay choices. It never starts on its own. Note that the entry itself is worded in Japanese. Uninstalling with the installer removes it, but the portable build — and other users’ copies on a shared PC — are not covered; turn this off before removing pictkura in those cases.",
  settingsAbout: "About",
  settingsAboutLicense: "Distributed under the MIT license.",
  settingsManual: "Manual",
  settingsOssLicenses: "Open source we use",
  settingsDocNotBundled: "(not bundled in a development build)",
  settingsLanguage: "Language",
  settingsLanguageSystem: "Match system",
  settingsLanguageNote:
    "Switching reloads the window. Copying itself keeps running in the background, but the import wizard closes and you lose sight of its progress — so wait until an import finishes before switching.",
  settingsTheme: "Theme",
  themeSystem: "Match system",
  themeLight: "Light",
  themeDark: "Dark",
  settingsEditors: "Editing apps",
  settingsEditorsNote: "Apps you picked in “Open with another app…”.",
  settingsForgetEditor: "Remove from the list",
  calendarEmpty: "No photos",
  weekdays: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
  speedPrefix: (sec: string) => `⚡ Startup check in ${sec}s — `,
  speedUsn: "USN journal delta: ",
  speedUsnNoChange: "no changes, no folders walked",
  speedUsnDirty: (records: number, dirs: number) =>
    `${records} journal records → rescanned only ${dirs} folders`,
  speedPruned: (skipped: number) => `pruned scan: skipped ${skipped} folders`,
  speedFull: (total: number) => `full scan (${total} files)`,
  speedNoDiff: " — no changes",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` — ${added} added, ${changed} changed, ${removed} removed`,
};

/** 対応言語。ここと `LOCALES` の両方に足すこと */
const DICTS: Record<string, Dict> = { ja, en };

/**
 * 選択肢に出す言語（コードと、その言語自身での呼び名）。
 *
 * **呼び名はその言語で書く**。英語しか読めない画面になってしまった人が
 * 日本語へ戻れるように、「日本語」は日本語のまま出す（"Japanese" にしない）。
 */
export const LOCALES: { code: string; label: string }[] = [
  { code: "ja", label: "日本語" },
  { code: "en", label: "English" },
];

/** 言語の指定を置く場所（テーマと同じくlocalStorage） */
const LOCALE_KEY = "pictkura.locale";

/**
 * その言語コードの辞書を持っているか。
 *
 * **`DICTS[code]` で判定してはいけない**。`DICTS` は素のオブジェクトなので
 * `"constructor"` や `"toString"` が真になり、辞書のつもりで**関数**を掴む。
 * そうなると画面じゅうが `undefined` になる（localStorageを直に書き換えた場合に届く）。
 *
 * `Object.hasOwn` はES2022なので、この設定（ES2021）では使えない。
 */
const hasDict = (code: string) =>
  Object.prototype.hasOwnProperty.call(DICTS, code);

/**
 * localStorageの読み書き。**失敗しても落とさない**。
 *
 * `locale` はモジュールを読んだ時点で決まるので、ここで例外が飛ぶと
 * i18nを読み込む画面すべてが真っ白になる。言語の指定は無くても既定で動く類の
 * 情報なので、読めない・書けないときは黙って諦める。
 */
function readStored(): string | null {
  try {
    return localStorage.getItem(LOCALE_KEY);
  } catch {
    return null;
  }
}
function writeStored(code: string | null): boolean {
  try {
    if (code === null) localStorage.removeItem(LOCALE_KEY);
    else localStorage.setItem(LOCALE_KEY, code);
    return true;
  } catch {
    return false;
  }
}

/**
 * 表示に使う言語コードを決める。
 *
 * **設定で選んだ言語 → OSの優先言語 → 英語** の順。設定を先に見るのは、
 * OSが日本語でも英語で使いたい人がいるため（説明書のスクリーンショットを
 * 撮るときにも要る）。
 */
function pickLocale(): string {
  const chosen = readStored();
  if (chosen && hasDict(chosen)) return chosen;
  for (const tag of navigator.languages ?? [navigator.language]) {
    // "ja-JP" → "ja" のように地域を落として照合する
    const base = tag.toLowerCase().split("-")[0];
    if (DICTS[tag.toLowerCase()]) return tag.toLowerCase();
    if (DICTS[base]) return base;
  }
  return "en";
}

/** 現在の言語コード（"ja" / "en" …） */
export const locale = pickLocale();

/** 設定で選ばれている言語（未指定なら `null` ＝ OSに合わせる） */
export function readLocaleChoice(): string | null {
  const chosen = readStored();
  return chosen && hasDict(chosen) ? chosen : null;
}

/**
 * 言語を切り替える。`null` を渡すとOSの優先言語に戻る。
 *
 * **画面を読み込み直す**のが要点。辞書 `t` はモジュールを読んだ時点で
 * 決まる定数で、画面のあちこちが直接それを読んでいる。差し替えて回るより、
 * 読み直したほうが取りこぼしが無い（ローカルのWebViewなので一瞬で、
 * 表示中の内容はバックエンドから引き直される）。
 *
 * **切り替わったかを返す**。保存できなければ言語は変わらないので、
 * 呼び出し側が「その言語になっている」と表示してしまわないようにする。
 * 保存が効かない状態でメモリ上だけ切り替えても、読み込み直した先で
 * 元に戻るだけで、かえって分からなくなる。
 */
export function setLocaleChoice(code: string | null): boolean {
  if (code !== null && !hasDict(code)) return false;
  if (!writeStored(code)) return false;
  // **書いたあとで決め直す**。「OSに合わせる」を選んだ結果いまと同じ言語に
  // なることもあるので、指定そのものではなく**結果**を見て判断する
  if (pickLocale() !== locale) location.reload();
  return true;
}

/** 現在の辞書。`t.searchPlaceholder` のように使う */
export const t: Dict = DICTS[locale] ?? en;

/** 日付・時刻の書式はIntlに任せる（全ロケールが自動的に正しくなる） */
export const formatDateTime = (ms: number) =>
  new Date(ms).toLocaleString(locale);

/** day_key（YYYYMMDD整数）を、その言語の日付表記にする */
export const formatDayKey = (dayKey: number) => {
  const y = Math.floor(dayKey / 10000);
  const m = Math.floor(dayKey / 100) % 100;
  const d = dayKey % 100;
  return new Date(y, m - 1, d).toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
};

/** 「2026年8月」等の月見出し */
export const formatMonth = (year: number, month: number) =>
  new Date(year, month - 1, 1).toLocaleDateString(locale, {
    year: "numeric",
    month: "long",
  });

/**
 * 動画の長さ（ミリ秒）を `0:12` / `1:02:03` にする（第9部）。
 *
 * Intlの `DurationFormat` は「1時間2分3秒」のように綴るので一覧のバッジには
 * 長すぎる。時計表記はどの言語でも同じ形なので、辞書にも入れない。
 */
export const formatDuration = (ms: number) => {
  const total = Math.max(0, Math.round(ms / 1000));
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
};

/** 件数などの数値（桁区切りをロケールに合わせる） */
export const formatNumber = (n: number) => n.toLocaleString(locale);
