/**
 * 日本語辞書。**これがキーの正**で、他の言語はこの形に合わせる。
 *
 * 辞書全体の方針——ランタイムを増やさない・言語の足し方・`Intl` に任せるもの——は
 * `index.ts` の冒頭にある。**言語を足す前にそちらを読むこと。**
 */
import { folderExample } from "./folderExample";
import { num } from "./plural";

export const ja = {
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
  /**
   * `Ctrl` の呼び名。**キーボードに印字されている綴りを出す**——
   * ドイツ語版Windowsのキーには `Strg` とある。`api.ts` の `modKey` が
   * `⌘C` / `Ctrl+C` を組み立てるときに読む。
   *
   * `shortcutGroups` の鍵の列は辞書に直接書いてあるので、**そちらと必ず揃えること**
   * （片方だけ直すと同じ画面に `Strg` と `Ctrl` が混ざる）。
   */
  keyCtrl: "Ctrl",
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
        ["Ctrl+C / ⌘C", "いま出ている絵をクリップボードへコピー"],
        ["Ctrl+S / ⌘S", "いま出ている絵をファイルに保存"],
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
  showMore: (n: number) => `他${num(n)}台`,
  collapse: "閉じる",
  photosCount: (n: number) => `${num(n)}枚`,
  memoriesTitle: (years: number) => `${num(years)}年前の今日`,
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
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "ボツの候補を確かめる",
  rejectGateTitle: (n: number) =>
    n === 1 ? "1枚をゴミ箱へ移動します" : `${num(n)}枚をゴミ箱へ移動します`,
  rejectGateNote: "ゴミ箱から戻せます（戻すと一覧にも戻ります）",
  rejectGateRestore: "戻す",
  rejectGateBack: "戻る",
  rejectGateDiscard: "入れずに閉じる",
  rejectGateConfirm: (n: number) => `${num(n)}枚をゴミ箱へ`,
  rejectGateTrashing: (done: number, total: number) =>
    `移動中… (${num(done)} / ${num(total)})`,
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
  // 抽出（Issue #13）。**「書き出し」とは別のもの**——あちらは選んだファイルを
  // フォルダへ運ぶ機能で、こちらは1枚から「いま見えている絵」だけを取り出す。
  // 同じ言葉を当てると、RAWを書き出したつもりでJPEGが出てくる（逆も）
  // 鍵の表記（`⌘S` / `Ctrl+S`）は `api.ts` の `modKey` が作って渡す
  // ——ここから import すると循環になる（このファイルの冒頭を参照）
  extractSave: (key: string) => `この絵をファイルに保存 (${key})`,
  extractCopy: (key: string) => `この絵をクリップボードへコピー (${key})`,
  extractSaveTitle: "画像の保存先",
  extractFilter: "画像",
  extractSaved: "保存しました",
  extractCopied: "コピーしました",
  extractFailed: "取り出せませんでした",
  extractSameFile: "元のファイルには上書きできません",
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
    "iPhoneなどの動画はHEVC（H.265）で記録されています。再生にはOSのデコーダが必要で、Windowsでは Microsoft Store の有料の拡張機能（数百円）になります。",
  /**
   * macOS。**買わせる話をしない**——OSがHEVCを最初から再生できる。
   * 見出し（`videoFailed`）と下のボタンが言っていることは繰り返さない
   */
  videoCodecNoteMac:
    "macOSはHEVCを最初から再生できるので、対応していない記録方式なのかもしれません。",
  /**
   * それ以外（Linux）。**デコーダがあるとは言い切らない**——
   * ディストリによってはHEVCのデコーダが入っていない（`video.rs` のとおり同梱しない）
   */
  videoCodecNoteOther:
    "この記録方式に対応するデコーダが、この環境に入っていないのかもしれません。",
  videoCodecHelp: "HEVC ビデオ拡張機能を見る（有料）",
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
  importing: (done: number, total: number) => `取り込み中… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `取り込み完了: コピー${num(copied)} スキップ${num(skipped)}`,
  importFailed: (n: number) => ` 失敗${num(n)}`,
  importIncomplete: " ⚠読み取れないフォルダあり（カードを消去しないでください）",
  syncDone: (added: number, changed: number, removed: number) =>
    `追加${num(added)} 変更${num(changed)} 削除${num(removed)}`,
  pickSource: "取り込み元フォルダ（USB/DCIM）を選択",
  pickDestination: "コピー先フォルダを選択",
  // 取り込みウィザード（第5部 段階E）
  wizardTitle: "取り込み",
  wizardSources: "取り込み元",
  wizardOtherFolder: "他のフォルダ…",
  wizardRefresh: "ドライブを再検出",
  wizardRemovable: "リムーバブル",
  wizardNoDrives: "ドライブが見つかりません",
  /* 一覧が空のときの説明（無言で `0 件` を出さない） */
  emptyTitle: "まだ写真がありません",
  /** 読み込みに失敗したときの見出し（「まだ写真がありません」は嘘になる） */
  emptyTitleFailed: "一覧を出せませんでした",
  /** まだ確かめ終わっていないときの見出し（本文と食い違わせない） */
  emptyTitleChecking: "確認しています",
  /** 起動時の同期が転んで終わった。**「空」と断定しない** */
  emptyTitleStartupFailed: "取り込みが最後まで終わりませんでした",
  emptyStartupFailed:
    "起動したときの同期が最後まで終わりませんでした。写真があっても、まだ一覧に入っていないことがあります。「再スキャン」を押してください。それでも直らないときは、アプリを開き直してください。",
  /** **写真は在るかもしれない**——場所に届いていないだけ。
   *  ここで「まだ写真がありません」と出すと、1万枚あって権限を許すだけの人に
   *  向かって空だと断定することになる */
  emptyTitleMissing: "見つからない場所があります",
  emptyTitleUnreadable: "開けない場所があります",
  emptyNoRoots:
    "ライブラリのフォルダがまだ設定されていません。カードから取り込むか、写真のあるフォルダを選んでください。",
  emptyMissing: (names: string) =>
    `次の場所が見つかりません: ${names}。外付けなら、つないでから「再スキャン」を押してください。`,
  /* 読めない理由はOSで違うので、次にやることも分ける。
     `unreadable` にはWindowsからも来る（ACLの拒否・切れたSMB共有）ので、
     macOSの案内だけ置くと**Windowsの人に次の一手が1つも無い**（ゲート1の指摘） */
  /* macOS側も**TCCだけではない**。`unreadable` は「`metadata` は通るのに
     `read_dir` が落ちる」「`NotFound` 以外で `metadata` が落ちる」で立つので、
     切れた `/Volumes/...` の共有や素の権限もここへ来る。許可の話だけ書くと、
     **落ちたSMB共有の人が効かない設定画面へ送られる**（ゲート2の指摘） */
  emptyUnreadableMac: (names: string) =>
    `次の場所を開けませんでした: ${names}。「システム設定 → プライバシーとセキュリティ」で、pictkura にそのフォルダ（デスクトップ・書類・外付けなど）へのアクセスを許してください。ネットワークのフォルダなら、つながっているかを確かめてから「再スキャン」を押してください。`,
  emptyUnreadableWin: (names: string) =>
    `次の場所を開けませんでした: ${names}。フォルダのアクセス許可を確かめてください。ネットワークドライブなら、つながっているかを確かめてから「再スキャン」を押してください。`,
  emptyUnreadableOther: (names: string) =>
    `次の場所を開けませんでした: ${names}。読み取りの権限があるかを確かめてから「再スキャン」を押してください。`,
  /** 並べるときの区切り（言語で違う） */
  listSeparator: "、",
  /** 名前を並べきらなかったぶん（**黙って落とさない**） */
  andMore: (n: number) => `ほか${num(n)}件`,
  /* **「この場所」と書かない**——写真.appの判定も除外の集計もルートをまたぐので、
     ルートが複数あると数が合わない（ゲート1の指摘）。
     「扱いません」も**既定では**に緩める: `*.photoslibrary` の除外は
     利用者が設定から外せる（scanner.rs の設計どおり。同） */
  /** ルートそのものが写真.appのライブラリ。**排他は主張しない** */
  emptyRootIsPackage:
    "ライブラリのフォルダに、写真.appのライブラリそのものが指定されています。pictkuraはその中を扱わないので、ここからは何も出てきません。写真のあるフォルダを選び直すか、カードから取り込んでください。",
  emptyPhotoLibrary:
    "写真.appのライブラリのほかに、扱えるものが見つかりません。pictkuraは既定では写真.appの中を扱いません（原本の多くはiCloud側にあり、手元にはありません）。カードから取り込むか、写真のあるフォルダを選んでください。",
  /** 写真.app以外（iPhoto・Aperture）が混ざっているとき。**iCloudの話をしない**
   *  ——あちらの中身は手元にあるので、在りかを取り違えさせる */
  emptyManagedLibrary:
    "写真を管理するアプリのライブラリ（写真.app・iPhoto・Apertureなど）のほかに、扱えるものが見つかりません。pictkuraはその中を扱いません（中を索引しても、あとからの変更が届かず古いままになるためです）。カードから取り込むか、写真のあるフォルダを選んでください。",
  emptyRootIsManagedLibrary:
    "ライブラリのフォルダに、写真を管理するアプリのライブラリ（写真.app・iPhoto・Apertureなど）そのものが指定されています。pictkuraはその中を扱わないので、ここからは何も出てきません。写真のあるフォルダを選び直すか、カードから取り込んでください。",
  emptyAllExcluded: (names: string) =>
    `見つかったものは、除外の設定で全部飛ばしています（例: ${names}）。設定フォルダの pictkura.toml で変えられます。`,
  emptyNothingHere:
    "扱える画像がまだ見つかりません。カードから取り込むか、写真のあるフォルダを選んでください。",
  /** カレンダーの空欄に置く一言。**断定しない**（確かめている最中） */
  calendarChecking: "確認しています…",
  /** 返事が来ないまま見切った場所。**「確かめています」とは別**——
   *  こちらは放っておいても変わらない */
  emptyTitleStalled: "応答がない場所があります",
  emptyStalled: (names: string) =>
    `次の場所から応答がありません: ${names}。ネットワークのフォルダなら、つながっているかを確かめて「再スキャン」を押してください。外したままにするなら、ライブラリのフォルダから取り除くと、残りの場所の結果が出ます。`,
  /** フォルダをまだ見終わっていない。**「何も無い」と言わない** */
  emptyChecking:
    "フォルダを確かめている途中です。ネットワークのフォルダなら、つながっているかを確かめてから「再スキャン」を押してください。",
  /** 一覧そのものが取れなかった。**「空です」と言わない**——空かどうかは分かっていない */
  emptyLoadFailed:
    "一覧を読み込めませんでした。上の帯に理由が出ています。「再スキャン」を押すか、アプリを開き直してください。",
  wizardPickFolderHint: "左からフォルダを選ぶと、この中の画像が並びます",
  wizardNoImages: "このフォルダに画像はありません",
  wizardUnreadable: "フォルダを読めませんでした（取り外された可能性があります）",
  wizardCounting: "読み込み中…",
  wizardSelectAll: "すべて選択",
  wizardSelectNew: "未取り込みだけ選択",
  wizardClearSelection: "選択を解除",
  wizardSelected: (n: number) => `${num(n)}枚を選択中`,
  wizardImportedBadge: "済",
  wizardImportedTitle: "取り込み済み（コピー先に同じファイルがあります）",
  wizardDestination: "コピー先",
  wizardChangeDestination: "変更",
  wizardStructure: "振り分け",
  wizardImportButton: (n: number) => `${num(n)}枚を取り込む`,
  wizardImportAll: "このフォルダを丸ごと取り込む（下の階層も含む）",
  wizardImportAllShort: "フォルダごと",
  wizardDeep: "下の階層も含める",
  wizardDeepHint: "メディアの中を全部さらって並べます（どこに入っているか分からなくてもOK）",
  wizardScanning: "メディアの中を探しています…",
  /**
   * **ここに単数形は要らない**（2026-09-02、据え置きの理由）。`n` は打ち切りの上限
   * そのもので、`lib.rs` の `TREE_LIMIT = 20_000` に達したときにしか出ない。
   * 独西の `die ersten 1` / `las primeras 1` は機械的には出せるが、**到達しない**。
   * 上限を人が選べるようにしたら、そのときに書く。
   */
  wizardTruncated: (n: number) =>
    `多すぎるため先頭${num(n)}枚だけ表示しています。全部入れるなら「フォルダごと」をどうぞ`,
  wizardScanIncomplete: "⚠読み取れないフォルダがありました（取りこぼしの可能性があります）",
  /**
   * **枚数は生の `number` で受け取り、辞書の中で `num()` に通す**（2026-09-02）。
   *
   * ここは逆だった——整形済みの `string` を渡す規約にしていた。循環参照を避けるためで、
   * 理屈は合っていたが、**整形済みの文字列からは件数が見えない**ので
   * `n === 1` の場合分けができず、独語で `Für 1 HEIC/HEIF-Fotos` と出ていた。
   * **単数形は辞書にしか書けないのに、その材料を取り上げていた。**
   * 循環参照は `plural.ts` への注入で解いてある（そちらの冒頭に理由）。
   */
  decoderHeifNotice: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}枚は、この環境ではサムネイルを作成できません（開いても表示できません）。無料の「HEIF 画像拡張機能」に加え、画素の展開に有料の「HEVC ビデオ拡張機能」（数百円）が要ります`,
  /**
   * macOS。**買わせる話をしない**——入れるものが無いので、次にやることが無い
   * （動画側と違い、ここは「デコーダが要る」と言っても利用者は動けない）
   */
  decoderHeifNoticeMac: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}枚は、この環境ではサムネイルを作成できません（開いても表示できません）`,
  /**
   * それ以外（Linux）。HEICもOSの部品次第なので、**理由は言うが買わせない**
   * ——動画側（`videoCodecNoteOther`）と揃える
   */
  decoderHeifNoticeOther: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}枚は、この環境ではサムネイルを作成できません（開いても表示できません）。HEIC/HEVCに対応するデコーダが入っていないのかもしれません`,
  decoderHeifHow: "HEIF 画像拡張機能（無料）",
  decoderHevcHow: "HEVC ビデオ拡張機能（有料）",
  decoderNoticeDismiss: "今後表示しない",
  wizardOfflineTitle:
    "クラウド上のファイルです（この場では絵を出しません。取り込むとダウンロードされます）",
  wizardHideImported: "取り込み済みを隠す",
  wizardAllImported: "新しい写真はありません（このフォルダはすべて取り込み済みです）",
  wizardHiddenCount: (n: number) => `取り込み済み${num(n)}枚は隠しています`,
  wizardCopying: "取り込み中",
  wizardEtaSeconds: (n: number) => `残り約${num(n)}秒`,
  wizardEtaMinutes: (n: number) => `残り約${num(n)}分`,
  wizardEtaCalculating: "残り時間を見積もっています…",
  wizardCapped: (n: number) => `${num(n)}+枚`,
  wizardMoreFiles: (n: number) => `ほか${num(n)}枚（スクロールで表示）`,
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
      : `${num(n)}枚の写真をゴミ箱へ移動しますか？`,
  deleted: (n: number) => `${num(n)}枚をゴミ箱へ移動しました`,
  // 関所に並べた数より少なかったとき。**黙って減らさない**（ゲート2の指摘）
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)}枚をゴミ箱へ移動しました（${num(left)}枚は見つからず残しました）`,
  // 複数選択と一括操作
  selectItem: "選択",
  selectedCount: (n: number) => `${num(n)}枚を選択中`,
  selectAll: "すべて選択",
  clearSelection: "選択を解除 (Esc)",
  selectDay: "この日をまとめて選ぶ",
  bulkFavoriteOn: "お気に入りに追加",
  bulkFavoriteOff: "お気に入りを外す",
  bulkDelete: "ゴミ箱へ",
  bulkCopy: "フォルダへコピー",
  bulkMove: "フォルダへ移動",
  bulkViewer: "選んだ写真を見る",
  pickExportFolder: "書き出し先のフォルダを選ぶ",
  moveConfirm: (n: number) =>
    n === 1
      ? "この写真を、このあと選ぶフォルダへ移動しますか？ 元の場所からは無くなり、ライブラリからも外れます（★と⚑の印は引き継がれません）。"
      : `${num(n)}枚の写真を、このあと選ぶフォルダへ移動しますか？ 元の場所からは無くなり、ライブラリからも外れます（★と⚑の印は引き継がれません）。`,
  exporting: (done: number, total: number, name: string) =>
    `書き出し中… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [`${num(done)}枚を書き出しました`];
    if (skipped > 0) parts.push(`${num(skipped)}枚は同じものが既にありました`);
    if (failed > 0) parts.push(`${num(failed)}枚は失敗しました`);
    if (leftBehind > 0) parts.push(`${num(leftBehind)}枚は元を消せませんでした`);
    return parts.join("。");
  },
  bulkPickOn: "選ぶ",
  bulkPickOff: "選ぶのをやめる",
  bulkPickDone: (n: number) => `${num(n)}枚を選別で選びました`,
  bulkUnpickDone: (n: number) => `${num(n)}枚の選別の印を外しました`,
  bulkFavoriteDone: (n: number) => `${num(n)}枚をお気に入りに追加しました`,
  bulkUnfavoriteDone: (n: number) => `${num(n)}枚のお気に入りを外しました`,
  // 設定
  settings: "設定",
  close: "閉じる",
  settingsTitle: "設定",
  settingsImportStructure: "取り込み先のフォルダ構成",
  settingsImportStructureNote:
    "USBから取り込んだ写真を、撮影日でどう振り分けるか。数千枚に効くので、後から変えるには相当な手間がかかります。フォルダ名に日付を入れるときは、言語を問わず年月日の順にします——名前順に並べたときに時系列になるためです。",
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
    "Windowsの「自動再生」の選択肢に並びます。自動的に起動することはありません。インストーラで消す場合はアンインストール時に自動で解除しますが、持ち歩き版と、同じPCの他の利用者の登録は残ります——その場合は、消す前にここを切ってください。",
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
  // ⚡爆速メーター
  speedPrefix: (sec: string) => `⚡ ${sec}秒で起動チェック — `,
  speedUsn: "USNジャーナル差分: ",
  speedUsnNoChange: "変更ゼロ、フォルダ走査なし",
  speedUsnDirty: (records: number, dirs: number) =>
    `${num(records)}件のログ → ${num(dirs)}フォルダだけ再走査`,
  speedPruned: (skipped: number) => `枝刈りスキャン: ${num(skipped)}フォルダをスキップ`,
  speedFull: (total: number) => `フルスキャン（${num(total)}件）`,
  speedNoDiff: " ／ 変更なし",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` ／ 追加${num(added)}・変更${num(changed)}・削除${num(removed)}`,
};

/** 辞書の形。追加言語はこの型を満たす必要がある（キーの抜けはコンパイルエラー） */
export type Dict = typeof ja;
