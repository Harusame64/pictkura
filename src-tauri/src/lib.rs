//! pictkura Tauriアプリ本体。
//!
//! `media://` カスタムプロトコルでSQLiteに登録された画像バイナリを
//! Webviewへ直接ストリーミングする（Base64禁止の原則）。
//! スキャンはDBロックの外で行い、走査中も画像配信を止めない。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use pictkura_core::protocol::{mime_for_path, parse_media_url, MediaKind, MediaTarget};
use pictkura_core::usn::{self, UsnOutcome, UsnPosition};
use pictkura_core::{Config, Db, ReadPool, SyncStats, ThumbnailService};
use tauri::http::{Response, StatusCode};
use tauri::{Emitter, Manager};

mod autoplay;

/// ポイズニングされていてもロックを取得する（パニックの連鎖でアプリ全体が死ぬのを防ぐ）。
fn lock_ok<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// アプリ全体の共有状態。
struct AppState {
    db: Mutex<Db>,
    /// 読み取り専用の接続プール（段階B-4）。media://配信とタイムライン系クエリを
    /// 分散し、スキャン反映等の書き込み中もUIの読み取りを止めない
    read_pool: ReadPool,
    /// スキャン反映用の専用接続を開くためのDBパス。
    /// 大規模ライブラリの差分反映で共有接続のMutexを長時間握らないようにする
    db_path: PathBuf,
    config: Mutex<Config>,
    config_path: PathBuf,
    thumbs: ThumbnailService,
    /// スキャン＋反映の全体を直列化するロック。
    /// 並走したスキャンの古いスナップショットが後から適用されると、
    /// 新しく追加されたルート配下のレコードを誤削除しうるため。
    scan_lock: Mutex<()>,
    /// ライブラリルートのファイルシステム監視（アプリ外の追加・削除に追従）。
    /// ルート構成が変わったら張り直す。
    watcher: Mutex<Option<pictkura_core::watch::LibraryWatcher>>,
    /// サムネイル配信のタッチ記録（id→利用時刻ms）。LRUの鮮度情報として
    /// メモリに集約し、ジャニターが定期的にDBへフラッシュする（段階B-3）
    thumb_touches: Mutex<HashMap<i64, i64>>,
    /// 起動時同期の⚡爆速メーター。USN差分は数msで終わり、Webviewの
    /// リスナー登録より先にイベントが発火して取りこぼされるため、
    /// ここに保持してフロントがマウント後にコマンドで取りにも来られるようにする
    startup_report: Mutex<Option<StartupScanDto>>,
    /// 検索インデックスの初期構築の進捗（第4部 段階D）。既存ライブラリを
    /// 後追いで索引化している間だけ building=true になる
    index_progress: Mutex<IndexProgressDto>,
    /// 取り込みウィザードで**実際に開いたフォルダ**（第5部 段階E）。
    /// `media://src/...` はライブラリ外の任意パスを配信できてしまうため、
    /// 「UIがユーザー操作で開いたフォルダの直下」だけに配信先を限る。
    browse_allow: Mutex<HashSet<PathBuf>>,
    /// AutoPlayや2重起動の `--import <パス>` で冷起動したときの取り込み対象。
    /// 冷起動ではフロントのリスナー登録より前にこの値が決まるので、
    /// マウント後に `take_pending_import` で取りに来る（起動レポートと同じ方式）。
    pending_import: Mutex<Option<String>>,
}

/// 起動引数から取り込み対象のドライブ/フォルダを取り出す。
/// AutoPlay（`pictkura.exe --import E:\`）や2重起動で渡る。
fn import_path_from_args(argv: &[String]) -> Option<String> {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a == "--import" {
            let p = it.next()?.trim();
            if p.is_empty() {
                return None;
            }
            // `E:` だけで来たらルートに直す（AutoPlayは通常 `E:\` を渡すが保険）。
            return Some(if p.ends_with(':') {
                format!("{p}\\")
            } else {
                p.to_string()
            });
        }
    }
    None
}

/// 既に起動中のインスタンスへ2重起動の引数が届いたとき（single-instance）。
/// 窓を前面に出し、取り込み対象があればウィザードを開くイベントを送る。
fn handle_second_instance(app: &tauri::AppHandle, argv: &[String]) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(path) = import_path_from_args(argv) {
        let _ = app.emit("open-import-drive", path);
    }
}

/// 冷起動時に `--import` で渡されたパスを一度だけ返す（取り込みウィザードを開くため）。
/// 2重起動はイベント（`open-import-drive`）で届くが、冷起動はフロントのリスナー登録より
/// 前に来て取りこぼすので、マウント後にこれで取りに来る。
#[tauri::command]
fn take_pending_import(state: tauri::State<AppState>) -> Option<String> {
    lock_ok(&state.pending_import).take()
}

/// 現在のルート構成でファイルシステム監視を張り直す。
fn rebuild_watcher(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let roots = lock_ok(&state.config).library.roots.clone();
    let event_app = app.clone();
    let watcher = pictkura_core::watch::watch_roots(
        &roots,
        std::time::Duration::from_millis(800),
        move |paths| handle_fs_events(&event_app, paths),
    )
    .ok();
    *lock_ok(&state.watcher) = watcher;
}

/// 設定されたルートのうち、アプリが管理するパッケージだったもの。
///
/// 走査（`scan_roots_pruned`）はこのルートを丸ごと飛ばすので、差分の入口
/// （監視・USN）も揃えて落とさないと、**差分で入れた行を次のフルスキャンが消す**
/// という往復になる。判定はルートだけで決まるので、イベントごとではなく
/// **1回だけ**求めて使い回す。
fn managed_package_roots(config: &Config) -> Vec<PathBuf> {
    config
        .library
        .roots
        .iter()
        .filter(|r| pictkura_core::import::is_managed_package_path(r))
        .cloned()
        .collect()
}

/// ウォッチャーのイベントバッチをDBへ追従させる。
/// イベントのあったパスだけを処理する（全ルートの再スキャンはしない）。
fn handle_fs_events(app: &tauri::AppHandle, paths: Vec<std::path::PathBuf>) {
    use pictkura_core::scanner::{self, ScannedFile};
    use std::time::UNIX_EPOCH;

    let state = app.state::<AppState>();
    let config = lock_ok(&state.config).clone();
    let mut changed = false;

    let stat_file = |p: &Path| -> Option<ScannedFile> {
        let meta = std::fs::metadata(p).ok()?;
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Some(ScannedFile {
            path: p.to_path_buf(),
            size: meta.len() as i64,
            mtime_ms,
        })
    };

    {
        let mut db = lock_ok(&state.db);
        // 新規・変更のみをupsertする（同一内容のupsertはサムネイルを無効化してしまう）
        let upsert_if_changed = |db: &mut Db, f: ScannedFile| -> bool {
            let same = db
                .get_meta_by_path(&f.path)
                .ok()
                .flatten()
                .is_some_and(|m| m.size == f.size && m.mtime_ms == f.mtime_ms);
            !same && db.upsert_files(std::slice::from_ref(&f)).is_ok()
        };

        // **ルートがパッケージなら、その配下は監視でも拾わない**（USN側と同じ）。
        // 走査（`scan_roots_pruned`）はそのルートを丸ごと飛ばすので、
        // ここだけ拾うと入れた行を次のフルスキャンが消す、の往復になる。
        // ルートの**配下**にあるパッケージは設定に従う（ここでは落とさない）。
        // 判定はルートだけで決まるので**ループの外で1回**（イベントごとに
        // 全構成要素を舐め直さない）
        let package_roots = managed_package_roots(&config);
        for p in paths {
            if scanner::is_excluded_path(&p, &config.library.exclude_patterns) {
                continue;
            }
            if package_roots.iter().any(|root| p.starts_with(root)) {
                continue;
            }
            if p.is_file() {
                if scanner::has_target_extension(&p, &config.import.extensions) {
                    if let Some(f) = stat_file(&p) {
                        changed |= upsert_if_changed(&mut db, f);
                    }
                }
            } else if p.is_dir() {
                // フォルダごと移動されてきた場合など: そのフォルダだけスキャン
                let outcome = scanner::scan_roots(
                    std::slice::from_ref(&p),
                    &config.import.extensions,
                    &config.library.exclude_patterns,
                );
                for f in outcome.files {
                    changed |= upsert_if_changed(&mut db, f);
                }
            } else {
                // パスが消えた: そのパス＋配下のレコードを削除
                if let Ok(n) = db.remove_by_prefix(&p) {
                    if n > 0 {
                        changed = true;
                    }
                }
            }
        }
    }

    if changed {
        enqueue_missing_thumbs(&state);
        let _ = app.emit("library-updated", ());
    }
}

/// 同期後にメタデータ未抽出分（＋即席サムネイル生成）をワーカーへ投入する。
/// 高品質サムネイルはここでは作らない（可視領域の要求時のみ: 段階B-3）。
fn enqueue_missing_thumbs(state: &AppState) {
    let db = lock_ok(&state.db);
    // 段階H-2の拾い直し。ファイル名から撮影日時を拾えるようにする前に走査した
    // DBには、撮影日時が mtime（＝同期やコピーの日）へ落ちた行が残っている。
    // 投げ直して決め直させる（DBを直接書き換えないので、EXIFが読めるファイルは
    // EXIFが勝つ。名前で上書きされることはない）。
    //
    // **済んだ印は持たない**。印を先に書くと、投げた仕事が終わる前にアプリが
    // 落ちたときに二度と拾い直せなくなる。代わりに**名前から日時を拾える行だけ**へ
    // 絞る: 拾えた行は日時が mtime と変わって条件から外れ、拾えない行は
    // ファイルを開かない文字列判定だけで弾けるので、毎回通っても実質ただ
    if let Ok(rows) = db.rows_with_fallback_taken_at() {
        let ids: Vec<i64> = rows
            .into_iter()
            // **mtimeと同じ値しか出てこない行は投げない**。スクリーンショットのように
            // 名前の日時とmtimeが同じ秒のファイルは、投げ直しても値が変わらないので
            // 条件から外れず、毎回の同期で投げ直され続ける。RAW/HEIFはその都度
            // 原寸を展開し直すので、放っておくと重い仕事が永久に回る
            .filter(|(_, path, mtime_ms)| {
                pictkura_core::namedate::guess_taken_at(path)
                    .is_some_and(|guessed| guessed != *mtime_ms)
            })
            .map(|(id, _, _)| id)
            .collect();
        state.thumbs.enqueue(&ids);
    }
    if let Ok(ids) = db.ids_missing_metadata() {
        state.thumbs.enqueue(&ids);
    }
}

/// 2つのパスが同じ場所を指すか。Windowsは大文字小文字・区切り文字の揺れを無視する。
fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        let norm = |p: &Path| {
            p.to_string_lossy()
                .trim_end_matches(['\\', '/'])
                .replace('/', "\\")
                .to_lowercase()
        };
        norm(a) == norm(b)
    } else {
        a == b
    }
}

