// **`window.confirm` は使えない**。TauriのWebViewでは何も出さずに true を返すので、
// 「確認したつもり」で消えてしまう。プラグインの confirm は本物のダイアログを出す
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import Calendar from "./Calendar";
import Palette, { type PaletteItem } from "./Palette";
import ContextMenu, { type MenuItem, type MenuPos } from "./ContextMenu";
import SettingsDialog from "./Settings";
import ImportWizard from "./ImportWizard";
import {
  addLibraryRoot,
  checkUpdate,
  withKind,
  cloudOnlyMedia,
  deleteMedia,
  openDownloadPage,
  fullSrc,
  videoSrc,
  videoStatus,
  getConfig,
  getExifInfo,
  getIndexProgress,
  getDecoderStatus,
  getEmptyLibraryReason,
  getStartupReport,
  startupScanFinished,
  getStats,
  listCameras,
  listDay,
  openDecoderHelp,
  listDrives,
  listMemories,
  modKeyLabel,
  openDefault,
  takePendingImport,
  openWith,
  removeLibraryRoot,
  revealInFolder,
  setFavorite,
  setFavorites,
  setPicked,
  setPickeds,
  exportMedia,
  listMediaIds,
  listMediaIdsBetween,
  scopeMedia,
  visibleMediaIds,
  setVisiblePriority,
  syncNow,
  thumbSrc,
  timelineSummary,
  type Camera,
  type DaySummary,
  type DriveInfo,
  type ExifInfo,
  type AppConfig,
  type DeleteProgress,
  type ExportProgress,
  type UpdateCheck,
  type ExternalApp,
  type ImportStats,
  type IndexProgress,
  type LibraryStats,
  type MediaFilter,
  type MediaKind,
  type MediaItem,
  type Memory,
  type ScopeItem,
  type StartupScanReport,
  type EmptyLibraryReason,
} from "./api";
import { usePlatform } from "./usePlatform";
import type { VideoStatus } from "./api";
import {
  formatDateTime,
  formatDayKey,
  formatDuration,
  formatNumber,
  t,
} from "./i18n";

const GAP = 4;
const HEADER_HEIGHT = 40;
/** グリッド左右のpadding（.grid-scrollのCSSと一致させる） */
const GRID_PADDING = 16;
/** 日キャッシュの上限（超えたら可視範囲外の古い日から間引く） */
const DAY_CACHE_MAX = 150;
const DAY_CACHE_TRIM_TO = 120;
/** 左ペインに常時見せるカメラの台数（多い人は畳んでおく） */
const CAMERAS_COLLAPSED = 3;

/**
 * 原寸の先読みが抱えてよい総画素（バイト）。
 *
 * 隠し `<img>` が保持するデコード済み画素には**総量の崖**がある。実測では
 * 24MPの7枚も12MPの14枚も641MiBで生き残り、687MiBで**全部まとめて捨てられた**
 * ——境界は約660MiBで、上限は枚数ではなく総画素。**崖は全か無か**で、1枚でも
 * 超えると先読み機構がまるごと無効になる。だからぎりぎりを狙わず下で止める。
 * この値はChromium側の判断で設定できず、機械や版で動きうる
 */
const PRELOAD_BUDGET_BYTES = 400 * 1024 * 1024;
/** DBに寸法が無い行（メタデータ未抽出）の見積り。多めに数える側へ倒す */
const PRELOAD_UNKNOWN_PIXELS = 24_000_000;
/** 前後それぞれ何枚まで候補にするか（たいていは上の予算が先に効く） */
const PRELOAD_MAX_ITEMS = 8;
/**
 * 裏で走らせてよい詰め直し（HEIC/RAW/TIFF）の枚数。
 * HEICは実測1枚0.6〜1秒（`bench --display-dir`）で、OSのWICデコードが
 * 1スレッドを持っていく。何枚も撒くと**表示中の1枚が飢える**ので、隣の1枚だけにする。
 * （RAWは横位置なら18msだが、縦位置は詰め直しに落ちるので同じ扱いにしてある）
 */
const PRELOAD_MAX_TRANSCODES = 1;
/** 一度にクラウド判定を聞ける件数（Rust側 `cloud_only_media` の上限と一致） */
const CLOUD_ASK_MAX = 64;
/**
 * 「ローカルにある」という答えの寿命。
 *
 * OneDriveの「空き容量を増やす」は、**中身を退避しても更新日時も大きさも
 * 変えない**。監視は更新日時と大きさの変化でしか `library-updated` を出さない
 * ので、ビューアを開いたままだと古い答えを信じ続け、先読みが
 * プレースホルダを触ってダウンロードを起こしうる。だから聞き直す
 */
const CLOUD_ANSWER_TTL_MS = 20_000;
/**
 * 仕掛かり中の詰め直しの画素を手放したとき、印を降ろすまでの待ち。
 * 変換の終わりが観測できなくなるので、時間で見切る（詰め直しは1枚0.6〜1秒。
 * mozjpeg化の前は最大1.1秒だったので、この4秒には余裕がある）
 */
const PENDING_WATCHDOG_MS = 4000;
/** 詰め直しの出力は長辺がここへ丸められる（`thumbs::display_jpeg` と一致） */
const DISPLAY_MAX_EDGE = 4096;
/**
 * 「連打で送っている最中」と見なす間隔。
 *
 * この速さで次の絵へ行っているあいだは、**詰め直しの要る形式の原寸を
 * すぐには取りに行かない**（0.2 ②の測り直し）。HEICは1枚0.6〜1秒かかり、
 * Rust側は要求のたびに**並列で**詰め直すので、通り過ぎた絵のぶんまで投げると
 * 止まった1枚がその競争に巻き込まれる。
 *
 * ゆっくり見ているとき（前の送りからこの時間より後）は今までどおり
 * すぐ取りに行く——1枚ずつ見る人に待ちを足さないため。
 *
 * **門が開くのは `settledId` と同じ250msの時計**なので、実効的に守れるのは
 * 250ms未満の連打だけ。250〜400msの間隔では「250ms遅れて要求が出る」形になる
 * （その速さなら見ている側なので、出してよい）。時計を2つ持たないための割り切り
 */
const FAST_FLIP_MS = 400;
/**
 * ビューア下部のフィルムストリップに出す**片側の枚数**（0.2 ②）。
 *
 * 出しているのは一覧と同じWebPサムネイル（長辺512px）なので、
 * 21枚でもデコード済み画素は約15MB——原寸の先読み予算
 * （[`PRELOAD_BUDGET_BYTES`]）に対して桁が2つ小さい。
 * 送るたびに端の1枚が増えるだけで、残りは要素ごと使い回される
 */
const STRIP_RADIUS = 10;

/**
 * その配信URLが、このidの絵か。
 *
 * **前方一致で見ない**——`full/1` は `full/12` にも当たるので、送った直後に
 * 遅れて届いた前の絵の完了を、新しい絵のものと取り違える。
 *
 * **`pathname` をそのまま比べてはいけない**。`convertFileSrc` は経路を
 * `encodeURIComponent` で包むので、`full/8132` は
 * `http://media.localhost/full%2F8132` として届く。素の `pathname` は
 * `/full%2F8132` で、`/full/8132` とは**永遠に一致しない**
 * （Rust側の `parse_media_url` も `percent_decode` してから見ている）。
 */
function isSrcOf(
  src: string,
  id: number,
  kind: "full" | "thumb" = "full",
): boolean {
  try {
    return decodeURIComponent(new URL(src).pathname) === `/${kind}/${id}`;
  } catch {
    // 壊れたパーセント記法は decodeURIComponent が投げる。ここも false 側へ
    return false;
  }
}

/** 拡張子がTIFFか（配信時に長辺を丸められる唯一の形式） */
const isTiffName = (name: string) => /\.tiff?$/i.test(name);

/**
 * **配信される絵の寸法**（原本の寸法とは限らない）。
 *
 * 原本と違うのは2通りある:
 *
 * - **RAW**は配るのが埋め込みプレビューで、原本より小さいことが多い
 *   （HDR PQのCR3は 6000x4000 に対して 1620x1080）。寸法はRust側が
 *   `preview_width`/`preview_height` に入れてくれる。ただし**見当**で、
 *   原寸プレビューを後ろに置く形式では届く絵のほうが大きい（下の `decodedBytes`）
 * - **TIFF**は長辺 [`DISPLAY_MAX_EDGE`] へ丸められる（`thumbs::display_jpeg`）。
 *   こちらは丸め方が決まっているのでここで計算する
 *
 * 下敷きの枠に原本の寸法を名乗らせると、**上限より大きな画面**では
 * 下敷きだけが大きく描かれ、差し替えで絵が縮む。
 */
function servedSize(item: MediaItem): [number, number] {
  const w = item.preview_width ?? item.width;
  const h = item.preview_height ?? item.height;
  const long = Math.max(w, h);
  if (!isTiffName(item.file_name) || long <= DISPLAY_MAX_EDGE) {
    return [w, h];
  }
  const k = DISPLAY_MAX_EDGE / long;
  return [Math.round(w * k), Math.round(h * k)];
}

/**
 * 先読みで抱えるデコード済み画素のバイト数（幅×高さ×4）。
 *
 * **デコード済み画素はOSのメモリ計に現れない**（10枚≒916MiB保持しても
 * WorkingSetは+19.5MB。discardable/GPUプロセス側にある）ので、崖を避けるには
 * 自前で数えるしかない。寸法はDBの値を使い、**ファイルには触らない**。
 */
function decodedBytes(it: MediaItem, measured?: [number, number]): number {
  // **一度でも実物が届いた絵は、その実寸で数える**。DBの寸法は当てにならない
  // ことがある——一覧が掴むのは「そのとき見つかった一番大きいプレビュー」で、
  // ファイル後方に原寸を置く形式（Ricoh GR IIIのDNG）では720x480が
  // 記録される一方、ビューアへ配信されるのは6000x4000＝95MiB。DBの値だけで
  // 数えると1.4MiBと見積もり、崖（実測660MiB）へ近づく（ゲート2のP3）
  if (measured && measured[0] > 0 && measured[1] > 0) {
    return measured[0] * measured[1] * 4;
  }
  // **数えるのは配信される絵**。RAWは原本（6000x4000）ではなく埋め込み
  // プレビュー（1620x1080）が届くので、原本で数えると予算を14倍に見積もって
  // 先読みの枚数が減る
  let w = it.preview_width ?? it.width;
  let h = it.preview_height ?? it.height;
  if (w <= 0 || h <= 0) return PRELOAD_UNKNOWN_PIXELS * 4;
  // 詰め直しの要る形式は**配信後の寸法**で数える。TIFFは `display_jpeg` が
  // 長辺4096へ丸めるので、24MPでも実画素は4096×2731＝44.7MiBしかない。
  // HEICは原寸で詰め直されるのでDBの寸法のまま
  const long = Math.max(w, h);
  if (isTiffName(it.file_name) && long > DISPLAY_MAX_EDGE) {
    const s = DISPLAY_MAX_EDGE / long;
    w = Math.round(w * s);
    h = Math.round(h * s);
  }
  return w * h * 4;
}

/** justifiedレイアウト済みのセル（表示px確定済み） */
type Cell = { item: MediaItem; w: number; h: number };

/**
 * 仮想スクロール用の行モデル。
 * スパースタイムライン: 全日付の骨組み（header）は常にあり、
 * 未取得の日は枚数から高さを見積もった placeholder 1行で表す。
 */
type Row =
  | { kind: "header"; dayKey: number; label: string; count: number }
  | { kind: "cells"; dayKey: number; cells: Cell[]; height: number }
  | { kind: "placeholder"; dayKey: number; height: number };

/** ビューアの位置。日をまたぐ移動先が未取得でも指せるよう "first"/"last" を許す */
type ViewerPos = { dayKey: number; id: number | "first" | "last" };

/** ビューアで判定したときに一瞬出す合図の種類 */
type JudgeFlashKind = "fav" | "unfav" | "pick" | "unflag" | "reject";

/** その合図の見た目と文言（**判定ごとに1行**。増えたらここへ足す） */
const JUDGE_FLASH: Record<JudgeFlashKind, { mark: string; word: () => string }> =
  {
    fav: { mark: "★", word: () => t.judgeFav },
    unfav: { mark: "☆", word: () => t.judgeUnfav },
    pick: { mark: "⚑", word: () => t.judgePick },
    unflag: { mark: "⌫", word: () => t.judgeUnflag },
    reject: { mark: "✕", word: () => t.viewerRejected },
  };

/**
 * 画面左に並べる種類（[`MediaKind`] の "all" 以外）。**この順で出す**
 * ——枚数の多い順。RAW を撮る人は動画も撮るが、逆は少ない
 */
const KINDS = ["photo", "raw", "video"] as const satisfies readonly MediaKind[];

/** 種類の名前（辞書は読み込み時に決まるので、[`JUDGE_FLASH`] と同じく遅らせて引く） */
const KIND_LABEL: Record<(typeof KINDS)[number], () => string> = {
  photo: () => t.kindPhoto,
  raw: () => t.kindRaw,
  video: () => t.kindVideo,
};

/** ⚡爆速メーターの表示文言（起動時同期の方式と成果） */
function speedLabel(r: StartupScanReport): string {
  const sec = (r.elapsed_ms / 1000).toFixed(r.elapsed_ms < 1000 ? 2 : 1);
  const diff =
    r.added || r.changed || r.removed
      ? t.speedDiff(r.added, r.changed, r.removed)
      : t.speedNoDiff;
  const head = t.speedPrefix(sec);
  if (r.method === "usn") {
    const detail =
      r.usn_records === 0
        ? t.speedUsnNoChange
        : t.speedUsnDirty(r.usn_records, r.dirty_dirs);
    return `${head}${t.speedUsn}${detail}${diff}`;
  }
  if (r.method === "pruned") {
    return `${head}${t.speedPruned(r.skipped_dirs)}${diff}`;
  }
  return `${head}${t.speedFull(r.total)}${diff}`;
}

/** アスペクト比（幅/高さ）。未抽出は4:3、極端なパノラマ等はレイアウト崩壊しない範囲に制限 */
function aspectOf(item: MediaItem): number {
  if (item.width > 0 && item.height > 0) {
    return Math.min(5, Math.max(0.33, item.width / item.height));
  }
  return 4 / 3;
}

/** 「今後表示しない」を覚えておく先（案内は毎回出すと邪魔になる） */
const DECODER_NOTICE_KEY = "pictkura.decoderNotice";

