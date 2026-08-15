// バックエンド(Rust)とのIPC境界。フロントエンドは表示のみを行う。
//
// スケールの原則（plan.md 第3部 段階A）:
// 全件を一括転送するAPIは存在しない。「日付→枚数」のサマリ（timelineSummary）で
// タイムラインの骨組みを作り、可視範囲の日だけを listDay で取得する。
import { invoke, convertFileSrc } from "@tauri-apps/api/core";

export interface MediaItem {
  id: number;
  file_name: string;
  width: number;
  height: number;
  /** 表示・グルーピング用の日時（撮影日時、なければmtime。Unixエポックミリ秒） */
  taken_at_ms: number;
  /** 属する表示日（ローカル日付のYYYYMMDD整数）。部分更新の宛先解決に使う */
  day_key: number;
  /** キャッシュバスティング用のファイル更新日時 */
  mtime_ms: number;
  has_thumb: boolean;
  /** サムネイルの品質段階: 0=なし, 1=即席, 2=高品質 */
  thumb_state: number;
  favorite: boolean;
  /** 動画の長さ（ミリ秒）。動画以外・未読取はnull（第9部） */
  duration_ms: number | null;
  /** 動画か（一覧の▶バッジ、ビューアのプレイヤー切り替え） */
  is_video: boolean;
  /** アプリ内で再生できるコンテナか。偽なら「既定のアプリで開く」へ逃がす */
  plays_in_app: boolean;
}

/** タイムライン索引の1日分（日付・枚数と、カレンダー用の代表サムネイル） */
export interface DaySummary {
  /** ローカル日付のYYYYMMDD整数（例: 20260812） */
  day_key: number;
  count: number;
  cover_id: number;
  cover_mtime_ms: number;
  cover_thumb_state: number;
}

/** 「〇年前の今日」の1件分 */
export interface Memory {
  years_ago: number;
  item: MediaItem;
}

export interface LibraryStats {
  total: number;
  favorites: number;
}

export interface SyncStats {
  added: number;
  changed: number;
  removed: number;
}

export interface ImportStats {
  copied: number;
  skipped: number;
  failed: number;
  /** 取り込み元の走査でエラーがあった（取りこぼしの可能性） */
  scan_incomplete: boolean;
}

export interface ImportProgress {
  done: number;
  total: number;
  /** いまコピーしたファイル（進捗表示のサムネイル用） */
  path: string;
}

/** 起動時同期の⚡爆速メーター（startup-scan-reportイベントのペイロード） */
export interface StartupScanReport {
  /** 同期方式: USNジャーナル差分 / mtime枝刈り / フルスキャン */
  method: "usn" | "pruned" | "full";
  elapsed_ms: number;
  added: number;
  changed: number;
  removed: number;
  usn_records: number;
  dirty_dirs: number;
  skipped_dirs: number;
  total: number;
}

export interface AppConfig {
  import: { last_source_dir: string | null };
  routing: { destination: string | null; folder_pattern: string };
  library: { roots: string[] };
  editors: { apps: ExternalApp[] };
}

/** 取り込み先フォルダ構成の選択肢（今日の日付での実例つき） */
export interface FolderPattern {
  pattern: string;
  example: string;
}

/** 登録済みの外部編集アプリ（Lightroom/digiKam流に、選んだものを覚えていく） */
export interface ExternalApp {
  name: string;
  path: string;
}

export interface DriveInfo {
  label: string;
  path: string;
  removable: boolean;
  /**
   * ドライブの種類。ネットワークドライブを取り込み元として勝手に走査しないための区別。
   * OneDrive/iCloud Driveは「ドライブ」ではなくCドライブ上のフォルダなので
   * ここには現れない（実体の有無は SourceFile.offline で見る）
   */
  kind: "removable" | "fixed" | "network" | "optical" | "other";
  /** DCIMフォルダを持つ（カメラメディアの可能性が高い） */
  has_dcim: boolean;
}

/** 取り込みウィザードのツリー1ノード（第5部 段階E） */
export interface SourceDir {
  name: string;
  path: string;
  /** さらに下の階層がある（展開ボタンを出すか） */
  has_subdirs: boolean;
  /** このフォルダ直下の画像枚数（再帰しない） */
  image_count: number;
  /** 数え切る前に打ち切った（「N+」と表示する） */
  count_capped: boolean;
}

/** 取り込み元の画像ファイル1件 */
export interface SourceFile {
  name: string;
  path: string;
  size: number;
  mtime_ms: number;
  /** 実体がクラウド上にしかない（OneDrive等）。開くとダウンロードが走るので絵は出さない */
  offline: boolean;
}

/** 下の階層まで集めた取り込み元の画像（メディアを選んだだけで全部並べる経路） */
export interface SourceTree {
  files: SourceFile[];
  /** 多すぎて打ち切った（先頭ぶんだけ返っている） */
  truncated: boolean;
  /** 読めないフォルダがあった（取りこぼしの可能性） */
  incomplete: boolean;
}