/// 設定を「クローン→変更→保存成功→メモリ反映」の順で更新する（失敗時の不整合防止）。
fn update_config(state: &AppState, mutate: impl FnOnce(&mut Config)) -> Result<(), String> {
    let mut config = lock_ok(&state.config);
    let mut updated = config.clone();
    mutate(&mut updated);
    updated
        .save(&state.config_path)
        .map_err(|e| e.to_string())?;
    *config = updated;
    Ok(())
}

/// フロントエンドへ渡すメディア1件分のDTO。
/// 幅・高さはグリッドが描画前に枠を確保するために必須で渡す（未抽出時は0）。
#[derive(serde::Serialize, Clone)]
struct MediaItemDto {
    id: i64,
    file_name: String,
    width: i64,
    height: i64,
    /// 表示・グルーピング用の日時（撮影日時、なければmtime。Unixエポックミリ秒）
    taken_at_ms: i64,
    /// 属する表示日（ローカル日付のYYYYMMDD整数）。スパースタイムラインの部分更新に使う
    day_key: i64,
    /// キャッシュバスティング用（サムネイルURLのバージョンに使う）
    mtime_ms: i64,
    has_thumb: bool,
    /// サムネイルの品質段階: 0=なし, 1=即席, 2=高品質。
    /// URLバージョンに含めて、即席→高品質の差し替え時にキャッシュを割る
    thumb_state: i64,
    favorite: bool,
    /// 動画の長さ（ミリ秒）。動画以外・未読取は null（第9部）
    duration_ms: Option<i64>,
    /// 動画か（一覧の▶バッジと、ビューアでプレイヤーを出すかの判断）
    is_video: bool,
    /// アプリ内で再生できるか。偽なら「既定のアプリで開く」へ逃がす
    /// （.m2ts/.avi はWebViewがコンテナごと相手にしない）
    plays_in_app: bool,
}

impl From<pictkura_core::MediaRecord> for MediaItemDto {
    fn from(r: pictkura_core::MediaRecord) -> Self {
        Self {
            id: r.id,
            file_name: r
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            width: r.width.unwrap_or(0),
            height: r.height.unwrap_or(0),
            taken_at_ms: r.taken_at_ms.unwrap_or(r.mtime_ms),
            day_key: r.day_key,
            mtime_ms: r.mtime_ms,
            has_thumb: r.thumb_path.is_some(),
            thumb_state: r.thumb_state,
            favorite: r.favorite,
            duration_ms: r.duration_ms,
            is_video: pictkura_core::video::is_video_path(&r.path),
            plays_in_app: pictkura_core::video::plays_in_webview(&r.path),
        }
    }
}

/// タイムライン索引の1日分のDTO。
#[derive(serde::Serialize)]
struct DaySummaryDto {
    /// ローカル日付のYYYYMMDD整数（例: 20260812）
    day_key: i64,
    count: i64,
    /// その日の代表（最新）レコード。カレンダーの日セルのカバー表示用
    cover_id: i64,
    cover_mtime_ms: i64,
    cover_thumb_state: i64,
}

/// 「〇年前の今日」の1件分のDTO。
#[derive(serde::Serialize)]
struct MemoryDto {
    years_ago: i64,
    item: MediaItemDto,
}

/// ライブラリ全体の件数（サイドバー表示用）。
#[derive(serde::Serialize)]
struct LibraryStatsDto {
    total: i64,
    favorites: i64,
}

/// カメラ別の枚数（左ペイン「カメラとメディア」用、第4部 段階D）。
#[derive(serde::Serialize)]
struct CameraDto {
    name: String,
    count: i64,
}

/// 検索インデックスの初期構築の進捗（第4部 段階D）。
/// 索引導入前に取り込んだ既存ライブラリを後追いで索引化している間だけ
/// `building` が true になる。分母・分子はIDなので百分率は概算。
#[derive(serde::Serialize, Clone, Default)]
struct IndexProgressDto {
    building: bool,
    /// 構築を最後まで終えられなかった（書き込み競合等）。次回起動で続きから再開する。
    /// 「終わった」と誤って伝えないためにUIへ知らせる
    incomplete: bool,
    /// 段階: "index"=全文索引の掃き寄せ / "camera"=カメラ情報の後追い補完
    phase: String,
    done: i64,
    total: i64,
}

#[derive(serde::Serialize, Clone)]
struct SyncStatsDto {
    added: usize,
    changed: usize,
    removed: usize,
}

impl From<SyncStats> for SyncStatsDto {
    fn from(s: SyncStats) -> Self {
        Self {
            added: s.added,
            changed: s.changed,
            removed: s.removed,
        }
    }
}

/// スキャン設定のフィンガープリント。拡張子・除外パターン・**ルート構成**が
/// 変わると差分系スキャンの前提（前回と同じ条件で走査した記録）が崩れるため、
/// これが変わっていたらフルスキャンへフォールバックする（段階B-2）。
/// ルートを含めるのは、アプリ終了中に設定ファイルを直接編集して追加された
/// ルートがUSN差分パス（ジャーナルに記録が無い）で永久に見えなくなるのを防ぐため。
fn scan_fingerprint(config: &Config) -> String {
    let mut ext: Vec<String> = config
        .import
        .extensions
        .iter()
        .map(|e| e.to_lowercase())
        .collect();
    ext.sort();
    let mut pat: Vec<String> = config
        .library
        .exclude_patterns
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    pat.sort();
    let mut roots: Vec<String> = config
        .library
        .roots
        .iter()
        .map(|r| {
            r.to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        })
        .collect();
    roots.sort();
    format!(
        "v2|ext={}|pat={}|roots={}",
        ext.join(","),
        pat.join(","),
        roots.join(";")
    )
}

/// 設定スナップショットでスキャン → 差分反映。
/// scan_lockで全体を直列化する（設定スナップショットはロック取得後に取る）。
/// 反映は**専用のDB接続**で行う: 共有接続のMutexを反映中ずっと握ると、
/// media://配信・タイムラインIPC・サムネイル完了通知まで数秒〜十数秒止まってしまう。
/// SQLite側の並行制御はWAL＋busy_timeoutに任せる（サムネイルワーカーと同じ方式）。
///
/// `full` = false ならディレクトリmtimeの枝刈り付き（段階B-2）。
/// mtime一致のディレクトリはファイルの列挙・statをスキップする。
/// 内容だけの上書き等は枝刈りでは見えないため、手動の「今すぐ同期」は
/// full = true で呼び、修復手段を兼ねる。
fn scan_and_apply(state: &AppState, full: bool) -> Result<SyncStats, String> {
    let _scan_guard = lock_ok(&state.scan_lock);
    let config = lock_ok(&state.config).clone();
    let fingerprint = scan_fingerprint(&config);
    let mut db = Db::open(&state.db_path).map_err(|e| e.to_string())?;
    let known_dirs = if full {
        HashMap::new()
    } else {
        match db.get_meta("scan_fingerprint") {
            Ok(Some(stored)) if stored == fingerprint => db.load_dirs().unwrap_or_default(),
            _ => HashMap::new(), // 設定が変わった・初回 → フルスキャン
        }
    };
    let scan = pictkura_core::scan_library_pruned(&config, &known_dirs);
    let stats = pictkura_core::apply_scan(&mut db, &scan).map_err(|e| e.to_string())?;
    let _ = db.set_meta("scan_fingerprint", &fingerprint);
    enqueue_missing_thumbs(state);
    Ok(stats)
}

/// 起動時同期の方式（爆速メーターの表示用）。
enum StartupMethod {
    /// USNジャーナル差分（処理レコード数・ダーティディレクトリ数）
    Usn { records: usize, dirty: usize },
    /// ディレクトリmtime枝刈りスキャン
    Pruned,
    /// フルスキャン（初回・設定変更後）
    Full,
}

/// 起動時同期の結果をフロントへ知らせる「⚡爆速メーター」のDTO。
#[derive(serde::Serialize, Clone)]
struct StartupScanDto {
    /// "usn" | "pruned" | "full"
    method: String,
    elapsed_ms: u64,
    added: usize,
    changed: usize,
    removed: usize,
    /// usn: 処理したジャーナルレコード数
    usn_records: usize,
    /// usn: 再走査したダーティディレクトリ数
    dirty_dirs: usize,
    /// 枝刈り: ファイル列挙をスキップしたディレクトリ数
    skipped_dirs: usize,
    /// ライブラリ総数
    total: i64,
}

