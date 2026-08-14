import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  importFromFolder,
  importPaths,
  listFolderPatterns,
  listSourceDir,
  listSourceTree,
  probeImported,
  setImportDestination,
  sourceThumbSrc,
  type AppConfig,
  type DriveInfo,
  type ImportProgress,
  type ImportStats,
  type SourceFile,
  type SourceListing,
  type SourceTree,
} from "./api";
import { t } from "./i18n";

/**
 * 取り込みウィザード（第5部 段階E）。PlayMemories Home のように
 * 「挿したメディアの中を**見てから**取り込む」ための画面。
 *
 * 速度と分かりやすさのための約束ごと:
 * - ツリーは開いた1階層だけを読む（USBを再帰で舐めない）
 * - ただし**フォルダを選んだときは既定で下の階層まで走査する**。
 *   「カードのどこに写真が入っているか分からない」人がツリーを辿らずに済む。
 *   例外は固定ドライブとネットワークドライブの直下（`C:\` や `Z:\` 全体を舐めない）
 * - サムネイルはEXIF埋め込みを使う `media://src/...` 経由（Base64禁止の原則どおり）
 * - 実体がクラウドにしか無いファイル（OneDrive等）はサムネイルを**要求しない**。
 *   一覧を出すためにユーザーの回線とクラウド容量を使わない
 * - 「取り込み済み」判定は一覧表示の**後追い**で塗る。判定を待って一覧が出ないより、
 *   写真がすぐ出て後からバッジが付く方が体感が速い
 */

/** 一度に描くタイル数。数千枚のフォルダでDOMを作り切って固まらせない */
const PAGE_SIZE = 300;
/** 「取り込み済み」判定をまとめて投げる単位（USBだと1件数ms） */
const PROBE_CHUNK = 100;

type TreeNode = {
  name: string;
  path: string;
  hasSubdirs: boolean;
  imageCount: number;
  countCapped: boolean;
  /** このノードを選んだとき、下の階層まで自動で走査してよいか */
  deepDefault: boolean;
  /** ドライブ行（種類でアイコンを変える） */
  drive?: DriveInfo;
};

/** ドライブ種別のアイコン。ネットワークと光学は「取り込み元らしくない」ことを示す */
const driveIcon = (drive: DriveInfo) =>
  drive.kind === "network"
    ? "🌐"
    : drive.kind === "optical"
      ? "💿"
      : drive.removable
        ? "🔌"
        : "💾";

/** パスの末尾（ファイル名）だけを取り出す */
const baseName = (path: string) =>
  path.split("\\").pop()?.split("/").pop() ?? path;