/** 取り込み元フォルダ1階層分の中身 */
export interface SourceListing {
  dirs: SourceDir[];
  files: SourceFile[];
  /** フォルダが読めなかった（取り外された・権限がない） */
  unreadable: boolean;
}

/** カメラ別の枚数（左ペイン「カメラとメディア」） */
export interface Camera {
  name: string;
  count: number;
}

/** 詳細ビューアの撮影情報（開いた瞬間に実ファイルのEXIFから読む） */
export interface ExifInfo {
  camera: string | null;
  lens: string | null;
  /** "f/2.8" 等、単位付きの表示用文字列 */
  aperture: string | null;
  shutter: string | null;
  iso: string | null;
  focal: string | null;
  /** 撮影地（緯度, 経度） */
  gps: [number, number] | null;
}

/** 検索インデックスの初期構築の進捗（既存ライブラリの後追い索引化） */
export interface IndexProgress {
  building: boolean;
  /** 最後まで終えられなかった（次回起動で続きから再開する） */
  incomplete: boolean;
  /** "index"=全文索引の掃き寄せ / "camera"=カメラ情報の後追い補完 */
  phase: "index" | "camera";
  done: number;
  total: number;
}

/**
 * 実行環境に合わせた修飾キーの表記。
 * macOSは⌘、Windows/Linuxは Ctrl。`navigator.platform` は非推奨のため
 * userAgentData→userAgent の順に見る。
 */
export const isMac = (() => {
  const data = (navigator as Navigator & { userAgentData?: { platform?: string } })
    .userAgentData;
  const platform = data?.platform ?? navigator.userAgent;
  return /mac/i.test(platform);
})();

/**
 * Windowsか。Microsoft Storeの拡張機能へ誘導できるのはここだけなので、
 * デコーダの案内ボタンを出すかの判断に使う（macOSは最初から読め、Linuxは
 * 配布元のパッケージの話でStoreのページを見せても意味が無い）。
 */
export const isWindows = (() => {
  const data = (navigator as Navigator & { userAgentData?: { platform?: string } })
    .userAgentData;
  const platform = data?.platform ?? navigator.userAgent;
  return /win/i.test(platform);
})();

/** コマンドパレットのショートカット表記（⌘K / Ctrl+K） */
export const modKeyLabel = isMac ? "⌘K" : "Ctrl+K";

export const timelineSummary = (query: string, favoritesOnly: boolean) =>
  invoke<DaySummary[]>("timeline_summary", { query, favoritesOnly });
export const listDay = (
  dayKey: number,
  query: string,
  favoritesOnly: boolean,
) => invoke<MediaItem[]>("list_day", { dayKey, query, favoritesOnly });
export const listCameras = () => invoke<Camera[]>("list_cameras");

/** この環境で開けない形式があるか（HEIC/HEIFはOSのデコーダ頼み） */
export interface DecoderStatus {
  heif_total: number;
  /** 試せなかったときも true（確かめずに「使えない」と言わない） */
  heif_ok: boolean;
  /** 「入れ方を見る」の導線があるか（Windowsのみ） */
  help_available: boolean;
}
export const getDecoderStatus = () =>
  invoke<DecoderStatus>("decoder_status");
/**
 * デコーダの入れ方の案内を開く（Windowsのみ）。
 * `heif`＝無料のHEIF Image Extensions（コンテナ）／`hevc`＝有料のHEVC Video
 * Extensions（画素。動画とHEIC画像の両方に要る）。
 */
export const openDecoderHelp = (kind: "heif" | "hevc") =>
  invoke<void>("open_decoder_help", { kind });
/** 動画を開く前の状態（第9部）。ビューアで動画を開いた1回だけ聞く */
export interface VideoStatus {
  /** アプリ内で再生できるコンテナか */
  plays_in_app: boolean;
  /** クラウドにしか実体が無い（再生するとダウンロードが始まる） */
  cloud_only: boolean;
  /** 実ファイルがまだそこにあるか */
  exists: boolean;
}

export const videoStatus = (id: number) =>
  invoke<VideoStatus>("video_status", { id });

export const getExifInfo = (id: number) =>
  invoke<ExifInfo>("get_exif_info", { id });
export const getIndexProgress = () =>
  invoke<IndexProgress>("get_index_progress");
export const listMemories = () => invoke<Memory[]>("list_memories");
export const getStats = () => invoke<LibraryStats>("get_stats");
export const getStartupReport = () =>
  invoke<StartupScanReport | null>("get_startup_report");
// USB挿入の自動起動（AutoPlay）で `--import <ドライブ>` 付きで冷起動したときの
// 取り込み対象を一度だけ受け取る。2重起動は open-import-drive イベントで届く
export const takePendingImport = () =>
  invoke<string | null>("take_pending_import");
export const listDrives = () => invoke<DriveInfo[]>("list_drives");
export const syncNow = () => invoke<SyncStats>("sync_now");
export const getConfig = () => invoke<AppConfig>("get_config");
export const setImportDestination = (path: string) =>
  invoke<void>("set_import_destination", { path });
export const removeLibraryRoot = (path: string) =>
  invoke<SyncStats>("remove_library_root", { path });