/// `\\?\` 拡張プレフィックスを外す（fs::canonicalizeの戻り値を通常形式へ）。
fn strip_verbatim(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// ルート群が属するボリューム一覧（"C:"形式・重複なし）。
/// UNC等の非対応パスが混ざっていたらNone（USNは使わない）。
/// ジャンクション/シンボリックリンクは実体へ解決してから判定する
/// （フォルダリダイレクトされたルートで別ボリュームのジャーナルを
/// 読んでしまい、オフライン変更を全部見落とす事故を防ぐ）。
fn volumes_of_roots(roots: &[PathBuf]) -> Option<Vec<String>> {
    let mut volumes: Vec<String> = Vec::new();
    for root in roots {
        let real = std::fs::canonicalize(root).ok()?;
        let volume = usn::volume_of(&real)?;
        if !volumes.contains(&volume) {
            volumes.push(volume);
        }
    }
    Some(volumes)
}

/// USNのダーティディレクトリをルートへ照合するための情報。
struct RootSpec {
    /// 設定ファイル上の綴り（DBのpath/parent_dir/dirsのキーはこの綴りで始まる）
    spelling: String,
    /// 照合に使うプレフィックス。設定綴りに加え、ジャンクション等を解決した
    /// 実体パスも含める（FRN解決は実体パスを返すため）
    prefixes: Vec<String>,
}

fn root_specs(roots: &[PathBuf]) -> Vec<RootSpec> {
    roots
        .iter()
        .map(|root| {
            let spelling = root
                .to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_string();
            let mut prefixes = vec![spelling.clone()];
            if let Ok(real) = std::fs::canonicalize(root) {
                let real = strip_verbatim(&real).trim_end_matches('\\').to_string();
                if !real.eq_ignore_ascii_case(&spelling) {
                    prefixes.push(real);
                }
            }
            RootSpec { spelling, prefixes }
        })
        .collect()
}

fn usn_meta_key(volume: &str) -> String {
    format!("usn|{volume}")
}

/// USNが返す正規のパス綴りを、設定ルートの綴りへ揃える。ルート配下でなければNone。
///
/// DBのpath/parent_dir/dirsのキーは「設定ルートの綴り＋ファイルシステムが返した
/// 構成要素名」で保存されている。USNのFRN解決はドライブレターの大小文字や
/// ジャンクション解決後の実体パスなど、設定と違う綴りを返すことがあり、
/// そのまま部分反映するとSQLiteの大小区別の比較で同じファイルが重複レコードになる。
/// 大文字小文字（ASCII）と区切り文字の揺れを無視して**最長一致**のプレフィックスを
/// 探し、その部分を設定の綴りへ置き換える（配下の構成要素名はFS由来なので一致する）。
fn rebase_to_root_spelling(path: &Path, specs: &[RootSpec]) -> Option<PathBuf> {
    // ASCIIのみ小文字化＋区切り統一。バイト長が変わらないため、
    // 正規化文字列上の位置で元の文字列をそのまま切り出せる
    let norm = |s: &str| s.replace('/', "\\").to_ascii_lowercase();
    let path_str = path.to_string_lossy().replace('/', "\\");
    let path_norm = norm(&path_str);
    let mut best: Option<(usize, &str)> = None; // (一致プレフィックス長, 揃える綴り)
    for spec in specs {
        for prefix in &spec.prefixes {
            let prefix_norm = norm(prefix.trim_end_matches('\\'));
            let matched =
                path_norm == prefix_norm || path_norm.starts_with(&format!("{prefix_norm}\\"));
            if matched && best.is_none_or(|(len, _)| prefix_norm.len() > len) {
                best = Some((prefix_norm.len(), &spec.spelling));
            }
        }
    }
    let (len, spelling) = best?;
    let suffix = &path_str[len..];
    Some(PathBuf::from(format!("{spelling}{suffix}")))
}

/// USNジャーナルの差分だけでライブラリを追従させる（段階B-1の最速パス）。
/// 全ルートのボリュームで差分が成立した場合のみ Some を返す。
/// None ならフォールバック（枝刈り/フルスキャン）が必要。
fn try_usn_sync(db: &mut Db, config: &Config) -> Option<(SyncStats, usize, usize)> {
    let roots = &config.library.roots;
    if roots.is_empty() {
        return None;
    }
    // ルート自体の削除・リネームはルートの「親」（ライブラリ外）にしか記録されず、
    // 配下フィルタで落ちて「変更なし」に見えてしまう。ルートが実在しなければ
    // 差分では扱えないのでフォールバックする（枝刈りスキャンの走査失敗ルート
    // 保護により、レコードは誤削除されない）
    if !roots.iter().all(|root| root.is_dir()) {
        return None;
    }
    let volumes = volumes_of_roots(roots)?;
    let mut dirty_dirs: Vec<PathBuf> = Vec::new();
    let mut positions: Vec<(String, UsnPosition)> = Vec::new();
    let mut records = 0usize;
    for volume in &volumes {
        let stored = db
            .get_meta(&usn_meta_key(volume))
            .ok()
            .flatten()
            .and_then(|s| UsnPosition::from_meta(&s));
        match usn::read_changes_since(volume, stored.as_ref()) {
            UsnOutcome::Delta(delta) => {
                records += delta.record_count;
                positions.push((volume.clone(), delta.position));
                dirty_dirs.extend(delta.dirty_dirs);
            }
            // 1ボリュームでも差分が取れなければ全体をフォールバックさせる
            // （部分反映は「全変更を把握できている」前提でのみ安全）
            UsnOutcome::FullScanNeeded(_) => return None,
        }
    }
    // ライブラリ配下のディレクトリだけに絞り、綴りを設定ルートへ揃える
    let specs = root_specs(roots);
    let package_roots = managed_package_roots(config);
    let mut seen_dirty = std::collections::HashSet::new();
    let dirty_dirs: Vec<PathBuf> = dirty_dirs
        .iter()
        .filter_map(|dir| rebase_to_root_spelling(dir, &specs))
        .filter(|dir| {
            !pictkura_core::scanner::is_excluded_path(dir, &config.library.exclude_patterns)
        })
        // **ルートがパッケージなら、その配下は差分でも拾わない。**
        // 走査側（`scan_roots_pruned`）はそのルートを丸ごと飛ばすので、
        // ここだけ拾うと差分で入れた行を次のフルスキャンが消す、の往復になる。
        // 逆にルートの**配下**にあるパッケージは設定に従う（利用者が
        // `*.photoslibrary` を消せば索引される）ので、ここでは落とさない
        .filter(|dir| !package_roots.iter().any(|root| dir.starts_with(root)))
        .filter(|dir| seen_dirty.insert(dir.clone()))
        .collect();
    let dirty_count = dirty_dirs.len();

    let stats = if dirty_dirs.is_empty() {
        SyncStats::default() // 変更なし: ファイルシステムを一切歩かない最速パス
    } else {
        let known_dirs = db.load_dirs().ok()?;
        let (outcome, ok) = pictkura_core::scanner::scan_dirty_dirs(
            &dirty_dirs,
            &config.import.extensions,
            &config.library.exclude_patterns,
            &known_dirs,
        );
        if !ok {
            return None; // 走査にエラー → 部分反映は危険なのでフォールバック
        }
        let (added, changed, removed) = db
            .apply_partial_scan(&outcome.files, &outcome.seen_dirs, &outcome.enumerated_dirs)
            .ok()?;
        SyncStats {
            added,
            changed,
            removed,
            skipped_dirs: outcome.skipped_dirs,
        }
    };
    // 反映が終わってから位置を保存する（途中で失敗したら次回再処理される安全側）
    for (volume, position) in positions {
        let _ = db.set_meta(&usn_meta_key(&volume), &position.to_meta());
    }
    Some((stats, records, dirty_count))
}

/// 起動時同期: USNジャーナル差分 → mtime枝刈り → フルの順で最速の方式を選ぶ。
fn startup_scan(state: &AppState) -> Result<(SyncStats, StartupMethod), String> {
    let _scan_guard = lock_ok(&state.scan_lock);
    let config = lock_ok(&state.config).clone();
    let fingerprint = scan_fingerprint(&config);
    let mut db = Db::open(&state.db_path).map_err(|e| e.to_string())?;
    let fingerprint_ok =
        db.get_meta("scan_fingerprint").ok().flatten().as_deref() == Some(fingerprint.as_str());

    if fingerprint_ok {
        if let Some((stats, records, dirty)) = try_usn_sync(&mut db, &config) {
            enqueue_missing_thumbs(state);
            return Ok((stats, StartupMethod::Usn { records, dirty }));
        }
    }

    // フォールバック。次回からUSN差分にできるよう、スキャン**前**に現在位置を取る
    // （スキャン中に起きた変更は次回のUSN差分で再処理される: 安全側の順序）
    let mut positions: Vec<(String, UsnPosition)> = Vec::new();
    if let Some(volumes) = volumes_of_roots(&config.library.roots) {
        for volume in volumes {
            if let UsnOutcome::FullScanNeeded(Some(position)) =
                usn::read_changes_since(&volume, None)
            {
                positions.push((volume, position));
            }
        }
    }
    let known_dirs = if fingerprint_ok {
        db.load_dirs().unwrap_or_default()
    } else {
        HashMap::new() // 初回・設定変更後は枝刈りせず全列挙
    };
    let method = if known_dirs.is_empty() {
        StartupMethod::Full
    } else {
        StartupMethod::Pruned
    };
    let scan = pictkura_core::scan_library_pruned(&config, &known_dirs);
    let stats = pictkura_core::apply_scan(&mut db, &scan).map_err(|e| e.to_string())?;
    let _ = db.set_meta("scan_fingerprint", &fingerprint);
    // 位置の保存は**全ルートの走査が成功したときだけ**。失敗したルートの
    // オフライン変更はこのスキャンに反映されておらず、位置だけ進めると
    // その変更のジャーナル記録が保存位置の後ろへ隠れ、以後のUSN差分起動で
    // 永久にスキップされてしまう（次回もフォールバックさせて拾い直す）
    let all_roots_ok = config
        .library
        .roots
        .iter()
        .all(|root| scan.outcome.ok_roots.contains(root));
    if all_roots_ok {
        for (volume, position) in positions {
            let _ = db.set_meta(&usn_meta_key(&volume), &position.to_meta());
        }
    }
    enqueue_missing_thumbs(state);
    Ok((stats, method))
}

/// タイムライン索引: 「日付→枚数」のサマリを新しい日付順で返す。
/// 全件のレコード本体は転送しない（スパースタイムラインの骨組み）。
///
/// `query` が空文字なら絞り込みなし（＝従来のタイムラインと同じ実行計画）。
/// 検索時も返す形は同じなので、フロントのタイムライン描画はそのまま使える。
#[tauri::command]
fn timeline_summary(
    state: tauri::State<'_, AppState>,
    query: String,
    favorites_only: bool,
) -> Result<Vec<DaySummaryDto>, String> {
    let query = pictkura_core::parse_query(&query, favorites_only);
    Ok(state
        .read_pool
        .with(|db| db.search_summary(&query))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|d| DaySummaryDto {
            day_key: d.day_key,
            count: d.count,
            cover_id: d.cover_id,
            cover_mtime_ms: d.cover_mtime_ms,
            cover_thumb_state: d.cover_thumb_state,
        })
        .collect())
}

/// 指定日（YYYYMMDD整数）のメディアを表示順（新しい順）で返す。
/// フロントは可視範囲の日だけをこれで取得する。
#[tauri::command]
fn list_day(
    state: tauri::State<'_, AppState>,
    day_key: i64,
    query: String,
    favorites_only: bool,
) -> Result<Vec<MediaItemDto>, String> {
    let query = pictkura_core::parse_query(&query, favorites_only);
    Ok(state
        .read_pool
        .with(|db| db.search_day(day_key, &query))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(Into::into)
        .collect())
}

/// カメラ別の枚数を多い順で返す（左ペイン「カメラとメディア」、第4部 段階D）。
#[tauri::command]
fn list_cameras(state: tauri::State<'_, AppState>) -> Result<Vec<CameraDto>, String> {
    Ok(state
        .read_pool
        .with(|db| db.list_cameras())
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(name, count)| CameraDto { name, count })
        .collect())
}