export default function ImportWizard({
  open: isOpen,
  onClose,
  drives,
  config,
  startPath,
  onImported,
  onError,
  onConfigChanged,
}: {
  open: boolean;
  onClose: () => void;
  drives: DriveInfo[];
  config: AppConfig | null;
  /** ドライブをクリックして開いたときの初期選択フォルダ */
  startPath?: string;
  onImported: (stats: ImportStats) => void;
  onError: (message: string) => void;
  onConfigChanged: () => void;
}) {
  const [listings, setListings] = useState<Record<string, SourceListing>>({});
  const [trees, setTrees] = useState<Record<string, SourceTree>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [extraRoots, setExtraRoots] = useState<string[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [deep, setDeep] = useState(true);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [imported, setImported] = useState<Record<string, boolean>>({});
  const [shown, setShown] = useState(PAGE_SIZE);
  /** 取り込み済みをグリッドから隠す（既定ON: 見たいのは「まだ入っていない写真」） */
  const [hideImported, setHideImported] = useState(true);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [patternExample, setPatternExample] = useState<string>("");

  /** 「未取り込みを自動で選ぶ」を続けてよいか（手動で触ったらやめる） */
  const autoSelect = useRef(true);
  /** フォルダを切り替えたら古い判定の到着を捨てるための世代番号 */
  const probeGen = useRef(0);
  /** 判定の到着時に「今隠しているか」を見るためのref */
  const hideImportedRef = useRef(hideImported);
  hideImportedRef.current = hideImported;
  /** 親から渡るコールバックはrefで持つ。依存に入れるとeffectが再実行される */
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  /** 取り込みを始めた時刻。残り時間の見積もりに使う */
  const importStartedAt = useRef(0);
  /** 開き直しの再読込で参照する現在地（effectの依存に入れずに読む） */
  const viewRef = useRef<{ path: string | null; deep: boolean }>({
    path: null,
    deep: true,
  });
  viewRef.current = { path: current, deep };

  const destination = config?.routing.destination ?? null;
  const tree = current && deep ? trees[current] : undefined;
  const listing = current ? listings[current] : undefined;
  const allFiles: SourceFile[] = (deep ? tree?.files : listing?.files) ?? [];
  const unreadable = !deep && listing?.unreadable === true;
  // グリッドと一括選択が扱うのは「見えているもの」だけにそろえる。
  // 隠れている取り込み済みが「すべて選択」で紛れ込むと事故になる
  const files = hideImported
    ? allFiles.filter((f) => !imported[f.path])
    : allFiles;
  const hiddenCount = allFiles.length - files.length;

  /** ツリー展開用に1階層だけ読む（読めたらキャッシュする） */
  const loadDir = useCallback(async (path: string) => {
    try {
      const result = await listSourceDir(path);
      setListings((prev) => ({ ...prev, [path]: result }));
      return result;
    } catch (e) {
      // 握りつぶすと「0件」と見分けが付かない。断られた理由（写真.appの
      // ライブラリの中など）は出す。ツリーの展開は続けたいので投げ直さない
      onErrorRef.current(String(e));
      return undefined;
    }
  }, []);

  /** グリッドに出す中身を読む。deepなら下の階層まで走査する */
  const loadView = useCallback(
    async (path: string, wantDeep: boolean) => {
      setLoading(true);
      try {
        if (wantDeep) {
          const result = await listSourceTree(path);
          setTrees((prev) => ({ ...prev, [path]: result }));
        }
        // ツリーの枝は常に要る（下まで走査していてもフォルダは辿れるようにする）
        await loadDir(path);
      } catch (e) {
        onErrorRef.current(String(e));
      } finally {
        setLoading(false);
      }
    },
    [loadDir],
  );

  // 取り込み済み判定は一覧が出てから少しずつ。
  // 判定できた分から「未取り込みだけ」を選択に反映していく。
  // コピー先が変われば判定はやり直しになる（destinationも依存に入れる）
  useEffect(() => {
    if (!current || allFiles.length === 0) return;
    probeGen.current += 1;
    const gen = probeGen.current;
    let cancelled = false;
    (async () => {
      for (let i = 0; i < allFiles.length; i += PROBE_CHUNK) {
        if (cancelled || gen !== probeGen.current) return;
        const chunk = allFiles.slice(i, i + PROBE_CHUNK);
        try {
          const results = await probeImported(chunk.map((f) => f.path));
          if (cancelled || gen !== probeGen.current) return;
          setImported((prev) => {
            const next = { ...prev };
            chunk.forEach((f, j) => (next[f.path] = results[j] ?? false));
            return next;
          });
          setSelected((prev) => {
            const next = new Set(prev);
            chunk.forEach((f, j) => {
              if (results[j]) {
                // 取り込み済みと分かったものは選択から外す。
                // 隠している（＝画面から消える）ものが選択に残ると、
                // ボタンの枚数だけ増えて外す手段が無くなる
                if (autoSelect.current || hideImportedRef.current) next.delete(f.path);
              } else if (autoSelect.current) {
                next.add(f.path);
              }
            });
            return next;
          });
        } catch {
          return; // 判定できないときはバッジ無しのまま（取り込み自体は可能）
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [current, allFiles, destination]);

  // コピー先が変わったら古い「済」は別の場所を見た判定なので捨てる
  useEffect(() => {
    setImported({});
  }, [destination]);

  // 取り込み中の進捗（ウィザードを開いている間はここで出す）
  useEffect(() => {
    const unlisten = listen<ImportProgress>("import-progress", (ev) => {
      setProgress(ev.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  // 現在の振り分けパターンの実例（設定ダイアログと同じ文言を使う）
  useEffect(() => {
    if (!isOpen) return;
    listFolderPatterns()
      .then((patterns) => {
        const hit = patterns.find(
          (p) => p.pattern === (config?.routing.folder_pattern ?? ""),
        );
        setPatternExample(hit?.example ?? "");
      })
      .catch(() => {});
  }, [isOpen, config?.routing.folder_pattern]);

  const openFolder = useCallback(
    async (path: string, wantDeep: boolean) => {
      autoSelect.current = true;
      probeGen.current += 1; // 前のフォルダの判定結果を捨てる
      setCurrent(path);
      setDeep(wantDeep);
      setSelected(new Set());
      setShown(PAGE_SIZE);
      setExpanded((prev) => new Set(prev).add(path));
      await loadView(path, wantDeep);
    },
    [loadView],
  );

  // 開いたときの初期表示: メディアから来たならその中を丸ごと見せる
  useEffect(() => {
    if (!isOpen || !startPath) return;
    openFolder(startPath, true);
  }, [isOpen, startPath, openFolder]);

  // 開き直したら「済」判定と中身を読み直す（前回の取り込みで状況が変わっている）
  useEffect(() => {
    if (!isOpen) return;
    setImported({});
    setSelected(new Set());
    autoSelect.current = true;
    const view = viewRef.current;
    if (view.path) loadView(view.path, view.deep);
  }, [isOpen, loadView]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    if (isOpen) window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, busy, onClose]);

  if (!isOpen) return null;

  const toggle = (path: string) => {
    autoSelect.current = false; // 以後は自動選択で上書きしない
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const selectAll = () => {
    autoSelect.current = false;
    setSelected(new Set(files.map((f) => f.path)));
  };
  const selectNew = () => {
    // 判定が途中でも、以後届く分は自動で足し引きされる（autoSelectを戻す）
    autoSelect.current = true;
    setSelected(new Set(files.filter((f) => !imported[f.path]).map((f) => f.path)));
  };
  const clearSelection = () => {
    autoSelect.current = false;
    setSelected(new Set());
  };

  /** コピー先が未設定なら選ばせる。設定できたらtrue */
  const ensureDestination = async () => {
    if (destination) return true;
    const dest = await open({ directory: true, title: t.pickDestination });
    if (!dest) return false;
    await setImportDestination(dest);
    onConfigChanged();
    return true;
  };

  const runImport = async (whole: boolean) => {
    if (!current || busy) return;
    setBusy(true);
    setProgress(null);
    try {
      // コピー先のダイアログを開いている時間は「コピー時間」ではない。
      // 起点をここに置かないと残り時間の見積もりが桁違いに膨らむ
      if (!(await ensureDestination())) return;
      importStartedAt.current = Date.now();
      const stats = whole
        ? await importFromFolder(current)
        : await importPaths([...selected], current);
      // 結果はグリッド側（ステータス行）で伝える。ウィザードは役目を終えて閉じる
      onImported(stats);
      onClose();
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  /** 実測ペースから残り時間を出す。最初の数件はまだ出さない（値が暴れるため） */
  const etaLabel = (p: ImportProgress) => {
    const elapsed = Date.now() - importStartedAt.current;
    if (p.done < 3 || elapsed < 500) return t.wizardEtaCalculating;
    const remainMs = (elapsed / p.done) * (p.total - p.done);
    const seconds = Math.ceil(remainMs / 1000);
    return seconds >= 90
      ? t.wizardEtaMinutes(Math.ceil(seconds / 60))
      : t.wizardEtaSeconds(seconds);
  };

  const addFolderRoot = async () => {
    const picked = await open({ directory: true, title: t.pickSource });
    if (!picked) return;
    // **読めることを確かめてから**ツリーに足す。写真.appのライブラリのように
    // 断られるフォルダを先に足すと、消す手段が無いまま残り、
    // 展開のたびに同じエラーを出し続ける。
    // この確認ぶんだけ列挙が1回増えるが、**押した本人が待っている操作**の
    // 1回だけで、キャッシュを持ち込んで取り込み元の変化を見落とすより安い
    setLoading(true);
    const listing = await loadDir(picked);
    setLoading(false);
    if (!listing) return;
    setExtraRoots((prev) => (prev.includes(picked) ? prev : [...prev, picked]));
    // 自分で選んだフォルダは「ここを見たい」が明確なので下まで走査する
    openFolder(picked, true);
  };

  const roots: TreeNode[] = [
    ...drives.map((d) => ({
      name: d.label,
      path: d.path,
      hasSubdirs: true,
      imageCount: 0,
      countCapped: false,
      // 固定ドライブとネットワークドライブの直下だけは自動走査しない
      // （`C:\` 全体や回線越しの共有を丸ごと舐めない）。
      // USB接続のHDD/SSDは fixed と申告されることが多いが、
      // その場合も中のフォルダを選べば下まで走査される
      deepDefault: d.kind === "removable",
      drive: d,
    })),
    ...extraRoots.map((path) => ({
      name: path,
      path,
      hasSubdirs: true,
      imageCount: 0,
      countCapped: false,
      deepDefault: true,
    })),
  ];

  const renderNode = (node: TreeNode, depth: number) => {
    const isOpenNode = expanded.has(node.path);
    const children = listings[node.path]?.dirs ?? [];
    return (
      <div key={node.path}>
        <div
          className={"wiz-node" + (current === node.path ? " active" : "")}
          style={{ paddingInlineStart: `${8 + depth * 14}px` }}
          onClick={() => openFolder(node.path, node.deepDefault)}
          title={node.path}
        >
          <button
            className="wiz-twisty"
            disabled={!node.hasSubdirs}
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((prev) => {
                const next = new Set(prev);
                if (next.has(node.path)) next.delete(node.path);
                else {
                  next.add(node.path);
                  if (!listings[node.path]) loadDir(node.path);
                }
                return next;
              });
            }}
          >
            {node.hasSubdirs ? (isOpenNode ? "▾" : "▸") : ""}
          </button>
          <span className="wiz-icon">{node.drive ? driveIcon(node.drive) : "📁"}</span>
          <span className="wiz-name">{node.name}</span>
          {node.imageCount > 0 && (
            <span className="wiz-count">
              {node.countCapped
                ? t.wizardCapped(node.imageCount)
                : t.photosCount(node.imageCount)}
            </span>
          )}
        </div>
        {isOpenNode &&
          children.map((child) =>
            renderNode(
              {
                name: child.name,
                path: child.path,
                hasSubdirs: child.has_subdirs,
                imageCount: child.image_count,
                countCapped: child.count_capped,
                // 枝の下は「そのフォルダを選んだ」＝中を全部見たい、とみなす
                deepDefault: true,
              },
              depth + 1,
            ),
          )}
      </div>
    );
  };

  const visible = files.slice(0, shown);
  // 走査が最後まで行けなかったときは、**空でも「画像がありません」と言わない**。
  // カードが空だと誤解して消されるのが一番まずい
  const scanIncomplete = deep ? tree?.incomplete === true : unreadable;
  const emptyMessage = !current
    ? t.wizardPickFolderHint
    : loading
      ? deep
        ? t.wizardScanning
        : t.wizardCounting
      : unreadable
        ? t.wizardUnreadable
        : files.length === 0
          ? allFiles.length > 0
            ? t.wizardAllImported // 隠した結果からっぽ＝新しい写真が無い
            : scanIncomplete
              ? t.wizardScanIncomplete
              : t.wizardNoImages
          : null;

  return (
    <div className="palette-backdrop" onClick={() => !busy && onClose()}>
      <div className="wizard" onClick={(e) => e.stopPropagation()}>
        <div className="settings-head">
          <h2>{t.wizardTitle}</h2>
          <button
            className="settings-close"
            onClick={onClose}
            disabled={busy}
            title={t.close}
          >
            ✕
          </button>
        </div>

        <div className="wizard-body">
          <div className="wizard-tree">
            <div className="wiz-tree-head">{t.wizardSources}</div>
            {roots.length === 0 && <div className="wiz-empty">{t.wizardNoDrives}</div>}
            {roots.map((r) => renderNode(r, 0))}
            <button className="wiz-add" onClick={addFolderRoot}>
              {t.wizardOtherFolder}
            </button>
          </div>

          <div className="wizard-main">
            {progress && (
              <div className="wiz-copying">
                {/* 今まさにコピーした1枚を見せる。待ち時間の手持ち無沙汰を減らす */}
                <img
                  className="wiz-copying-shot"
                  src={sourceThumbSrc(progress.path, progress.done)}
                  alt=""
                  decoding="async"
                  draggable={false}
                />
                <div className="wiz-copying-body">
                  <div className="wiz-copying-head">
                    <span>
                      {t.wizardCopying} {progress.done}/{progress.total}
                    </span>
                    <span className="wiz-copying-eta">{etaLabel(progress)}</span>
                  </div>
                  <div className="wiz-bar">
                    <div
                      className="wiz-bar-fill"
                      style={{
                        inlineSize: `${(progress.done / Math.max(progress.total, 1)) * 100}%`,
                      }}
                    />
                  </div>
                  <div className="wiz-copying-name">{baseName(progress.path)}</div>
                </div>
              </div>
            )}
            <div className="wizard-toolbar">
              <label className="wiz-deep" title={t.wizardDeepHint}>
                <input
                  type="checkbox"
                  checked={deep}
                  disabled={!current || loading || busy}
                  onChange={(e) => current && openFolder(current, e.target.checked)}
                />
                {t.wizardDeep}
              </label>
              <label className="wiz-deep" title={t.wizardHideImported}>
                <input
                  type="checkbox"
                  checked={hideImported}
                  onChange={(e) => {
                    const hide = e.target.checked;
                    setHideImported(hide);
                    if (hide) {
                      // 隠した瞬間に、見えなくなるものを選択から外す
                      setSelected((prev) => {
                        const next = new Set(prev);
                        allFiles.forEach((f) => {
                          if (imported[f.path]) next.delete(f.path);
                        });
                        return next;
                      });
                    }
                  }}
                />
                {t.wizardHideImported}
              </label>
              {!hideImported && (
                <button onClick={selectNew} disabled={files.length === 0}>
                  {t.wizardSelectNew}
                </button>
              )}
              <button onClick={selectAll} disabled={files.length === 0}>
                {t.wizardSelectAll}
              </button>
              <button onClick={clearSelection} disabled={selected.size === 0}>
                {t.wizardClearSelection}
              </button>
              <span className="wiz-selected">
                {hiddenCount > 0 && (
                  <span className="wiz-hidden">{t.wizardHiddenCount(hiddenCount)}</span>
                )}
                {t.wizardSelected(selected.size)}
              </span>
            </div>

            {tree?.truncated && (
              <div className="wiz-notice">{t.wizardTruncated(allFiles.length)}</div>
            )}
            {scanIncomplete && files.length > 0 && (
              <div className="wiz-notice warn">{t.wizardScanIncomplete}</div>
            )}
            {emptyMessage ? (
              <div className={"wiz-empty big" + (scanIncomplete ? " warn" : "")}>
                {emptyMessage}
              </div>
            ) : (
              <>
                <div className="wizard-grid">
                  {visible.map((f) => {
                    const isSelected = selected.has(f.path);
                    const done = imported[f.path];
                    return (
                      <button
                        key={f.path}
                        className={
                          "wiz-tile" +
                          (isSelected ? " selected" : "") +
                          (done ? " imported" : "")
                        }
                        onClick={() => toggle(f.path)}
                        title={
                          f.offline
                            ? `${f.path}\n${t.wizardOfflineTitle}`
                            : done
                              ? `${f.path}\n${t.wizardImportedTitle}`
                              : f.path
                        }
                      >
                        {f.offline ? (
                          // クラウド上にしか無いファイル: サムネイルを取りに行くと
                          // 実体のダウンロードが始まるので、雲マークだけ出す
                          <span className="wiz-cloud">☁</span>
                        ) : (
                          <img
                            src={sourceThumbSrc(f.path, f.mtime_ms)}
                            alt=""
                            loading="lazy"
                            decoding="async"
                            draggable={false}
                          />
                        )}
                        <span className="wiz-check">{isSelected ? "✓" : ""}</span>
                        {done && (
                          <span className="wiz-done">{t.wizardImportedBadge}</span>
                        )}
                        <span className="wiz-fname">{f.name}</span>
                      </button>
                    );
                  })}
                </div>
                {files.length > shown && (
                  <button
                    className="wiz-more"
                    onClick={() => setShown((n) => n + PAGE_SIZE)}
                  >
                    {t.wizardMoreFiles(files.length - shown)}
                  </button>
                )}
              </>
            )}
          </div>
        </div>

        <div className="wizard-foot">
          <div className="wiz-dest">
            <span className="wiz-dest-label">{t.wizardDestination}</span>
            <code>{destination ?? t.settingsDestinationUnset}</code>
            <button
              className="wiz-dest-change"
              disabled={busy}
              onClick={async () => {
                const dest = await open({
                  directory: true,
                  title: t.pickDestination,
                });
                if (!dest) return;
                // 断られうる（写真.appのライブラリの中など）。投げっぱなしにすると
                // 未処理の拒否になり、コピー先が黙って元のまま残る
                try {
                  await setImportDestination(dest);
                } catch (e) {
                  onErrorRef.current(String(e));
                  return;
                }
                onConfigChanged();
              }}
            >
              {t.wizardChangeDestination}
            </button>
            {patternExample && (
              <span className="wiz-pattern">
                {t.wizardStructure}: <code>{patternExample}</code>
              </span>
            )}
          </div>
          <div className="wiz-actions">
            <button
              disabled={!current || busy}
              onClick={() => runImport(true)}
              title={t.wizardImportAll}
            >
              {t.wizardImportAllShort}
            </button>
            <button
              className="primary"
              disabled={busy || selected.size === 0}
              onClick={() => runImport(false)}
            >
              {t.wizardImportButton(selected.size)}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