export default function App() {
  /** タイムラインの骨組み（日付→枚数、新しい日付順）。全件レコードは持たない */
  const [summary, setSummary] = useState<DaySummary[]>([]);
  /**
   * いまの `summary` が、いまの絞り込みに対する**確定した答え**か。
   *
   * `reloadAll` の入口で偽に戻す。**0件の検索を消した直後**のように、
   * 絞り込みだけ先に変わって `summary` が古いままの瞬間があり、そこで
   * 「まだ写真がありません」と出すと**数千枚あるライブラリの上に**出る
   */
  const [settled, setSettled] = useState(false);
  /**
   * 直前の `reloadAll` が**落ちた**か。
   *
   * `settled` を `finally` で立てるのは**仕掛けを詰まらせない**ため
   * （立てないと、呼び出し側がリトライしないので起動中ずっと偽のまま固まる）。
   * だがそれだけだと、**取得に失敗しただけの一覧の上に**「まだ写真がありません」が
   * 出る——検索を消した直後に一度転べば、5万枚のライブラリに向かって
   * 「カードから取り込んでください」と言うことになる。
   * **2つを分ける**: `settled` は仕掛けのため、これは名乗ってよいかの判断のため。
   * 失敗そのものは上部の帯に出たまま残るので、黙ってはいない
   */
  const [loadFailed, setLoadFailed] = useState(false);
  /**
   * 骨組みが**最後に取れた**のは何回目か。
   *
   * `reloadAll` と `refreshSummary` は同じ骨組みを取り直す別の口で、
   * 世代（`generationRef`）は共有している——`refreshSummary` は追い越しを
   * 起こさないよう番号を進めない。そのため「先に `refreshSummary` が成功して、
   * あとから同じ世代の `reloadAll` が転ぶ」順があり、**取れたばかりのデータの
   * 上に「出せませんでした」が出て居座る**（ゲート2の指摘）。
   * 転んだ側は、自分が走り出したあとに誰かが成功していないかをこれで見る
   */
  const gotSummaryRef = useRef(0);
  /**
   * [`settled`] の**同じ瞬間に読める写し**。
   *
   * 絞り込みが変わった直後の1描画では、`filtering` はもう偽なのに
   * `summary` は前の絞り込みのまま——`settled` の `false` はまだ描画に
   * 反映されていないので、そこで走る効果は「空だ」と読んでしまう。
   * 画面には出ない（`emptyReason` がまだ無い）が、**ルート直下を全部
   * `read_dir` する問い合わせが1回無駄に飛ぶ**——ネットワークのルートなら
   * ブロッキングのスレッドをマウントのタイムアウトぶん占める（ゲート2の指摘）。
   * 効果は発火の瞬間にこちらを見る
   */
  const settledRef = useRef(false);
  /**
   * 起動時の走査が報告を出したか。**出るまで「空です」と言わない**。
   *
   * 走査は別スレッドで走り、終わってから `library-updated` と
   * `startup-scan-report` を出す。最初の `reloadAll` はそれより前にDBを読むので、
   * **初回起動では正しく `[]` が返る**——そこで理由を出すと、索引中のライブラリに
   * 「扱える画像がありません」と言うことになる（ゲート2の指摘）。
   *
   * **時間で見切らない。** 初回のNASや10万枚のフォルダは走査だけで
   * 数十秒かかる——そこで待つのをやめると、索引の最中に「空です」と
   * 言うことになり、この旗を足した意味が消える（同）
   */
  const [scanSettled, setScanSettled] = useState(false);
  /** 取得済みの日 → その日のレコード（可視範囲＋α だけを保持） */
  const [dayItems, setDayItems] = useState<Map<number, MediaItem[]>>(
    () => new Map(),
  );
  const [stats, setStats] = useState<LibraryStats>({
    total: 0,
    favorites: 0,
    picked: 0,
  });
  const [memories, setMemories] = useState<Memory[]>([]);
  const [cellSize, setCellSize] = useState(180);
  const [status, setStatus] = useState("");
  const [folderInput, setFolderInput] = useState("");
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [roots, setRoots] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"grid" | "calendar">("grid");
  const [filter, setFilter] = useState<MediaFilter>("all");
  /**
   * 一覧が空のときだけ聞く「なぜ空か」。
   *
   * **無言で `0 件` を出さない**ための説明で、`null` は「まだ聞いていない」。
   * 常時聞かないのは、ルートの直下を1回読むため——空でないときに払う理由が無い
   */
  const [emptyReason, setEmptyReason] = useState<EmptyLibraryReason | null>(
    null,
  );
  /**
   * 種類の絞り込み（画像 / RAW / 動画）。**★ / ⚑ とは別の軸**なので重ねて効く。
   * 送るときは検索語の `kind:` に畳む（[`withKind`]）
   */
  const [kind, setKind] = useState<MediaKind>("all");
  /**
   * 選択中のID。**空なら選択モードではない**——モードを別の状態で持つと、
   * 「選択が0なのにモードだけ残る」というずれ方をする。
   */
  const [selected, setSelected] = useState<ReadonlySet<number>>(new Set());
  /** Shift+クリックの起点。最後に単独で触ったもの */
  const [anchorId, setAnchorId] = useState<number | null>(null);
  /** 検索ボックスの入力（打鍵ごとの値） */
  const [queryInput, setQueryInput] = useState("");
  /** 実際にバックエンドへ投げている検索クエリ（入力のデバウンス後） */
  const [query, setQuery] = useState("");
  /**
   * 何かで絞り込んでいるか（★ / ⚑ / 種類 / 検索語のどれか）。
   * 絞っているあいだは「N年前の今日」を出さない——絞り込みの外から
   * 拾ってきた写真が混じって見え、いま見ている列と食い違う
   */
  const filtering = filter !== "all" || kind !== "all" || query !== "";
  /** カメラ別の枚数（左ペインとコマンドパレットの候補） */
  const [cameras, setCameras] = useState<Camera[]>([]);
  /** カメラ一覧を全部見せるか（既定は上位数台だけ） */
  const [camerasExpanded, setCamerasExpanded] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  /** ショートカット一覧（`?` / `F1`）。ビューアの上にも出す */
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  /** 右クリックメニューの表示位置と対象（nullで非表示） */
  const [menu, setMenu] = useState<{ pos: MenuPos; item: MediaItem } | null>(
    null,
  );
  /** 登録済みの外部編集アプリ（設定から読む） */
  const [editors, setEditors] = useState<ExternalApp[]>([]);
  /** 設定ダイアログの表示と、その中身になる設定スナップショット */
  const [settingsOpen, setSettingsOpen] = useState(false);
  // 取り込みウィザード（第5部 段階E）。startPathはドライブから開いたときの初期フォルダ
  const [wizardOpen, setWizardOpen] = useState(false);
  const [wizardStart, setWizardStart] = useState<string | undefined>(undefined);
  const [wizardNonce, setWizardNonce] = useState(0);
  const [config, setConfig] = useState<AppConfig | null>(null);
  /** ビューアの撮影情報パネル（iキーで開閉） */
  const [showExif, setShowExif] = useState(false);
  const [exif, setExif] = useState<ExifInfo | null>(null);
  /** カレンダーの日クリックでグリッドのこの日付見出しへスクロールする */
  const [pendingScrollDay, setPendingScrollDay] = useState<number | null>(null);
  /** 詳細ビューアの位置（nullで非表示） */
  const [viewer, setViewer] = useState<ViewerPos | null>(null);
  /**
   * ビューアの**選択スコープ**（0.2 ②）。複数選択から開いたときだけ入り、
   * 送りも分母もこの列の中だけになる（連写の集中選別のため）。
   *
   * 中身は開く瞬間にRust側で「いま並んでいるものだけ」へ絞って並べたもの
   * （`scopeMedia`）。**あとから作り直さない**——見ている最中に一覧が
   * 変わっても、選んだ範囲を歩き切れるほうが選別の道具として正しい
   */
  const [viewerScope, setViewerScope] = useState<ScopeItem[] | null>(null);
  /**
   * ボツの候補（0.2 ③）。`X` で付く印で、**ファイルは1バイトも動かさない**。
   *
   * 判定のたびにゴミ箱へ入れる形は測って捨てた（1件あたり中央20ms・最悪215msの
   * 固定費が判定のリズムに刺さる。`dev/plan.0.2.research.md` §2-1）。
   * 印だけなら判定の追加コストは0で、確定するまで何も失われない。
   *
   * **idの集合ではなく写真そのものを持つ**。関所（確認のモーダル）で
   * サムネイルと名前を出すのに要るが、日をまたいで印を付けたあと
   * その日のキャッシュが間引かれると、idからは引き直せなくなる。
   *
   * 不変条件: **ビューアが閉じている間は常に空**（確定・破棄のどちらでも
   * 空にしてから閉じる）。閉じたまま印だけが残ると、次に開いたときに
   * 身に覚えのない✕が並ぶ
   */
  const [rejected, setRejected] = useState<Map<number, MediaItem>>(new Map());
  /**
   * 関所（ボツをゴミ箱へ入れる前の確認）。`closeAfter` は
   * **ビューアを閉じようとして開いた**か（true）、チップからの途中確認か（false）。
   * `quitAfter` は**窓の×から開いた**——片付いたら窓ごと閉じる（利用者の元の意思は
   * 「アプリを終わる」なので、ビューアだけ閉じて残ると×が効かないように見える）。
   */
  const [rejectGate, setRejectGate] = useState<{
    closeAfter: boolean;
    quitAfter?: boolean;
  } | null>(null);
  /** 窓の×の受け口（登録は一度きり）から、いま関所が出ているかを見るための控え */
  const rejectGateRef = useRef(rejectGate);
  useEffect(() => {
    rejectGateRef.current = rejectGate;
  }, [rejectGate]);
  /** ゴミ箱へ移動している最中か（**実機の500件で約4.9秒**・2026-08-19の実測） */
  const [trashing, setTrashing] = useState(false);
  /** 窓の×の受け口（登録は一度きり）から、移動中かどうかを見るための控え */
  const trashingRef = useRef(false);
  /**
   * 移動中に×が押されたら、ここに控えて**終わってから**閉じる。
   * 途中で窓を壊すと、ゴミ箱へ入れ終えたぶんがDBから落ちないまま残る
   * （ゲート1の指摘）。次の起動の同期で拾い直せるとはいえ、
   * **自分から不整合を作る道は塞ぐ**
   */
  const quitAfterTrashRef = useRef(false);
  /**
   * その4.9秒のあいだの進み具合（`delete-progress`）。**待たせる時間が3秒を
   * 超えたら件数を出す**——「動いている」ことを、止まっていない証拠として見せる
   * （`dev/plan.0.2.rev.md` 3-3）。始まる前は `null`
   */
  const [trashProgress, setTrashProgress] = useState<DeleteProgress | null>(
    null,
  );
  useEffect(() => {
    trashingRef.current = trashing;
  }, [trashing]);
  /** 窓を閉じる要求など、**登録が1回きりの経路**から今の印を見るための控え */
  const rejectedRef = useRef(rejected);
  useEffect(() => {
    rejectedRef.current = rejected;
  }, [rejected]);
  /** ビューアのズーム・パン・スライドショー。zoomは「画面に収めた状態」を1とする倍率 */
  const [zoom, setZoom] = useState(1);
  /**
   * 等倍(100%)を選んだ写真のid。倍率そのものではなく「選んだ」を覚える。
   *
   * 分母（[`fitScale`]）は後から変わる——絵が届いて実寸が分かったときと、
   * ウィンドウの大きさが変わったとき。倍率だけを覚えていると、そのたびに
   * 100%から外れる（絵が出る前に等倍へ切り替えると、DBの寸法で決めた倍率が
   * そのまま残る。ゲート1のP2）。
   *
   * **真偽値ではなくidで持つ**のが要点。写真を送ると「ズームのリセット」と
   * 「分母が変わったので取り直す」が同じ描画で走り、後者が勝って**次の写真が
   * 100%で開く**（ゲート2のP2）。idなら送った時点で持ち主が変わるので、
   * 取り直しは自分の写真にしか効かない
   */
  const [pinnedActualId, setPinnedActualId] = useState<number | null>(null);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [playing, setPlaying] = useState(false);
  /** ビューアのUI（キャプション・ツール・矢印）を隠しているか（マウス静止で自動） */
  const [viewerIdle, setViewerIdle] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  /** ウィンドウサイズ。表示倍率（実ピクセル比）の計算に使う */
  const [windowSize, setWindowSize] = useState({
    w: window.innerWidth,
    h: window.innerHeight,
  });
  const dragRef = useRef<{
    startX: number;
    startY: number;
    panX: number;
    panY: number;
  } | null>(null);
  // イベントリスナーから最新のfilter/query状態を参照するためのref
  const filterRef = useRef<MediaFilter>("all");
  /**
   * Shift+クリックの土台。**範囲を始めた時点の選択**を覚える。
   *
   * 同じ起点から選び直すときは、前の範囲を消して新しい範囲を足す——のではなく、
   * **土台に新しい範囲を足し直す**。前の範囲をIDごと引くと、
   * **範囲より前に選んでいたもの**まで巻き添えで消える（範囲の中にたまたま
   * 入っていた場合）。
   */
  const lastRangeRef = useRef<{
    anchor: number;
    base: ReadonlySet<number>;
  } | null>(null);
  /**
   * 選択操作の世代。**解除するたびに進める**。
   *
   * 検索条件やライブラリの世代とは別に要る——Escで解除しても条件は変わらないので、
   * それだけでは「待っている最中に解除されたか」を見分けられず、
   * `listMediaIds` の応答が**消したはずの選択を作り直す**。
   */
  const selectEpochRef = useRef(0);
  /** 書き出しが走っている印。**ダイアログを開く前**に立てて二度押しを断る */
  const exportingRef = useRef(false);
  /**
   * ゴミ箱への移動が走っているか。**一覧からの削除と1枚の削除で共有する**
   * ——別スレッドへ出したぶん、走っている最中も画面は動く（ゲート1の指摘）
   */
  const deletingRef = useRef(false);
  /**
   * 選択操作を1つ始める。**世代を進めて、待っている古い応答を無効にする**。
   *
   * 解除だけでなく**あらゆる選択操作**で進めるのが要点。3万件のライブラリで
   * Ctrl+Aの応答を待つ間にタイルを1枚触ると、あとから返ってきた全選択が
   * **新しい方の選択を上書きする**。
   */
  const beginSelectOp = () => (selectEpochRef.current += 1);
  /**
   * キー操作のハンドラから読む最新の値。
   * 依存に入れるとキーを押すたびにリスナーを張り替えることになる。
   */
  const selectedRef = useRef<ReadonlySet<number>>(new Set());
  const selectAllRef = useRef<() => Promise<void>>(async () => {});
  const clearSelectionRef = useRef<() => void>(() => {});
  const queryRef = useRef("");
  /** フィルタ切替・全体再読込のたびに増える世代番号。古い応答を捨てる */
  const generationRef = useRef(0);
  /** 取得中の日（重複リクエスト防止） */
  const inflightRef = useRef(new Set<number>());
  /** 現在可視の日（キャッシュ間引きから保護する） */
  const visibleDaysRef = useRef(new Set<number>());
  const scrollRef = useRef<HTMLDivElement>(null);
  const [viewportWidth, setViewportWidth] = useState(0);

  // グリッド幅の変化（ウィンドウリサイズ・ビュー切替）を追跡して行組みを再計算する
  useEffect(() => {
    if (view !== "grid") return;
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewportWidth(el.clientWidth));
    ro.observe(el);
    setViewportWidth(el.clientWidth);
    return () => ro.disconnect();
  }, [view]);

  /** 骨組み（サマリ・件数・思い出）を取り直し、日キャッシュを捨てる。
   * 可視範囲の日は placeholder になった瞬間に自動で再取得される。
   *
   * **落ちても `settled` は立てる**（`finally`）。立てないと、
   * 呼び出し側は例外を `status` へ流すだけでリトライしないので、
   * `settled` が**その起動のあいだ偽のまま固まり**、この機能が丸ごと出なくなる。
   * ただし立てるだけだと今度は**失敗を「空」と読む**ので、
   * 落ちたことは [`loadFailed`] に分けて持つ */
  const reloadAll = useCallback(async () => {
    const gen = ++generationRef.current;
    const wasGotAt = gotSummaryRef.current;
    inflightRef.current.clear();
    // **同期で倒す。** 効果は同じ描画の中で順に走るので、状態の更新を待つと
    // 後ろの効果が古い値を見る
    settledRef.current = false;
    setSettled(false);
    try {
      const [sum, st, mem] = await Promise.all([
        timelineSummary(queryRef.current, filterRef.current),
        getStats(),
        listMemories(),
      ]);
      // 応答待ちの間に次のリロード（フィルタ切替等）が始まっていたら、古い応答は捨てる
      if (generationRef.current !== gen) return;
      setSummary(sum);
      setStats(st);
      setMemories(mem);
      setDayItems(new Map());
      if (generationRef.current === gen) {
        gotSummaryRef.current += 1;
        setLoadFailed(false);
      }
    } catch (err) {
      // **自分が走っている間に誰かが成功していたら、失敗を名乗らない**
      if (
        generationRef.current === gen &&
        gotSummaryRef.current === wasGotAt
      ) {
        setLoadFailed(true);
      }
      throw err;
    } finally {
      // **追い越されていたら立てない**——新しい方がまだ走っている
      if (generationRef.current === gen) {
        settledRef.current = true;
        setSettled(true);
      }
    }
  }, []);

  /** サマリ・件数だけを取り直す（日キャッシュは基本的に維持。部分更新の整合回復用）。
   *
   * 取り直したサマリの枚数と、キャッシュ済みの日の実際の件数がズレていたら、
   * その日だけキャッシュを捨てて取り直させる。ズレは
   * 「撮影日が確定して別の日から移ってきた」「メタデータ抽出で検索条件に
   * 合致するようになった」等で起きる。listDayは常にその日の全件を返すので、
   * 「枚数が違う＝キャッシュが古い」は確実に成り立つ */
  const refreshSummary = useCallback(async () => {
    const gen = generationRef.current;
    const [sum, st] = await Promise.all([
      timelineSummary(queryRef.current, filterRef.current),
      getStats(),
    ]);
    if (generationRef.current !== gen) return; // リロードが割り込んだら捨てる
    setSummary(sum);
    // **成功したら失敗の表示を下ろす。** ここは `reloadAll` と同じ骨組みを
    // 取り直している。下ろさないと、1回転んだあと部分更新が何度成功しても
    // 「一覧を出せませんでした」が居座る——次の `reloadAll` まで固まる
    // （ゲート1の指摘）
    gotSummaryRef.current += 1;
    setLoadFailed(false);
    setStats(st);
    setDayItems((prev) => {
      if (prev.size === 0) return prev;
      const counts = new Map(sum.map((d) => [d.day_key, d.count]));
      let next: Map<number, MediaItem[]> | null = null;
      for (const [dayKey, items] of prev) {
        if (counts.get(dayKey) !== items.length) {
          if (!next) next = new Map(prev);
          next.delete(dayKey);
        }
      }
      return next ?? prev;
    });
  }, []);
  const summaryRefreshTimer = useRef<number | null>(null);
  // サムネイル一括生成中はパッチが大量に届くため、骨組みの再取得は2秒デバウンス
  // （最後のパッチの2秒後に必ず1回走り、最終状態には確実に追従する）
  const scheduleSummaryRefresh = useCallback(() => {
    if (summaryRefreshTimer.current != null) return;
    summaryRefreshTimer.current = window.setTimeout(() => {
      summaryRefreshTimer.current = null;
      refreshSummary().catch(() => {});
    }, 2000);
  }, [refreshSummary]);

  /** ビューアが開いている間、その表示日をキャッシュ間引きから保護する */
  const viewerDayRef = useRef<number | null>(null);

  /** 1日分をオンデマンド取得する（重複・世代違いは無視。失敗は2回まで自動再試行） */
  const loadDay = useCallback((dayKey: number, attempt = 0) => {
    if (inflightRef.current.has(dayKey)) return;
    inflightRef.current.add(dayKey);
    const gen = generationRef.current;
    listDay(dayKey, queryRef.current, filterRef.current)
      .then((items) => {
        inflightRef.current.delete(dayKey);
        if (generationRef.current !== gen) return; // フィルタ切替等で無効化済み
        setDayItems((prev) => {
          const next = new Map(prev);
          next.set(dayKey, items);
          // キャッシュ上限を超えたら、可視範囲外の古い日から間引く（挿入順=FIFO）。
          // ビューアの表示日はスライドショー中に消えないよう保護する
          if (next.size > DAY_CACHE_MAX) {
            for (const key of next.keys()) {
              if (next.size <= DAY_CACHE_TRIM_TO) break;
              if (
                key !== dayKey &&
                key !== viewerDayRef.current &&
                !visibleDaysRef.current.has(key)
              ) {
                next.delete(key);
              }
            }
          }
          return next;
        });
      })
      .catch((e) => {
        inflightRef.current.delete(dayKey);
        setStatus(String(e));
        // 一時的な失敗（DBロック競合等）に備え、可視のままなら少し待って再試行。
        // それでも失敗が続く場合はスクロール等で可視範囲が変わったときに再試行される
        if (attempt < 2) {
          window.setTimeout(() => {
            if (
              generationRef.current === gen &&
              visibleDaysRef.current.has(dayKey)
            ) {
              loadDay(dayKey, attempt + 1);
            }
          }, 2000);
        }
      });
  }, []);

  /** カメラ別の枚数（左ペイン＋パレットの候補）。メタデータ抽出が進むと
   * 増えるため、ライブラリ更新のたびに取り直す */
  const refreshCameras = useCallback(async () => {
    try {
      setCameras(await listCameras());
    } catch {
      /* カメラ集計の失敗は無視（次の更新で再試行される） */
    }
  }, []);

  const refreshRoots = useCallback(async () => {
    try {
      const config = await getConfig();
      setConfig(config);
      setRoots(config.library.roots);
      setEditors(config.editors?.apps ?? []);
    } catch {
      /* 設定取得失敗は無視 */
    }
  }, []);

  useEffect(() => {
    refreshRoots();
  }, [refreshRoots]);

  // ライブラリ更新（起動時同期・取り込み・ファイル監視）で骨組みを取り直す。
  // リスナー登録を先に済ませてから初回取得する（登録前にイベントが
  // 発火して取りこぼす競合を防ぐ）。クリーンアップが登録完了より先に走った場合
  // （StrictModeの二重マウント等）もリスナーを漏らさない
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      // **登録が転んでも下の初回取得へ進む。** ここで例外が上がると
      // `reloadAll` に届かず、`settled` がその起動のあいだ偽のまま固まる——
      // **データも説明も出ない完全な無言**になる（ゲート1の指摘。
      // 隣のリスナーには入れてあるのに、こちらだけ抜けていた）
      const f = await listen("library-updated", () => {
        reloadAll().catch((e) => setStatus(String(e)));
        refreshCameras();
        // クラウド判定の覚えも捨てる。OneDriveは「空き容量を増やす」で
        // 実体を後から退避するので、古い「ローカルにある」を信じない
        cloudOnlyRef.current.clear();
      }).catch(() => null);
      if (cancelled) {
        f?.();
        return;
      }
      unlisten = f ?? undefined;
      await reloadAll().catch((e) => setStatus(String(e)));
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [reloadAll, refreshCameras]);

  // ⚡爆速メーター: 起動時同期の方式と所要時間を数秒だけトースト表示する。
  // USN差分は数msで完了し、リスナー登録前にイベントが発火して取りこぼされる
  // ことがあるため、登録後にコマンドでも一度取りに行く（両方来ても表示は同じ）
  const [speedReport, setSpeedReport] = useState<StartupScanReport | null>(null);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    const show = (report: StartupScanReport) => {
      if (cancelled) return;
      setSpeedReport(report);
      if (timer != null) window.clearTimeout(timer);
      timer = window.setTimeout(() => setSpeedReport(null), 8000);
    };
    (async () => {
      const f = await listen<StartupScanReport>("startup-scan-report", (ev) =>
        show(ev.payload),
      );
      if (cancelled) {
        f();
        return;
      }
      unlisten = f;
      const missed = await getStartupReport().catch(() => null);
      if (missed) show(missed);
    })();
    return () => {
      cancelled = true;
      unlisten?.();
      if (timer != null) window.clearTimeout(timer);
    };
  }, []);

  // 起動時の同期が**終わった**合図。⚡爆速メーターの報告とは別に要る——
  // 走査が落ちれば報告は一度も出ないので、来ないことを「まだ走っている」と
  // 読むと**永久に無言**になる（ゲート2の指摘）。バックエンドは成功でも失敗でも
  // これを出し、取りこぼした側が後から聞けるよう旗も立てている
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let poll: number | undefined;
    const settle = () => {
      if (cancelled) return;
      if (poll != null) window.clearInterval(poll);
      poll = undefined;
      setScanSettled(true);
    };
    (async () => {
      // **登録が転んでも先へ進む。** ここで例外が上がると下の問い合わせに
      // 届かず、`scanSettled` がその起動のあいだ偽のまま固まる——
      // つまり**この機能が丸ごと出なくなる**。
      // 隣の⚡爆速メーターは取りこぼしても帯が1つ出ないだけだが、こちらは違う
      const f = await listen("startup-scan-finished", settle).catch(() => null);
      if (cancelled) {
        f?.();
        return;
      }
      unlisten = f ?? undefined;
      // **転んだときに倒す向きは、リスナーの有無で決める。** リスナーが
      // 生きているなら合図が来るので待ってよい——ここで一律に「終わった」と
      // 言うと、走査の最中にパネルが出て `scanSettled` の目的が崩れる
      const done = await startupScanFinished().catch(() => f === null);
      if (cancelled) return;
      if (done) {
        settle();
        return;
      }
      // **まだ終わっていない。ここから先は必ず自分でも聞きに行く。**
      // リスナーの有無で分けてはいけない——合図は**一度きり**で、
      // 空のライブラリなら数msで出る。登録が間に合わず、しかもこの
      // 問い合わせが転ぶと、`false` を掴んだまま二度と来ない合図を待つ
      // ことになり、この機能が丸ごと出ない（ゲート2の指摘）。
      // 合図が先に来れば `settle` が止めるので、二重には走らない
      poll = window.setInterval(() => {
        void startupScanFinished()
          // **転んだ1回で「終わった」と言わない。** ここで倒すと、
          // NASの初回走査の最中に一度失敗しただけでパネルが出る。
          // 次の2秒後にもう一度聞けばよい
          .catch(() => false)
          .then((ok) => {
            // **終わったら止める。** 片付けはAppのunmountでしか走らないので、
            // 止めないとプロセスの一生ぶん2秒ごとに叩き続ける
            if (ok) settle();
          });
      }, 2000);
    })();
    return () => {
      cancelled = true;
      unlisten?.();
      if (poll != null) window.clearInterval(poll);
    };
  }, []);

  // この環境で開けない形式の案内（第7部 段階G-6）。
  //
  // AVIFはデコーダを同梱しているので常に出せるが、HEIC（HEVC）は特許の都合で
  // 同梱していないためOSのデコーダが要る。無い環境ではサムネイルが付かないので、
  // **黙って空欄にせず理由を伝える**。ライブラリにHEICが1枚も無ければ出さない。
  const [heifMissing, setHeifMissing] = useState<number | null>(null);
  const [decoderHelp, setDecoderHelp] = useState(false);
  const platform = usePlatform();
  // 動画（第9部）。コンテナはWebViewが扱えても、中のコーデック（HEVC）が
  // OSに無ければ再生は失敗する。しかも `canPlayType` は当てにならない
  // （実測: hvc1 に空を返しながら普通に再生した）ので、**実際に失敗してから**
  // 逃げ道を出す
  const [videoError, setVideoError] = useState(false);
  // 動画を開いた瞬間に1回だけ聞く「これは再生できるか」。
  // クラウドのみ・ファイル欠けを**開く前に**知るためで、これが返るまで
  // `<video>` は出さない（出すと実ファイルを取りに行ってしまう）
  const [videoInfo, setVideoInfo] = useState<VideoStatus | null>(null);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  // ライブラリの中身は取り込みやルート追加で増える。起動時に1回だけ見ると、
  // 「最初は空だったので黙っていた」まま次の起動まで案内が出ない
  const checkDecoders = useCallback(() => {
    if (localStorage.getItem(DECODER_NOTICE_KEY) === "off") return;
    getDecoderStatus()
      .then((s) => {
        setDecoderHelp(s.help_available);
        setHeifMissing(!s.heif_ok && s.heif_total > 0 ? s.heif_total : null);
      })
      .catch(() => {});
  }, []);
  useEffect(() => {
    checkDecoders();
  }, [checkDecoders]);


  // 検索ボックスの入力をデバウンスして実クエリへ落とす。
  // 打鍵のたびにIPCを撃たないが、体感で待たされない間隔にする
  useEffect(() => {
    const trimmed = queryInput.trim();
    if (trimmed === query) return;
    const t = window.setTimeout(() => setQuery(trimmed), 160);
    return () => window.clearTimeout(t);
  }, [queryInput, query]);

  // フィルタ・検索クエリの変更で骨組みを読み込み直す（初回はマウント時のeffectが行う）
  const filterInitRef = useRef(true);
  useEffect(() => {
    filterRef.current = filter;
    // **種類は検索語に畳んで持つ**。バックエンドへ渡る経路が一本になるので、
    // 一覧・サマリ・全選択・範囲選択・削除前の確認のどれも取りこぼさない。
    // 「絞り込み中はサムネイル完成の差分でその日を捨て直さない」判断
    // （`applyPatches`）も、種類で絞っているあいだは検索と同じ扱いになる
    queryRef.current = withKind(query, kind);
    if (filterInitRef.current) {
      filterInitRef.current = false;
      return;
    }
    reloadAll().catch((e) => setStatus(String(e)));
  }, [filter, query, kind, reloadAll]);

  useEffect(() => {
    refreshCameras();
  }, [refreshCameras]);

  // 検索インデックスの初期構築（既存ライブラリの後追い索引化）の進捗。
  // 構築中は検索結果が不完全になりうるので、その旨をUIに出す
  const [indexProgress, setIndexProgress] = useState<IndexProgress | null>(null);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let unlistenCameras: (() => void) | undefined;
    let unlistenExport: (() => void) | undefined;
    let unlistenDelete: (() => void) | undefined;
    (async () => {
      const f = await listen<IndexProgress>("index-progress", (ev) => {
        // 中断した場合は「終わった」と誤解させないよう表示を残す
        const p = ev.payload;
        if (!cancelled) setIndexProgress(p.building || p.incomplete ? p : null);
      });
      // 書き出しの進捗はステータス行に出す（枚数が多いと数分かかる）
      const exportProgress = await listen<ExportProgress>(
        "export-progress",
        (ev) =>
          setStatus(
            t.exporting(ev.payload.done, ev.payload.total, ev.payload.name),
          ),
      );
      if (cancelled) exportProgress();
      else unlistenExport = exportProgress;
      // ゴミ箱への移動の進捗。関所が出ているならボタンの文字が受けるので、
      // ここでは触らない。**一覧からのまとめて削除には関所が無い**ので、
      // そのときだけステータス行に出す（数千枚選べる以上、待つ間の手掛かりが要る）
      const deleteProgress = await listen<DeleteProgress>(
        "delete-progress",
        (ev) => {
          if (cancelled) return;
          setTrashProgress(ev.payload);
          // **最後の束のぶんは書かない**（ゲート2のP3）。完了の「n枚を
          // ゴミ箱へ移動しました」より後に届くと、それを上書きして
          // 「移動中… (n / n)」で止まって見える
          if (!rejectGateRef.current && ev.payload.done < ev.payload.total) {
            setStatus(t.rejectGateTrashing(ev.payload.done, ev.payload.total));
          }
        },
      );
      if (cancelled) deleteProgress();
      else unlistenDelete = deleteProgress;
      const camerasDone = await listen("cameras-updated", () => refreshCameras());
      if (cancelled) camerasDone();
      else unlistenCameras = camerasDone;
      if (cancelled) {
        f();
        return;
      }
      unlisten = f;
      // 構築が速すぎてリスナー登録前に終わる／始まる場合に備えて一度取りに行く
      const now = await getIndexProgress().catch(() => null);
      if ((now?.building || now?.incomplete) && !cancelled) setIndexProgress(now);
    })();
    return () => {
      cancelled = true;
      unlisten?.();
      unlistenCameras?.();
      unlistenExport?.();
      unlistenDelete?.();
    };
  }, [refreshCameras]);
  /**
   * 新しい版が出ていないかの知らせ（0.2）。**アプリで唯一の外向き通信**で、
   * 設定で切れる（既定はON・24時間に1回）。
   *
   * 起動の瞬間には走らせない——最初の数秒は起動時同期とサムネイル生成で
   * いちばん忙しく、体感の速さがこのアプリの看板だから。落ちた（繋がらない・
   * 弾かれた・形が違う）ときは**黙って諦める**: 見に行けなかったことは
   * 利用者の用事ではない。
   */
  const [updateFound, setUpdateFound] = useState<UpdateCheck | null>(null);
  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      checkUpdate(false)
        .then((r) => {
          if (!cancelled && r.newer) setUpdateFound(r);
        })
        .catch(() => {});
    }, 4000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, []);

  // 索引の構築が終わったらカメラ一覧も揃うので取り直す
  const wasBuilding = useRef(false);
  useEffect(() => {
    const building = indexProgress !== null;
    if (wasBuilding.current && !building) {
      refreshCameras();
      if (queryRef.current) reloadAll().catch(() => {});
    }
    wasBuilding.current = building;
  }, [indexProgress, refreshCameras, reloadAll]);

  // サムネイル完成・メタデータ抽出はレコード単位のpushで届く。
  // バッファにためて150ms間隔でまとめて反映する（イベントの嵐でも再描画は最小限）
  const patchBuffer = useRef<MediaItem[]>([]);
  const patchTimer = useRef<number | null>(null);
  const applyPatches = useCallback(
    (patches: MediaItem[]) => {
      const filter = filterRef.current;
      // 検索中は「その項目が結果に含まれるか」をフロント側で判定できない。
      // 表示中の項目の差し替え（サムネイル完成）だけを行い、日の捨て直しはしない
      // （判定できないまま捨てると、生成のたびにシマー→再取得を繰り返す）。
      // 件数・日付の骨組みはデバウンスされたサマリ再取得が追従させる
      const searching = queryRef.current !== "";
      setDayItems((prev) => {
        let next: Map<number, MediaItem[]> | null = null;
        const current = () => next ?? prev;
        for (const p of patches) {
          if (searching) {
            const arr = current().get(p.day_key);
            const idx = arr?.findIndex((it) => it.id === p.id) ?? -1;
            if (arr && idx >= 0) {
              if (!next) next = new Map(prev);
              const copy = arr.slice();
              copy[idx] = p;
              next.set(p.day_key, copy);
            }
            continue;
          }
          // ★（または⚑）のみ表示中、その印が無い項目は表示対象外。
          // 読み込み済みの日に古い姿が残っていれば外すだけで、日の再取得はしない
          // （サムネイル完成イベントは印の有無に関わらず届くため、ここで日を
          //  捨てると可視日が生成のたびにシマー→再取得を繰り返してしまう）
          const shown =
            filter === "fav" ? p.favorite : filter === "picked" ? p.picked : true;
          const arr = current().get(p.day_key);
          const idx = arr?.findIndex((it) => it.id === p.id) ?? -1;
          if (arr && idx >= 0 && shown) {
            // その日に載っている: 差し替えるだけ
            if (!next) next = new Map(prev);
            const copy = arr.slice();
            copy[idx] = p;
            next.set(p.day_key, copy);
            continue;
          }
          // 旧い日（または表示対象外になった項目）の残骸があれば除去する
          for (const [key, items] of current()) {
            if (key === p.day_key && idx >= 0 && shown) continue;
            const i = items.findIndex((it) => it.id === p.id);
            if (i >= 0) {
              if (!next) next = new Map(prev);
              const copy = items.slice();
              copy.splice(i, 1);
              next.set(key, copy);
              break;
            }
          }
          // 宛先の日が読み込み済みなのに載っていない（日付が移ってきた）:
          // その日を捨てて再取得させる
          if (shown && arr && idx < 0) {
            if (!next) next = new Map(prev);
            next.delete(p.day_key);
          }
        }
        return next ?? prev;
      });
      // 思い出バナーのサムネイル品質も追従させる
      setMemories((prev) => {
        const byId = new Map(patches.map((p) => [p.id, p]));
        let changed = false;
        const out = prev.map((m) => {
          const p = byId.get(m.item.id);
          if (p && p.day_key === m.item.day_key) {
            changed = true;
            return { ...m, item: p };
          }
          return m;
        });
        return changed ? out : prev;
      });
      // 骨組み（日ごとの枚数・カレンダーの代表サムネイル品質）をデバウンス追従させる。
      // 日移動の検知に加え、代表サムネイルのキャッシュバスティングにも必要
      scheduleSummaryRefresh();
    },
    [scheduleSummaryRefresh],
  );

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<MediaItem>("media-updated", (ev) => {
      patchBuffer.current.push(ev.payload);
      if (patchTimer.current != null) return;
      patchTimer.current = window.setTimeout(() => {
        patchTimer.current = null;
        const patches = patchBuffer.current;
        patchBuffer.current = [];
        applyPatches(patches);
      }, 150);
    }).then((f) => {
      // クリーンアップが先に走っていたら即解除する（リスナーの取り残し防止）
      if (cancelled) {
        f();
      } else {
        unlisten = f;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (patchTimer.current != null) {
        window.clearTimeout(patchTimer.current);
        patchTimer.current = null;
      }
    };
  }, [applyPatches]);

  const onSync = useCallback(async () => {
    setBusy(true);
    try {
      const stats = await syncNow();
      await reloadAll();
      setStatus(t.syncDone(stats.added, stats.changed, stats.removed));
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  }, [reloadAll]);

  // ドライブ一覧を5秒間隔でポーリング（USB挿抜をOS固有APIなしで検知）
  useEffect(() => {
    let stopped = false;
    const load = async () => {
      try {
        const list = await listDrives();
        // 中身が同じなら参照を変えない。5秒ごとに新しい配列を入れると
        // アプリ全体が再描画され、取り込みウィザードの状態まで揺れる
        if (!stopped)
          setDrives((prev) =>
            prev.length === list.length &&
            prev.every(
              (d, i) =>
                d.path === list[i].path &&
                d.label === list[i].label &&
                d.kind === list[i].kind &&
                d.dcim_path === list[i].dcim_path,
            )
              ? prev
              : list,
          );
      } catch {
        /* ドライブ列挙失敗は無視（次のポーリングで再試行） */
      }
    };
    load();
    const timer = window.setInterval(load, 5000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, []);

  /** ウィザードからのエラー通知。毎レンダで作り直すと向こうのeffectが再実行される */
  const onWizardError = useCallback((message: string) => setStatus(message), []);

  /**
   * 「まだ写真がありません」と言ってよい状態か。
   *
   * 絞り込みの結果0件・読み込み途中・起動時の走査の途中は**どれも違う**。
   * 判定は**ここ1か所**に置く——出す条件と聞く条件がずれると、
   * 古い理由が新しい一覧の上に出る
   */
  const canSayEmpty =
    settled && !loadFailed && scanSettled && !filtering && summary.length === 0;
  /**
   * 一覧の代わりに何か言う枠を出してよいか。
   *
   * **失敗も黙らない**——ただし言うことが違う。空だと分かっているなら理由、
   * 読めなかっただけなら「出せませんでした」。`0 件` だけの画面に
   * 戻さないのがこのPRの主題なので、失敗でパネルごと消すのは筋が違う
   * （ゲート1の指摘）
   */
  const showEmptyPanel =
    (canSayEmpty && emptyReason != null) ||
    // **失敗の枝にも「一覧が空」が要る。** 5万枚が並んでいる最中に
    // 取り込みの `library-updated` で走った `reloadAll` が1回転ぶと、
    // `summary` は正しいまま残っているのに一覧を消して
    // 「出せませんでした」に差し替えてしまう（ゲート2の指摘）
    (settled && loadFailed && !filtering && summary.length === 0);
  /** パネルに出す本文。旗の優先順は「利用者が次に何をするか」の順 */
  const emptyMessage = () => {
    if (loadFailed) return t.emptyLoadFailed;
    if (emptyReason == null) return t.emptyNothingHere;
    const r = emptyReason;
    if (r.noRoots) return t.emptyNoRoots;
    if (r.missing.length > 0) return t.emptyMissing(nameList(r.missing));
    if (r.unreadable.length > 0) {
      const say =
        platform === "macos"
          ? t.emptyUnreadableMac
          : platform === "windows"
            ? t.emptyUnreadableWin
            : t.emptyUnreadableOther;
      return say(nameList(r.unreadable));
    }
    if (r.photoLibrary) return t.emptyPhotoLibrary;
    if (r.excluded.length > 0)
      return t.emptyAllExcluded(nameList(r.excluded, r.excludedTotal));
    return t.emptyNothingHere;
  };
  /**
   * 名前の一覧を1文へ。**3件で切って、切ったことを言う**。
   *
   * 外付けを4台ぶらさげている人が全部外して起動すると、一番効くはずの文が
   * パスの羅列で読めなくなる（ゲート2の指摘）。**黙って落とさない**のが条件で、
   * 落としたぶんは「ほか2件」と数で言う。
   *
   * `total` は**手元の名前より多いことがある**——除外の一覧はバックエンドが
   * 3件で切って持ってくるので、数だけ別に受け取る（同）
   */
  const nameList = useCallback(
    (names: string[], total = names.length) => {
      // **引くのは「出した数」**。`3` を引くと、名前が畳まれて1件しか
      // 来ていない（6つのルートの `Lightroom Catalog/` など）ときに
      // 数が合わない（ゲート2の指摘）
      const shown = names.slice(0, 3);
      const rest = total - shown.length;
      return rest > 0
        ? [...shown, t.andMore(rest)].join(t.listSeparator)
        : shown.join(t.listSeparator);
    },
    [t],
  );

  // 一覧が空になったときだけ理由を聞く。空でなくなったら忘れる
  useEffect(() => {
    if (!canSayEmpty) {
      setEmptyReason(null);
      return;
    }
    // **発火の瞬間にもう一度見る。** `canSayEmpty` はこの描画の値で、
    // 絞り込みが変わった直後は既に古い（上の `settledRef` の説明）
    if (!settledRef.current) return;
    let alive = true;
    /** 旗の立っていない理由＝「まだ見つかりません」＋取り込みと参照の2つ */
    const nothingKnown = {
      noRoots: false,
      missing: [],
      unreadable: [],
      excluded: [],
      excludedTotal: 0,
      photoLibrary: false,
    };
    // **返ってこない筋にも答えを出す。** 切れたSMB/NFSのルートでは
    // `read_dir` がマウントのタイムアウトぶん返らず、`catch` は呼ばれない
    // ——`emptyReason` が `null` のままなので、**無言の `0 件` に逆戻り**する。
    // ネットワークのフォルダこそ、この機能が説明したい相手（ゲート2の指摘）
    const giveUp = window.setTimeout(() => {
      if (alive) setEmptyReason(nothingKnown);
    }, 5000);
    getEmptyLibraryReason()
      .then((r) => {
        if (alive) setEmptyReason(r);
      })
      // **聞けなくても無言に戻らない。** 表示は `emptyReason` が入って
      // いることを条件にしているので、ここで捨てると `0 件` だけの画面へ
      // 逆戻りする——このPRが消そうとしている当のもの（ゲート2の指摘）。
      // 旗の立っていない理由＝「まだ見つかりません」＋取り込みと参照の2つ
      .catch(() => {
        if (alive) setEmptyReason(nothingKnown);
      })
      .finally(() => window.clearTimeout(giveUp));
    return () => {
      alive = false;
      window.clearTimeout(giveUp);
    };
  }, [canSayEmpty]);

  // ウィザードを開く。startPath指定時（ドライブクリック）はそのフォルダから始める。
  // 同じパスで開き直されても中身を読み直せるよう、要求ごとに番号を進める
  const openWizard = useCallback((startPath?: string) => {
    setWizardStart(startPath);
    setWizardNonce((n) => n + 1);
    setWizardOpen(true);
  }, []);

  // USB/SDカードの自動起動（AutoPlay）で「pictkuraで取り込む」が選ばれたとき、
  // そのドライブで取り込みウィザードを開く。2重起動はバックエンドが
  // open-import-drive イベントで届け、冷起動はマウント後に take_pending_import で拾う。
  // バックエンドは2重起動ぶんも必ず置き場へ積んでから通知してくる（起動途中や
  // WebViewの再読み込み中は聞き手が居ないため）。受け取れたらこちらで消す——
  // 消さないと、次にマウントしたときに同じ要求でもう一度ウィザードが開く
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      const f = await listen<string>("open-import-drive", (ev) => {
        if (!ev.payload) return;
        void takePendingImport().catch(() => null);
        openWizard(ev.payload);
      });
      if (cancelled) {
        f();
        return;
      }
      unlisten = f;
      const pending = await takePendingImport().catch(() => null);
      if (!cancelled && pending) openWizard(pending);
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [openWizard]);

  // ウィザードの取り込み結果をグリッドへ反映する（コピー先の走査は
  // バックエンド側で済んでおり、ここではルート一覧と表示の更新だけ）
  const onImported = useCallback(
    async (stats: ImportStats) => {
      setStatus(
        t.importDone(stats.copied, stats.skipped) +
          (stats.failed > 0 ? t.importFailed(stats.failed) : "") +
          (stats.scan_incomplete ? t.importIncomplete : ""),
      );
      await refreshRoots();
      checkDecoders();
    },
    [refreshRoots, checkDecoders],
  );

  // ライブラリのルート追加の本体。手入力（onAddFolder）と
  // ネイティブのフォルダ選択（onBrowseFolder）の両方から呼ぶ
  const addFolder = async (path: string) => {
    setBusy(true);
    try {
      await addLibraryRoot(path);
      await reloadAll();
      await refreshRoots();
      checkDecoders();
      return true;
    } catch (e) {
      setStatus(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const onAddFolder = () => {
    // ボタンは disabled になるが Enter キーは素通りするので、ここでも弾く
    if (busy) return;
    const path = folderInput.trim();
    // 入力欄を空にするのは手入力から追加できたときだけ。「参照…」からの
    // 追加でここを消すと、打ちかけのパスが黙って消える
    if (path)
      void addFolder(path).then((ok) => {
        if (ok) setFolderInput("");
      });
  };

  // 「参照…」。取り込みウィザードや設定と同じネイティブのフォルダ選択を使う。
  // ここだけ手入力のままだったのを揃える（選んだら即追加する）
  const onBrowseFolder = async () => {
    if (busy) return;
    try {
      const picked = await open({ directory: true, title: t.pickLibraryFolder });
      if (typeof picked === "string") await addFolder(picked);
    } catch (e) {
      // 握りつぶすと「押したのに何も起きない」になる（onOpenWithOther と同じ扱い）
      setStatus(String(e));
    }
  };

  const onRemoveRoot = async (path: string) => {
    setBusy(true);
    try {
      await removeLibraryRoot(path);
      await reloadAll();
      await refreshRoots();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  /**
   * 写真に付ける印の種類。★（お気に入り）と ⚑（選別）は**同じ仕組みの別の棚**
   * （0.2 ②）。「あとで見返したい写真」と「この連写から残す1枚」を混ぜない
   */
  type MarkKind = "favorite" | "picked";

  /**
   * 印を付ける・外す（楽観的更新: 先にUIへ反映してからバックエンドへ）。
   *
   * **その印で絞り込んでいる最中は、外したものが画面から消える**。消えるものを
   * 選択に残すと、見えていない写真へ一括操作が効いてしまうので、選択・起点・
   * 範囲の土台もそこで忘れる
   */
  const setMark = useCallback(
    async (item: MediaItem, kind: MarkKind, next: boolean) => {
      if (item[kind] === next) return; // 既にその向きなら触らない
      const patch = (on: boolean) => {
        setDayItems((prev) => {
          const arr = prev.get(item.day_key);
          if (!arr) return prev;
          const out = new Map(prev);
          out.set(
            item.day_key,
            arr.map((it) => (it.id === item.id ? { ...it, [kind]: on } : it)),
          );
          return out;
        });
        setStats((s) =>
          kind === "favorite"
            ? { ...s, favorites: s.favorites + (on ? 1 : -1) }
            : { ...s, picked: s.picked + (on ? 1 : -1) },
        );
      };
      patch(next);
      try {
        if (kind === "favorite") await setFavorite(item.id, next);
        else await setPicked(item.id, next);
        // その印で絞り込み中は骨組み（枚数・日の有無）が変わる
        if (filterRef.current === (kind === "favorite" ? "fav" : "picked")) {
          setSelected((prev) => {
            if (!prev.has(item.id)) return prev;
            const s2 = new Set(prev);
            s2.delete(item.id);
            return s2;
          });
          // **起点と範囲の土台も忘れる**（単独削除と同じ）。居なくなったものを
          // 起点にしたまま Shift+クリックを続けると、古い土台が
          // 「もう画面に無いもの」を選び直し、間の写真のほうが外れる
          setAnchorId((a) => (a === item.id ? null : a));
          lastRangeRef.current = null;
          await reloadAll();
        }
      } catch {
        patch(!next); // 失敗したら戻す
      }
    },
    [reloadAll],
  );

  /** お気に入り（★）のトグル */
  const toggleFavorite = useCallback(
    (item: MediaItem) => setMark(item, "favorite", !item.favorite),
    [setMark],
  );

  /** 現在のフィルタでの総枚数（ヘッダ表示用） */
  const totalShown = useMemo(
    () => summary.reduce((n, d) => n + d.count, 0),
    [summary],
  );
  /** 各日の先頭までの累積枚数（ビューアの「n / 総数」表示用） */
  const prefixCounts = useMemo(() => {
    const out = new Array<number>(summary.length);
    let acc = 0;
    for (let i = 0; i < summary.length; i++) {
      out[i] = acc;
      acc += summary[i].count;
    }
    return out;
  }, [summary]);
  const dayIdxByKey = useMemo(() => {
    const map = new Map<number, number>();
    summary.forEach((d, i) => map.set(d.day_key, i));
    return map;
  }, [summary]);

  // 日付の骨組み＋取得済みの日のjustifiedレイアウトを行モデルへフラット化する。
  // 未取得の日は平均アスペクト4:3で高さを見積もった placeholder 1行になる
  const rows = useMemo<Row[]>(() => {
    const usable = Math.max(120, viewportWidth - GRID_PADDING);
    const target = cellSize;
    const out: Row[] = [];

    for (const day of summary) {
      out.push({
        kind: "header",
        dayKey: day.day_key,
        label: formatDayKey(day.day_key),
        count: day.count,
      });
      const items = dayItems.get(day.day_key);
      if (!items) {
        const perRow = Math.max(
          1,
          Math.floor((usable + GAP) / (target * (4 / 3) + GAP)),
        );
        const nRows = Math.ceil(day.count / perRow);
        out.push({
          kind: "placeholder",
          dayKey: day.day_key,
          height: nRows * (target + GAP),
        });
        continue;
      }
      // justifiedレイアウト: 行の高さはスライダー(cellSize)基準、
      // 各写真はアスペクト比どおりの幅（切り抜きなし）。行ごとに幅ピッタリへ伸縮
      let rowItems: MediaItem[] = [];
      let sumAspect = 0;
      const flushRow = (justify: boolean) => {
        if (rowItems.length === 0) return;
        const gaps = GAP * (rowItems.length - 1);
        let h = justify ? (usable - gaps) / sumAspect : target;
        // 最終行や1枚パノラマ行が巨大化しないよう上限を設ける
        h = Math.min(h, target * 1.3);
        const cells = rowItems.map((it) => ({
          item: it,
          w: Math.floor(aspectOf(it) * h),
          h: Math.round(h),
        }));
        out.push({
          kind: "cells",
          dayKey: day.day_key,
          cells,
          height: Math.round(h) + GAP,
        });
        rowItems = [];
        sumAspect = 0;
      };
      for (const it of items) {
        rowItems.push(it);
        sumAspect += aspectOf(it);
        // 基準高さで並べて幅が埋まったら、その行を幅ピッタリに伸縮して確定
        if (sumAspect * target + GAP * (rowItems.length - 1) >= usable) {
          flushRow(true);
        }
      }
      flushRow(false); // 端数の最終行は基準高さのまま（右側は空ける）
    }
    return out;
  }, [summary, dayItems, cellSize, viewportWidth]);

  /**
   * 各行の上端オフセットの累積（末尾は総高さ）。
   * 行の高さは `rows` から決まるので、仮想スクローラの実測を待たずに計算できる。
   * タイムライン・スクラバーの目盛り位置に使う
   */
  const rowTops = useMemo(() => {
    const tops = new Array<number>(rows.length + 1);
    let acc = 0;
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i];
      tops[i] = acc;
      acc += row.kind === "header" ? HEADER_HEIGHT : row.height;
    }
    tops[rows.length] = acc;
    return tops;
  }, [rows]);

  /**
   * 年の変わり目を目盛りにしたスクラバー用のマーカー。
   * 10年分のライブラリでも1クリックで飛べるようにする（骨組みだけで作れるので
   * 追加のIPCもレコード取得も要らない）。ラベルは重なる位置では省いて点だけ描く
   */
  const yearMarkers = useMemo(() => {
    const total = rowTops[rows.length] || 1;
    const out: { year: number; rowIndex: number; top: number; label: boolean }[] =
      [];
    let lastYear = -1;
    let lastLabelTop = -Infinity;
    for (let i = 0; i < rows.length; i++) {
      const row = rows[i];
      if (row.kind !== "header") continue;
      const year = Math.floor(row.dayKey / 10000);
      if (year === lastYear) continue;
      lastYear = year;
      const top = (rowTops[i] / total) * 100;
      // ラベルは4%（≒24px相当）以上離れているときだけ出す
      const label = top - lastLabelTop >= 4;
      if (label) lastLabelTop = top;
      out.push({ year, rowIndex: i, top, label });
    }
    return out;
  }, [rows, rowTops]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => {
      const row = rows[i];
      if (!row) return HEADER_HEIGHT;
      return row.kind === "header" ? HEADER_HEIGHT : row.height;
    },
    overscan: 6,
  });

  // 行モデルが変わったら行の高さを測り直す
  useEffect(() => {
    virtualizer.measure();
  }, [virtualizer, rows]);

  // カレンダーの日クリック → グリッドへ切り替えて該当日付の見出しへスクロール
  useEffect(() => {
    if (view !== "grid" || pendingScrollDay === null) return;
    const idx = rows.findIndex(
      (r) => r.kind === "header" && r.dayKey === pendingScrollDay,
    );
    if (idx >= 0) {
      virtualizer.scrollToIndex(idx, { align: "start" });
      setPendingScrollDay(null);
    }
  }, [view, pendingScrollDay, rows, virtualizer]);

  const virtualItems = virtualizer.getVirtualItems();

  // 可視範囲の日を追跡し、未取得（placeholder）の日をオンデマンドで取得する
  useEffect(() => {
    const visible = new Set<number>();
    const wanted: number[] = [];
    for (const vr of virtualItems) {
      const row = rows[vr.index];
      if (!row) continue;
      visible.add(row.dayKey);
      if (row.kind === "placeholder") wanted.push(row.dayKey);
    }
    visibleDaysRef.current = visible;
    for (const dayKey of wanted) loadDay(dayKey);
  }, [virtualItems, rows, loadDay]);

  // 可視領域のサムネイル未生成IDをバックエンドへ通知し、生成を優先させる
  const visibleMissingIds = useMemo(() => {
    const ids: number[] = [];
    for (const vr of virtualItems) {
      const row = rows[vr.index];
      if (row?.kind !== "cells") continue;
      for (const cell of row.cells) {
        // 高品質サムネイル未生成（なし or 即席のみ）を優先対象にする
        if (cell.item.thumb_state < 2) ids.push(cell.item.id);
      }
    }
    return ids;
  }, [virtualItems, rows]);
  useEffect(() => {
    if (visibleMissingIds.length === 0) return;
    const t = window.setTimeout(() => {
      setVisiblePriority(visibleMissingIds).catch(() => {});
    }, 150);
    return () => window.clearTimeout(t);
  }, [visibleMissingIds]);

  // 詳細ビューア: 位置 {dayKey, id} を実レコードへ解決する
  const viewerInfo = useMemo(() => {
    if (!viewer) return null;
    const dayIdx = dayIdxByKey.get(viewer.dayKey);
    if (dayIdx === undefined) return null;
    const items = dayItems.get(viewer.dayKey);
    if (!items || items.length === 0) return null;
    const itemIdx =
      viewer.id === "first"
        ? 0
        : viewer.id === "last"
          ? items.length - 1
          : items.findIndex((it) => it.id === viewer.id);
    if (itemIdx < 0) return null;
    return { item: items[itemIdx], dayIdx, itemIdx, dayLength: items.length };
  }, [viewer, dayItems, dayIdxByKey]);

  /** スコープのid → 列の中での位置。送りのたびに端から探さないため */
  const scopeIndexById = useMemo(() => {
    const map = new Map<number, number>();
    viewerScope?.forEach((e, i) => map.set(e.id, i));
    return map;
  }, [viewerScope]);
  /**
   * いまの絵がスコープの何番目か。**スコープなし・列の外なら `undefined`**。
   *
   * 列の外に出るのは、見ている最中に★を外した等で選んだものが一覧から
   * 消えたとき。そのときは一覧の歩き方へ落ちる（送りが利かなくなるより良い）
   */
  const scopeIdx =
    viewerScope && viewerInfo
      ? scopeIndexById.get(viewerInfo.item.id)
      : undefined;

  // ビューアの表示日をキャッシュ間引きの保護対象として共有する
  useEffect(() => {
    viewerDayRef.current = viewer?.dayKey ?? null;
  }, [viewer]);

  // ビューアの表示日と前後の日を先読みする（日またぎ移動を途切れさせない）
  useEffect(() => {
    if (!viewer) return;
    if (!dayItems.has(viewer.dayKey)) loadDay(viewer.dayKey);
    const dayIdx = dayIdxByKey.get(viewer.dayKey);
    if (dayIdx === undefined) return;
    for (const nd of [dayIdx - 1, dayIdx + 1]) {
      const key = summary[nd]?.day_key;
      if (key !== undefined && !dayItems.has(key)) loadDay(key);
    }
  }, [viewer, dayItems, dayIdxByKey, summary, loadDay]);

  /**
   * いま見ている絵が、その日の中で何番目だったか。
   *
   * **居なくなったときの寄せ先**に使う。同じ番号には、いま繰り上がってきた
   * 次の1枚が居る（`U` で外した写真が一覧から消えたときの自然な着地点）
   */
  const lastViewerIdxRef = useRef(0);
  useEffect(() => {
    if (viewerInfo) lastViewerIdxRef.current = viewerInfo.itemIdx;
  }, [viewerInfo]);

  // 位置が解決不能になったとき（⚑や★を外して絞り込みから外れた、削除された、
  // 撮影日確定で別の日へ移った、サマリ更新でその日が消えた等）の後始末。
  //
  // **閉じずに隣へ寄せる**（0.2 ②・ゲート2のP2）。⚑で絞り込みながら `U` で
  // 外していくのは選別の本線の使い方で、そのたびにビューアが消えると
  // 選別が続けられない。寄せ先が無いときだけ閉じる（「読み込み中…」のまま
  // 操作不能で固まるのは防ぐ、という元の目的はそのまま）
  useEffect(() => {
    if (!viewer) return;
    const items = dayItems.get(viewer.dayKey);
    if (!items) return; // 未取得: ロード完了待ち（スタックではない）
    const resolvable =
      viewer.id === "first" || viewer.id === "last"
        ? items.length > 0
        : items.some((it) => it.id === viewer.id);
    if (resolvable && dayIdxByKey.has(viewer.dayKey)) return;

    // スコープで開いているなら、列の**次**（無ければ前）で、いま一覧に
    // 居るものへ寄せる。列は作り直さない設計なので、消えたidも席は残っている
    if (viewerScope && typeof viewer.id === "number") {
      const idx = scopeIndexById.get(viewer.id);
      if (idx !== undefined) {
        for (const dir of [1, -1] as const) {
          for (let k = idx + dir; k >= 0 && k < viewerScope.length; k += dir) {
            const at = viewerScope[k];
            const dayList = dayItems.get(at.day_key);
            // 未取得の日は「居るかもしれない」側に倒す（行けば取りに行く）
            if (dayList && !dayList.some((x) => x.id === at.id)) continue;
            setViewer({ dayKey: at.day_key, id: at.id });
            return;
          }
        }
      }
    }
    // 通常オープン: **同じ日の同じ番号**へ寄せる（繰り上がってきた1枚）
    if (items.length > 0 && dayIdxByKey.has(viewer.dayKey)) {
      const i = Math.min(lastViewerIdxRef.current, items.length - 1);
      setViewer({ dayKey: viewer.dayKey, id: items[i].id });
      return;
    }
    setViewer(null); // その日ごと消えた
  }, [viewer, dayItems, dayIdxByKey, viewerScope, scopeIndexById]);

  /** ビューアを1枚進める(+1)/戻す(-1)。wrapは末尾→先頭のループ（スライドショー用） */
  const moveViewer = useCallback(
    (dir: 1 | -1, wrap = false) => {
      if (!viewerInfo) return;
      // 選択スコープで開いているあいだは、その列の中だけを歩く（0.2 ②）。
      // 日をまたいでも列の順に進む——選んだ範囲が一覧の並びで固定してある
      if (viewerScope && scopeIdx !== undefined) {
        // **一覧から消えた席は飛ばす**（ゲート2のP2）。⚑を外した・消したものが
        // 列には残っているので、そこへ歩くと行き止まりになる。
        // 未取得の日は「居るかもしれない」側に倒す（行けば取りに行く）
        for (
          let k = scopeIdx + dir;
          k >= 0 && k < viewerScope.length;
          k += dir
        ) {
          const at = viewerScope[k];
          const dayList = dayItems.get(at.day_key);
          if (dayList && !dayList.some((x) => x.id === at.id)) continue;
          setViewer({ dayKey: at.day_key, id: at.id });
          return;
        }
        if (wrap && viewerScope.length > 0) {
          const at = viewerScope[0];
          setViewer({ dayKey: at.day_key, id: at.id });
        }
        return;
      }
      const { dayIdx, itemIdx } = viewerInfo;
      const items = dayItems.get(summary[dayIdx].day_key);
      if (!items) return;
      const ni = itemIdx + dir;
      if (ni >= 0 && ni < items.length) {
        setViewer({ dayKey: summary[dayIdx].day_key, id: items[ni].id });
        return;
      }
      const nd = dayIdx + dir;
      if (nd >= 0 && nd < summary.length) {
        setViewer({
          dayKey: summary[nd].day_key,
          id: dir === 1 ? "first" : "last",
        });
      } else if (wrap && summary.length > 0) {
        setViewer({ dayKey: summary[0].day_key, id: "first" });
      }
    },
    [viewerInfo, dayItems, summary, viewerScope, scopeIdx],
  );

  /** 判定キーのあと次の絵へ送るか（設定・既定ON）。古い設定ファイルには無い */
  const autoAdvance = config?.viewer?.auto_advance ?? true;
  /**
   * 選別の判定（0.2 ②）。`P` で選び（⚑を付け）、`U` で選び直す（⚑を外す）。
   *
   * **★とは別の棚**に印を付ける。★は「あとで見返したい写真」、⚑は
   * 「この連写から残す1枚」で、混ぜると選別のたびに★の棚が荒れる
   * （2026-08-18の利用者判断）。
   *
   * トグルの `f` と違い**押した向きに決める**ので、連写を見ながら `P` を
   * 連打しても付けたり外したりにならない。自動送りがONなら続けて次の絵へ。
   * 印の反映は楽観的更新なので、書き込みを待たずに送ってよい
   */
  /**
   * ボツの候補に入れる・戻す（0.2 ③）。**印だけでファイルは触らない**。
   * 実際にゴミ箱へ入るのは関所で確定したときの1回だけ。
   */
  const markReject = useCallback((item: MediaItem, on: boolean) => {
    setRejected((prev) => {
      if (on === prev.has(item.id)) return prev;
      const next = new Map(prev);
      if (on) next.set(item.id, item);
      else next.delete(item.id);
      return next;
    });
  }, []);

  /**
   * 判定した合図を絵の上に一瞬出す（2026-08-19の利用者指摘）。
   *
   * **自動送りがONだと、押した相手はもう画面に居ない**。道具の帯の★や⚑は
   * 1.8秒で消えるので、押したそばから「いま何をしたのか」の手掛かりが
   * 無くなっていた。ここは*何をしたか*を短く出す係で、*いまの絵がどうなって
   * いるか*は絵の上の常設バッジが受け持つ。
   *
   * `seq` を毎回変えるのは、同じ判定を連打したときに**アニメーションを
   * 掛け直す**ため（Reactはkeyが同じだと要素を作り直さない）。
   */
  const [judgeFlash, setJudgeFlash] = useState<{
    kind: JudgeFlashKind;
    seq: number;
  } | null>(null);
  const flashSeq = useRef(0);
  const flashTimer = useRef<number | undefined>(undefined);
  const flashJudge = useCallback((kind: JudgeFlashKind) => {
    flashSeq.current += 1;
    setJudgeFlash({ kind, seq: flashSeq.current });
    window.clearTimeout(flashTimer.current);
    // CSSの表示時間より少しだけ長く置く（消え際で要素を抜くとちらつく）
    flashTimer.current = window.setTimeout(() => setJudgeFlash(null), 900);
  }, []);
  useEffect(() => () => window.clearTimeout(flashTimer.current), []);

  /** ビューアの `F`・☆ボタン。トグルなので、**どちらに転んだか**を合図に出す */
  const favoriteViewer = useCallback(
    (item: MediaItem) => {
      flashJudge(item.favorite ? "unfav" : "fav");
      toggleFavorite(item);
    },
    [flashJudge, toggleFavorite],
  );

  /**
   * ビューアの✕ボタン。キーの `X` と違って**トグル**で、**送らない**
   * ——マウスで押す人は、押した1枚がその場でどうなったかを見たい
   */
  const rejectTool = useCallback(
    (item: MediaItem) => {
      const on = !rejectedRef.current.has(item.id);
      markReject(item, on);
      if (on && item.picked) void setMark(item, "picked", false);
      flashJudge(on ? "reject" : "unflag");
    },
    [markReject, setMark, flashJudge],
  );

  /** ビューアの⚑ボタン。**送らない**——押した相手を見たままにする */
  const pickViewer = useCallback(
    (item: MediaItem, pick: boolean) => {
      flashJudge(pick ? "pick" : "unflag");
      void setMark(item, "picked", pick);
    },
    [flashJudge, setMark],
  );

  const judgeViewer = useCallback(
    (item: MediaItem, pick: boolean) => {
      void setMark(item, "picked", pick);
      // **判定は1枚につき1つ**。⚑を付けた写真がボツの候補に残っていると、
      // 関所で「選んだはずの1枚」がゴミ箱の列に並ぶ。`U` は両方を外す
      markReject(item, false);
      flashJudge(pick ? "pick" : "unflag");
      if (autoAdvance) moveViewer(1);
    },
    [setMark, markReject, flashJudge, autoAdvance, moveViewer],
  );

  /**
   * `X` の判定（0.2 ③）。`P` と同じく**押した向きに決める**ので、
   * 連写を見ながら連打してもトグルにならない。自動送りがONなら続けて次へ。
   */
  const rejectViewer = useCallback(
    (item: MediaItem) => {
      markReject(item, true);
      // **判定は1枚につき1つ**（`judgeViewer` の逆向き）。⚑を付けた1枚を
      // あとで✕にしたとき、⚑が残っていると「入れずに閉じる」で戻ったあとに
      // **最後の判定と逆の印だけが残る**（ゲート1の指摘）
      if (item.picked) void setMark(item, "picked", false);
      flashJudge("reject");
      if (autoAdvance) moveViewer(1);
    },
    [markReject, setMark, flashJudge, autoAdvance, moveViewer],
  );

  /**
   * ビューアを閉じる要求（0.2 ③）。**✕の印が残っていれば関所を出す**。
   *
   * 印はファイルを動かしていないので、ここを素通しすると
   * 「ボツにしたのに何も起きていない」に見える。逆に、関所を出せずに
   * 落ちた場合も**何も消えない側**に倒れる
   */
  const requestCloseViewer = useCallback(() => {
    if (rejectedRef.current.size > 0) setRejectGate({ closeAfter: true });
    else setViewer(null);
  }, []);

  const viewerItem = viewerInfo?.item ?? null;

  // 動画を開いたら、絵を出す前に**1回だけ**状態を聞く。
  // クラウドのみのファイルは開いた瞬間にダウンロードが始まってしまうので、
  // `<video>` を出す前に知る必要がある
  const viewerVideoId = viewerItem?.is_video ? viewerItem.id : undefined;
  useEffect(() => {
    if (viewerVideoId === undefined) return;
    let cancelled = false;
    videoStatus(viewerVideoId)
      .then((info) => {
        if (!cancelled) setVideoInfo(info);
      })
      .catch(() => {
        // 聞けなかったら今までどおり再生を試みる（黙って止めない）
        if (!cancelled)
          setVideoInfo({ plays_in_app: true, cloud_only: false, exists: true });
      });
    return () => {
      cancelled = true;
    };
  }, [viewerVideoId]);

  // ===== 原寸の先読み（0.2 ①）=====
  //
  // 送りで隣へ行ったとき「もう出ている」状態を作る。実測で24MP JPEGの
  // デコードは `<img>.decode()` 経由で130ms、**先読み済みなら0.0〜0.5ms**。
  // 守るべき制約が3つある:
  //
  // - **総画素の崖**（[`PRELOAD_BUDGET_BYTES`]）。踏むと全滅するので下で止める
  // - **クラウドのみのファイルは触らない**。先読みは利用者の意思ではないので、
  //   裏でOneDriveのダウンロードを走らせてはいけない
  // - **詰め直しの要る形式は隣の1枚だけ**（HEICは1枚0.6〜1秒）

  // 以下の印は**真偽値ではなくidで持つ**。送った直後の1コミットでは、
  // 状態を戻すeffectがまだ走っておらず、真偽値だと「前の絵が出ている」を
  // 新しい絵の判定に使ってしまう（ゲート2のP2-1）。idなら一致しないので、
  // 1コミット目から正しく閉じる

  /**
   * 原寸が**実際に出た**絵のid（0.2 ②）。
   *
   * 下の [`loadedId`] と分けてあるのは、原本が消えている・取り寄せられない・
   * 詰め直せないときに `onError` が走るから。あちらで下敷きを外すと、
   * **出ていたサムネイルまで消えて真っ黒になる**（ゲート1のP2）
   */
  const [fullShownId, setFullShownId] = useState<number | null>(null);
  /** 下敷きのサムネイルが出た絵のid（0.2 ②） */
  const [thumbShownId, setThumbShownId] = useState<number | null>(null);
  /** 原寸が**出せなかった**絵のid（0.2 ②）。壊れた <img> の見せ方を変えるため */
  const [fullFailedId, setFullFailedId] = useState<number | null>(null);
  /** 送りが落ち着いた（250ms動かなかった）絵のid */
  const [settledId, setSettledId] = useState<number | null>(null);
  /**
   * **原寸を取りに行ってよい**絵のid（0.2 ②）。
   *
   * 詰め直しの要る形式を連打で通り過ぎているあいだは閉じておく
   * （[`FAST_FLIP_MS`]）。開くのは送りが止まった250ms後
   */
  const [fullGateId, setFullGateId] = useState<number | null>(null);
  /** 直前に絵が変わった時刻（連打かどうかの判定に使う） */
  const lastViewerChangeRef = useRef(0);
  /** 「読み込み中」を出してよい絵のid（詰め直しの要る形式だけ・300ms超） */
  const [slowId, setSlowId] = useState<number | null>(null);
  const viewerItemId = viewerItem?.id;
  /**
   * **待つのをやめてよい**絵のid。原寸が届いたときと、届かないと分かったとき
   * （`onError`）の両方で立つ——「読み込み中」を畳み、裏の詰め直しを始めてよい
   * 合図であって、**絵が出たという意味ではない**。
   *
   * 上の2つから**導く**（別の印として持たない）。持っていたころは
   * 「出た／出せなかった」を立てるたびにこちらも立てる必要があり、
   * 置き忘れると「読み込み中」が消えなくなった（PR #22 のゲート2 P2-1がそれ）。
   *
   * **いまの絵に一致する印だけを見る**。`fullShownId ?? fullFailedId` と書くと、
   * 送った直後に**前の絵の「出た」が残っている**あいだは、新しい絵が失敗しても
   * 前の絵のidを答えてしまう（印を捨てるeffectとイベントの前後関係に賭ける形に
   * なる。PR #23 のゲート1 P2）。ここは順序に依らない形で書く
   */
  const loadedId =
    viewerItemId !== undefined &&
    (fullShownId === viewerItemId || fullFailedId === viewerItemId)
      ? viewerItemId
      : null;
  /** 表示に詰め直しが要るか（HEIC 0.6〜1秒 / TIFF 約300ms） */
  const viewerTranscoding = Boolean(
    viewerItem && !viewerItem.is_video && viewerItem.needs_transcode,
  );
  useEffect(() => {
    // 前の絵に立てた印は**5つとも**捨てる。残しておくと、A→B→Aと戻ったときに
    // 「Aはもう出ている・落ち着いている」が最初から成立し、**まだ動いている
    // 最中なのに裏の詰め直しが始まる**（門の意味が消える）。
    // nullへ戻すのは安全な向き——古いidが新しいidと一致して門が開くことは無い
    setSlowId(null);
    setSettledId(null);
    setThumbShownId(null);
    setFullShownId(null);
    setFullFailedId(null);
    if (viewerItemId === undefined) {
      setFullGateId(null);
      return;
    }
    // 連打で送っている最中は、詰め直しの要る形式の原寸を**まだ取りに行かない**。
    // 通り過ぎた絵の変換が積み上がると、止まった1枚がその後ろに並ぶ
    const now = Date.now();
    const flipping = now - lastViewerChangeRef.current < FAST_FLIP_MS;
    lastViewerChangeRef.current = now;
    setFullGateId(viewerTranscoding && flipping ? null : viewerItemId);
    const settle = window.setTimeout(() => {
      setSettledId(viewerItemId);
      // 止まった。ここで初めて取りに行く（上で閉じていた場合）
      setFullGateId(viewerItemId);
    }, 250);
    // 待たせないもの（JPEG/PNG/AVIF）に読み込み中は出さない。先読みが
    // 効いていれば詰め直しの要る形式も数msで出るので、少し待ってから出す
    const slow = viewerTranscoding
      ? window.setTimeout(() => setSlowId(viewerItemId), 300)
      : undefined;
    return () => {
      window.clearTimeout(settle);
      if (slow !== undefined) window.clearTimeout(slow);
    };
  }, [viewerItemId, viewerTranscoding]);

  /**
   * 原寸が出せず、下敷きのサムネイルが唯一の絵になっているか（0.2 ②）。
   *
   * このとき原寸の `<img>` は**中身の無い絵の枠**として残す（当たり判定と
   * 拡大・移動・右クリックのため）。
   */
  const fallbackToThumb =
    viewerItem !== null &&
    fullFailedId === viewerItem.id &&
    thumbShownId === viewerItem.id;
  /** 配信される絵の寸法（TIFFだけ長辺が丸められる） */
  const [servedW, servedH] = viewerItem ? servedSize(viewerItem) : [0, 0];
  /**
   * いま出ている絵の**実寸**（届いてから分かる）。等倍100%の較正に使う。
   *
   * DBの寸法で較正すると、配信される絵と食い違う形式で**100%が100%でなくなる**
   * ——RAWはDBに埋め込みプレビューの寸法が入っており、ファイル後方に原寸を置く
   * 形式（Ricoh GR IIIのDNG）では720x480と6000x4000ほど離れる。
   * TIFFの長辺丸めも同じ形（ゲート2のP3）
   */
  const [servedNatural, setServedNatural] = useState<{
    id: number;
    w: number;
    h: number;
  } | null>(null);

  /** 先読みする隣接画像（表示順に 次1→前1→次2→…） */
  const [preload, setPreload] = useState<MediaItem[]>([]);
  /**
   * id → 実体がクラウドにしか無いか。答えは覚えておくが、**寿命がある**
   * （[`CLOUD_ANSWER_TTL_MS`]）。「ローカルにある」は後から嘘になるため
   */
  const cloudOnlyRef = useRef(new Map<number, { cloud: boolean; at: number }>());
  /** 覚えている答えを引く。古い「ローカルにある」は忘れて聞き直させる */
  const cloudAnswer = useCallback((id: number): boolean | undefined => {
    const a = cloudOnlyRef.current.get(id);
    if (a === undefined) return undefined;
    // 「クラウドにしか無い」側は寝かせてよい——先読みしない方向の答えなので、
    // 古くても危なくない。危ないのは「ローカルにある」が古びたときだけ
    if (!a.cloud && Date.now() - a.at > CLOUD_ANSWER_TTL_MS) {
      cloudOnlyRef.current.delete(id);
      return undefined;
    }
    return a.cloud;
  }, []);
  /** 先読みの `<img>` 実体。decodeを表示順にかけるために持つ */
  const preloadElsRef = useRef(new Map<number, HTMLImageElement>());
  /**
   * id → **実際に届いた絵の実寸**（`naturalWidth`/`naturalHeight`）。
   *
   * DBの寸法は「配信される絵の寸法」とは限らない（[`decodedBytes`] の説明）。
   * 一度読めた絵は実寸が分かるので、先読みの予算はそちらで数える。
   * 見た絵のぶんだけ増えるので、**溜まったらまとめて捨てる**——捨てても
   * DBの値へ戻るだけで、次に読めばまた入る
   */
  const naturalRef = useRef(new Map<number, [number, number]>());
  const rememberNatural = useCallback((id: number, w: number, h: number) => {
    if (w <= 0 || h <= 0) return;
    if (naturalRef.current.size >= 4096) naturalRef.current.clear();
    naturalRef.current.set(id, [w, h]);
  }, []);
  /**
   * 裏で作らせている詰め直し（HEIC/RAW/TIFF）の1枚。**始めたら終わるまで手放さない**。
   *
   * 隠し `<img>` を外しても、Rust側で走り出した変換は取り消せない。送るたびに
   * 隣へ乗り換えると、**捨てた変換だけが積み上がってCPUを食い、あとで開いた
   * 1枚がその陰で遅くなる**（ゲート1のP1）。裏で走るのは常にこの1枚だけにする
   */
  const pendingTranscodeRef = useRef<MediaItem | null>(null);
  /** 仕掛かりの画素を手放したときの見張り（終わりを観測できなくなるため） */
  const pendingWatchdogRef = useRef<number | null>(null);
  /** 上の1枚が終わったことを、先読みの選び直しへ伝える目印 */
  const [transcodeTick, setTranscodeTick] = useState(0);
  // ビューアを閉じたらクラウド判定の覚えを捨てる。OneDriveは後から実体を
  // 落としたり空けたりするので、古い答えをいつまでも信じない
  useEffect(() => {
    if (viewer === null) {
      cloudOnlyRef.current.clear();
      // **仕掛かりの印は消さない**。閉じても走り出した変換は取り消せないので、
      // 消すと開き直すたびに新しい変換を始められてしまう。終わりを観測する
      // 隠し `<img>` はもう無いので、見張り時計に委ねる
      if (
        pendingTranscodeRef.current !== null &&
        pendingWatchdogRef.current === null
      ) {
        pendingWatchdogRef.current = window.setTimeout(() => {
          pendingWatchdogRef.current = null;
          pendingTranscodeRef.current = null;
          setTranscodeTick((t) => t + 1);
        }, PENDING_WATCHDOG_MS);
      }
    }
  }, [viewer]);

  /**
   * 一覧の並びで**1つ隣の位置**へ（`moveViewer` と同じ歩き方）。
   * 端と**未取得の日**で止まる（届けば呼び直される）。
   *
   * 先読み（0.2 ①）とフィルムストリップ（0.2 ②）が同じ歩き方を使う
   */
  const stepPos = useCallback(
    (
      pos: { d: number; i: number },
      dir: 1 | -1,
    ): { d: number; i: number } | null => {
      const items = dayItems.get(summary[pos.d]?.day_key);
      if (!items) return null;
      const ni = pos.i + dir;
      if (ni >= 0 && ni < items.length) return { d: pos.d, i: ni };
      const nd = pos.d + dir;
      const next =
        nd >= 0 && nd < summary.length
          ? dayItems.get(summary[nd].day_key)
          : undefined;
      if (!next || next.length === 0) return null;
      return { d: nd, i: dir === 1 ? 0 : next.length - 1 };
    },
    [dayItems, summary],
  );

  /**
   * スコープで開いていれば列の、そうでなければ一覧の**隣の絵**を1枚返す。
   * `k` は正で次、負で前（`k = 0` はいまの絵）。取れなければ null
   */
  const neighborItem = useCallback(
    (k: number): MediaItem | null => {
      if (!viewerInfo) return null;
      if (k === 0) return viewerInfo.item;
      if (viewerScope && scopeIdx !== undefined) {
        const at = viewerScope[scopeIdx + k];
        if (!at) return null;
        return dayItems.get(at.day_key)?.find((x) => x.id === at.id) ?? null;
      }
      const dir: 1 | -1 = k > 0 ? 1 : -1;
      let cur: { d: number; i: number } | null = {
        d: viewerInfo.dayIdx,
        i: viewerInfo.itemIdx,
      };
      for (let n = 0; n < Math.abs(k); n++) {
        cur = stepPos(cur, dir);
        if (!cur) return null;
      }
      return dayItems.get(summary[cur.d].day_key)?.[cur.i] ?? null;
    },
    [viewerInfo, viewerScope, scopeIdx, dayItems, summary, stepPos],
  );

  /**
   * ビューア下部に出す前後の絵（0.2 ②）。いまの絵を真ん中に、
   * 前後 [`STRIP_RADIUS`] 枚ずつ。
   *
   * **取れないもの（端・未取得の日）はその席ごと並べない**ので、帯は端で短く
   * なり、いまの絵は中央から寄る。反対側から余計に取って枚数を揃えることは
   * しない——「前後に何があるか」を見せる帯で、左右の距離感を偽らないため
   */
  const strip = useMemo(() => {
    if (!viewerItem) return [];
    const out: { item: MediaItem; offset: number }[] = [];
    for (let k = -STRIP_RADIUS; k <= STRIP_RADIUS; k++) {
      const it = neighborItem(k);
      if (it) out.push({ item: it, offset: k });
    }
    return out;
  }, [viewerItem, neighborItem]);

  useEffect(() => {
    if (!viewerInfo) {
      setPreload([]);
      return;
    }
    const start = { d: viewerInfo.dayIdx, i: viewerInfo.itemIdx };
    const step = stepPos;
    /**
     * 送る向きの隣を近い順に集める。**スコープで開いていればその列を歩く**
     * （0.2 ②）——一覧の隣ではなく、送りが実際に行く先を先読みするため
     */
    const walk = (dir: 1 | -1) => {
      const out: MediaItem[] = [];
      if (viewerScope && scopeIdx !== undefined) {
        for (let k = 1; k <= PRELOAD_MAX_ITEMS; k++) {
          const at = viewerScope[scopeIdx + dir * k];
          if (!at) break;
          const it = dayItems.get(at.day_key)?.find((x) => x.id === at.id);
          // 未取得の日は諦める（送って行けば取りに行く。届けば再実行される）
          if (!it) break;
          out.push(it);
        }
        return out;
      }
      let cur = start;
      for (let k = 0; k < PRELOAD_MAX_ITEMS; k++) {
        const next = step(cur, dir);
        if (!next) break;
        cur = next;
        const it = dayItems.get(summary[cur.d].day_key)?.[cur.i];
        if (!it) break;
        out.push(it);
      }
      return out;
    };
    const fwd = walk(1);
    const back = walk(-1);
    // 順番は 次1 → 前1 → 次2 → …。送りは前へ進むほうが多い
    const ordered: MediaItem[] = [];
    for (let k = 0; k < PRELOAD_MAX_ITEMS; k++) {
      if (fwd[k]) ordered.push(fwd[k]);
      if (back[k]) ordered.push(back[k]);
    }

    /**
     * 候補から実際に先読みする分を選ぶ。**クラウド判定の答えが返るたびに
     * 選び直す**——判定待ちの1枚を予算に数えたままにすると、結局読まない
     * プレースホルダが、その先の近所を予算から押し出してしまう（ゲート1のP2）。
     */
    const select = () => {
      // 予算は**表示中の1枚と、裏で作らせている1枚**を含めて数える
      // （崖は全体の総量で来る）
      const started = pendingTranscodeRef.current;
      /** 実物が届いたことのある絵は、その実寸で数える */
      const bytesOf = (it: MediaItem) =>
        decodedBytes(it, naturalRef.current.get(it.id));
      // 仕掛かりを抱えたままでは予算を割るなら、**画素のほうを手放す**。
      // 崖は全か無かなので、1枚のために先読み全部を捨てられるほうが高くつく。
      // 変換自体は取り消せないから、印は見張り時計で降ろす（それまで次の
      // 詰め直しは始めない）
      const pending =
        started !== null &&
        bytesOf(viewerInfo.item) + bytesOf(started) <= PRELOAD_BUDGET_BYTES
          ? started
          : null;
      if (started !== null && pending === null && pendingWatchdogRef.current === null) {
        pendingWatchdogRef.current = window.setTimeout(() => {
          pendingWatchdogRef.current = null;
          pendingTranscodeRef.current = null;
          setTranscodeTick((t) => t + 1);
        }, PENDING_WATCHDOG_MS);
      }
      let bytes = bytesOf(viewerInfo.item) + (pending ? bytesOf(pending) : 0);
      let transcodes = 0;
      const out: MediaItem[] = [];
      for (const it of ordered) {
        // 動画は先読みしない。Range配信で待ちが小さく、予算の桁も違う
        if (it.is_video) continue;
        // **クラウドにしか無いものと、まだ聞いていないものは数にも入れない**。
        // 聞いていないものを先読みしないのは、プレースホルダを裏で読んで
        // ダウンロードを起こさないため（答えが返ったら選び直す）
        if (cloudAnswer(it.id) !== false) continue;
        // 詰め直しの要る形式は、**表示中の1枚が出てから**しか裏で作らない。
        // 1枚1秒級なので、出る前に投げると取り消せない変換が
        // 積み上がり、CPUも通信も見ていない絵に取られる——実測で送りが3.0秒に
        // なったのがこの形（当時は錠があり、見ている側がその後ろで待った）
        const current = viewerInfo.item;
        if (
          it.needs_transcode &&
          // 仕掛かり中の1枚があるなら、終わるまで次の詰め直しは始めない
          (started !== null ||
            settledId !== current.id ||
            !(loadedId === current.id || current.is_video) ||
            transcodes >= PRELOAD_MAX_TRANSCODES)
        )
          continue;
        const b = bytesOf(it);
        // 入らない1枚が出たら、そこで打ち切る（近い順に並んでいる）
        if (bytes + b > PRELOAD_BUDGET_BYTES) break;
        bytes += b;
        if (it.needs_transcode) transcodes++;
        out.push(it);
      }
      // 仕掛かり中の1枚は、いま隣でなくても持ち続ける（外すと、取り消せない
      // 変換だけが宙に浮く）。**列の最後**に置く——decodeを待つ順で先頭に来ると、
      // 速いJPEGの先読みが1秒級の変換の後ろに並んでしまう
      if (pending) out.push(pending);
      return out;
    };

    let cancelled = false;
    const apply = (items: MediaItem[]) => {
      if (cancelled) return;
      setPreload((prev) =>
        prev.length === items.length &&
        prev.every(
          // mtimeまで見る。差し替わったファイルを古い `?v=` で先読みしない
          (x, i) => x.id === items[i].id && x.mtime_ms === items[i].mtime_ms,
        )
          ? prev // 中身が同じなら参照を変えない（decodeのやり直しを防ぐ）
          : items,
      );
    };
    apply(select());
    // 聞くのは候補ぜんぶ（多くても前後8枚ずつ）。判定はファイル属性を見るだけで
    // 中身は開かないので、ダウンロードは起きない
    const unknown = ordered
      .filter((it) => !it.is_video && cloudAnswer(it.id) === undefined)
      .map((it) => it.id)
      .slice(0, CLOUD_ASK_MAX);
    if (unknown.length > 0) {
      cloudOnlyMedia(unknown)
        .then((cloud) => {
          const at = Date.now();
          for (const id of unknown)
            cloudOnlyRef.current.set(id, { cloud: false, at });
          for (const id of cloud) cloudOnlyRef.current.set(id, { cloud: true, at });
          apply(select());
        })
        .catch(() => {
          // 聞けなかったら先読みしない（勝手にダウンロードを起こさない）
        });
    }
    return () => {
      cancelled = true;
    };
  }, [
    viewerInfo,
    dayItems,
    summary,
    settledId,
    loadedId,
    transcodeTick,
    viewerScope,
    scopeIdx,
    stepPos,
  ]);

  // 先読みは**表示順に1枚ずつ**取りに行かせる。`src` をまとめて置くと
  // ブラウザは全部を同時に取りに行き、decodeを順に待っても
  // **いちばん要る「次の1枚」が最後に仕上がる**ことがある（ゲート1の指摘）。
  // だから `src` はここで1枚ずつ入れる——JSX側は空の `<img>` を置くだけ
  useEffect(() => {
    if (preload.length === 0) return;
    let cancelled = false;
    (async () => {
      for (const it of preload) {
        if (cancelled) return;
        const el = preloadElsRef.current.get(it.id);
        if (!el) continue;
        const src = fullSrc(it.id, it.mtime_ms);
        // 既に読んだものは触らない（同じ値を入れ直すと読み込みが起き直る）
        if (el.getAttribute("src") !== src) {
          // Rust側の変換はここで始まる。**始めた1枚を控えておく**
          if (it.needs_transcode) pendingTranscodeRef.current = it;
          el.setAttribute("src", src);
        }
        let underEstimated = false;
        try {
          await el.decode();
          // **届いた絵の実寸を控える**。DBの寸法で見積もった量より大きければ、
          // その場で選び直させる——予算は崖を避けるためのものなので、
          // 過小に数えたまま近所を抱え続けると意味が無い（ゲート2のP3）
          const before = decodedBytes(it, naturalRef.current.get(it.id));
          rememberNatural(it.id, el.naturalWidth, el.naturalHeight);
          // 逆（見積もりより小さかった）では選び直さない。予算が空くだけで
          // 崖には近づかないので、次の契機（変換の完了・送り）まで待てばよい
          underEstimated =
            decodedBytes(it, naturalRef.current.get(it.id)) > before;
        } catch {
          // 読めない絵（壊れている・消えた）は諦める。実際に開いたときに出る
        }
        // 仕掛かりが終わった。次の1枚を選び直させる
        if (pendingTranscodeRef.current?.id === it.id) {
          pendingTranscodeRef.current = null;
          if (pendingWatchdogRef.current !== null) {
            window.clearTimeout(pendingWatchdogRef.current);
            pendingWatchdogRef.current = null;
          }
          setTranscodeTick((t) => t + 1);
        } else if (underEstimated) {
          // 見積もりより大きかった。予算を数え直させる
          setTranscodeTick((t) => t + 1);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [preload, rememberNatural]);


  /** いま `<video>` を出している（＝最後まで見せたい）か */
  const playingVideo = Boolean(
    viewerItem?.is_video &&
      !videoError &&
      (videoInfo === null ||
        (videoInfo.exists && videoInfo.plays_in_app && !videoInfo.cloud_only)),
  );

  // 写真が切り替わったらズーム・パンをリセット。ビューアを閉じたら停止
  useEffect(() => {
    setZoom(1);
    setPinnedActualId(null);
    setPan({ x: 0, y: 0 });
    setVideoError(false);
    setVideoInfo(null);
  }, [viewer?.dayKey, viewer?.id]);

  // ウィンドウサイズを追う（表示倍率の分母になる）
  useEffect(() => {
    const onResize = () =>
      setWindowSize({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  /**
   * 画面に収めたときの表示倍率（実ピクセル比）。
   * CSSの max-width/max-height（.viewer-image と一致させること）で縮小され、
   * 元画像より大きくは表示されない。zoom=1 のときの実倍率がこれ。
   */
  const fitScale = useMemo(() => {
    if (!viewerItem) return 1;
    // 分母は**配信された絵の実寸**。届く前はDBの寸法で見当を付ける
    // （等倍は「選んだ」を [`pinnedActualId`] で覚えているので、届いた時点で
    // 正しい倍率に取り直される）
    const [w, h] =
      servedNatural && servedNatural.id === viewerItem.id
        ? [servedNatural.w, servedNatural.h]
        : servedSize(viewerItem);
    if (w <= 0 || h <= 0) return 1;
    const maxW = windowSize.w - 120;
    const maxH = windowSize.h - 80;
    return Math.min(1, maxW / w, maxH / h);
  }, [viewerItem, servedNatural, windowSize]);
  /** 実ピクセルに対する表示倍率（100%＝等倍） */
  const displayScale = fitScale * zoom;
  const isActualSize = Math.abs(displayScale - 1) < 0.005;

  /** 等倍（100%）と画面に合わせる表示を切り替える */
  const toggleActualSize = useCallback(() => {
    setPan({ x: 0, y: 0 });
    setPinnedActualId(isActualSize ? null : (viewerItem?.id ?? null));
    setZoom(isActualSize ? 1 : 1 / fitScale);
  }, [fitScale, isActualSize, viewerItem?.id]);

  // 等倍を選んでいるあいだは、分母が変わるたびに倍率を取り直す。
  // 絵が届いて実寸が分かったときと、ウィンドウの大きさが変わったときに効く。
  // **選んだ写真を見ているときだけ**（送った先へは持ち込まない）
  useEffect(() => {
    if (pinnedActualId === null || pinnedActualId !== viewerItem?.id) return;
    setZoom(1 / fitScale);
  }, [pinnedActualId, viewerItem?.id, fitScale]);

  // フルスクリーン（F11）。Webviewの標準APIで、ウィンドウ枠ごと消す
  const toggleFullscreen = useCallback(() => {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      document.documentElement.requestFullscreen().catch(() => {});
    }
  }, []);
  useEffect(() => {
    const onChange = () => setIsFullscreen(document.fullscreenElement !== null);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);
  // ビューアを閉じたら全画面も解く（2026-08-22）。
  //
  // **全画面はビューアの見方であって、一覧の見方ではない**。一覧に戻ったあとも
  // 枠が消えたままだと、そこから出る手が無い——全画面の切り替えはビューアの
  // 道具バーとF11にしかなく、そのどちらもビューアが開いているあいだしか効かない。
  // 地をクリックして閉じたときにここへ来る（閉じ方が増えても取りこぼさないよう、
  // 閉じる処理ではなく「閉じている」状態を見る）
  useEffect(() => {
    if (viewer !== null) return;
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  }, [viewer]);

  // ビューアのUIはマウスを止めると消す（写真だけが残るのが現代の標準）。
  // スライドショー中も同じ扱いで、鑑賞の邪魔をしない
  useEffect(() => {
    if (viewer === null) {
      setViewerIdle(false);
      return;
    }
    let timer = window.setTimeout(() => setViewerIdle(true), 1800);
    const wake = () => {
      setViewerIdle(false);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setViewerIdle(true), 1800);
    };
    window.addEventListener("mousemove", wake);
    window.addEventListener("keydown", wake);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("mousemove", wake);
      window.removeEventListener("keydown", wake);
    };
  }, [viewer]);
  useEffect(() => {
    if (viewer === null) {
      setPlaying(false);
      // スコープは**閉じたら捨てる**。次にタイルから開いたときに、
      // 前の選択の列を歩き続けてしまわないように
      setViewerScope(null);
      // ボツの印も捨てる（0.2 ③の不変条件）。関所を通さずに閉じる経路
      // （その日ごと消えた等）でも、**何も消えない側**へ倒れる
      setRejected((prev) => (prev.size === 0 ? prev : new Map()));
      setRejectGate(null);
    }
  }, [viewer]);

  /**
   * 窓の×で閉じようとしたとき（0.2 ③）。**ボツの印が残っていれば止めて
   * 関所を出す**——「✕を付けたのに何も消えていない」という誤解を塞ぐ。
   *
   * 出せずに落ちる経路（OSのシャットダウン等）が残っても、印はファイルを
   * 触っていないので**何も消えない側**へ倒れる。
   *
   * 登録は一度きりなので、印は [`rejectedRef`] から見る。
   */
  useEffect(() => {
    const un = getCurrentWindow().onCloseRequested((e) => {
      // **移動中の1度目は通さない**。ここを通すと、ゴミ箱へ入れ終えたぶんが
      // DBに残ったまま窓が消える。押した意思は控えておき、移動が終わった時点で
      // 閉じる（500件で約5秒）。
      //
      // **2度目は通す**。ゴミ箱側が詰まって戻ってこない機械があったときに、
      // また「閉じられない窓」を作らないため。そのとき残る不整合
      // （実体はゴミ箱・行はDBに残る）は、次の起動の同期が拾って消す
      if (trashingRef.current) {
        if (quitAfterTrashRef.current) return;
        e.preventDefault();
        quitAfterTrashRef.current = true;
        return;
      }
      if (rejectedRef.current.size === 0) return;
      // **2度目の×は通す**。1度目で関所を出しているので、それでも×を押すのは
      // 「終わる」という意思表示。印はファイルを1バイトも触っていないので、
      // ここで落としても失うものは無い——**止め続ける方が危ない**
      // （2026-08-19の利用者報告: ×で落ちず、タスクマネージャで落とすことになった）
      if (rejectGateRef.current) return;
      e.preventDefault();
      setRejectGate({ closeAfter: true, quitAfter: true });
    });
    return () => {
      void un.then((off) => off()).catch(() => {});
    };
  }, []);

  // スライドショー: 3秒ごとに次へ（末尾までいったら先頭へループ）。
  //
  // **再生中の動画の間は送らない**。3秒で切り替えては見られたものではないし、
  // スペースは動画の再生/一時停止に取られているので止める手も無くなる。
  // 代わりに動画が終わったら次へ進む（`onEnded`）。再生できない動画
  // （コンテナ違い・クラウド）は絵が出ないので、今までどおり3秒で送る
  useEffect(() => {
    if (!playing || playingVideo) return;
    const t = window.setInterval(() => moveViewer(1, true), 3000);
    return () => window.clearInterval(t);
  }, [playing, playingVideo, moveViewer]);

  useEffect(() => {
    if (viewer === null) return;
    /**
     * 選別の判定キーか（0.2 ②）。**修飾キーと一緒なら見送る**——
     * `Ctrl` + `P` はブラウザ由来の印刷で、押した人に⚑を付ける気は無い。
     * 他の1文字キーと違い、判定は写真の状態を書き換えるので念を入れる
     */
    const judging = (e: KeyboardEvent, key: string) =>
      e.key.toLowerCase() === key && !e.ctrlKey && !e.metaKey && !e.altKey;
    const onKey = (e: KeyboardEvent) => {
      // 文字入力中はビューアの1文字ショートカットを効かせない。
      // パレットはビューアの上にも開けるので、"family" と打つと f で★が
      // トグルされ i で撮影情報が開き、スペースでスライドショーが始まってしまう
      const target = e.target as HTMLElement | null;
      const typing =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.isContentEditable === true;
      if (paletteOpen || shortcutsOpen || typing) return;
      // 関所が開いているあいだは、下のキーを一切通さない（0.2 ③）。
      // Escapeは「関所を閉じる」＝ビューアへ戻る（印はそのまま）
      if (rejectGate) {
        if (e.key === "Escape" && !trashing) setRejectGate(null);
        return;
      }
      if (e.key === "Escape") requestCloseViewer();
      else if (e.key === "ArrowLeft") moveViewer(-1);
      else if (e.key === "ArrowRight") moveViewer(1);
      else if (e.key === "f" || e.key === "F") {
        if (viewerItem) favoriteViewer(viewerItem);
      } else if (judging(e, "p")) {
        if (viewerItem) judgeViewer(viewerItem, true);
      } else if (judging(e, "x")) {
        if (viewerItem) rejectViewer(viewerItem);
      } else if (judging(e, "u")) {
        if (viewerItem) judgeViewer(viewerItem, false);
      } else if (e.key === "i" || e.key === "I") {
        setShowExif((s) => !s);
      } else if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen();
      } else if (e.key === "1") {
        toggleActualSize();
      } else if (e.key === "0") {
        setZoom(1);
        setPinnedActualId(null);
        setPan({ x: 0, y: 0 });
      } else if (e.key === " ") {
        e.preventDefault();
        // 動画を開いている間のスペースは再生/一時停止（スライドショーではなく）。
        // `<video controls>` にフォーカスが無くても効くようにここで拾う
        const video = videoRef.current;
        if (viewerItem?.is_video && video) {
          if (video.paused) void video.play().catch(() => {});
          else video.pause();
        } else {
          setPlaying((p) => !p);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    viewer,
    viewerItem,
    moveViewer,
    favoriteViewer,
    judgeViewer,
    rejectViewer,
    requestCloseViewer,
    rejectGate,
    trashing,
    paletteOpen,
    shortcutsOpen,
    toggleFullscreen,
    toggleActualSize,
  ]);

  // 撮影情報は**パネルを開いている間だけ**、表示中の1枚について実ファイルから読む。
  // DBに列を持たないので常に実物と一致し、1000万件でもDBは1バイトも増えない
  // （`viewerItemId` は先読みの節で作ったものを使い回す）
  useEffect(() => {
    if (!showExif || viewerItemId === undefined) {
      setExif(null);
      return;
    }
    let cancelled = false;
    setExif(null);
    getExifInfo(viewerItemId)
      .then((info) => {
        if (!cancelled) setExif(info);
      })
      .catch(() => {
        /* EXIFが読めない画像（PNG等）はパネルに「情報なし」を出す */
      });
    return () => {
      cancelled = true;
    };
  }, [showExif, viewerItemId]);

  // ビューアを閉じたら撮影情報パネルも畳む
  useEffect(() => {
    if (viewer === null) setShowExif(false);
  }, [viewer]);

  // コマンドパレット（Ctrl+K / ⌘K）とショートカット一覧（`?` / `F1`）。
  // どちらもビューア表示中でも開ける
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        setPaletteOpen((p) => !p);
        return;
      }
      // 文字入力中は効かせない（検索欄に「?」と打てなくなる）
      const el = e.target as HTMLElement | null;
      const typing =
        el?.tagName === "INPUT" ||
        el?.tagName === "TEXTAREA" ||
        el?.isContentEditable === true;
      if (typing) return;
      if (e.key === "?" || e.key === "F1") {
        e.preventDefault();
        setShortcutsOpen((v) => !v);
      } else if (e.key === "Escape") {
        // **開いていたら閉じるだけ**。他のEsc（ビューアを閉じる・選択解除）は
        // それぞれの担当が見ているので、ここでは畳むだけにして横取りしない
        setShortcutsOpen((v) => (v ? false : v));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 複数選択のキー操作。**入力欄では効かせない**（検索中のCtrl+Aは文字の全選択）
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA")) return;
      // 何かが手前に出ているときは、そちらのEscを邪魔しない。
      // **取り込みウィザードと右クリックメニューも含める**——これが抜けていると、
      // モーダルの裏でグリッドが全選択されたり、Escの1押しで
      // 「メニューを閉じる」と「選択を解除」が同時に起きたりする。
      // 一覧を出していないカレンダー表示でも、選択だけ作られても仕方がない
      if (
        viewer !== null ||
        paletteOpen ||
        shortcutsOpen ||
        settingsOpen ||
        wizardOpen ||
        menu !== null ||
        view !== "grid"
      )
        return;
      if ((e.ctrlKey || e.metaKey) && (e.key === "a" || e.key === "A")) {
        e.preventDefault();
        selectAllRef.current().catch(() => {});
        return;
      }
      if (e.key === "Escape" && selectedRef.current.size > 0) {
        e.preventDefault();
        clearSelectionRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [viewer, paletteOpen, shortcutsOpen, settingsOpen, wizardOpen, menu, view]);

  // 検索条件が変わったら選択を捨てる。
  // **見えていないものを選んだまま**にすると、一括操作が思わぬ範囲に効く。
  // **種類（画像 / RAW / 動画）も並べる**——一覧に出るものが変わる条件は
  // すべてここを通す必要がある。世代（`selectEpochRef`）を進めるのも要点で、
  // 前の条件で走っていたCtrl+Aや範囲選択の返事が、切り替えた後の一覧へ
  // 遅れて流れ込むのを止める
  useEffect(() => {
    lastRangeRef.current = null;
    selectEpochRef.current += 1;
    setSelected(new Set());
    setAnchorId(null);
  }, [query, filter, kind]);

  /** 選択中かどうか。**選択が0なら選択モードではない** */
  const selecting = selected.size > 0;

  /** 選択をやめる */
  const clearSelection = useCallback(() => {
    setSelected(new Set());
    setAnchorId(null);
    lastRangeRef.current = null;
    // 待っている選択の応答を無効にする（Escの直後に範囲が復活しないように）
    beginSelectOp();
  }, []);

  // **一覧から離れたら選択をやめる**。カレンダーには写真が並ばないのに選択バーだけ
  // 残ると、画面に出ていないものへ一括削除が効いてしまう。
  // `clearSelection` は選択の世代も進めるので、待っている全選択・範囲選択の応答も落ちる
  useEffect(() => {
    if (view !== "grid") clearSelection();
  }, [view, clearSelection]);

  /** 1枚の選択を入れ替える */
  const toggleOne = useCallback((id: number) => {
    beginSelectOp();
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
    setAnchorId(id);
    lastRangeRef.current = null; // 起点が動くので、前の範囲は忘れる
  }, []);

  /**
   * 起点から今のものまでをまとめて選ぶ（Shift+クリック）。
   *
   * **画面に出ているものだけで決めない**。間に未読み込みの日が挟まると、
   * その日の写真が黙って外れる。並びの切り出しはDB側でやり、
   * **範囲のぶんだけ**を受け取る。
   */
  const selectRange = useCallback(
    async (fromId: number, toId: number) => {
      beginSelectOp();
      const epoch = selectEpochRef.current;
      // **範囲のぶんだけ**をDBから受け取る。切り出しはSQL側の仕事で、
      // 隣り合う2枚のために全IDを送らせない
      const range = await listMediaIdsBetween(
        queryRef.current,
        filterRef.current,
        fromId,
        toId,
      );
      // 待っている間に解除・別の選択・条件変更が入ったら、古い並びで選択を作らない
      if (selectEpochRef.current !== epoch) return;
      if (range.length === 0) {
        // 条件が変わって両端とも消えている等。単独の選択に落とす
        toggleOne(toId);
        return;
      }
      // 同じ起点で選び直すなら土台は据え置き。初回なら「いまの選択」が土台
      const base =
        lastRangeRef.current?.anchor === fromId
          ? lastRangeRef.current.base
          : selectedRef.current;
      lastRangeRef.current = { anchor: fromId, base };
      // **土台に範囲を足し直す**。前の範囲をIDごと引く形にすると、範囲の中に
      // たまたま入っていた「前から選んでいたもの」まで消える
      const next = new Set(base);
      for (const id of range) next.add(id);
      setSelected(next);
      // 起点は動かさない（続けてShift+クリックすると範囲を伸縮できる）
    },
    [toggleOne],
  );

  /** その日をまとめて選ぶ／外す（日付の見出しを押したとき） */
  const toggleDay = useCallback(
    async (dayKey: number) => {
      beginSelectOp();
      const epoch = selectEpochRef.current;
      const loaded = dayItems.get(dayKey);
      const items =
        loaded ??
        (await listDay(dayKey, queryRef.current, filterRef.current));
      // 待っている間に解除・別の選択・条件変更が入っていたら捨てる
      // （条件の変更も選択の世代を進める）
      if (selectEpochRef.current !== epoch) return;
      const ids = items.map((it) => it.id);
      if (ids.length === 0) return;
      setSelected((prev) => {
        const next = new Set(prev);
        // **全部入っているときだけ外す**。半端に入っているなら足す方が素直
        if (ids.every((id) => prev.has(id))) {
          for (const id of ids) next.delete(id);
        } else {
          for (const id of ids) next.add(id);
        }
        return next;
      });
      setAnchorId(ids[0]);
      lastRangeRef.current = null;
    },
    [dayItems],
  );

  /** 表示中の条件に一致する全部を選ぶ（Ctrl+A） */
  const selectAll = useCallback(async () => {
    beginSelectOp();
    const epoch = selectEpochRef.current;
    // ここだけは全IDを引く（「全部」を選ぶ操作なので、件数ぶんの選択が要る）
    const ids = await listMediaIds(
      queryRef.current,
      filterRef.current,
    );
    if (selectEpochRef.current !== epoch) return;
    setSelected(new Set(ids));
    setAnchorId(ids[0] ?? null);
    lastRangeRef.current = null;
  }, []);

  /**
   * 一括操作に掛ける前に、**いまの条件で本当に出ているものだけへ絞る**。
   *
   * 選択したあとに条件が変わらなくても、中身が変わることがある——★表示中に
   * 右クリックからお気に入りを外すと、その写真は画面から消えるのに選択には残る。
   * そのまま削除すると**画面に出ていない写真がゴミ箱へ行く**。
   * 絞った結果が減っていたら、選択の表示もそこへ合わせる。
   */
  const visibleSelection = useCallback(async () => {
    const current = selectedRef.current;
    if (current.size === 0) return [];
    const epoch = selectEpochRef.current;
    // **選んだIDだけを渡して確かめる**。全IDを引いて突き合わせると、
    // 数枚の確認のために一覧の全件がIPCを渡ることになる
    const kept = await visibleMediaIds(
      queryRef.current,
      filterRef.current,
      [...current],
    );
    if (selectEpochRef.current !== epoch) return [];
    if (kept.length !== current.size) setSelected(new Set(kept));
    return kept;
  }, []);

  /**
   * 選んだぶんだけをビューアで見る（0.2 ②）。連写ゾーンの集中選別の入口。
   *
   * 並べ直しと「いま並んでいるものだけ」への絞り込みは**Rust側で1回**やる
   * （`scopeMedia`）。選択はJS側では集合なので、こちらで並べると一覧とずれる
   * ——まだ読んでいない日の写真は、そもそも順番が分からない
   */
  const openSelectionInViewer = useCallback(async () => {
    const ids = [...selectedRef.current];
    if (ids.length === 0) return;
    const epoch = selectEpochRef.current;
    const scope = await scopeMedia(
      queryRef.current,
      filterRef.current,
      ids,
    );
    // 待っている間に選択が変わっていたら開かない（一括操作と同じ守り）
    if (selectEpochRef.current !== epoch) return;
    if (scope.length === 0) return;
    setViewerScope(scope);
    setViewer({ dayKey: scope[0].day_key, id: scope[0].id });
  }, []);

  // **レンダー中にrefを書き換えない**（レンダーは純粋であるべきで、破棄された
  // レンダーの値が残りうる）。描画が確定してから差し替える
  useLayoutEffect(() => {
    selectedRef.current = selected;
    selectAllRef.current = selectAll;
    clearSelectionRef.current = clearSelection;
  });

  /** タイルを押したときの振り分け */
  const onCellClick = useCallback(
    (item: MediaItem, dayKey: number, e: React.MouseEvent) => {
      if (e.shiftKey && anchorId !== null) {
        e.preventDefault();
        selectRange(anchorId, item.id).catch((err) => setStatus(String(err)));
        return;
      }
      if (e.ctrlKey || e.metaKey || selecting) {
        toggleOne(item.id);
        return;
      }
      setViewer({ dayKey, id: item.id });
    },
    [anchorId, selecting, selectRange, toggleOne],
  );

  /** 1枚をゴミ箱へ移動する（確認あり）。写真は取り返しがつかないのでOSのゴミ箱経由 */
  const onDelete = useCallback(
    async (item: MediaItem) => {
      // 一覧からのまとめて削除と**同じ鍵**を使う。1枚と数千枚が同時に走ると、
      // 進捗イベントもステータスも混ざる（ゲート1の指摘）
      if (deletingRef.current) return;
      deletingRef.current = true;
      try {
        const ok = await confirmDialog(t.deleteConfirm(1), {
          title: t.appName,
          kind: "warning",
        });
        if (!ok) return;
        const n = await deleteMedia([item.id]);
        setStatus(t.deleted(n));
        // 削除された日だけを捨てて取り直す（骨組みの件数も変わる）
        setDayItems((prev) => {
          if (!prev.has(item.day_key)) return prev;
          const next = new Map(prev);
          next.delete(item.day_key);
          return next;
        });
        // **ビューアは閉じない**（ゲート2のP2）。ここで閉じると、閉じたときの
        // 後始末が走って**✕の印が全部黙って消える**——30枚に印を付けた途中で
        // 1枚だけ🗑を使うと、残り29枚を付け直すことになる。居なくなった1枚からは
        // 「隣へ寄せる」効果が勝手に動くので、閉じる必要がそもそも無い。
        // **消した1枚は候補から外す**（関所に幽霊を並べない）
        setRejected((prev) => {
          if (!prev.has(item.id)) return prev;
          const next = new Map(prev);
          next.delete(item.id);
          return next;
        });
        // **選択からも外す**。残すと、操作バーの枚数と次の確認文言が実際より
        // 多く出て、居ないIDに一括操作を掛けることになる
        setSelected((prev) => {
          if (!prev.has(item.id)) return prev;
          const next = new Set(prev);
          next.delete(item.id);
          return next;
        });
        setAnchorId((a) => (a === item.id ? null : a));
        lastRangeRef.current = null;
        await refreshSummary();
      } catch (e) {
        setStatus(String(e));
      } finally {
        deletingRef.current = false;
      }
    },
    [refreshSummary],
  );

  /**
   * 選んだものをまとめて印を付ける／外す（★も⚑も同じ道を通る）。
   *
   * 画面は先に書き換えて、失敗したら戻す（1枚のときと同じ流儀）。
   * **その印で絞り込み中は骨組みが変わる**ので取り直す。
   */
  const onBulkMark = useCallback(
    async (kind: MarkKind, on: boolean) => {
      const ids = await visibleSelection();
      if (ids.length === 0) return;
      // **絞ったあとのIDで画面も触る**。`visibleSelection` が落としたぶん
      // （もう画面に出ていないもの）まで印を付け替えると、表示だけが嘘になる
      const touched = new Set(ids);
      const patch = (value: boolean) => {
        setDayItems((prev) => {
          const next = new Map(prev);
          for (const [dayKey, items] of prev) {
            if (!items.some((it) => touched.has(it.id))) continue;
            next.set(
              dayKey,
              items.map((it) =>
                touched.has(it.id) ? { ...it, [kind]: value } : it,
              ),
            );
          }
          return next;
        });
      };
      patch(on);
      let n: number;
      try {
        n =
          kind === "favorite"
            ? await setFavorites(ids, on)
            : await setPickeds(ids, on);
      } catch (e) {
        // **反転で戻さない**。選択に付いている・付いていないが混ざっていると、
        // 反転では元に戻らない（付ける操作の失敗で、全部が「外れた」表示になる）。
        // 触った日を捨ててDBから引き直すのが確実
        setDayItems((prev) => {
          const next = new Map(prev);
          for (const [dayKey, items] of prev) {
            if (items.some((it) => touched.has(it.id))) next.delete(dayKey);
          }
          return next;
        });
        setStatus(String(e));
        return;
      }
      setStatus(
        kind === "favorite"
          ? on
            ? t.bulkFavoriteDone(n)
            : t.bulkUnfavoriteDone(n)
          : on
            ? t.bulkPickDone(n)
            : t.bulkUnpickDone(n),
      );
      clearSelection();
      // ここから先が転んでも、DBは既に正しい。画面の巻き戻しはしない
      // その印で絞り込み中なら、外したものが画面から消える＝骨組みが変わる
      if (filterRef.current === (kind === "favorite" ? "fav" : "picked"))
        await reloadAll();
      else await refreshSummary();
    },
    [visibleSelection, reloadAll, refreshSummary, clearSelection],
  );

  /**
   * 消したあとの後始末（0.2 ③で括り出した。`onBulkDelete` と関所で同じ形）。
   *
   * **消えた日を丸ごと捨てて取り直す**——枚数も骨組みも変わるので、
   * 行を1つずつ抜くより取り直すほうが確か。
   *
   * **スコープの席は抜かない**。0.2 ②で「消えたidの席は残し、送りでは飛ばす」
   * 形にしてあり（ゲート2のP2）、寄せ先を探すときの目印にもなっている。
   * 抜くと、いま見ている1枚を消したときに**隣がどこか分からなくなる**
   */
  const forgetDeleted = useCallback((touched: Set<number>) => {
    setDayItems((prev) => {
      const next = new Map(prev);
      for (const [dayKey, items] of prev) {
        if (items.some((it) => touched.has(it.id))) next.delete(dayKey);
      }
      return next;
    });
  }, []);

  /**
   * 選んだものをまとめてゴミ箱へ（確認あり）。
   *
   * **確認ダイアログを待つ前に鍵を掛ける**（書き出しと同じ形・ゲート1の指摘）。
   * ゴミ箱への移動は別スレッドへ出したので、走っているあいだも画面は動く
   * ——2回押せば2本ともダイアログまで進み、同じIDに二重の操作が掛かって
   * 進捗とステータスが混ざる。ボタンの非活性（`busy`）は次のレンダーを待つので、
   * **即座に効くのはrefだけ**。
   */
  const onBulkDelete = useCallback(async () => {
    if (deletingRef.current) return;
    deletingRef.current = true;
    setBusy(true);
    try {
      const ids = await visibleSelection();
      if (ids.length === 0) return;
      // 消すのは絞ったあとのIDだけ。画面の巻き取りも同じ顔ぶれで見る
      const touched = new Set(ids);
      const ok = await confirmDialog(t.deleteConfirm(ids.length), {
        title: t.appName,
        kind: "warning",
      });
      if (!ok) return;
      try {
        const n = await deleteMedia(ids);
        setStatus(t.deleted(n));
        forgetDeleted(touched);
        setViewer((v) => (v && touched.has(v.id as number) ? null : v));
        clearSelection();
        await refreshSummary();
      } catch (e) {
        // **一部だけ成功していることがある**。バックエンドは消せたぶんをDBから
        // 落としてからエラーを返すので、画面をそのままにすると
        // 「もう無い写真が並んだまま、選択にも残る」状態になる。取り直す
        setStatus(String(e));
        forgetDeleted(touched);
        clearSelection();
        await refreshSummary().catch(() => {});
      }
    } finally {
      deletingRef.current = false;
      setBusy(false);
    }
  }, [visibleSelection, refreshSummary, clearSelection, forgetDeleted]);

  /**
   * 関所が片付いたあとの行き先。**開いた理由のところまで戻す**——
   * 窓の×から来たなら窓ごと閉じ、ビューアを閉じようとして来たならビューアだけ、
   * チップからの途中確認なら何も閉じない。
   *
   * 窓は `close()` ではなく `destroy()` で閉じる。`close()` は「閉じる要求」から
   * やり直すので、上の受け口をもう一度通る——**関所を通ったあとに、また関所の
   * 判断をさせない**。
   */
  const finishGate = useCallback(
    (gate: { closeAfter: boolean; quitAfter?: boolean }) => {
      if (gate.quitAfter) {
        void getCurrentWindow().destroy();
        return;
      }
      if (gate.closeAfter) setViewer(null);
    },
    [],
  );

  /**
   * 関所で確定して、ボツの候補をまとめてゴミ箱へ（0.2 ③）。
   *
   * **待ちはここ1回だけ**。判定のたびに消す形（1件20ms〜215ms）を避けた
   * 眼目がこれで、200件917ms・**実機の500件で約4.9秒**を1回にまとめて払う。
   */
  const confirmTrash = useCallback(async () => {
    const gate = rejectGate;
    if (!gate || trashing) return;
    const ids = [...rejected.keys()];
    if (ids.length === 0) return;
    setTrashing(true);
    setTrashProgress(null);
    try {
      // **実行直前に、まだ在るものだけへ絞る**（一括操作の作法・PR #18）。
      // `media.id` の使い回し（既知のP3）への安い緩和でもある。
      //
      // **ただし ★ / ⚑ の絞り込みは掛けない**（"all" を渡す。ゲート2の指摘）。
      // 関所に並んだ顔がそのまま約束なのに、絞り込みを掛けると
      // **自分の操作で棚から外れた1枚が黙って残る**——⚑の棚で選別しながら
      // `X` を押すと、その場で⚑が外れて⚑の絞り込みから落ちる。検索語は残す
      // （別の言葉で探し直したなら、それは見ていた列ではない）
      const kept = await visibleMediaIds(queryRef.current, "all", ids);
      if (kept.length > 0) {
        const n = await deleteMedia(kept);
        // **数が合わないときは黙らない**（ゲート2のP3）。もう無い・検索語から
        // 外れた等でここまで来られなかったぶんを、そのまま件数で言う
        const left = ids.length - n;
        setStatus(left > 0 ? t.deletedSomeLeft(n, left) : t.deleted(n));
        forgetDeleted(new Set(kept));
        await refreshSummary();
      }
      setRejected(new Map());
      setRejectGate(null);
      // 移動中に×が押されていたなら、**その意思のとおり**ここで閉じる
      finishGate(
        quitAfterTrashRef.current ? { closeAfter: true, quitAfter: true } : gate,
      );
    } catch (e) {
      // **一部だけ成功していることがある**（消せたぶんはDBから落ちている）。
      // 画面は取り直し、**印は残す**——残っている写真をもう一度確かめられる
      // ようにする。閉じようとして開いた関所でも、ここでは閉じない
      setStatus(String(e));
      forgetDeleted(new Set(ids));
      await refreshSummary().catch(() => {});
      setRejectGate(null);
    } finally {
      setTrashing(false);
      setTrashProgress(null);
      // **失敗した回は閉じない**。何が起きたかを読める場所に残す
      // （もう一度×を押せば、そのときは通る）
      quitAfterTrashRef.current = false;
    }
  }, [
    rejectGate,
    trashing,
    rejected,
    forgetDeleted,
    refreshSummary,
    finishGate,
  ]);

  /**
   * 印を全部「戻す」と、確かめるものが無くなる。関所は畳む——
   * 開いた理由のところまで戻す（窓の×から来ていたなら、窓ごと閉じる）
   */
  useEffect(() => {
    if (!rejectGate || rejected.size > 0 || trashing) return;
    const gate = rejectGate;
    setRejectGate(null);
    finishGate(gate);
  }, [rejectGate, rejected, trashing, finishGate]);

  /**
   * 選んだものを、フォルダへコピー／移動する。
   *
   * **一括操作の作法は削除と同じ**——先に `visibleSelection` でいま出ているものへ
   * 絞ってから渡す。移動は元の場所から無くなるので確認を取る。
   */
  const onBulkExport = useCallback(
    async (moveFiles: boolean) => {
      // **確認とフォルダ選択を待つ前に鍵を掛ける**。ボタンの非活性は
      // `busy` の反映（次のレンダー）を待つので、続けて2回押されると
      // 2本ともダイアログまで進んでしまう。refなら即座に効く
      if (exportingRef.current) return;
      exportingRef.current = true;
      setBusy(true);
      try {
        const ids = await visibleSelection();
        if (ids.length === 0) return;
        if (moveFiles) {
          const ok = await confirmDialog(t.moveConfirm(ids.length), {
            title: t.appName,
            kind: "warning",
          });
          if (!ok) return;
        }
        const dest = await open({ directory: true, title: t.pickExportFolder });
        if (typeof dest !== "string") return;
        const st = await exportMedia(ids, dest, moveFiles);
        setStatus(t.exportDone(st.done, st.skipped, st.failed, st.left_behind));
        if (moveFiles) {
          // 移動したぶんはライブラリから外れている。選択も画面も取り直す
          clearSelection();
          await reloadAll();
        }
      } catch (e) {
        // **一部だけ動いていることがある**（DBへの反映で転んだ場合など）。
        // 画面をそのままにすると、もう別の場所にある写真が並んだまま残る
        setStatus(String(e));
        if (moveFiles) {
          clearSelection();
          await reloadAll().catch(() => {});
        }
      } finally {
        exportingRef.current = false;
        setBusy(false);
      }
    },
    [visibleSelection, clearSelection, reloadAll],
  );

  /** 「他のアプリで開く…」: 実行ファイルを選ばせ、選んだアプリは設定に覚える */
  const onOpenWithOther = useCallback(
    async (item: MediaItem) => {
      try {
        const app = await open({
          title: t.pickEditor,
          multiple: false,
          directory: false,
        });
        if (typeof app !== "string") return;
        await openWith(item.id, app, true);
        await refreshRoots();
      } catch (e) {
        setStatus(String(e));
      }
    },
    [refreshRoots],
  );

  /** 対象1枚に対する右クリックメニューの項目 */
  const menuItemsFor = useCallback(
    (item: MediaItem): MenuItem[] => [
      { label: t.menuOpen, run: () => openDefault(item.id).catch(() => {}) },
      ...editors.map((app) => ({
        label: t.menuOpenWith(app.name),
        run: () => openWith(item.id, app.path, false).catch(() => {}),
      })),
      { label: t.menuOpenWithOther, run: () => onOpenWithOther(item) },
      {
        label: t.menuReveal,
        separator: true,
        run: () => revealInFolder(item.id).catch(() => {}),
      },
      {
        label: item.favorite ? t.menuFavoriteOff : t.menuFavoriteOn,
        run: () => toggleFavorite(item),
      },
      {
        label: item.picked ? t.bulkPickOff : t.bulkPickOn,
        run: () => void setMark(item, "picked", !item.picked),
      },
      {
        label: t.menuDelete,
        danger: true,
        separator: true,
        run: () => onDelete(item),
      },
    ],
    [editors, onOpenWithOther, onDelete, toggleFavorite],
  );

  const openDay = useCallback((dayKey: number) => {
    setView("grid");
    setPendingScrollDay(dayKey);
  }, []);

  /** パレットから検索を実行する（検索ボックスへ流し込んで通常の検索経路に乗せる） */
  const runSearch = useCallback((q: string) => {
    setView("grid");
    setQueryInput(q);
  }, []);

  /** コマンドパレットのアプリ操作 */
  const paletteActions = useMemo<PaletteItem[]>(
    () => [
      {
        group: t.paletteGroupActions,
        icon: "📥",
        label: t.importFromUsb,
        run: () => openWizard(),
      },
      {
        group: t.paletteGroupActions,
        icon: "🔄",
        label: t.rescan,
        run: () => onSync(),
      },
      {
        group: t.paletteGroupActions,
        icon: "★",
        label: t.actionShowFavorites,
        run: () => setFilter("fav"),
      },
      {
        group: t.paletteGroupActions,
        icon: "⚑",
        label: t.actionShowPicked,
        run: () => setFilter("picked"),
      },
      {
        group: t.paletteGroupActions,
        icon: "⌨",
        label: t.actionShortcuts,
        run: () => setShortcutsOpen(true),
      },
      {
        group: t.paletteGroupActions,
        icon: "🖼",
        label: t.actionShowAll,
        run: () => {
          setFilter("all");
          setQueryInput("");
        },
      },
      {
        group: t.paletteGroupActions,
        icon: "📆",
        label: t.actionCalendar,
        run: () => setView("calendar"),
      },
      {
        group: t.paletteGroupActions,
        icon: "▦",
        label: t.actionThumbnails,
        run: () => setView("grid"),
      },
    ],
    [openWizard, onSync],
  );

  // 選択スコープで開いているあいだは、端も分母も**その列**で決まる（0.2 ②）
  const hasPrev =
    scopeIdx !== undefined
      ? scopeIdx > 0
      : viewerInfo !== null &&
        !(viewerInfo.dayIdx === 0 && viewerInfo.itemIdx === 0);
  const hasNext =
    scopeIdx !== undefined
      ? scopeIdx < (viewerScope?.length ?? 0) - 1
      : viewerInfo !== null &&
        !(
          viewerInfo.dayIdx === summary.length - 1 &&
          viewerInfo.itemIdx === viewerInfo.dayLength - 1
        );
  const viewerPos =
    scopeIdx !== undefined
      ? scopeIdx + 1
      : viewerInfo !== null
        ? prefixCounts[viewerInfo.dayIdx] + viewerInfo.itemIdx + 1
        : 0;
  /** カウンターの分母。スコープで開いていれば選んだ枚数 */
  const viewerTotal =
    scopeIdx !== undefined ? (viewerScope?.length ?? 0) : totalShown;

  return (
    <div className="app">
      {selecting && (
        <div className="select-bar">
          <button
            className="select-bar-close"
            title={t.clearSelection}
            onClick={clearSelection}
          >
            ✕
          </button>
          <span className="select-bar-count">
            {t.selectedCount(selected.size)}
          </span>
          <button
            disabled={busy}
            onClick={() => selectAll().catch((e) => setStatus(String(e)))}
          >
            {t.selectAll}
          </button>
          <button
            disabled={busy}
            onClick={() =>
              openSelectionInViewer().catch((e) => setStatus(String(e)))
            }
          >
            {t.bulkViewer}
          </button>
          <span className="select-bar-spacer" />
          <button
            disabled={busy}
            onClick={() =>
              onBulkMark("picked", true).catch((e) => setStatus(String(e)))
            }
          >
            ⚑ {t.bulkPickOn}
          </button>
          <button
            disabled={busy}
            onClick={() =>
              onBulkMark("picked", false).catch((e) => setStatus(String(e)))
            }
          >
            {t.bulkPickOff}
          </button>
          <button
            disabled={busy}
            onClick={() =>
              onBulkMark("favorite", true).catch((e) => setStatus(String(e)))
            }
          >
            ★ {t.bulkFavoriteOn}
          </button>
          <button
            disabled={busy}
            onClick={() =>
              onBulkMark("favorite", false).catch((e) => setStatus(String(e)))
            }
          >
            {t.bulkFavoriteOff}
          </button>
          <button
            disabled={busy}
            onClick={() => onBulkExport(false).catch((e) => setStatus(String(e)))}
          >
            {t.bulkCopy}
          </button>
          <button
            disabled={busy}
            onClick={() => onBulkExport(true).catch((e) => setStatus(String(e)))}
          >
            {t.bulkMove}
          </button>
          <button
            className="danger"
            disabled={busy}
            onClick={() => onBulkDelete().catch((e) => setStatus(String(e)))}
          >
            🗑 {t.bulkDelete}
          </button>
        </div>
      )}
      <header className="topbar">
        <span className="logo">{t.appName}</span>
        <div className="view-switch">
          <button
            className={view === "grid" ? "active" : ""}
            onClick={() => setView("grid")}
          >
            {t.viewThumbnails}
          </button>
          <button
            className={view === "calendar" ? "active" : ""}
            onClick={() => setView("calendar")}
          >
            {t.viewCalendar}
          </button>
        </div>
        <div className="search-box">
          <span className="search-icon">🔍</span>
          <input
            type="search"
            placeholder={t.searchPlaceholder}
            value={queryInput}
            onChange={(e) => setQueryInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setQueryInput("");
                (e.target as HTMLInputElement).blur();
              }
            }}
          />
          {queryInput ? (
            <button
              className="search-clear"
              title={t.searchClear}
              onClick={() => setQueryInput("")}
            >
              ✕
            </button>
          ) : (
            <span className="search-kbd" title={t.commandPalette}>
              {modKeyLabel}
            </span>
          )}
        </div>
        <button className="primary" onClick={() => openWizard()} disabled={busy}>
          {t.importFromUsb}
        </button>
        <button onClick={onSync} disabled={busy}>
          {t.rescan}
        </button>
        <button title={t.settings} onClick={() => setSettingsOpen(true)}>
          ⚙
        </button>
        <label className="slider">
          {t.size}
          <input
            type="range"
            min={100}
            max={320}
            step={20}
            value={cellSize}
            onChange={(e) => setCellSize(Number(e.target.value))}
          />
        </label>
        {/* 32ch で省略されるので、全文はホバーで読めるようにする */}
        <span className="status" title={status}>
          {status}
        </span>
        <span className={"count" + (query ? " searching" : "")}>
          {query && "🔍 "}
          {formatNumber(totalShown)} {t.itemsSuffix}
        </span>
      </header>
      {speedReport && (
        <div className="speed-toast" onClick={() => setSpeedReport(null)}>
          {speedLabel(speedReport)}
        </div>
      )}
      {heifMissing != null && (
        <div className="speed-toast index warn decoder-notice">
          <span>
            {platform === "windows"
              ? t.decoderHeifNotice(heifMissing)
              : platform === "macos"
                ? t.decoderHeifNoticeMac(heifMissing)
                : t.decoderHeifNoticeOther(heifMissing)}
          </span>
          {/* **文言と同じ判定に揃える。** 片方が先に届いた瞬間に
              「デコーダが無いのかも」と「拡張機能を買え」が同時に出うる
              ——2つの往復に順番の保証は無い（ゲート2の指摘） */}
          {platform === "windows" && decoderHelp && (
            <>
              <button onClick={() => openDecoderHelp("heif").catch(() => {})}>
                {t.decoderHeifHow}
              </button>
              <button onClick={() => openDecoderHelp("hevc").catch(() => {})}>
                {t.decoderHevcHow}
              </button>
            </>
          )}
          <button
            onClick={() => {
              localStorage.setItem(DECODER_NOTICE_KEY, "off");
              setHeifMissing(null);
            }}
          >
            {t.decoderNoticeDismiss}
          </button>
        </div>
      )}
      {/* 新しい版の知らせ（0.2）。**押さなければ何も起きない**——
          落として入れ替えるのはブラウザとインストーラの仕事で、ここは
          「出ていますよ」と言うだけ */}
      {updateFound && (
        <div className="speed-toast index decoder-notice update-notice">
          <span>{t.updateFound(updateFound.latest ?? "")}</span>
          <button onClick={() => openDownloadPage().catch(() => {})}>
            {t.updateOpenPage}
          </button>
          <button onClick={() => setUpdateFound(null)}>{t.updateLater}</button>
        </div>
      )}
      {indexProgress &&
        (indexProgress.building ? (
          <div className="speed-toast index">
            {indexProgress.phase === "camera"
              ? t.cameraScanning
              : t.indexBuilding}
            {Math.min(
              99,
              Math.floor(
                (indexProgress.done / (indexProgress.total || 1)) * 100,
              ),
            )}
            {t.indexProgressSuffix}
          </div>
        ) : (
          <div
            className="speed-toast index warn"
            onClick={() => setIndexProgress(null)}
          >
            {t.indexIncompleteWarning}
          </div>
        ))}
      <div className="body">
        <nav className="sidebar">
          <div className="nav-section">{t.navPlaces}</div>
          <div
            className={"nav-item" + (filter === "all" ? " active" : "")}
            onClick={() => setFilter("all")}
          >
            {t.navAllPhotos}
          </div>
          <div
            className={"nav-item" + (filter === "fav" ? " active" : "")}
            onClick={() => setFilter("fav")}
          >
            {t.navFavorites}
            {stats.favorites > 0 && (
              <span className="fav-count">{stats.favorites}</span>
            )}
          </div>
          {/* 選別で選んだもの（⚑。0.2 ②）。★とは別の棚なので入口も分ける */}
          <div
            className={"nav-item" + (filter === "picked" ? " active" : "")}
            onClick={() => setFilter("picked")}
          >
            {t.navPicked}
            {stats.picked > 0 && (
              <span className="fav-count">{stats.picked}</span>
            )}
          </div>
          {/* 種類（画像 / RAW / 動画）。**★ / ⚑ とは別の軸**なので節を分ける
              ——重ねて効くものを同じ列に並べると、片方を押したときに
              もう片方が外れる棚に見える。カメラと同じで、押している1つを
              もう一度押せば外れる（「すべて」の行は置かない） */}
          <div className="nav-section">{t.navKinds}</div>
          {KINDS.map((k) => (
            <div
              key={k}
              className={"nav-item" + (kind === k ? " active" : "")}
              onClick={() => setKind(kind === k ? "all" : k)}
            >
              {KIND_LABEL[k]()}
            </div>
          ))}
          {cameras.length > 0 && (
            <>
              <div className="nav-section">
                {t.navCameras}
                {cameras.length > CAMERAS_COLLAPSED && (
                  <button
                    className="nav-more"
                    onClick={() => setCamerasExpanded((v) => !v)}
                  >
                    {camerasExpanded
                      ? t.collapse
                      : t.showMore(cameras.length - CAMERAS_COLLAPSED)}
                  </button>
                )}
              </div>
              {(camerasExpanded
                ? cameras
                : cameras.slice(0, CAMERAS_COLLAPSED)
              ).map((cam) => {
                const term = `camera:"${cam.name}"`;
                const active = queryInput.includes(term);
                return (
                  <div
                    key={cam.name}
                    className={"nav-item camera" + (active ? " active" : "")}
                    title={t.filterByCamera(cam.name)}
                    onClick={() => setQueryInput(active ? "" : term)}
                  >
                    <span className="camera-icon">📷</span>
                    <span className="camera-name">{cam.name}</span>
                    <span className="fav-count">{cam.count}</span>
                  </div>
                );
              })}
            </>
          )}
          <div className="nav-section">{t.navLibraryFolders}</div>
          {roots.map((r) => (
            <div key={r} className="nav-item root" title={r}>
              <span className="root-name">
                {r.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || r}
              </span>
              <button
                className="root-remove"
                title={t.removeRoot(r)}
                disabled={busy}
                onClick={() => onRemoveRoot(r)}
              >
                ✕
              </button>
            </div>
          ))}
          <div className="nav-section">{t.navDrives}</div>
          {drives.map((d) => (
            <div
              key={d.path}
              className={"nav-item drive" + (d.removable ? " removable" : "")}
              title={t.importFrom(d.path)}
              onClick={() =>
                !busy && openWizard(d.dcim_path ?? d.path)
              }
            >
              <span className="drive-icon">{d.removable ? "🔌" : "💾"}</span>
              {d.label}
              {d.dcim_path && <span className="dcim-badge">DCIM</span>}
            </div>
          ))}
          <div className="nav-section">{t.navAddFolder}</div>
          <div className="add-folder">
            <button
              className="browse-folder"
              onClick={onBrowseFolder}
              disabled={busy}
            >
              <span aria-hidden="true">📂</span> {t.browse}
            </button>
            <div className="add-folder-manual">
              <input
                type="text"
                placeholder={t.addFolderPlaceholder}
                value={folderInput}
                onChange={(e) => setFolderInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && onAddFolder()}
              />
              <button onClick={onAddFolder} disabled={busy}>
                {t.add}
              </button>
            </div>
          </div>
        </nav>
        <div className="main">
          {/* 検索中は隠す: 思い出は検索条件を無視して取っているため、結果と
              無関係な写真が混ざるうえ、開いてもタイムライン上に居場所がなく
              ビューアがすぐ閉じてしまう */}
          {view === "grid" && !filtering && memories.length > 0 && (
            <div className="memories">
              <span className="memories-title">
                🎞 {t.memoriesTitle(memories[0].years_ago)}
              </span>
              <div className="memories-strip">
                {memories.map(({ years_ago, item }) => (
                  <img
                    key={item.id}
                    src={thumbSrc(item)}
                    title={`${t.memoriesTitle(years_ago)} — ${item.file_name}`}
                    onClick={() =>
                      setViewer({ dayKey: item.day_key, id: item.id })
                    }
                  />
                ))}
              </div>
            </div>
          )}
          {/* **無言で `0 件` を出さない。** 全部正しく動いたうえで空になることが
              ある——macOSの `~/Pictures` は写真.appのライブラリしか持たず、
              それは既定の除外に入っている。理由が出ないと「壊れている」と読まれる。
              **サムネイルでもカレンダーでも出す**（片方だけだと、切り替えた
              とたんに無言へ戻る。ゲート1の指摘） */}
          {showEmptyPanel && (
            <div className="empty-library">
              <h2>{loadFailed ? t.emptyTitleFailed : t.emptyTitle}</h2>
              <p>{emptyMessage()}</p>
              <div className="empty-actions">
                {/* 取り込みを先に置く——macOSでは**そちらが本来の入口** */}
                <button
                  className="primary"
                  onClick={() => !busy && openWizard()}
                  disabled={busy}
                >
                  {t.importFromUsb}
                </button>
                <button onClick={onBrowseFolder} disabled={busy}>
                  <span aria-hidden="true">📂</span> {t.browse}
                </button>
              </div>
            </div>
          )}
          {view === "calendar" ? (
            <div className="calendar-scroll">
              {/* パネルが出ているなら、カレンダー側の「写真がありません」は
                  出さない。**同じ画面に空の知らせが2つ並ぶ**（ゲート1の指摘） */}
              {!showEmptyPanel && (
                <Calendar summary={summary} onOpenDay={openDay} />
              )}
            </div>
          ) : (
            <div className="grid-wrap">
              {yearMarkers.length > 1 && (
                <div className="scrubber">
                  {yearMarkers.map((m) => (
                    <button
                      key={m.year}
                      className={"scrub-tick" + (m.label ? " labeled" : "")}
                      style={{ top: `${m.top}%` }}
                      title={t.jumpToYear(m.year)}
                      onClick={() =>
                        virtualizer.scrollToIndex(m.rowIndex, { align: "start" })
                      }
                    >
                      {m.label ? m.year : ""}
                    </button>
                  ))}
                </div>
              )}
              <div className="grid-scroll" ref={scrollRef}>
              <div
                style={{
                  height: virtualizer.getTotalSize(),
                  position: "relative",
                }}
              >
                {virtualItems.map((vr) => {
                  const row = rows[vr.index];
                  if (!row) return null;
                  return (
                    <div
                      key={vr.key}
                      style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: "100%",
                        height: vr.size,
                        transform: `translateY(${vr.start}px)`,
                      }}
                    >
                      {row.kind === "header" ? (
                        <div className="date-header">
                          {/*
                            日付を押すとその日をまとめて選ぶ。**全部入っているときだけ
                            外す**——半端に入っている状態から押したら足す方が素直。
                          */}
                          <button
                            className="date-header-btn"
                            title={t.selectDay}
                            onClick={() => {
                              const key = row.dayKey;
                              toggleDay(key).catch((e) => setStatus(String(e)));
                            }}
                          >
                            {row.label}
                            <span className="date-count">
                              {t.photosCount(row.count)}
                            </span>
                          </button>
                        </div>
                      ) : row.kind === "placeholder" ? (
                        <div
                          className="placeholder-row"
                          style={{ height: row.height - GAP }}
                        />
                      ) : (
                        <div className="cell-row" style={{ gap: GAP }}>
                          {row.cells.map((cell) => (
                            <div
                              key={cell.item.id}
                              className={
                                "cell-wrap" +
                                (selected.has(cell.item.id) ? " picked" : "")
                              }
                              style={{ width: cell.w, height: cell.h }}
                            >
                              <img
                                className="cell"
                                loading="lazy"
                                decoding="async"
                                src={thumbSrc(cell.item)}
                                title={cell.item.file_name}
                                // サムネイル未生成のHEIC/RAWや、まだ手元に無い
                                // クラウド上のファイルは配信されない（404）。
                                // `alt` 未指定だと Chromium は title を代替テキストとして
                                // 枠に描いてしまう（ファイル名が並ぶ）。装飾用として空にする。
                                // サムネイルが出せないセルは、バックエンドが透明な1x1を返す
                                alt=""
                                onClick={(e) =>
                                  onCellClick(cell.item, row.dayKey, e)
                                }
                                onContextMenu={(e) => {
                                  e.preventDefault();
                                  setMenu({
                                    pos: { x: e.clientX, y: e.clientY },
                                    item: cell.item,
                                  });
                                }}
                              />
                              {cell.item.is_video && (
                                <span className="cell-video">
                                  ▶
                                  {cell.item.duration_ms != null && (
                                    <span className="cell-duration">
                                      {formatDuration(cell.item.duration_ms)}
                                    </span>
                                  )}
                                </span>
                              )}
                              {cell.item.favorite && (
                                <span className="cell-fav">★</span>
                              )}
                              {/* 選別の印。`cell-pick` は複数選択の丸なので
                                  名前を分ける（`cell-flag`） */}
                              {cell.item.picked && (
                                <span className="cell-flag" title={t.viewerPicked}>
                                  ⚑
                                </span>
                              )}
                              {/*
                                選択の丸。ホバーで出て、選択中は出しっぱなし。
                                タイルのクリックとは別扱いにする（**選択モードに
                                入っていなくてもここからは選べる**）
                              */}
                              <button
                                className="cell-pick"
                                title={t.selectItem}
                                // 未選択のときは中身が空なので、名前を明示する
                                aria-label={t.selectItem}
                                aria-pressed={selected.has(cell.item.id)}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  if (e.shiftKey && anchorId !== null) {
                                    selectRange(anchorId, cell.item.id).catch(
                                      (err) => setStatus(String(err)),
                                    );
                                    return;
                                  }
                                  toggleOne(cell.item.id);
                                }}
                              >
                                {selected.has(cell.item.id) ? "✓" : ""}
                              </button>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
      {viewer && (
        <div
          className={"viewer" + (viewerIdle ? " idle" : "")}
          onClick={requestCloseViewer}
        >
          {viewerItem && viewerItem.is_video ? (
            // 動画（第9部）。`<img>` ではなく `<video>` に渡す。
            // 実体は `media://video/<id>` から**Rangeで刻んで**届くので、
            // 2GBのファイルでもメモリに載るのは4MiBずつ
            !videoInfo ? (
              <div className="viewer-loading">{t.loading}</div>
            ) : playingVideo ? (
              <video
                // idをkeyにして、次の動画へ進んだときに要素を作り直す
                // （src差し替えだけだと前の再生位置やバッファが残る）
                key={viewerItem.id}
                ref={videoRef}
                className="viewer-video"
                src={videoSrc(viewerItem.id, viewerItem.mtime_ms)}
                controls
                autoPlay
                onClick={(e) => e.stopPropagation()}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setMenu({
                    pos: { x: e.clientX, y: e.clientY },
                    item: viewerItem,
                  });
                }}
                // スライドショー中は最後まで見せてから次へ送る
                onEnded={() => {
                  if (playing) moveViewer(1, true);
                }}
                // コンテナは扱えてもコーデック（HEVC）がOSに無ければここへ来る
                onError={() => setVideoError(true)}
              />
            ) : (
              <div
                className="viewer-fallback"
                onClick={(e) => e.stopPropagation()}
              >
                <p className="fallback-title">
                  {!videoInfo.exists
                    ? t.videoMissing
                    : videoInfo.cloud_only
                      ? t.videoCloudOnly
                      : videoInfo.plays_in_app
                        ? t.videoFailed
                        : t.videoUnsupported}
                </p>
                {/* 拡張機能の案内を出すのは「コンテナは扱えるのに再生できなかった」
                    ときだけ。ファイルが無い・コンテナが違う・クラウドにしか無いのを
                    コーデック不足と取り違えて、有料の拡張機能を勧めてしまわない */}
                {videoInfo.cloud_only && (
                  <p className="fallback-note">{t.videoCloudOnlyNote}</p>
                )}
                {/* 文言は3つに分ける。Windowsは拡張機能を買う話、macOSは最初から
                    再生できる、Linuxは**デコーダがあるとは言い切れない**
                    （ディストリ次第。HEVCは同梱しない方針＝`video.rs`） */}
                {videoInfo.exists &&
                  !videoInfo.cloud_only &&
                  videoInfo.plays_in_app && (
                    <p className="fallback-note">
                      {platform === "windows"
                        ? t.videoCodecNote
                        : platform === "macos"
                          ? t.videoCodecNoteMac
                          : t.videoCodecNoteOther}
                    </p>
                  )}
                <div className="fallback-actions">
                  {videoInfo.exists && (
                    <button
                      onClick={() => openDefault(viewerItem.id).catch(() => {})}
                    >
                      {t.videoOpenExternal}
                    </button>
                  )}
                  {videoInfo.exists &&
                    !videoInfo.cloud_only &&
                    videoInfo.plays_in_app &&
                    platform === "windows" && (
                      <button
                        onClick={() => openDecoderHelp("hevc").catch(() => {})}
                      >
                        {t.videoCodecHelp}
                      </button>
                    )}
                </div>
              </div>
            )
          ) : viewerItem ? (
            <img
              className="viewer-image"
              // 門が開くまで `src` を置かない＝要求そのものを出さない。
              // 置いてから消しても、Rust側で走り出した変換は取り消せない
              src={
                fullGateId === viewerItem.id
                  ? fullSrc(viewerItem.id, viewerItem.mtime_ms)
                  : undefined
              }
              // 絵が出ていないあいだ（門が閉じている・原寸が出せなかった）は
              // 代替テキストを黙らせる——絵の真ん中にファイル名が浮くと、
              // 情報ではなくゴミに見える
              alt={
                fallbackToThumb || fullGateId !== viewerItem.id
                  ? ""
                  : viewerItem.file_name
              }
              draggable={false}
              // 読み込み中表示をここで畳む。失敗（onError）でも畳む——
              // 出ない絵のために「読み込み中」を出し続けない。
              // **どの絵のloadか要素側で確かめる**: 送った直後は前の絵の
              // 完了が遅れて届くことがあり、そのまま信じると「まだ出ていない
              // 新しい絵」を出たものとして扱ってしまう
              onLoad={(e) => {
                if (isSrcOf(e.currentTarget.currentSrc, viewerItem.id)) {
                  // 届いた絵の実寸を控える（等倍100%の較正と先読みの予算）
                  const { naturalWidth: nw, naturalHeight: nh } =
                    e.currentTarget;
                  if (nw > 0 && nh > 0) {
                    setServedNatural({ id: viewerItem.id, w: nw, h: nh });
                    rememberNatural(viewerItem.id, nw, nh);
                  }
                  // 下敷きを外してよいのは**ここだけ**（onErrorでは外さない）。
                  // `loadedId` はこれから導かれるので、別に立てるものは無い
                  setFullShownId(viewerItem.id);
                  // **失敗の印は必ず消す**。`currentSrc` が空のまま error が
                  // 来ることがあり（下のonError参照）、その後で本命の load が
                  // 成功する。印を残すと「原寸は隠す・下敷きは外す」が同時に
                  // 成立して**何も映らない**（ゲート2のP2）
                  setFullFailedId(null);
                }
              }}
              onError={(e) => {
                // 失敗したのが**いまの絵**のときだけ畳む。ただし `currentSrc` が
                // 空のまま失敗することもあるので、そのときは畳む側に倒す
                // （出ない絵のために「読み込み中」を出し続けないため）
                const src = e.currentTarget.currentSrc;
                if (!src || isSrcOf(src, viewerItem.id)) {
                  setFullFailedId(viewerItem.id);
                }
              }}
              style={{
                transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                cursor: zoom > 1 ? "grab" : "zoom-in",
                // 原寸が出せず下敷きが唯一の絵になったときも、**この <img> は
                // 消さずに絵の枠として残す**。消すと拡大・移動・右クリックが
                // 効かない絵になり、写真をクリックしただけでビューアが閉じる
                // （下敷きは `pointer-events: none` なので、当たり判定はここ）。
                // **透明にするだけで消さない**——寸法を持つ失敗画像には、
                // Chromiumが枠と壊れアイコンを描く（`alt` を空にしても消えない）。
                // `opacity: 0` なら何も描かれず、当たり判定だけが残る
                ...(fallbackToThumb
                  ? { width: servedW, height: servedH, opacity: 0 }
                  : null),
              }}
              onClick={(e) => e.stopPropagation()}
              onContextMenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                setMenu({
                  pos: { x: e.clientX, y: e.clientY },
                  item: viewerItem,
                });
              }}
              onDoubleClick={(e) => {
                e.stopPropagation();
                // ダブルクリックは「画面に合わせる」と「等倍(100%)」の行き来。
                // 中途半端な倍率にせず、今どちらを見ているかが常に分かるようにする
                toggleActualSize();
              }}
              onWheel={(e) => {
                const next = Math.min(
                  8,
                  Math.max(1, zoom * (e.deltaY < 0 ? 1.25 : 0.8)),
                );
                setZoom(next);
                // 自分で倍率を動かしたら、等倍を選んでいた状態は解ける
                setPinnedActualId(null);
                if (next === 1) setPan({ x: 0, y: 0 });
              }}
              onPointerDown={(e) => {
                if (zoom <= 1) return;
                e.preventDefault();
                (e.target as HTMLElement).setPointerCapture(e.pointerId);
                dragRef.current = {
                  startX: e.clientX,
                  startY: e.clientY,
                  panX: pan.x,
                  panY: pan.y,
                };
              }}
              onPointerMove={(e) => {
                const d = dragRef.current;
                if (!d) return;
                setPan({
                  x: d.panX + e.clientX - d.startX,
                  y: d.panY + e.clientY - d.startY,
                });
              }}
              onPointerUp={() => {
                dragRef.current = null;
              }}
            />
          ) : (
            <div className="viewer-loading">{t.loading}</div>
          )}
          {/* 原寸が届くまで、グリッドで既に描いたサムネイルを下に敷く（0.2 ②）。
              クリックした瞬間、その絵の512pxはブラウザにデコード済みで載っている
              ＝**待ち時間ゼロで絵が出る**。原寸は詰め直しに約0.6秒かかる
              （HEIC実測: 24.5MP。大半はOSのWICデコード）ので、その間を埋める。

              詰め直しの要る形式（HEIC・RAW・TIFF）だけに敷く——JPEGは6msで
              出るので、敷いても一瞬ぼやけた絵が見えるだけ損。

              **原寸の <img> より後に置くこと**。ブラウザは `src` を差し替えても
              新しい絵の最初のフレームが出るまで**前の絵を描き続ける**ので、
              送りで冷えた1枚へ行くと約1秒「前の写真」が残る。位置指定つきの
              こちらを後に置くと、その残像の上に新しい絵の下敷きが載る
              （ゲート2のP2）。字幕・送りボタン・読み込み中はさらに後ろにあるので
              こちらより前に描かれる */}
          {viewerItem &&
            viewerTranscoding &&
            viewerItem.has_thumb &&
            viewerItem.width > 0 &&
            viewerItem.height > 0 &&
            fullShownId !== viewerItem.id && (
              <img
                className="viewer-thumb"
                src={thumbSrc(viewerItem)}
                // ふだんは原寸の下に敷くだけの飾り。**原寸が出せなかったときは
                // これが唯一見えている絵**になるので、そのときだけ名前を名乗る
                alt={fullFailedId === viewerItem.id ? viewerItem.file_name : ""}
                aria-hidden={fullFailedId !== viewerItem.id}
                draggable={false}
                onLoad={(e) => {
                  if (
                    isSrcOf(e.currentTarget.currentSrc, viewerItem.id, "thumb")
                  )
                    setThumbShownId(viewerItem.id);
                }}
                // **原寸と同じ場所に同じ大きさで描く**。枠には配られる絵の寸法
                // （`servedSize`）を名乗らせ、`object-fit: contain` で中に収める
                // ——こうすると
                // 原寸の <img>（`max-width`/`max-height` で縮む）と描画結果が
                // ぴったり一致し、差し替わるときに絵が動かない
                style={{
                  width: servedW,
                  height: servedH,
                  transform: `translate(-50%, -50%) translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                }}
              />
            )}
          {/* 先読み（0.2 ①）。`display:none` でも画素は保持される（実測で
              opacity:0・画面外配置と同じ。小細工は要らない）。クリックも
              受けないので、地をクリックして閉じる操作の邪魔にならない */}
          {preload.map((it) => (
            <img
              key={`${it.id}-${it.mtime_ms}`}
              ref={(el) => {
                if (el) preloadElsRef.current.set(it.id, el);
                else preloadElsRef.current.delete(it.id);
              }}
              alt=""
              aria-hidden
              style={{ display: "none" }}
            />
          ))}
          {/* 粗い絵（下敷きのサムネイル）を見ているあいだの細い線（0.2 ②）。
              **原寸が出た瞬間に消える**ので、切り替わりがそのまま見える。
              CSSで140ms待ってから現れるので、先読み済みの絵（6ms）では
              一度も光らない。動画には出さない（Rangeで刻んで届くので、
              「粗い絵を見ている」状態が無い） */}
          {viewerItem && !viewerItem.is_video && loadedId !== viewerItem.id && (
            <div className="viewer-progress" aria-hidden />
          )}
          {/* いま見ている1枚に付いている印を、写真の上に小さく出す（0.2 ③）。
              下の帯にも出ているが、**見ている絵そのもの**に付いていないと
              「どっちだったか」を帯で数え直すことになる。
              ★と⚑を並べたのは2026-08-19——道具の帯は1.8秒で消えるので、
              それまで「いまの1枚がどうなっているか」を出す場所が無かった */}
          {viewerItem &&
            (viewerItem.favorite ||
              viewerItem.picked ||
              rejected.has(viewerItem.id)) && (
              <div className="viewer-badges">
                {viewerItem.favorite && (
                  <span className="viewer-badge fav">★ {t.judgeFav}</span>
                )}
                {viewerItem.picked && (
                  <span className="viewer-badge pick">⚑ {t.judgePick}</span>
                )}
                {rejected.has(viewerItem.id) && (
                  <span className="viewer-badge reject">
                    ✕ {t.viewerRejected}
                  </span>
                )}
              </div>
            )}
          {/* 判定した合図（2026-08-19）。**絵の真ん中で一度だけ膨らんで消える**。
              印そのものは上のバッジと下の帯に残るので、ここは残らなくてよい */}
          {judgeFlash && (
            <div
              key={judgeFlash.seq}
              className={"judge-flash " + judgeFlash.kind}
              aria-live="polite"
            >
              <span className="judge-flash-mark">
                {JUDGE_FLASH[judgeFlash.kind].mark}
              </span>
              <span className="judge-flash-word">
                {JUDGE_FLASH[judgeFlash.kind].word()}
              </span>
            </div>
          )}
          {/* 絵が何も見えていないときだけ「読み込み中」を出す。下敷きの
              サムネイルが出ているなら、待たせている合図はもう要らない（0.2 ②） */}
          {viewerItem &&
            slowId === viewerItem.id &&
            loadedId !== viewerItem.id &&
            thumbShownId !== viewerItem.id && (
              <div className="viewer-loading viewer-loading-overlay">
                {t.loading}
              </div>
            )}
          {/* 前後に何があるか（0.2 ②）。送りが速いので、次に何が来るかが
              見えていると選別が進む。クリックでそこへ飛ぶ。

              **動画には出さない**（2026-08-22）。動画の操作バーは絵の下端に
              重なって出るので、その上に帯を敷くと音量つまみと再生位置が
              隠れて押せない。帯の分だけ絵を持ち上げる手もあるが、縦長の動画が
              目に見えて縮んで窮屈になる——動画は「送って選ぶ」より「見る」ものなので、
              見ているあいだは帯を畳むほうを採った。何枚目かは字幕に出ているし、
              送りは ‹ › とキーでできる */}
          {viewerItem && !viewerItem.is_video && strip.length > 1 && (
            <div className="viewer-strip" onClick={(e) => e.stopPropagation()}>
              {strip.map(({ item, offset }) => (
                <button
                  key={item.id}
                  className={
                    "strip-cell" +
                    (offset === 0 ? " current" : "") +
                    (rejected.has(item.id) ? " rejected" : "")
                  }
                  title={item.file_name}
                  onClick={() =>
                    setViewer({ dayKey: item.day_key, id: item.id })
                  }
                >
                  {item.has_thumb ? (
                    <img src={thumbSrc(item)} alt="" draggable={false} />
                  ) : (
                    <span className="strip-blank" />
                  )}
                  {item.picked && <span className="strip-flag">⚑</span>}
                  {item.favorite && <span className="strip-fav">★</span>}
                  {/* ✕の足あと（0.2 ③）。ここに残るので、連打で巻き添えに
                      したときも数枚のうちに気付ける */}
                  {rejected.has(item.id) && (
                    <span className="strip-reject">✕</span>
                  )}
                </button>
              ))}
            </div>
          )}
          {viewerItem && (
            <div
              className="viewer-caption"
              onClick={(e) => e.stopPropagation()}
            >
              {viewerItem.file_name}
              <span className="viewer-date">
                {formatDateTime(viewerItem.taken_at_ms)}
              </span>
              <span className="viewer-pos">
                {viewerPos} / {viewerTotal}
              </span>
              {/* ボツの候補の数（0.2 ③）。押すといつでも関所で顔を見られる。
                  0件のときは出さない——選別中に面を1つも増やさないため */}
              {rejected.size > 0 && (
                <button
                  className="viewer-reject-chip"
                  title={t.rejectChipTitle}
                  onClick={() => setRejectGate({ closeAfter: false })}
                >
                  {t.rejectChip(rejected.size)}
                </button>
              )}
            </div>
          )}
          {showExif && viewerItem && (
            <div className="exif-panel" onClick={(e) => e.stopPropagation()}>
              <div className="exif-title">{t.exifTitle}</div>
              {exif === null ? (
                <div className="exif-row">{t.loading}</div>
              ) : (
                <>
                  {(
                    [
                      [t.exifCamera, exif.camera],
                      [t.exifLens, exif.lens],
                      [t.exifAperture, exif.aperture],
                      [t.exifShutter, exif.shutter],
                      [t.exifIso, exif.iso],
                      [t.exifFocal, exif.focal],
                    ] as const
                  ).map(([label, value]) =>
                    value ? (
                      <div className="exif-row" key={label}>
                        <span className="exif-key">{label}</span>
                        <span className="exif-value">{value}</span>
                      </div>
                    ) : null,
                  )}
                  {exif.gps && (
                    <div className="exif-row">
                      <span className="exif-key">{t.exifLocation}</span>
                      <a
                        className="exif-value link"
                        href={`https://www.openstreetmap.org/?mlat=${exif.gps[0]}&mlon=${exif.gps[1]}#map=15/${exif.gps[0]}/${exif.gps[1]}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        📍 {exif.gps[0].toFixed(4)}, {exif.gps[1].toFixed(4)}
                      </a>
                    </div>
                  )}
                  {!exif.camera &&
                    !exif.lens &&
                    !exif.aperture &&
                    !exif.iso &&
                    !exif.gps && (
                      <div className="exif-row">{t.exifNone}</div>
                    )}
                </>
              )}
            </div>
          )}
          <div className="viewer-tools" onClick={(e) => e.stopPropagation()}>
            {viewerItem && (
              <button
                className={"viewer-zoom" + (isActualSize ? " actual" : "")}
                title={
                  isActualSize
                    ? t.viewerFitToScreen
                    : t.viewerActualSize
                }
                onClick={toggleActualSize}
              >
                {Math.round(displayScale * 100)}%
                {isActualSize ? ` ${t.actualSizeBadge}` : ""}
              </button>
            )}
            {viewerItem && (
              <button
                className={"viewer-tool" + (viewerItem.favorite ? " on" : "")}
                title={t.viewerFavorite}
                onClick={() => favoriteViewer(viewerItem)}
              >
                {viewerItem.favorite ? "★" : "☆"}
              </button>
            )}
            {viewerItem && (
              <button
                className={"viewer-tool" + (viewerItem.picked ? " picked" : "")}
                title={viewerItem.picked ? t.viewerUnpick : t.viewerPick}
                onClick={() => pickViewer(viewerItem, !viewerItem.picked)}
              >
                ⚑
              </button>
            )}
            {/* マウスだけで選別する人の✕（ゲート2のP3）。隣の🗑と違って
                **この場では何も消えない**——閉じるときに関所へ集まる */}
            {viewerItem && (
              <button
                className={
                  "viewer-tool reject-tool" +
                  (rejected.has(viewerItem.id) ? " rejecting" : "")
                }
                title={t.viewerReject}
                onClick={() => rejectTool(viewerItem)}
              >
                ✕
              </button>
            )}
            <button
              className={"viewer-tool" + (showExif ? " on" : "")}
              title={t.viewerExif}
              onClick={() => setShowExif((s) => !s)}
            >
              ℹ
            </button>
            <button
              className={"viewer-tool" + (playing ? " on" : "")}
              title={t.viewerSlideshow}
              onClick={() => setPlaying((p) => !p)}
            >
              {playing ? "⏸" : "▶"}
            </button>
            {viewerItem && (
              <button
                className="viewer-tool"
                title={t.menuReveal}
                onClick={() => revealInFolder(viewerItem.id).catch(() => {})}
              >
                📁
              </button>
            )}
            {viewerItem && (
              <button
                className="viewer-tool danger"
                title={t.menuDelete}
                onClick={() => onDelete(viewerItem)}
              >
                🗑
              </button>
            )}
            <button
              className="viewer-tool"
              title={t.shortcutsTitle}
              onClick={() => setShortcutsOpen(true)}
            >
              ?
            </button>
            <button
              className={"viewer-tool" + (isFullscreen ? " on" : "")}
              title={t.viewerFullscreen}
              onClick={toggleFullscreen}
            >
              {isFullscreen ? "⤡" : "⛶"}
            </button>
            <button
              className="viewer-tool"
              title={t.viewerClose}
              onClick={requestCloseViewer}
            >
              ✕
            </button>
          </div>
          {hasPrev && (
            <button
              className="viewer-nav prev"
              title={t.viewerPrev}
              onClick={(e) => {
                e.stopPropagation();
                moveViewer(-1);
              }}
            >
              ‹
            </button>
          )}
          {hasNext && (
            <button
              className="viewer-nav next"
              title={t.viewerNext}
              onClick={(e) => {
                e.stopPropagation();
                moveViewer(1);
              }}
            >
              ›
            </button>
          )}
        </div>
      )}
      <ImportWizard
        open={wizardOpen}
        onClose={() => setWizardOpen(false)}
        drives={drives}
        config={config}
        startPath={wizardStart}
        startNonce={wizardNonce}
        onImported={onImported}
        onError={onWizardError}
        onConfigChanged={refreshRoots}
      />
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        config={config}
        onConfigChanged={refreshRoots}
        onError={onWizardError}
      />
      <ContextMenu
        pos={menu?.pos ?? null}
        items={menu ? menuItemsFor(menu.item) : []}
        onClose={() => setMenu(null)}
      />
      {/* ショートカット一覧（`?` / `F1`）。キーを覚えていなくても、
          いま押せるものがその場で分かるように出す */}
      {/* 関所（0.2 ③）。**顔を見てから確定する**——文字だけの
          「200枚をゴミ箱へ移動しますか？」では、何が消えるのか押す瞬間に
          確かめられない。ここは自前のDOMなので、`window.confirm` が無言で
          trueを返す罠（2026-08-16）には当たらない */}
      {rejectGate && (
        <div
          className="reject-gate-backdrop"
          onClick={() => {
            if (!trashing) setRejectGate(null);
          }}
        >
          <div
            className="reject-gate"
            role="dialog"
            aria-label={t.rejectGateTitle(rejected.size)}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="reject-gate-head">
              <h2>{t.rejectGateTitle(rejected.size)}</h2>
              {/* 逃げ道が見えていることが、不安感を消す要 */}
              <p>{t.rejectGateNote}</p>
            </div>
            <div className="reject-gate-grid">
              {[...rejected.values()].map((item) => (
                <div className="reject-cell" key={item.id}>
                  {item.has_thumb ? (
                    <img
                      src={thumbSrc(item)}
                      alt={item.file_name}
                      title={item.file_name}
                      // **飾りではなく崖対策**（0.2 ①の予算）。96pxで出しても
                      // デコードは512pxの実寸で走るので、200枚を一度に開くと
                      // 約140MiB。見えているぶんだけに絞る
                      loading="lazy"
                      draggable={false}
                    />
                  ) : (
                    <span className="strip-blank" />
                  )}
                  <button
                    className="reject-restore"
                    onClick={() => markReject(item, false)}
                    disabled={trashing}
                  >
                    {t.rejectGateRestore}
                  </button>
                </div>
              ))}
            </div>
            <div className="reject-gate-actions">
              {/* 初期フォーカスは**安全側**。Enter連打は「戻る」に落ちる */}
              <button autoFocus onClick={() => setRejectGate(null)} disabled={trashing}>
                {t.rejectGateBack}
              </button>
              {rejectGate.closeAfter && (
                <button
                  className="quiet"
                  onClick={() => {
                    setRejected(new Map());
                    setRejectGate(null);
                    finishGate(rejectGate);
                  }}
                  disabled={trashing}
                >
                  {t.rejectGateDiscard}
                </button>
              )}
              <button
                className="danger"
                onClick={() => void confirmTrash()}
                disabled={trashing}
              >
                {trashing
                  ? t.rejectGateTrashing(
                      trashProgress?.done ?? 0,
                      trashProgress?.total ?? rejected.size,
                    )
                  : t.rejectGateConfirm(rejected.size)}
              </button>
            </div>
          </div>
        </div>
      )}
      {shortcutsOpen && (
        <div
          className="shortcuts-backdrop"
          onClick={() => setShortcutsOpen(false)}
        >
          <div
            className="shortcuts"
            role="dialog"
            aria-label={t.shortcutsTitle}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="shortcuts-head">
              <h2>{t.shortcutsTitle}</h2>
              <button
                className="shortcuts-close"
                title={t.close}
                onClick={() => setShortcutsOpen(false)}
              >
                ✕
              </button>
            </div>
            <div className="shortcuts-cols">
              {t.shortcutGroups.map((group) => (
                <section key={group.title}>
                  <h3>{group.title}</h3>
                  <dl>
                    {group.keys.map(([key, what]) => (
                      <div key={key + what}>
                        <dt>
                          {key.split(" / ").map((k, i) => (
                            <span key={k}>
                              {i > 0 && <span className="shortcut-or"> / </span>}
                              <kbd>{k}</kbd>
                            </span>
                          ))}
                        </dt>
                        <dd>{what}</dd>
                      </div>
                    ))}
                  </dl>
                </section>
              ))}
            </div>
          </div>
        </div>
      )}
      <Palette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        summary={summary}
        cameras={cameras}
        onJumpDay={openDay}
        onSearch={runSearch}
        actions={paletteActions}
      />
    </div>
  );
}