/// 詳細ビューアの撮影情報（第4部 段階D）。
///
/// DBには持たず、**開いた瞬間に実ファイルのEXIFヘッダを読む**（数百µs）。
/// 1000万件ぶんの列を増やさずに済み、表示内容は常に実ファイルと一致する。
#[tauri::command]
fn get_exif_info(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<pictkura_core::thumbs::ExifInfo, String> {
    let record = state
        .read_pool
        .with(|db| db.get_by_id(id))
        .map_err(|e| e.to_string())?
        .ok_or("レコードが見つかりません")?;
    Ok(pictkura_core::thumbs::read_exif_info(&record.path))
}

/// この環境で開けない形式があるかの報せ（第7部 段階G-6）。
#[derive(serde::Serialize)]
struct DecoderStatusDto {
    /// ライブラリにあるHEIC/HEIFの枚数（0なら黙っていてよい）
    heif_total: i64,
    /// 実際に展開できたか。**試せなかったときも真**にする
    /// （確かめていないのに「使えません」と言わない）
    heif_ok: bool,
    /// 「入れ方を見る」の導線があるか（Windowsだけ）
    help_available: bool,
}

/// HEICを展開できるかを実地で確かめ、UIの案内を出すかどうかを返す。
///
/// **HEVCは特許の都合でデコーダを同梱していない**（プールがデコーダの配布にも
/// 課金するため。第7部 段階G-6）。OSに委ねている以上、拡張機能が入っていない
/// 環境ではサムネイルが付かないので、黙って空欄にせず理由を伝える。
/// AVIFは自前のデコーダを積んでいるので、この案内の対象にしない。
///
/// 判定は**ライブラリにHEICが1枚でもあるときだけ**行う。無い環境に
/// 「拡張機能を入れてください」と出しても意味が無い。
// ファイルを開いて実際に展開するので、主スレッドで走らせない
// （HEIFの主画像は実測260〜480ms。ここで止めると起動直後の画面が固まる）
#[tauri::command(async)]
fn decoder_status(state: tauri::State<'_, AppState>) -> Result<DecoderStatusDto, String> {
    // 見本の候補を多めにもらう。**クラウドにしか実体が無いものは試さない**
    // （確認のためにダウンロードを始めてしまうのは本末転倒）。
    // 実測したライブラリではHEICの94%がクラウドのみだったので、
    // 数枚しか見ないと候補が全滅する
    const SAMPLE_CANDIDATES: usize = 256;
    let (heif_total, samples) = state
        .read_pool
        .with(|db| db.count_by_extensions(&["heic", "heif", "hif"], SAMPLE_CANDIDATES))
        .map_err(|e| e.to_string())?;
    let mut testable = samples
        .iter()
        .filter(|p| !pictkura_core::cloud::is_cloud_only_path(p))
        .peekable();
    // 試せる見本が1枚も無いときは「分からない」であって「駄目」ではない。
    // ここを偽に倒すと、クラウドだけのライブラリで**嘘の警告**が出る
    let heif_ok = testable.peek().is_none() || testable.any(|p| pictkura_core::heif::can_decode(p));
    Ok(DecoderStatusDto {
        heif_total,
        heif_ok,
        help_available: cfg!(windows),
    })
}

/// デコーダの入れ方の案内を開く（Windowsのみ）。
///
/// Microsoft Storeの該当ページを直接開く。**無料と有料で別のページ**なので
/// どちらを開くかを `kind` で受ける:
///
/// - `heif` … HEIF Image Extensions（無料）。HEICの**コンテナ**を読む側
/// - `hevc` … HEVC Video Extensions（有料・数百円）。**画素**を展開する側。
///   動画だけでなく**HEIC画像にも要る**（HEICの中身はHEVCのタイル）
///
/// **他のOSでは呼ばれない**（`help_available` が偽なのでボタン自体を出さない）。
/// macOSはImageIO/VideoToolboxが最初から読めるので案内が要らず、Linuxは
/// 配布元のパッケージ（libheif/libavcodec）の話なのでStoreのページは無意味。
#[tauri::command]
fn open_decoder_help(kind: String) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("このOSには案内できる導線がありません".into());
    }
    let product = match kind.as_str() {
        "heif" => "9pmmsr1cgpwg",
        "hevc" => "9nmzlz57r3t7",
        _ => return Err("案内の種類が不正です".into()),
    };
    tauri_plugin_opener::open_url(
        format!("ms-windows-store://pdp/?ProductId={product}"),
        None::<&str>,
    )
    .map_err(|e| e.to_string())
}

/// 「このアプリについて」に出す情報（v0.1）。
#[derive(serde::Serialize)]
struct AboutDto {
    version: String,
    /// 同梱した取扱説明書。開発中の実行では見つからないので `None` になりうる
    manual_path: Option<String>,
    /// 同梱したOSSライセンス一覧
    licenses_path: Option<String>,
}

/// 版と、同梱した文書の場所を返す。
///
/// **パスの解決はここでやる**。配布した実行ファイルの隣（`resource_dir`）に
/// 置かれるが、`cargo run` で動かしているときは存在しない。無いものを
/// 「開く」と言って失敗させないよう、**実在するものだけ**返す。
///
/// **罠**: `tauri.conf.json` の `resources` に `../` を含めて書いたものは、
/// 資源置き場の直下ではなく **`_up_/` の下**へ集められる（設定ファイルより
/// 上の階層を、資源置き場の中で表現するための決まり）。素のパスだけを見ると
/// 配布したときだけ「同梱されていない」と誤判定するので、両方を試す。
#[tauri::command]
fn about_info(app: tauri::AppHandle) -> AboutDto {
    use tauri::Manager;
    let resolve = |name: &str| -> Option<String> {
        let dir = app.path().resource_dir().ok()?;
        [dir.join(name), dir.join("_up_").join(name)]
            .into_iter()
            .find(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    };
    AboutDto {
        version: app.package_info().version.to_string(),
        manual_path: resolve("docs/manual.html"),
        licenses_path: resolve("THIRD-PARTY-LICENSES.txt"),
    }
}

/// 同梱した文書をOSの既定のアプリで開く。
///
/// 引数はパスではなく**種類**にする。任意のパスを受け取る口を作ると、
/// フロント側の不具合や細工でどこでも開けてしまう。
#[tauri::command]
fn open_bundled_doc(app: tauri::AppHandle, kind: String) -> Result<(), String> {
    let info = about_info(app.clone());
    let path = match kind.as_str() {
        "manual" => info.manual_path,
        "licenses" => info.licenses_path,
        _ => return Err("文書の種類が不正です".into()),
    }
    .ok_or("この実行環境には同梱されていません")?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// 動画を開く前に聞く「これは再生できるか」（第9部）。
#[derive(serde::Serialize)]
struct VideoStatusDto {
    /// アプリ内で再生できるコンテナか（.m2ts/.avi は偽）
    plays_in_app: bool,
    /// クラウドにしか実体が無い。再生するとダウンロードが始まる
    cloud_only: bool,
    /// 実ファイルがまだそこにあるか。無いなら「コーデックが無い」ではなく
    /// 「ファイルが無い」と言う（有料の拡張機能を勧めてしまわないため）
    exists: bool,
}

/// 動画の再生可否を返す（第9部）。**ビューアで動画を開いた1回だけ**呼ぶ。
///
/// 一覧のDTOに載せない理由は、クラウド判定がファイル属性の読み出しだからで、
/// 1000件の日を開くたびに1000回のsyscallを撒くのは割に合わない
/// （判定そのものはダウンロードを誘発しない。属性を見るだけ）。
#[tauri::command]
fn video_status(state: tauri::State<'_, AppState>, id: i64) -> Result<VideoStatusDto, String> {
    let path = path_of(&state, id)?;
    Ok(VideoStatusDto {
        plays_in_app: pictkura_core::video::plays_in_webview(&path),
        cloud_only: pictkura_core::cloud::is_cloud_only_path(&path),
        // クラウドのプレースホルダも「ある」と答える（属性を見るだけで、
        // ここではダウンロードは起きない）
        exists: path.exists(),
    })
}

/// レコードIDから実ファイルのパスを引く（ファイル操作コマンドの共通部）。
fn path_of(state: &AppState, id: i64) -> Result<PathBuf, String> {
    state
        .read_pool
        .with(|db| db.get_by_id(id))
        .map_err(|e| e.to_string())?
        .map(|r| r.path)
        .ok_or_else(|| "レコードが見つかりません".to_string())
}

/// OS既定のアプリで開く（Windowsの「開く」相当）。
#[tauri::command]
fn open_default(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let path = path_of(&state, id)?;
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// エクスプローラー／Finderでファイルの場所を開き、その項目を選択する。
#[tauri::command]
fn reveal_in_folder(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let path = path_of(&state, id)?;
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

/// 指定した外部アプリでファイルを開き、そのアプリを設定へ覚える。
///
/// Lightroom / digiKam と同じ流儀で、一度選んだアプリはメニューに残す
/// （利用者が使う編集アプリは人それぞれなので、特定アプリを探しには行かない）。
#[tauri::command]
fn open_with(
    state: tauri::State<'_, AppState>,
    id: i64,
    app_path: String,
    remember: bool,
) -> Result<(), String> {
    let path = path_of(&state, id)?;
    let app = PathBuf::from(&app_path);
    tauri_plugin_opener::open_path(&path, Some(app_path.as_str())).map_err(|e| e.to_string())?;
    if remember {
        update_config(&state, |c| {
            c.editors.remember(&app);
        })?;
    }
    Ok(())
}

/// 登録済みの外部エディタを設定から外す。
#[tauri::command]
fn forget_editor(state: tauri::State<'_, AppState>, app_path: String) -> Result<(), String> {
    update_config(&state, |c| {
        c.editors
            .apps
            .retain(|a| a.path.as_os_str() != app_path.as_str());
    })
}

/// ファイルを**ゴミ箱へ**移動し、DBからも取り除く。
///
/// 完全削除はしない（写真は取り返しがつかないため、OSのゴミ箱経由にして
/// 誤操作から戻せるようにする）。ゴミ箱に入れられたものだけをDBから消す。
/// 戻り値は実際に削除できた件数。
#[tauri::command]
fn delete_media(state: tauri::State<'_, AppState>, ids: Vec<i64>) -> Result<usize, String> {
    let mut deleted: Vec<PathBuf> = Vec::new();
    for id in &ids {
        let Ok(path) = path_of(&state, *id) else {
            continue;
        };
        // ファイルが既に無い場合もDBからは消す（実体に合わせる）
        match trash::delete(&path) {
            Ok(()) => deleted.push(path),
            Err(_) if !path.exists() => deleted.push(path),
            Err(e) => return Err(format!("ゴミ箱へ移動できません: {e}")),
        }
    }
    if deleted.is_empty() {
        return Ok(0);
    }
    lock_ok(&state.db)
        .remove_paths(&deleted)
        .map_err(|e| e.to_string())?;
    Ok(deleted.len())
}

/// 取り込み先フォルダ構成の選択肢1つ（パターンと、今日の日付での実例）。
#[derive(serde::Serialize)]
struct FolderPatternDto {
    pattern: String,
    /// そのパターンで今日取り込んだ場合にできるフォルダ（UIのプレビュー用）
    example: String,
}

/// 取り込み先フォルダ構成の選択肢を返す。
///
/// 自由記述のパターンを利用者に書かせず「選ぶだけ」にするための一覧
/// （Lightroom Classic の「日付で整理」と同じ考え方）。
#[tauri::command]
fn list_folder_patterns() -> Vec<FolderPatternDto> {
    use chrono::Datelike;
    let now = chrono::Local::now();
    let today = pictkura_core::import::CivilDate {
        year: now.year(),
        month: now.month(),
        day: now.day(),
    };
    pictkura_core::import::FOLDER_PATTERN_PRESETS
        .iter()
        .map(|p| FolderPatternDto {
            pattern: p.to_string(),
            example: pictkura_core::import::render_folder_pattern(p, today),
        })
        .collect()
}

/// 取り込み先のフォルダ構成を設定して保存する。
#[tauri::command]
fn set_folder_pattern(state: tauri::State<'_, AppState>, pattern: String) -> Result<(), String> {
    update_config(&state, |c| c.routing.folder_pattern = pattern)
}

/// 自由記述のフォルダ構成が、実際にどんなフォルダ名になるかを返す。
///
/// **フロントで組み立てない**のが要点。`render_folder_pattern` は置換だけでなく
/// 無害化（`..`・絶対パス・使えない文字を落とす）まで行うので、同じ処理を
/// TypeScript側に書くと必ずずれる。ここを通せば、利用者が `../..` と打ったときに
/// 「そうはならない」ことがその場で見える。
#[tauri::command]
fn preview_folder_pattern(pattern: String) -> String {
    use chrono::Datelike;
    let now = chrono::Local::now();
    let today = pictkura_core::import::CivilDate {
        year: now.year(),
        month: now.month(),
        day: now.day(),
    };
    pictkura_core::import::render_folder_pattern(&pattern, today)
}

/// 検索インデックスの初期構築の進捗を返す（イベント取りこぼし時のフォールバック）。
#[tauri::command]
fn get_index_progress(state: tauri::State<'_, AppState>) -> IndexProgressDto {
    lock_ok(&state.index_progress).clone()
}

/// 「〇年前の今日」の思い出（過去の各年の同じ月日）を返す。
#[tauri::command]
fn list_memories(state: tauri::State<'_, AppState>) -> Result<Vec<MemoryDto>, String> {
    Ok(state
        .read_pool
        .with(|db| db.list_memories(24))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(years_ago, r)| MemoryDto {
            years_ago,
            item: r.into(),
        })
        .collect())
}

/// 起動時同期の⚡爆速メーターを返す（イベント取りこぼし時のフォールバック）。
#[tauri::command]
fn get_startup_report(state: tauri::State<'_, AppState>) -> Option<StartupScanDto> {
    lock_ok(&state.startup_report).clone()
}

/// ライブラリ全体の件数（総数・お気に入り数）を返す。
#[tauri::command]
fn get_stats(state: tauri::State<'_, AppState>) -> Result<LibraryStatsDto, String> {
    state
        .read_pool
        .with(|db| {
            Ok(LibraryStatsDto {
                total: db.count()?,
                favorites: db.count_favorites()?,
            })
        })
        .map_err(|e: pictkura_core::DbError| e.to_string())
}

/// 指定ルートだけを走査して差分反映する（取り込み直後用）。
/// 走査しなかったルートはok_rootsに入らないため、その配下のレコードは削除されない。
fn scan_and_apply_root(state: &AppState, root: &Path) -> Result<SyncStats, String> {
    let _scan_guard = lock_ok(&state.scan_lock);
    let config = lock_ok(&state.config).clone();
    let scan = pictkura_core::LibraryScan {
        outcome: pictkura_core::scanner::scan_roots_pruned(
            std::slice::from_ref(&root.to_path_buf()),
            &config.import.extensions,
            &config.library.exclude_patterns,
            &HashMap::new(), // 取り込み直後は全列挙（対象は1ルートだけなので軽い）
        ),
        roots: config.library.roots.clone(),
    };
    let mut db = Db::open(&state.db_path).map_err(|e| e.to_string())?;
    let stats = pictkura_core::apply_scan(&mut db, &scan).map_err(|e| e.to_string())?;
    enqueue_missing_thumbs(state);
    Ok(stats)
}

/// ライブラリを再スキャンして差分をDBへ反映する。
/// 走査はブロッキングI/Oなので専用スレッドで実行し、非同期ランタイムを塞がない。
#[tauri::command]
async fn sync_now(app: tauri::AppHandle) -> Result<SyncStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        scan_and_apply(&state, true).map(Into::into)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// お気に入り（★）を設定する。
#[tauri::command]
fn set_favorite(state: tauri::State<'_, AppState>, id: i64, favorite: bool) -> Result<(), String> {
    lock_ok(&state.db)
        .set_favorite(id, favorite)
        .map_err(|e| e.to_string())
}

/// 可視領域のIDをサムネイル生成キューへオンデマンド投入し、最優先へ引き上げる。
/// 段階B-3: 高品質サムネイルはこの経路でのみ生成される（全件事前生成の廃止）。
/// LRU削除済み（state=0へ戻ったもの）もここで再生成される。
#[tauri::command]
fn set_visible_priority(state: tauri::State<'_, AppState>, ids: Vec<i64>) {
    let need: Vec<i64> = state.read_pool.with(|db| {
        ids.into_iter()
            .filter(|&id| {
                db.get_by_id(id)
                    .ok()
                    .flatten()
                    .is_some_and(|r| r.thumb_state < 2)
            })
            .collect()
    });
    if !need.is_empty() {
        state.thumbs.enqueue(&need);
        state.thumbs.prioritize(&need);
    }
}

/// ライブラリのルートフォルダを追加して保存し、即スキャンする。
#[tauri::command]
async fn add_library_root(app: tauri::AppHandle, path: String) -> Result<SyncStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Err(format!("フォルダが見つかりません: {path}"));
        }
        // 走査は `depth 0` を除外判定しないので、パッケージ（やその中）を
        // ルートに登録すると内部ファイルが索引される。しかも監視とUSNは
        // `is_excluded_path` が全構成要素を見るため更新イベントを全部落とす
        // ——**索引されるのに永久に古いまま**という壊れた状態になる。
        // 取り込み側と同じ理由で断る
        if pictkura_core::import::is_managed_package_path(&root) {
            return Err(format!(
                "アプリが管理するライブラリはフォルダとして登録できません（中身は内部ファイルです）: {path}"
            ));
        }
        update_config(&state, |c| {
            if !c.library.roots.iter().any(|r| same_path(r, &root)) {
                c.library.roots.push(root.clone());
            }
        })?;
        rebuild_watcher(&app);
        scan_and_apply(&state, false).map(Into::into)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// ライブラリのルートフォルダを削除して保存し、DBからも配下のレコードを消す。
#[tauri::command]
async fn remove_library_root(app: tauri::AppHandle, path: String) -> Result<SyncStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let root = PathBuf::from(&path);
        update_config(&state, |c| {
            c.library.roots.retain(|r| !same_path(r, &root));
        })?;
        rebuild_watcher(&app);
        // 再スキャンすると、どのルートにも属さなくなったレコードが削除される
        scan_and_apply(&state, false).map(Into::into)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 現在の設定をJSONで返す（デバッグ・設定画面用）。
#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let config = lock_ok(&state.config);
    serde_json::to_value(&*config).map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
struct ImportStatsDto {
    copied: usize,
    skipped: usize,
    failed: usize,
    /// 取り込み元の走査でエラーがあった（取りこぼしの可能性）
    scan_incomplete: bool,
}

#[derive(serde::Serialize, Clone)]
struct ImportProgress {
    done: usize,
    total: usize,
    /// いまコピーしたファイル（UIが「今これを入れています」を出すため）
    path: String,
}

#[derive(serde::Serialize)]
struct DriveDto {
    label: String,
    path: String,
    removable: bool,
    /// ドライブの種類: "removable" / "fixed" / "network" / "optical" / "other"。
    /// ネットワークドライブを取り込み元として勝手に走査しないための区別
    kind: &'static str,
    /// DCIMフォルダを持つ（カメラメディアの可能性が高い）
    has_dcim: bool,
}

/// マウント位置からドライブの種類を判定する。
///
/// `sysinfo` は removable かどうかしか教えてくれないので、Windowsでは
/// `GetDriveTypeW` を直接呼ぶ。ネットワークドライブ（`DRIVE_REMOTE`）を
/// 取り込み元として自動走査すると、回線越しに数万ファイルを舐めてしまう。
/// なお OneDrive や iCloud Drive は**ドライブではなくCドライブ上のフォルダ**なので
/// ここでは判別できない（実体の有無はファイル属性で見る: `browse::SourceFile::offline`）。
#[cfg(windows)]
fn drive_kind(mount: &Path, removable: bool) -> &'static str {
    use std::os::windows::ffi::OsStrExt;
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    let mut wide: Vec<u16> = mount.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: NUL終端したUTF-16のパスを渡すだけ。副作用のない問い合わせAPI
    let kind = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
    match kind {
        DRIVE_REMOVABLE => "removable",
        DRIVE_FIXED => "fixed",
        DRIVE_REMOTE => "network",
        DRIVE_CDROM => "optical",
        DRIVE_RAMDISK => "other",
        // 判定できないときは sysinfo の申告に従う
        _ if removable => "removable",
        _ => "other",
    }
}

/// Windows以外は種別APIが共通化されていないため、removableの申告だけを使う。
#[cfg(not(windows))]
fn drive_kind(_mount: &Path, removable: bool) -> &'static str {
    if removable {
        "removable"
    } else {
        "fixed"
    }
}

