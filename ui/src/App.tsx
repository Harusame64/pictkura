import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Calendar from "./Calendar";
import Palette, { type PaletteItem } from "./Palette";
import ContextMenu, { type MenuItem, type MenuPos } from "./ContextMenu";
import SettingsDialog from "./Settings";
import ImportWizard from "./ImportWizard";
import {
  addLibraryRoot,
  deleteMedia,
  fullSrc,
  isWindows,
  videoSrc,
  videoStatus,
  getConfig,
  getExifInfo,
  getIndexProgress,
  getDecoderStatus,
  getStartupReport,
  getStats,
  listCameras,
  listDay,
  openDecoderHelp,
  listDrives,
  listMemories,
  modKeyLabel,
  openDefault,
  openWith,
  removeLibraryRoot,
  revealInFolder,
  setFavorite,
  setVisiblePriority,
  syncNow,
  thumbSrc,
  timelineSummary,
  type Camera,
  type DaySummary,
  type DriveInfo,
  type ExifInfo,
  type AppConfig,
  type ExternalApp,
  type ImportStats,
  type IndexProgress,
  type LibraryStats,
  type MediaItem,
  type Memory,
  type StartupScanReport,
} from "./api";
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
  /** 取得済みの日 → その日のレコード（可視範囲＋α だけを保持） */
  const [dayItems, setDayItems] = useState<Map<number, MediaItem[]>>(
    () => new Map(),
  );
  const [stats, setStats] = useState<LibraryStats>({ total: 0, favorites: 0 });
  const [memories, setMemories] = useState<Memory[]>([]);
  const [cellSize, setCellSize] = useState(180);
  const [status, setStatus] = useState("");
  const [folderInput, setFolderInput] = useState("");
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [roots, setRoots] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [view, setView] = useState<"grid" | "calendar">("grid");
  const [filter, setFilter] = useState<"all" | "fav">("all");
  /** 検索ボックスの入力（打鍵ごとの値） */
  const [queryInput, setQueryInput] = useState("");
  /** 実際にバックエンドへ投げている検索クエリ（入力のデバウンス後） */
  const [query, setQuery] = useState("");
  /** カメラ別の枚数（左ペインとコマンドパレットの候補） */
  const [cameras, setCameras] = useState<Camera[]>([]);
  /** カメラ一覧を全部見せるか（既定は上位数台だけ） */
  const [camerasExpanded, setCamerasExpanded] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
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
  const [config, setConfig] = useState<AppConfig | null>(null);
  /** ビューアの撮影情報パネル（iキーで開閉） */
  const [showExif, setShowExif] = useState(false);
  const [exif, setExif] = useState<ExifInfo | null>(null);
  /** カレンダーの日クリックでグリッドのこの日付見出しへスクロールする */
  const [pendingScrollDay, setPendingScrollDay] = useState<number | null>(null);
  /** 詳細ビューアの位置（nullで非表示） */
  const [viewer, setViewer] = useState<ViewerPos | null>(null);
  /** ビューアのズーム・パン・スライドショー。zoomは「画面に収めた状態」を1とする倍率 */
  const [zoom, setZoom] = useState(1);
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
  const filterRef = useRef<"all" | "fav">("all");
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
   * 可視範囲の日は placeholder になった瞬間に自動で再取得される */
  const reloadAll = useCallback(async () => {
    const gen = ++generationRef.current;
    inflightRef.current.clear();
    const fav = filterRef.current === "fav";
    const [sum, st, mem] = await Promise.all([
      timelineSummary(queryRef.current, fav),
      getStats(),
      listMemories(),
    ]);
    // 応答待ちの間に次のリロード（フィルタ切替等）が始まっていたら、古い応答は捨てる
    if (generationRef.current !== gen) return;
    setSummary(sum);
    setStats(st);
    setMemories(mem);
    setDayItems(new Map());
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
    const fav = filterRef.current === "fav";
    const [sum, st] = await Promise.all([
      timelineSummary(queryRef.current, fav),
      getStats(),
    ]);
    if (generationRef.current !== gen) return; // リロードが割り込んだら捨てる
    setSummary(sum);
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
    listDay(dayKey, queryRef.current, filterRef.current === "fav")
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
      const f = await listen("library-updated", () => {
        reloadAll().catch((e) => setStatus(String(e)));
        refreshCameras();
      });
      if (cancelled) {
        f();
        return;
      }
      unlisten = f;
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

  // この環境で開けない形式の案内（第7部 段階G-6）。
  //
  // AVIFはデコーダを同梱しているので常に出せるが、HEIC（HEVC）は特許の都合で
  // 同梱していないためOSのデコーダが要る。無い環境ではサムネイルが付かないので、
  // **黙って空欄にせず理由を伝える**。ライブラリにHEICが1枚も無ければ出さない。
  const [heifMissing, setHeifMissing] = useState<number | null>(null);
  const [decoderHelp, setDecoderHelp] = useState(false);
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
    queryRef.current = query;
    if (filterInitRef.current) {
      filterInitRef.current = false;
      return;
    }
    reloadAll().catch((e) => setStatus(String(e)));
  }, [filter, query, reloadAll]);

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
    (async () => {
      const f = await listen<IndexProgress>("index-progress", (ev) => {
        // 中断した場合は「終わった」と誤解させないよう表示を残す
        const p = ev.payload;
        if (!cancelled) setIndexProgress(p.building || p.incomplete ? p : null);
      });
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
    };
  }, [refreshCameras]);
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
      const fav = filterRef.current === "fav";
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
          // ★のみ表示中、★でない項目は表示対象外。
          // 読み込み済みの日に古い姿が残っていれば外すだけで、日の再取得はしない
          // （サムネイル完成イベントは★以外にも届くため、ここで日を捨てると
          //  可視日が生成のたびにシマー→再取得を繰り返してしまう）
          const shown = !fav || p.favorite;
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
                d.has_dcim === list[i].has_dcim,
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

  // ウィザードを開く。startPath指定時（ドライブクリック）はそのフォルダから始める
  const openWizard = useCallback((startPath?: string) => {
    setWizardStart(startPath);
    setWizardOpen(true);
  }, []);

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
      setFolderInput("");
      await reloadAll();
      await refreshRoots();
      checkDecoders();
    } catch (e) {
      setStatus(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onAddFolder = () => {
    const path = folderInput.trim();
    if (path) void addFolder(path);
  };

  // 「参照…」。取り込みウィザードや設定と同じネイティブのフォルダ選択を使う。
  // ここだけ手入力のままだったのを揃える（選んだら即追加する）
  const onBrowseFolder = async () => {
    const picked = await open({ directory: true, title: t.pickLibraryFolder });
    if (typeof picked === "string") await addFolder(picked);
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

  // お気に入りトグル（楽観的更新: 先にUIへ反映してからバックエンドへ）
  const toggleFavorite = useCallback(
    async (item: MediaItem) => {
      const next = !item.favorite;
      const patch = (fav: boolean) => {
        setDayItems((prev) => {
          const arr = prev.get(item.day_key);
          if (!arr) return prev;
          const out = new Map(prev);
          out.set(
            item.day_key,
            arr.map((it) => (it.id === item.id ? { ...it, favorite: fav } : it)),
          );
          return out;
        });
        setStats((s) => ({
          ...s,
          favorites: s.favorites + (fav ? 1 : -1),
        }));
      };
      patch(next);
      try {
        await setFavorite(item.id, next);
        // ★のみ表示中は骨組み（枚数・日の有無）が変わる
        if (filterRef.current === "fav") await reloadAll();
      } catch {
        patch(!next); // 失敗したら戻す
      }
    },
    [reloadAll],
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

  // 位置が解決不能になったらビューアを閉じる（★解除で対象外になった、
  // 撮影日確定で別の日へ移った、サマリ更新でその日が消えた等）。
  // 「読み込み中…」のまま操作不能で固まるのを防ぐ
  useEffect(() => {
    if (!viewer) return;
    const items = dayItems.get(viewer.dayKey);
    if (!items) return; // 未取得: ロード完了待ち（スタックではない）
    const resolvable =
      viewer.id === "first" || viewer.id === "last"
        ? items.length > 0
        : items.some((it) => it.id === viewer.id);
    if (!resolvable || !dayIdxByKey.has(viewer.dayKey)) setViewer(null);
  }, [viewer, dayItems, dayIdxByKey]);

  /** ビューアを1枚進める(+1)/戻す(-1)。wrapは末尾→先頭のループ（スライドショー用） */
  const moveViewer = useCallback(
    (dir: 1 | -1, wrap = false) => {
      if (!viewerInfo) return;
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
    [viewerInfo, dayItems, summary],
  );

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
    if (!viewerItem || viewerItem.width <= 0 || viewerItem.height <= 0) return 1;
    const maxW = windowSize.w - 120;
    const maxH = windowSize.h - 80;
    return Math.min(1, maxW / viewerItem.width, maxH / viewerItem.height);
  }, [viewerItem, windowSize]);
  /** 実ピクセルに対する表示倍率（100%＝等倍） */
  const displayScale = fitScale * zoom;
  const isActualSize = Math.abs(displayScale - 1) < 0.005;

  /** 等倍（100%）と画面に合わせる表示を切り替える */
  const toggleActualSize = useCallback(() => {
    setPan({ x: 0, y: 0 });
    setZoom((z) => (Math.abs(fitScale * z - 1) < 0.005 ? 1 : 1 / fitScale));
  }, [fitScale]);

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
    if (viewer === null) setPlaying(false);
  }, [viewer]);

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
    const onKey = (e: KeyboardEvent) => {
      // 文字入力中はビューアの1文字ショートカットを効かせない。
      // パレットはビューアの上にも開けるので、"family" と打つと f で★が
      // トグルされ i で撮影情報が開き、スペースでスライドショーが始まってしまう
      const target = e.target as HTMLElement | null;
      const typing =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.isContentEditable === true;
      if (paletteOpen || typing) return;
      if (e.key === "Escape") setViewer(null);
      else if (e.key === "ArrowLeft") moveViewer(-1);
      else if (e.key === "ArrowRight") moveViewer(1);
      else if (e.key === "f" || e.key === "F") {
        if (viewerItem) toggleFavorite(viewerItem);
      } else if (e.key === "i" || e.key === "I") {
        setShowExif((s) => !s);
      } else if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen();
      } else if (e.key === "1") {
        toggleActualSize();
      } else if (e.key === "0") {
        setZoom(1);
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
    toggleFavorite,
    paletteOpen,
    toggleFullscreen,
    toggleActualSize,
  ]);

  // 撮影情報は**パネルを開いている間だけ**、表示中の1枚について実ファイルから読む。
  // DBに列を持たないので常に実物と一致し、1000万件でもDBは1バイトも増えない
  const viewerItemId = viewerItem?.id;
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

  // コマンドパレット（Ctrl+K / ⌘K）。ビューア表示中でも開ける
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        setPaletteOpen((p) => !p);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /** 1枚をゴミ箱へ移動する（確認あり）。写真は取り返しがつかないのでOSのゴミ箱経由 */
  const onDelete = useCallback(
    async (item: MediaItem) => {
      if (!window.confirm(t.deleteConfirm(1))) return;
      try {
        const n = await deleteMedia([item.id]);
        setStatus(t.deleted(n));
        // 削除された日だけを捨てて取り直す（骨組みの件数も変わる）
        setDayItems((prev) => {
          if (!prev.has(item.day_key)) return prev;
          const next = new Map(prev);
          next.delete(item.day_key);
          return next;
        });
        setViewer((v) => (v && v.id === item.id ? null : v));
        await refreshSummary();
      } catch (e) {
        setStatus(String(e));
      }
    },
    [refreshSummary],
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

  const hasPrev =
    viewerInfo !== null && !(viewerInfo.dayIdx === 0 && viewerInfo.itemIdx === 0);
  const hasNext =
    viewerInfo !== null &&
    !(
      viewerInfo.dayIdx === summary.length - 1 &&
      viewerInfo.itemIdx === viewerInfo.dayLength - 1
    );
  const viewerPos =
    viewerInfo !== null
      ? prefixCounts[viewerInfo.dayIdx] + viewerInfo.itemIdx + 1
      : 0;

  return (
    <div className="app">
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
          <span>{t.decoderHeifNotice(heifMissing)}</span>
          {decoderHelp && (
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
                !busy && openWizard(d.has_dcim ? d.path + "DCIM" : d.path)
              }
            >
              <span className="drive-icon">{d.removable ? "🔌" : "💾"}</span>
              {d.label}
              {d.has_dcim && <span className="dcim-badge">DCIM</span>}
            </div>
          ))}
          <div className="nav-section">{t.navAddFolder}</div>
          <div className="add-folder">
            <button
              className="browse-folder"
              onClick={onBrowseFolder}
              disabled={busy}
            >
              📂 {t.browse}
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
          {view === "grid" && filter === "all" && !query && memories.length > 0 && (
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
          {view === "calendar" ? (
            <div className="calendar-scroll">
              <Calendar summary={summary} onOpenDay={openDay} />
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
                          {row.label}
                          <span className="date-count">
                            {t.photosCount(row.count)}
                          </span>
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
                              className="cell-wrap"
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
                                onClick={() =>
                                  setViewer({
                                    dayKey: row.dayKey,
                                    id: cell.item.id,
                                  })
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
          onClick={() => setViewer(null)}
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
                {videoInfo.exists &&
                  !videoInfo.cloud_only &&
                  videoInfo.plays_in_app && (
                    <p className="fallback-note">{t.videoCodecNote}</p>
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
                    isWindows && (
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
              src={fullSrc(viewerItem.id, viewerItem.mtime_ms)}
              alt={viewerItem.file_name}
              draggable={false}
              style={{
                transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                cursor: zoom > 1 ? "grab" : "zoom-in",
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
                {viewerPos} / {totalShown}
              </span>
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
                onClick={() => toggleFavorite(viewerItem)}
              >
                {viewerItem.favorite ? "★" : "☆"}
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
              className={"viewer-tool" + (isFullscreen ? " on" : "")}
              title={t.viewerFullscreen}
              onClick={toggleFullscreen}
            >
              {isFullscreen ? "⤡" : "⛶"}
            </button>
            <button
              className="viewer-tool"
              title={t.viewerClose}
              onClick={() => setViewer(null)}
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