export const setFavorite = (id: number, favorite: boolean) =>
  invoke<void>("set_favorite", { id, favorite });
export const importFromFolder = (source: string) =>
  invoke<ImportStats>("import_from_folder", { source });
export const addLibraryRoot = (path: string) =>
  invoke<SyncStats>("add_library_root", { path });
export const setVisiblePriority = (ids: number[]) =>
  invoke<void>("set_visible_priority", { ids });

/** 取り込み元フォルダを1階層だけ読む（DBには載せない） */
export const listSourceDir = (path: string) =>
  invoke<SourceListing>("list_source_dir", { path });
/** 取り込み元を下の階層まで走査して画像を集める */
export const listSourceTree = (path: string) =>
  invoke<SourceTree>("list_source_tree", { path });
/** 各ファイルが既に取り込み済みかを返す（サムネイル表示の後追いで塗る） */
export const probeImported = (paths: string[]) =>
  invoke<boolean[]>("probe_imported", { paths });
/** ウィザードで選んだファイルだけを取り込む */
export const importPaths = (paths: string[], sourceDir: string) =>
  invoke<ImportStats>("import_paths", { paths, sourceDir });

/** OS既定のアプリで開く */
export const openDefault = (id: number) => invoke<void>("open_default", { id });
/** エクスプローラー／Finderで場所を開いて選択する */
export const revealInFolder = (id: number) =>
  invoke<void>("reveal_in_folder", { id });
/** 指定した外部アプリで開く（rememberで設定に登録する） */
export const openWith = (id: number, appPath: string, remember: boolean) =>
  invoke<void>("open_with", { id, appPath, remember });
/** 登録済みの外部エディタを設定から外す */
export const forgetEditor = (appPath: string) =>
  invoke<void>("forget_editor", { appPath });
/** ゴミ箱へ移動してDBからも取り除く。戻り値は実際に削除できた件数 */
export const deleteMedia = (ids: number[]) =>
  invoke<number>("delete_media", { ids });
/** 取り込み先フォルダ構成の選択肢 */
export const listFolderPatterns = () =>
  invoke<FolderPattern[]>("list_folder_patterns");
/** 取り込み先フォルダ構成を設定して保存する */
/** 「このアプリについて」に出す情報。同梱していない文書のパスは null。 */
export interface AboutInfo {
  version: string;
  manual_path: string | null;
  licenses_path: string | null;
}

export const aboutInfo = () => invoke<AboutInfo>("about_info");

/** 同梱した文書をOSの既定のアプリで開く。 */
export const openBundledDoc = (kind: "manual" | "licenses") =>
  invoke<void>("open_bundled_doc", { kind });

/** 自由記述のパターンが実際どんなフォルダ名になるか（無害化まで通した結果）。 */
export const previewFolderPattern = (pattern: string) =>
  invoke<string>("preview_folder_pattern", { pattern });

export const setFolderPattern = (pattern: string) =>
  invoke<void>("set_folder_pattern", { pattern });

/**
 * media:// カスタムプロトコル経由のサムネイルURL（Base64禁止・直接ストリーミング）。
 * レスポンスはimmutableキャッシュされるため、内容が変わる要素（ファイル更新日時と
 * サムネイル品質段階）をURLのバージョンに含めて古いキャッシュを掴まないようにする。
 */
export const thumbSrcOf = (id: number, mtimeMs: number, thumbState: number) =>
  convertFileSrc(`thumb/${id}`, "media") + `?v=${mtimeMs}-${thumbState}`;

export const thumbSrc = (item: MediaItem) =>
  thumbSrcOf(item.id, item.mtime_ms, item.thumb_state);

/**
 * 取り込み**前**のファイルのプレビューURL（第5部 段階E）。
 *
 * DBに無いのでパスで指すしかないが、Windowsのパスは `\` `:` を含み、
 * WebViewによってURLの正規化のされ方が違う。**UTF-8バイト列の16進**で運べば
 * URLに現れるのは `[0-9a-f]` だけになり、どの環境でも同じに届く。
 */
export const sourceThumbSrc = (path: string, mtimeMs: number) => {
  const hex = Array.from(new TextEncoder().encode(path))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return convertFileSrc(`src/${hex}`, "media") + `?v=${mtimeMs}`;
};

/** 原寸画像のURL（詳細ビューア用） */
export const fullSrc = (id: number, mtimeMs: number) =>
  convertFileSrc(`full/${id}`, "media") + `?v=${mtimeMs}`;

/**
 * 動画の実体のURL（第9部）。`<video>` へ渡す。
 *
 * 画像と別の経路にしてあるのは、こちらだけがRangeリクエスト（部分要求）に
 * 応えるため。206を返せないとWebViewはシークを諦める。
 */
export const videoSrc = (id: number, mtimeMs: number) =>
  convertFileSrc(`video/${id}`, "media") + `?v=${mtimeMs}`;

/** day_key（YYYYMMDD整数）→ (年, 月, 日) */
export const splitDayKey = (dayKey: number): [number, number, number] => [
  Math.floor(dayKey / 10000),
  Math.floor(dayKey / 100) % 100,
  dayKey % 100,
];