/// 接続中のドライブ一覧を返す。フロントがポーリングしてUSB挿入を検知する。
#[tauri::command]
fn list_drives() -> Vec<DriveDto> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut seen = std::collections::HashSet::new();
    let mut drives: Vec<DriveDto> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let mount = d.mount_point().to_path_buf();
            if !seen.insert(mount.clone()) {
                return None;
            }
            let name = d.name().to_string_lossy().into_owned();
            let letter = mount
                .to_string_lossy()
                .trim_end_matches(['\\', '/'])
                .to_string();
            let label = if name.is_empty() {
                letter
            } else {
                format!("{name} ({letter})")
            };
            let kind = drive_kind(&mount, d.is_removable());
            Some(DriveDto {
                label,
                path: mount.to_string_lossy().into_owned(),
                removable: d.is_removable() || kind == "removable",
                kind,
                // ネットワークドライブの有無確認は待たされることがあるので触らない
                has_dcim: kind != "network" && mount.join("DCIM").is_dir(),
            })
        })
        .collect();
    // マウント位置（Windowsならドライブレター）順に並べる。OSの列挙順のままだと
    // C: D: E: の並びが起動ごとに入れ替わって見え、目的のドライブを探しにくい
    drives.sort_by_key(|d| d.path.to_uppercase());
    drives
}

/// 取り込みのコピー先ルートを設定して保存する。
#[tauri::command]
fn set_import_destination(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let dest = PathBuf::from(&path);
    if !dest.is_dir() {
        return Err(format!("フォルダが見つかりません: {path}"));
    }
    // コピー先は取り込み後に**ライブラリのルートへ足される**（`finish_import`）ので、
    // ルート登録と同じ理由でパッケージの中は断る
    if pictkura_core::import::is_managed_package_path(&dest) {
        return Err(format!(
            "アプリが管理するライブラリはコピー先にできません（中身は内部ファイルです）: {path}"
        ));
    }
    update_config(&state, |c| c.routing.destination = Some(dest))
}

/// 取り込みの進捗通知。少件数はそのまま、大量時は0.5%刻みにまとめる。
///
/// UIはこのイベントで**いま入れている写真のサムネイル**も出すので、
/// 「動いているのが見える」程度には細かく送る（最大200回）。
/// 送る側でパスの置き場所をプレビュー配信の許可に加える
/// （ツリーで開いていないフォルダのファイルでも、今まさに取り込んでいる
/// ものは見せてよい）。
fn emit_import_progress(app: &tauri::AppHandle, done: usize, total: usize, path: &Path) {
    let step = (total / 200).max(1);
    if !done.is_multiple_of(step) && done != total {
        return;
    }
    if let Some(parent) = path.parent() {
        let state = app.state::<AppState>();
        let mut allow = lock_ok(&state.browse_allow);
        if !allow.contains(parent) {
            allow.insert(parent.to_path_buf());
        }
    }
    let _ = app.emit(
        "import-progress",
        ImportProgress {
            done,
            total,
            path: path.to_string_lossy().into_owned(),
        },
    );
}

/// 取り込み後の後始末: 取り込み元を記憶し、コピー先をライブラリへ反映する。
/// フォルダ丸ごと（[`import_from_folder`]）と選択取り込み（[`import_paths`]）で共通。
fn finish_import(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    source: PathBuf,
    dest: Option<PathBuf>,
) -> Result<(), String> {
    update_config(state, |c| {
        c.import.last_source_dir = Some(source);
        if let Some(dest) = dest.clone() {
            if !c.library.roots.iter().any(|r| same_path(r, &dest)) {
                c.library.roots.push(dest);
            }
        }
    })?;

    // コピー先ルートだけを走査してグリッドへ反映（全ルート再走査を避ける）
    if let Some(dest) = &dest {
        rebuild_watcher(app); // コピー先がルートに追加された可能性がある
        let sync_stats = scan_and_apply_root(state, dest)?;
        let _ = app.emit("library-updated", SyncStatsDto::from(sync_stats));
    }
    Ok(())
}

/// USB等のフォルダから取り込み（コピー）を実行し、コピー先をライブラリへ反映する。
/// 進捗は `import-progress` イベントで通知する。
/// コピーは長時間のブロッキングI/Oなので専用スレッドで実行する。
#[tauri::command]
async fn import_from_folder(
    app: tauri::AppHandle,
    source: String,
) -> Result<ImportStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let source = PathBuf::from(&source);
        let config = lock_ok(&state.config).clone();
        // コピー先はこのスナップショットのものを最後まで使う
        // （コピー中に設定が変わっても、実際にコピーした場所を登録する）
        let dest = config.routing.destination.clone();

        let progress_app = app.clone();
        let stats = pictkura_core::import_from(&source, &config, move |done, total, path| {
            emit_import_progress(&progress_app, done, total, path);
        })
        .map_err(|e| e.to_string())?;

        finish_import(&app, &state, source, dest)?;

        Ok(ImportStatsDto {
            copied: stats.copied,
            skipped: stats.skipped,
            failed: stats.failed,
            scan_incomplete: stats.scan_incomplete,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 取り込みウィザードのフォルダ閲覧（第5部 段階E）。
///
/// USBの読み出しは遅いので、コマンド自体をブロッキングプールへ逃がす
/// （同期コマンドはメインスレッドで走り、UI全体が固まるため）。
#[tauri::command]
async fn list_source_dir(
    app: tauri::AppHandle,
    path: String,
) -> Result<pictkura_core::browse::DirListing, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let dir = PathBuf::from(&path);
        // **取り込み元はネイティブのフォルダ選択ダイアログで選ぶ**ので、
        // 一覧から隠しても写真.appのライブラリそのものを名指しで選べてしまう。
        // 中身はUUID名の内部ファイルなので、開かずに理由を返す
        if pictkura_core::import::is_managed_package_path(&dir) {
            return Err(pictkura_core::ImportError::SourceIsManagedPackage(dir).to_string());
        }
        let extensions = lock_ok(&state.config).import.extensions.clone();
        let listing = pictkura_core::browse::list_dir(&dir, &extensions);
        // 実際に開けたフォルダだけをプレビュー配信の許可に加える
        if !listing.unreadable {
            lock_ok(&state.browse_allow).insert(dir);
        }
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 取り込み元の**下の階層まで**画像を集める（第5部 段階E-5）。
///
/// 「カードのどこに写真が入っているか分からない」人のための経路。
/// メディアを選んだだけで中の写真が全部並ぶ。上限を超えたら打ち切って申告する
/// （返した分のプレビューは配信できるよう、含まれるフォルダを許可に加える）。
#[tauri::command]
async fn list_source_tree(
    app: tauri::AppHandle,
    path: String,
) -> Result<pictkura_core::browse::TreeListing, String> {
    /// 一度に並べる上限。これ以上は人が見て選ぶ枚数ではないので、
    /// 「フォルダごと取り込む」に誘導する
    const TREE_LIMIT: usize = 20_000;

    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let dir = PathBuf::from(&path);
        // `list_source_dir` と同じ理由（ネイティブのダイアログで名指しできる）。
        // こちらは**下の階層まで**集めるので、通すと内部の派生画像が
        // そのまま選択候補として並ぶ
        if pictkura_core::import::is_managed_package_path(&dir) {
            return Err(pictkura_core::ImportError::SourceIsManagedPackage(dir).to_string());
        }
        let extensions = lock_ok(&state.config).import.extensions.clone();
        let listing = pictkura_core::list_tree(&dir, &extensions, TREE_LIMIT);
        {
            // 走査で見つけたファイルの置き場所だけをプレビュー配信の許可に加える
            let mut allow = lock_ok(&state.browse_allow);
            allow.insert(dir);
            for file in &listing.files {
                if let Some(parent) = file.path.parent() {
                    if !allow.contains(parent) {
                        allow.insert(parent.to_path_buf());
                    }
                }
            }
        }
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 渡したファイルが既にコピー先へ取り込み済みかを1件ずつ返す（第5部 段階E）。
///
/// 一覧表示とは別コマンドにしてある: EXIF読み＋コピー先の存在確認は
/// USBだと1件あたり数msかかるので、**サムネイルを先に出してから**
/// 「済」バッジを後追いで塗る。一覧が出るまでの待ちを増やさないため。
#[tauri::command]
async fn probe_imported(app: tauri::AppHandle, paths: Vec<String>) -> Result<Vec<bool>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let config = lock_ok(&state.config).clone();
        paths
            .iter()
            .map(|p| pictkura_core::is_already_imported(Path::new(p), &config))
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// ウィザードで選んだファイルだけを取り込む（第5部 段階E）。
/// 進捗・後処理は [`import_from_folder`] と同じ経路を通る。
#[tauri::command]
async fn import_paths(
    app: tauri::AppHandle,
    paths: Vec<String>,
    source_dir: String,
) -> Result<ImportStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let config = lock_ok(&state.config).clone();
        let dest = config.routing.destination.clone();
        let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();

        let progress_app = app.clone();
        let stats = pictkura_core::import_files(&files, &config, move |done, total, path| {
            emit_import_progress(&progress_app, done, total, path);
        })
        .map_err(|e| e.to_string())?;

        finish_import(&app, &state, PathBuf::from(&source_dir), dest)?;
        Ok(ImportStatsDto {
            copied: stats.copied,
            skipped: stats.skipped,
            failed: stats.failed,
            scan_incomplete: stats.scan_incomplete,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 取り込み元ファイルのプレビューを返す（第5部 段階E）。
///
/// ライブラリ外の任意パスを読むことになるため、**UIが開いたフォルダの直下**に
/// あるファイルしか配信しない。Webview側のスクリプトが細工したURLで
/// `C:\Windows\...` を読み出せる、という穴を塞ぐ。
fn source_preview_response(state: &AppState, path: &Path) -> Response<Vec<u8>> {
    let denied = || {
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Vec::new())
            .unwrap()
    };
    let Some(parent) = path.parent() else {
        return denied();
    };
    if !lock_ok(&state.browse_allow).contains(parent) {
        return denied();
    }
    let thumb_size = lock_ok(&state.config).performance.thumbnail_size;
    match pictkura_core::browse::preview(path, thumb_size) {
        Some(preview) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", preview.mime)
            // URLに ?v=<mtime> が付くので、内容が変わったら別URLになる
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(preview.bytes)
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap(),
    }
}

/// 1x1の透明PNG（67バイト）。
///
/// サムネイルが出せないセルへ返す。404を返すとWebViewが**壊れた画像アイコン**を
/// 描いてしまい（`alt=""` でも消えない）、一覧が割れた写真だらけに見える。
/// 透明な絵を返せばCSSの背景がそのまま出て、生成待ちのセルと同じ見た目になる。
const BLANK_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// 透明な1x1を返す（キャッシュさせない: 次の要求までにサムネイルが出来ている）。
fn blank_thumb() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "no-store")
        .body(BLANK_PNG.to_vec())
        .unwrap()
}

/// 1回の部分応答で返す最大バイト数（4MiB）。
///
/// `<video>` は `Range: bytes=0-`（末尾まで）を投げてくるが、言われたとおりに
/// 2GBの.m2tsを丸ごと載せたらメモリが飛ぶ。**要求より少なく返してよい**のが
/// Rangeの規則なので、ここで刻む。足りない分はWebViewが続きを取りに来る。
const RANGE_CHUNK: u64 = 4 * 1024 * 1024;

/// Rangeヘッダが無いときに丸ごと返して良い上限（64MiB）。
/// これを超えるファイルは先頭だけを206で返す。
const WHOLE_FILE_LIMIT: u64 = 64 * 1024 * 1024;

/// 動画の実体を返す（第9部）。**Rangeリクエストに応える**のがここの仕事。
///
/// 206を返せないと `<video>` はシークを諦め、長い動画では再生自体が始まらない。
/// 同時に「ファイルを丸ごとメモリへ読む」問題も消える——読むのは要求された区間の
/// 先頭 [`RANGE_CHUNK`] バイトだけで、2GBの動画でも常駐は4MiBで済む。
fn video_response(path: &Path, range: Option<&str>, mime: &str) -> Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let fail = |status: StatusCode| Response::builder().status(status).body(Vec::new()).unwrap();
    // クラウドにしか実体が無いファイルは**開かない**。開いた瞬間に
    // ハイドレート（全体のダウンロード）が始まり、数GBの動画なら
    // 何分もの間このスレッドが刺さったまま、UIには黒い枠しか出ない。
    // UIは `video_status` で先に気付いて案内を出すが、直接叩かれても守る
    if pictkura_core::cloud::is_cloud_only_path(path) {
        return fail(StatusCode::SERVICE_UNAVAILABLE);
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return fail(StatusCode::NOT_FOUND);
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return fail(StatusCode::NOT_FOUND);
    };
    let mut read_at = |start: u64, end: u64| -> Option<Vec<u8>> {
        let count = end - start + 1;
        let mut buf = Vec::with_capacity(count as usize);
        file.seek(SeekFrom::Start(start)).ok()?;
        std::io::Read::by_ref(&mut file)
            .take(count)
            .read_to_end(&mut buf)
            .ok()?;
        // 読めた量が足りなければ**返さない**。`Content-Range` は読む前の
        // ファイル長で書くので、途中でファイルが縮む（クラウドの同期・
        // 書き出しで上書き）と長さが食い違い、WebViewにはプロトコル違反として
        // 見える。素直に404にした方が原因が分かる
        (buf.len() as u64 == count).then_some(buf)
    };

    let (start, end) =
        match pictkura_core::protocol::plan_range(range, len, RANGE_CHUNK, WHOLE_FILE_LIMIT) {
            pictkura_core::protocol::RangeReply::Partial { start, end } => (start, end),
            pictkura_core::protocol::RangeReply::Whole => {
                return match read_at(0, len.saturating_sub(1)) {
                    Some(bytes) => Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", mime)
                        .header("Accept-Ranges", "bytes")
                        .header("Content-Length", bytes.len())
                        .header("Cache-Control", "max-age=31536000, immutable")
                        .body(bytes)
                        .unwrap(),
                    None => fail(StatusCode::NOT_FOUND),
                }
            }
            // 満たせない要求には416と「本当の長さ」を返す。これでWebViewが
            // 正しい範囲を計算し直せる
            pictkura_core::protocol::RangeReply::Unsatisfiable => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("Content-Range", format!("bytes */{len}"))
                    .body(Vec::new())
                    .unwrap()
            }
        };

    match read_at(start, end) {
        Some(bytes) => Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header("Content-Type", mime)
            .header("Accept-Ranges", "bytes")
            .header("Content-Range", format!("bytes {start}-{end}/{len}"))
            .header("Content-Length", bytes.len())
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(bytes)
            .unwrap(),
        None => fail(StatusCode::NOT_FOUND),
    }
}

/// サムネイル未生成のとき、原本を一覧へ出して良い最大の辺（ピクセル）。
///
/// 既定のサムネイル（512px）の4倍。設定値から計算したくなるが、
/// **この関数は配信のたびに通る**ので設定のロックを取ってはいけない
/// （起動直後のスキャンがそのロックを持っている間、一覧が丸ごと止まる。実測で踏んだ）。
const MAX_ORIGINAL_THUMB_EDGE: i64 = 2048;

/// サムネイル未生成のとき、元ファイルをそのまま一覧へ出して良いか。
///
/// 条件は3つ:
///
/// 1. **WebViewが描ける形式**（RAW・HEIC・TIFFは描けない）
/// 2. **実体がローカルにある**（クラウドのみのファイルは読むとダウンロードが走る）
/// 3. **一覧へ出せる大きさ**
///
/// 3つ目が要る理由は実測で分かった。AV1のデコーダが無い環境のAVIFは
/// サムネイルを作れないので原本を配ることになるが、原本は 4032x3024・1.11MB
/// （サムネイルは平均49KB）で、ブラウザは**タイルごとに1200万画素を展開する**。
/// 40枚並べたら半分のタイルが最後まで描かれなかった（画像デコードの予算切れ）。
/// 大きすぎるものは配らず、空の枠のままにする方が速くて予測できる。
fn can_serve_original(record: &pictkura_core::MediaRecord) -> bool {
    if pictkura_core::thumbs::needs_display_transcode(&record.path)
        || pictkura_core::cloud::is_cloud_only_path(&record.path)
    {
        return false;
    }
    // 動画は原本を代わりに配ってはいけない（第9部）。
    // `<img>` に動画を渡しても絵にならないうえ、ここは**ファイルを丸ごと
    // メモリへ読む**ので、2GBの.m2tsが可視領域に入っただけで落ちかねない。
    // サムネイルができるまでは空タイルを出す
    if pictkura_core::video::is_video_path(&record.path) {
        return false;
    }
    // ベクタは「原寸」が無く、ブラウザがタイルの大きさで描くので上限を掛けない
    if pictkura_core::svg::is_svg_path(&record.path) {
        return true;
    }
    // 寸法が未確認のうちは今までどおり配る（最初の一覧をすぐ出すため）
    let Some(longest) = record.width.zip(record.height).map(|(w, h)| w.max(h)) else {
        return true;
    };
    longest <= MAX_ORIGINAL_THUMB_EDGE
}

/// `media://` プロトコルのハンドラ。IDからパスを引き、ファイルバイナリをそのまま返す。
///
/// `range` は要求の `Range` ヘッダ（動画の部分要求。画像では使わない）。
fn handle_media_request(state: &AppState, url: &str, range: Option<&str>) -> Response<Vec<u8>> {
    let not_found = || {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap()
    };

    let target = match parse_media_url(url) {
        Some(t) => t,
        None => return not_found(),
    };
    let (kind, id) = match target {
        MediaTarget::Library { kind, id } => (kind, id),
        // 取り込み元のプレビュー（第5部 段階E）: DBを通らずファイルから直接作る
        MediaTarget::Source { path } => return source_preview_response(state, &path),
    };
    let record = match state.read_pool.with(|db| db.get_by_id(id)) {
        Ok(Some(r)) => r,
        _ => return not_found(),
    };
    // 高品質サムネイルの配信はLRUのタッチとして記録する（メモリ集約→定期フラッシュ）
    if kind == MediaKind::Thumb && record.thumb_state == 2 {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        lock_ok(&state.thumb_touches).insert(id, now_ms);
    }
    // thumbはサムネイル生成済みならそれを、なければオリジナルへフォールバック。
    // ただしフォールバックして良いのは「WebViewがそのまま描けて、実体が手元にある」
    // ファイルだけ。
    //
    // - HEIC/RAW を返してもWebViewは描けず、MIMEも octet-stream になる。
    //   壊れた枠が残るだけなので出さない（生成が済めば次のURLで出る）
    // - クラウドにしか実体が無いファイルは、**読んだ瞬間にダウンロードが走る**。
    //   サムネイル生成を後回しにした意味が消えるうえ、可視セルごとに毎回起きる
    let is_video = pictkura_core::video::is_video_path(&record.path);
    // 動画の再生（第9部）。ここだけが原本を丸ごと相手にする経路で、
    // Rangeで刻んで返す。WebViewが扱えないコンテナ（.m2ts/.avi）は渡しても
    // 黒い枠になるだけなので、415を返してUIの「既定のアプリで開く」へ寄せる
    if kind == MediaKind::Video {
        if !is_video {
            return not_found();
        }
        if !pictkura_core::video::plays_in_webview(&record.path) {
            return Response::builder()
                .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                .body(Vec::new())
                .unwrap();
        }
        return video_response(&record.path, range, mime_for_path(&record.path));
    }
    // 動画の**絵**の要求は原本へ流さない。`<img>` に動画を渡しても絵にならず、
    // 下の `std::fs::read` はファイルを丸ごとメモリへ載せる。
    // サムネイル（あれば）で代用し、無ければ空タイルを出す
    let path = match kind {
        MediaKind::Thumb | MediaKind::Full if is_video => match record.thumb_path {
            Some(thumb) => thumb,
            None => return blank_thumb(),
        },
        MediaKind::Video => unreachable!("上で返している"),
        MediaKind::Thumb => match record.thumb_path {
            Some(thumb) => thumb,
            None if !can_serve_original(&record) => return blank_thumb(),
            None => record.path,
        },
        MediaKind::Full => record.path,
    };

    // WebViewが描けない形式の原寸表示: RAW（第6部 段階F）・HEIC（第7部 段階G）・
    // TIFF。いずれも原本をそのまま返しても絵にならないのでJPEGへ詰め直す。
    // AVIF・SVG・BMP・GIF はブラウザが直接描けるので、この下で原本を返す
    if kind == MediaKind::Full && pictkura_core::thumbs::needs_display_transcode(&path) {
        return match pictkura_core::thumbs::display_jpeg(&path) {
            Some(bytes) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "image/jpeg")
                .header("Cache-Control", "max-age=31536000, immutable")
                .body(bytes)
                .unwrap(),
            None => not_found(),
        };
    }

    match std::fs::read(&path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime_for_path(&path))
            // URL側に ?v=<mtime>-<thumb有無> のバージョンが付くため、immutableでも
            // 内容変更時は別URLになり古いキャッシュを掴み続けない
            .header("Cache-Control", "max-age=31536000, immutable")
            .body(bytes)
            .unwrap(),
        Err(_) => not_found(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instanceは**最初のプラグイン**でなければならない（Tauriの規約）。
        // USB挿入の自動起動で2重に立ち上がっても、新しい窓を開かず動作中の
        // インスタンスへ引数（`--import <ドライブ>`）を渡してウィザードを開く
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_second_instance(app, &argv);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            std::fs::create_dir_all(&data_dir)?;

            let config_path = config_dir.join("pictkura.toml");
            let first_launch = !config_path.exists();
            let mut config = Config::load(&config_path)?;
            // 初回起動時のみ、OS標準の写真フォルダをデフォルトのライブラリに登録する
            // （ユーザーが後から消した場合に勝手に復活させないため、初回限定）
            if first_launch {
                if let Ok(pictures) = app.path().picture_dir() {
                    if pictures.is_dir() {
                        config.library.roots.push(pictures);
                        config.save(&config_path)?;
                    }
                }
            }
            let db_path = data_dir.join("pictkura.db");
            let db = Db::open(&db_path)?;
            // 読み取り接続プール（段階B-4）。本数はコア数の半分・2〜4本で十分
            // （読み取りは短時間で返り、WALによりライターと並行に動ける）
            let pool_size = std::thread::available_parallelism()
                .map(|n| (n.get() / 2).clamp(2, 4))
                .unwrap_or(2);
            let read_pool = ReadPool::open(&db_path, pool_size)?;

            // サムネイルワーカーを起動。1件完了ごとに**更新後のレコードだけ**を
            // フロントへpushする（全件再取得のイベントの嵐を防ぐ）
            let thumb_handle = app.handle().clone();
            let thumbs = ThumbnailService::start(
                db_path.clone(),
                data_dir.join("thumbs"),
                config.performance.thumbnail_size,
                config.performance.worker_threads,
                move |id| {
                    let state = thumb_handle.state::<AppState>();
                    let record = state.read_pool.with(|db| db.get_by_id(id)).ok().flatten();
                    if let Some(record) = record {
                        let _ = thumb_handle.emit("media-updated", MediaItemDto::from(record));
                    }
                },
            );

            let thumb_cache_mb = config.performance.thumb_cache_mb;
            // AutoPlayの登録可否は設定から。configはこの後AppStateへ移すので先に読む
            let register_autoplay = config.import.register_autoplay;
            // 冷起動（AutoPlay/2重起動でこのプロセスが最初のとき）の取り込み対象
            let pending_import = import_path_from_args(&std::env::args().collect::<Vec<_>>());
            app.manage(AppState {
                db: Mutex::new(db),
                read_pool,
                db_path: db_path.clone(),
                config: Mutex::new(config),
                config_path,
                thumbs,
                scan_lock: Mutex::new(()),
                watcher: Mutex::new(None),
                thumb_touches: Mutex::new(HashMap::new()),
                startup_report: Mutex::new(None),
                index_progress: Mutex::new(IndexProgressDto::default()),
                browse_allow: Mutex::new(HashSet::new()),
                pending_import: Mutex::new(pending_import),
            });

            // USB/SDカードの自動再生（AutoPlay）に「pictkuraで取り込む」を候補として
            // 足す（Windowsのみ・HKCU・管理者不要）。既定は乗っ取らない。冪等なので
            // 起動のたびに書き直し、ポータブル版を移動しても実行ファイルのパスが追従する。
            // 失敗しても起動は続ける（レジストリが書けない環境でもアプリは動くべき）
            if let Ok(exe) = std::env::current_exe() {
                let result = if register_autoplay {
                    autoplay::register(&exe)
                } else {
                    autoplay::unregister()
                };
                if let Err(e) = result {
                    eprintln!("AutoPlayの登録に失敗（無視して継続）: {e}");
                }
            }

            // 検索インデックスの初期構築（第4部 段階D）。
            // 索引導入前に取り込んだ既存レコードを後追いで索引化する。
            // **起動は1msも待たせない**ためバックグラウンドで少しずつ進め、
            // バッチの合間に休ませてスキャンやサムネイル生成の邪魔をしない。
            // 構築中に増えた行（id > 上限）はトリガが直接索引化するので対象外
            let index_handle = app.handle().clone();
            let index_db_path = db_path.clone();
            // 掃き寄せの範囲は**ここで**（スキャンスレッドを起こす前に）確定させる。
            // スレッドの中で採ると、先に走り出した起動同期が入れた行が上限IDより
            // 小さくなり、トリガと掃き寄せの両方で索引に入ってしまう
            let (fts_cursor, fts_max_id) = lock_ok(&app.state::<AppState>().db)
                .fts_build_range()
                .unwrap_or((0, 0));
            std::thread::spawn(move || {
                let state = index_handle.state::<AppState>();
                let Ok(mut db) = Db::open(&index_db_path) else {
                    return;
                };
                let publish =
                    |phase: &str, done: i64, total: i64, building: bool, incomplete: bool| {
                        let progress = IndexProgressDto {
                            building,
                            incomplete,
                            phase: phase.into(),
                            done,
                            total,
                        };
                        *lock_ok(&state.index_progress) = progress.clone();
                        let _ = index_handle.emit("index-progress", progress);
                    };
                /// 一時的な失敗（他の接続との書き込み競合等）はこの回数まで待って再試行する。
                /// 諦めた場合もカーソルは進めないので、次回起動が続きから再開する
                const MAX_RETRY: u32 = 5;
                let mut incomplete = false;

                // 第1段: 全文索引の掃き寄せ。範囲（上限ID）は**スキャンスレッドを
                // 起動する前に**確定させてある（あとで採ると、起動同期が入れた行が
                // 上限より小さいIDで入り、トリガと掃き寄せの二重投入になりうる）
                if fts_cursor < fts_max_id {
                    publish("index", fts_cursor, fts_max_id, true, false);
                    let mut retry = 0;
                    loop {
                        match db.fts_build_step(fts_max_id, 2000) {
                            Ok((_, next)) => {
                                retry = 0;
                                if next >= fts_max_id {
                                    break;
                                }
                                publish("index", next, fts_max_id, true, false);
                                std::thread::sleep(std::time::Duration::from_millis(20));
                            }
                            Err(_) if retry < MAX_RETRY => {
                                retry += 1;
                                std::thread::sleep(std::time::Duration::from_millis(
                                    200 * retry as u64,
                                ));
                            }
                            Err(_) => {
                                incomplete = true;
                                break;
                            }
                        }
                    }
                }

                // 第2段: カメラの後追い補完。検索導入前に取り込んだレコードは
                // メタデータ抽出済みで再処理されないため、ここでEXIFヘッダだけを
                // 読んでカメラを埋める（画像デコードなし）
                let total = db.cameras_pending().unwrap_or(0);
                let mut done = 0i64;
                if total > 0 {
                    publish("camera", 0, total, true, incomplete);
                    let mut after_id = 0i64;
                    while let Ok(batch) = db.cameras_to_backfill(after_id, 200) {
                        if batch.is_empty() {
                            break;
                        }
                        after_id = batch.last().map(|(id, _)| *id).unwrap_or(after_id);
                        let results: Vec<(i64, Option<String>)> = batch
                            .into_iter()
                            // **開けなかったファイルは印を付けずに飛ばす**。
                            // 外付けドライブが未マウントの状態で起動すると、
                            // read_exif_info は「EXIFなし」と区別のつかない空を返す。
                            // それを「確認済み・カメラなし」として書くと、
                            // マウント後も二度と読み直されず永久に間違ったままになる
                            .filter(|(_, path)| path.is_file())
                            // **実体がクラウドにしか無いファイルも同じ扱い**（段階H）。
                            // ここは `width IS NOT NULL` で対象を選んでおり、段階Hで
                            // OSから寸法を借りるようにした結果、**クラウドのみの
                            // ファイルが丸ごとこの列に並ぶようになった**（実測3,165件）。
                            // read_exif_info はファイルを開くので、そのまま流すと
                            // ユーザーが何も要求していないのにライブラリ全体が
                            // ダウンロードされる。開ける日が来たときに読めばよい
                            .filter(|(_, path)| !pictkura_core::cloud::is_cloud_only_path(path))
                            .map(|(id, path)| {
                                (id, pictkura_core::thumbs::read_exif_info(&path).camera)
                            })
                            .collect();
                        done += results.len() as i64;
                        if !results.is_empty() && db.set_cameras(&results).is_err() {
                            incomplete = true;
                            break; // 印を付けていないので次回起動でやり直せる
                        }
                        publish("camera", done.min(total), total, true, incomplete);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    // 埋まったカメラを左ペインへ反映させる
                    let _ = index_handle.emit("cameras-updated", ());
                }
                publish("camera", total, total, false, incomplete);
            });

            // サムネイルキャッシュのジャニター（段階B-3）: 60秒ごとに
            // タッチ記録をDBへフラッシュし、上限容量超過分をLRU削除する。
            // 順序が安全の要: フラッシュ→選定（キュー内ID除外＋直近利用ガード）→
            // DB更新→ファイル削除。途中でクラッシュしても「state=0だがファイルが
            // 残っている」で済み、再生成時に同名パスへ上書きされる
            let janitor_handle = app.handle().clone();
            let janitor_db_path = db_path.clone();
            std::thread::spawn(move || {
                let cap_bytes = (thumb_cache_mb.min(8 * 1024 * 1024) as i64) * 1024 * 1024;
                let mut db: Option<Db> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    let state = janitor_handle.state::<AppState>();
                    let touches: Vec<(i64, i64)> = lock_ok(&state.thumb_touches).drain().collect();
                    if touches.is_empty() && cap_bytes == 0 {
                        continue;
                    }
                    if db.is_none() {
                        db = Db::open(&janitor_db_path).ok();
                    }
                    let Some(db) = db.as_mut() else { continue };
                    if !touches.is_empty() {
                        // 書き込み失敗（長時間の書き込みトランザクションとの競合等）で
                        // タッチを失うと、直近閲覧中のサムネイルほどLRUで削除されやすく
                        // なってしまう。失敗分はマップへ書き戻して次サイクルで再試行する
                        if db.touch_thumbs(&touches).is_err() {
                            let mut map = lock_ok(&state.thumb_touches);
                            for (id, used_ms) in touches {
                                map.entry(id)
                                    .and_modify(|v| *v = (*v).max(used_ms))
                                    .or_insert(used_ms);
                            }
                        }
                    }
                    if cap_bytes == 0 {
                        continue;
                    }
                    // 移行前（サイズ未記録）のサムネイルを少しずつ補完する。
                    // 補完しないとキャッシュ使用量に数えられず上限管理から漏れる
                    let _ = db.backfill_thumb_sizes(2000);
                    let exclude = state.thumbs.active_ids();
                    let Ok(victims) = db.evict_final_thumbs(cap_bytes, &exclude) else {
                        continue;
                    };
                    for (id, path) in victims {
                        // 削除の直前に再確認する: LRU選定（コミット済み）の後に
                        // オンデマンド再生成が走った行のファイルを消すと、
                        // 「state=2なのにファイルが無い」壊れた状態になり自然回復しない。
                        // キューに入り直した/再生成済みのIDはスキップする
                        // （スキップ分のファイルは同名パスへの再生成で上書きされる）
                        if state.thumbs.is_active(id) {
                            continue;
                        }
                        match db.get_by_id(id) {
                            Ok(Some(rec)) if rec.thumb_state == 2 => continue,
                            _ => {}
                        }
                        let _ = std::fs::remove_file(path);
                        // フロントの古い「高品質あり」表示を正す（未ロードの日ならno-op）
                        if let Ok(Some(rec)) = db.get_by_id(id) {
                            let _ = janitor_handle.emit("media-updated", MediaItemDto::from(rec));
                        }
                    }
                }
            });

            // ライブラリルートのファイルシステム監視を開始（アプリ外の操作に追従）
            rebuild_watcher(app.handle());

            // 起動時にバックグラウンドでライブラリを同期し、完了をフロントへ通知する。
            // 方式（USN差分/枝刈り/フル）と所要時間は⚡爆速メーターとして別途知らせる
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                let state = handle.state::<AppState>();
                let Ok((stats, method)) = startup_scan(&state) else {
                    return;
                };
                let elapsed_ms = started.elapsed().as_millis() as u64;
                let total = state.read_pool.with(|db| db.count()).unwrap_or(0);
                let (method_name, usn_records, dirty_dirs) = match method {
                    StartupMethod::Usn { records, dirty } => ("usn", records, dirty),
                    StartupMethod::Pruned => ("pruned", 0, 0),
                    StartupMethod::Full => ("full", 0, 0),
                };
                let report = StartupScanDto {
                    method: method_name.into(),
                    elapsed_ms,
                    added: stats.added,
                    changed: stats.changed,
                    removed: stats.removed,
                    usn_records,
                    dirty_dirs,
                    skipped_dirs: stats.skipped_dirs,
                    total,
                };
                *lock_ok(&state.startup_report) = Some(report.clone());
                let _ = handle.emit("library-updated", SyncStatsDto::from(stats));
                let _ = handle.emit("startup-scan-report", report);
            });
            Ok(())
        })
        // 非同期ハンドラで登録する: 同期ハンドラはメインスレッドを塞ぐため、
        // 取り込み元プレビュー（USBからの読み出し＋デコードで数十ms）が混ざると
        // スクロールが引っかかる。ブロッキングプールへ逃がして並行に返す
        .register_asynchronous_uri_scheme_protocol("media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().to_string();
            // 動画の部分要求（第9部）。`<video>` はシークのたびにこれを投げる
            let range = request
                .headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                responder.respond(handle_media_request(&state, &uri, range.as_deref()));
            });
        })
        .invoke_handler(tauri::generate_handler![
            timeline_summary,
            list_day,
            list_memories,
            get_stats,
            get_startup_report,
            sync_now,
            add_library_root,
            remove_library_root,
            get_config,
            set_visible_priority,
            set_import_destination,
            import_from_folder,
            list_drives,
            list_source_dir,
            list_source_tree,
            probe_imported,
            import_paths,
            set_favorite,
            list_cameras,
            get_exif_info,
            decoder_status,
            open_decoder_help,
            get_index_progress,
            video_status,
            open_default,
            reveal_in_folder,
            about_info,
            open_bundled_doc,
            open_with,
            forget_editor,
            delete_media,
            list_folder_patterns,
            set_folder_pattern,
            preview_folder_pattern,
            take_pending_import
        ])
        .run(tauri::generate_context!())
        .expect("Tauriアプリの起動に失敗");
}

#[cfg(test)]
mod tests {
    use super::import_path_from_args;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn autoplayの引数からドライブを取り出す() {
        // AutoPlayは `exe --import E:\` の形で渡す（argv[0]は実行ファイル）
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "E:\\"])),
            Some("E:\\".to_string())
        );
    }

    #[test]
    fn ドライブ文字だけならルートに直す() {
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "E:"])),
            Some("E:\\".to_string())
        );
    }

    #[test]
    fn importが無ければnone() {
        assert_eq!(import_path_from_args(&args(&["pictkura.exe"])), None);
    }

    #[test]
    fn importの後ろが無ければnone() {
        // 末尾に値が無い（壊れた呼び出し）ときにパニックしない
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import"])),
            None
        );
    }

    #[test]
    fn 値が空文字ならnone() {
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "   "])),
            None
        );
    }
}
