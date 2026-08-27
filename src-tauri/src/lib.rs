//! pictkura Tauriアプリ本体。
//!
//! `media://` カスタムプロトコルでSQLiteに登録された画像バイナリを
//! Webviewへ直接ストリーミングする（Base64禁止の原則）。
//! スキャンはDBロックの外で行い、走査中も画像配信を止めない。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use pictkura_core::protocol::{mime_for_path, parse_media_url, MediaTarget, ServeKind};
use pictkura_core::usn::{self, UsnOutcome, UsnPosition};
use pictkura_core::{Config, Db, ReadPool, SyncStats, ThumbnailService};
use tauri::http::{Response, StatusCode};
use tauri::{Emitter, Manager};

/// アプリの識別子。`tauri.conf.json` の `identifier` と**同じ綴り**でなければ、
/// [`config_path_without_app`] が別の場所を指す（テストで突き合わせている）。
///
/// **Windows専用**——使うのは AutoPlay まわりだけなので、他のOSでは
/// 「使われていない定数」になり clippy の `dead_code` で落ちる（CIで踏んだ）
#[cfg(windows)]
const APP_IDENTIFIER: &str = "dev.harusame.pictkura";

mod autoplay;
// 新しい版が出ていないかの確認（0.2）。外向きの通信はこのモジュールに閉じている
mod update;

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
    /// 起動時の同期が**終わったか**（成功・失敗・パニックのどれでも真）。
    ///
    /// `startup_report` では代わりにならない: 走査が `Err` で早く返ったときや
    /// パニックを `panics::catching` が拾ったときは、報告が**一度も出ない**。
    /// フロントは「走査が続いている間は空だと言わない」ので、**終わったことを
    /// 別に知らせないと永久に無言**になる（ゲート2の指摘）。
    /// イベントを取りこぼしたフロントが後から聞けるよう、旗としても置く
    startup_done: std::sync::atomic::AtomicBool,
    /// ルートごとに走っている「空の理由」の探り（[`empty_library_reason`]）。
    ///
    /// **1本ずつ独立して探る。** 走査はブロッキングプールで動くが、
    /// **切れたSMB/NFSでは `read_dir` が返ってこない**——実測で、応答を止めた
    /// サーバに対して120秒待っても返らなかった（`metadata` の方はキャッシュで
    /// 即座に「在る」と答える）。フロントは5秒で見切るのに、スレッドはそのまま
    /// 残る。**ルートをまとめて直列に見ていた頃**は、刺さった1本の後ろにある
    /// `~/Pictures` まで巻き添えになり、一覧が空になるたびに聞き直すので
    /// プールを食い尽くした。
    ///
    /// **同じルートを2度は探らない。** 走っているものがあれば、後から来た
    /// 問い合わせは受け口を複製して**相乗りする**。StrictModeの二重マウントの
    /// ように2本が必ず重なる場面でも、使うスレッドはルート1本につき1つ。
    /// **旗と前回値で代用しない**——待つ側はタスクが止まるだけでスレッドを
    /// 食わないので、2本目も**自分で本当の答えを受け取れる**（前の版で
    /// 「前の答え」を配ると本当の理由が一度も出ない筋があった。ゲート2の指摘）。
    ///
    /// **返ってこなかったルートには印が残る。** 探りが終われば
    /// [`ProbeGuard`] が消すので、**消えていない＝まだ返っていない**。
    /// 刺さったルートはここに残り続け、二度と触らない——刺さりが食う
    /// スレッドは**そのルートにつき1本きり**で、後ろの健康なルートは毎回
    /// ちゃんと答える。長く返らないものは「確かめている途中」ではなく
    /// **「応答がない」と名指しする**（[`EmptyLibraryDto::stalled`]）——
    /// 探りを諦めた以上、「途中です」は嘘になる。
    ///
    /// **積み上がる方にも蓋がいる。** 死んだ共有が何本もあれば、相乗りが
    /// 効いていてもその数だけスレッドが減る（[`MAX_STUCK_PROBES`]）。
    /// **外して足し直したときは印を捨てる**（[`forget_probes_for_changed_roots`]）
    /// ——貼り直した共有を一度も見ずに「応答がありません」と言い続けないため。
    /// `plan.macos.md` の2ゲートの節に経緯がある
    root_probes: RootProbes,
    /// **直前の走査が開けなかった場所**（[`empty_library_reason`] が借りる）。
    ///
    /// あちらはルートの直下しか読まない（速さが仕様の走査路を太らせないため）。
    /// そのぶん、**奥の1フォルダだけ許可が無い**形が見えない——走査はそこへ
    /// 降りて `had_error` を立てているのに、説明側は候補として数えて
    /// 「扱える画像がありません」に落ちる。写真は権限の壁の向こうに在るのに、
    /// **許可の話へ辿り着けない**（ゲート1の指摘）。
    ///
    /// **全ルートを走る走査だけが書き換える。** 取り込み直後の1ルートだけの
    /// 走査（`scan_and_apply_root`）で丸ごと置き換えると、他のルートで
    /// 分かっていたことが消える
    scan_unreadable: Mutex<ScanUnreadable>,
    /// 検索インデックスの初期構築の進捗（第4部 段階D）。既存ライブラリを
    /// 後追いで索引化している間だけ building=true になる
    index_progress: Mutex<IndexProgressDto>,
    /// 取り込みウィザードで**実際に開いたフォルダ**（第5部 段階E）。
    /// `media://src/...` はライブラリ外の任意パスを配信できてしまうため、
    /// 「UIがユーザー操作で開いたフォルダの直下」だけに配信先を限る。
    browse_allow: Mutex<HashSet<PathBuf>>,
    /// AutoPlayや2重起動の `--import <パス>` で渡された取り込み対象のうち、
    /// **まだフロントが受け取っていない**もの（起動レポートと同じ方式）。
    ///
    /// 冷起動ではリスナー登録より前に値が決まる。2重起動はイベントでも送るが、
    /// 起動途中やWebViewの再読み込み中は聞き手が居ないので、必ずここにも積む。
    /// 受け取れたフロントが `take_pending_import` で消すので、取りこぼしと
    /// 二重処理のどちらも起きない。
    pending_import: Mutex<Option<String>>,
    /// 原寸表示用JPEG（HEIC/RAW/TIFF）のバイト列LRU（0.2 ①）。
    ///
    /// WebViewが描けない形式だけがここを通る。実測でHEICは1枚0.6〜1秒かかり、
    /// ビューアで前後へ行き来するたびに払い直していた。バイト列は1枚3.29MBで、
    /// 同じ絵をデコード済み画素で持つ93MiBより30倍安い
    display_cache: pictkura_core::display_cache::DisplayCache,
}

/// 走査が開けなかった場所の控え（[`AppState::scan_unreadable`]）。
#[derive(Default, Clone)]
struct ScanUnreadable {
    /// 見せる名前（走査が控えた先頭3件）
    dirs: Vec<PathBuf>,
    /// 開けなかったフォルダの総数（**見せる件数より多いことがある**）
    total: usize,
}

/// 走査の結果から、開けなかった場所の控えを更新する。
///
/// **全ルートを走った走査のときだけ呼ぶこと**（`scan_and_apply_root` は
/// 1ルートしか見ていないので、ここで置き換えると他のルートの記録が消える）。
fn remember_unreadable(
    state: &AppState,
    outcome: &pictkura_core::PrunedScanOutcome,
    roots: &[PathBuf],
) {
    let mut kept = lock_ok(&state.scan_unreadable);
    *kept = merged_unreadable(&kept, outcome, roots);
}

/// 前回の控えと今回の走査を**混ぜる**（[`remember_unreadable`] の中身）。
///
/// **丸ごと置き換えてはいけない。** 開けなかったフォルダは `seen_dirs` に
/// 載らない（`scanner.rs` の `dir_error` の枝）ので、次回の枝刈り走査の
/// `known_dirs` にも入らない。ところが**親のmtimeは変わっていない**ので、
/// 親ごと飛ばされ、そのフォルダには**二度と足を踏み入れない**——
/// 今回の結果は空になり、正しかった控えを空で上書きする。
/// 結果、**2回目の起動から穴が元通り**になる（ゲート2が実測で示した）。
///
/// 残すのは「今回の走査が見直していない場所」だけ。見直した場所
/// （＝親を列挙した、あるいはルートそのもの）は今回の結果で置き換える
/// ——直した権限がいつまでも「開けません」と言われないため。
fn merged_unreadable(
    previous: &ScanUnreadable,
    outcome: &pictkura_core::PrunedScanOutcome,
    roots: &[PathBuf],
) -> ScanUnreadable {
    let enumerated: HashSet<&Path> = outcome
        .enumerated_dirs
        .iter()
        .map(|dir| dir.as_path())
        .collect();
    // **今回踏み直していない控え**だけ持ち越す
    let stale = |dir: &PathBuf| {
        // ルートは毎回 `walk_pruned` に入るので、必ず見直されている
        !roots.iter().any(|root| root == dir)
            && !dir
                .parent()
                .is_some_and(|parent| enumerated.contains(parent))
    };
    let mut dirs: Vec<PathBuf> = previous.dirs.iter().filter(|d| stale(d)).cloned().collect();
    for dir in &outcome.unreadable_dirs {
        if !dirs.contains(dir) {
            dirs.push(dir.clone());
        }
    }
    // **数は少なめに倒す。** 持ち越したぶんの「見せていない残り」は数え直せない
    // ので、名前にできる件数と今回の総数の大きい方を採る——「ほか N件」が
    // **在りもしない場所を指す**よりは、少なく言う方がまし
    let total = dirs.len().max(outcome.unreadable_total);
    ScanUnreadable { dirs, total }
}

/// 起動引数から取り込み対象のドライブ/フォルダを取り出す。
/// AutoPlay（`pictkura.exe --import E:\`）や2重起動で渡る。
fn import_path_from_args(argv: &[String]) -> Option<String> {
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        if a == "--import" {
            let mut p = it.next()?.trim().to_string();
            if p.is_empty() {
                return None;
            }
            // レジストリの verb は `"%L"` と囲んである（スペースを含むパスのため）。
            // ところが `%L` がドライブ直下だと `E:\` で終わるので、展開後は
            // `"E:\"` となり、末尾の `\"` がエスケープと解釈されて引用符が
            // 引数に残る（`E:"`）。ここで元のバックスラッシュへ戻す。
            if p.ends_with('"') {
                p.pop();
                p.push('\\');
            }
            // `E:` だけで来たらルートに直す（AutoPlayは通常 `E:\` を渡すが保険）。
            if p.ends_with(':') {
                p.push('\\');
            }
            return Some(p);
        }
    }
    None
}

/// 取り込みの起点を、あれば `DCIM` まで寄せる。
///
/// UIのドライブ一覧クリックは `DriveDto::dcim_path` をそのまま開く。AutoPlayは
/// ドライブ直下（`%L`）しか渡してこないので、ここで揃えないと**同じカードでも
/// 入口によって拾う範囲が変わる**——写真以外も入っているカードだと、ボリューム
/// 全体を深く走査して未取り込みの画像を軒並み選んでしまう。
fn narrow_to_dcim(path: String) -> String {
    let dcim = Path::new(&path).join("DCIM");
    if dcim.is_dir() {
        return dcim.display().to_string();
    }
    path
}

/// 既に起動中のインスタンスへ2重起動の引数が届いたとき（single-instance）。
/// 窓を前面に出し、取り込み対象があればウィザードを開くイベントを送る。
fn handle_second_instance(app: &tauri::AppHandle, argv: &[String]) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    if let Some(path) = import_path_from_args(argv) {
        let path = narrow_to_dcim(path);
        // 聞き手が居るとは限らない——起動途中（リスナー登録前）も、WebViewの
        // 再読み込み中もある。2つ目のプロセスは用が済んだら終了するので、
        // 落とすと**要求は永久に消える**。まず置き場へ積んでから通知し、
        // 受け取れたフロントが `take_pending_import` で消す、という順にする。
        // `try_state` なのは、macOS/Linux ではこのコールバックが `setup` の
        // `manage` より先に走りうるため（`state` だとパニックする）。
        if let Some(state) = app.try_state::<AppState>() {
            *lock_ok(&state.pending_import) = Some(path.clone());
        }
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
    // **綴りの照合表はここで1回だけ作る。** `root_specs` はルートごとに
    // `std::fs::canonicalize` を呼ぶので、イベントの束が来るたびに作り直すと、
    // **オフラインのNASが1つあるだけで**その束ごとに数秒〜数十秒止まる。
    // ルートが変わるのはこの関数が呼ばれるときだけなので、閉じ込めて持ち回る
    // （ゲート2の指摘）
    let specs = root_specs(&roots);
    let watcher = pictkura_core::watch::watch_roots(
        &roots,
        std::time::Duration::from_millis(800),
        move |paths| handle_fs_events(&event_app, &specs, paths),
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
///
/// **入口で綴りを設定ルートへ揃える**（USNの経路と同じ手当て）。notify が返すのは
/// OSがくれたパスで、**設定と綴りが違うことがある**——macOSの `/var` は
/// `/private/var` へのシンボリックリンクで、FSEvents は解決後を返す。
/// そのまま入れると害が2つ:
///
/// - 次の全走査が**綴りの合わない行を掃き出す**。掃き出された行は別のidで
///   入り直すので、**★や選別の印が消える**（実測: 監視中に足した1枚に★を付け、
///   次の起動で `id=2 favorite=0` になった）
/// - 下の除外の門は**設定の綴りが前提**の前方一致なので、綴りが違うと外れる
///   ——ルートがパッケージのとき、その中身が**監視の経路だけ**索引される
fn handle_fs_events(app: &tauri::AppHandle, specs: &[RootSpec], paths: Vec<std::path::PathBuf>) {
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

    // 照合表は監視を張ったときに1回だけ作ってある（`rebuild_watcher`）。
    // ここで作り直すと、束が来るたびに `canonicalize` を全ルートへ投げることになる。
    //
    // 揃えられなかったものは**そのまま通す**。notify は監視しているルートの
    // 下しか返さないので、ここへ来るのはルートが解決できない等の異常時だけ
    // ——落とすより今までどおりに扱う。ただし**黙って通すと、直したはずの
    // 「印が消える」がまた起きても気付けない**ので、記録だけは残す
    let paths: Vec<PathBuf> = paths
        .into_iter()
        .map(|p| {
            rebase_to_root_spelling(&p, specs).unwrap_or_else(|| {
                eprintln!(
                    "監視: ルート配下と照合できなかったので綴りをそのまま使う: {}",
                    p.display()
                );
                p
            })
        })
        .collect();

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
    // **ルートを触る口はここ1つ**なので、探りの印の見直しもここでやる
    // （[`forget_probes_for_changed_roots`]）。外して足し直した共有を
    // 一度も見ずに「応答がありません」と言い続けないため
    forget_probes_for_changed_roots(
        &state.root_probes,
        &config.library.roots,
        &updated.library.roots,
    );
    *config = updated;
    Ok(())
}

/// フロントエンドへ渡すメディア1件分のDTO。
/// 幅・高さはグリッドが描画前に枠を確保するために必須で渡す（未抽出時は0）。
#[derive(serde::Serialize, Clone)]
struct MediaItemDto {
    id: i64,
    file_name: String,
    /// **原本**の寸法。グリッドの枠確保に使う（未抽出時は0）
    width: i64,
    height: i64,
    /// **埋め込みプレビュー**の寸法。RAWで配るのはこの絵で、原本より小さいことが
    /// 多い（HDR PQのCR3は 6000x4000 に対して 1620x1080）。
    ///
    /// **null は「まだ確かめていない」**。RAW以外は配るのが原本そのものなので
    /// 常に null、RAWは確かめた時点で必ず入る（原本と同じ値のこともある）。
    /// どちらにせよ、**null なら `width`/`height` を使えばよい**
    ///
    /// ビューアは下敷きの大きさ・等倍の倍率・先読みの予算をこちらで決める
    /// （`ui/src/App.tsx` の `servedSize`）。原本で決めると、届いた絵と
    /// 大きさが合わずに**差し替えの瞬間に絵が縮む**
    preview_width: Option<i64>,
    preview_height: Option<i64>,
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
    /// 選別で選んだ印（⚑ Pick。0.2 ②）。★とは別の棚
    picked: bool,
    /// 動画の長さ（ミリ秒）。動画以外・未読取は null（第9部）
    duration_ms: Option<i64>,
    /// 動画か（一覧の▶バッジと、ビューアでプレイヤーを出すかの判断）
    is_video: bool,
    /// アプリ内で再生できるか。偽なら「既定のアプリで開く」へ逃がす
    /// （.m2ts/.avi はWebViewがコンテナごと相手にしない）
    plays_in_app: bool,
    /// 原寸表示にRust側の詰め直しが要るか（HEIC・RAW・TIFF）。
    ///
    /// 実測でHEICは1枚0.6〜1秒・TIFFは約300ms。ビューアはこれを見て
    /// 「先読みを1枚に絞る」「読み込み中と正直に出す」を決める（0.2 ①）。
    /// **判定は拡張子だけ**なのでファイルには触らない——`cloud_only` を
    /// ここに載せないのと違い、1件あたりの費用がゼロなのでDTOに載せてよい。
    ///
    /// **RAWは横位置なら18ms**（Canon CR3・24MP・`bench --display-dir` で実測）で
    /// JPEG並みに安い。それでも同じ枠に入れているのは、**向きが1でないRAW**が
    /// 埋め込みプレビューを起こして回して詰め直す経路（[`pictkura_core::thumbs::raw_display_jpeg`]）
    /// に落ち、24MPの再エンコードぶん桁が変わるため。拡張子だけでは見分けられない
    needs_transcode: bool,
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
            preview_width: r.preview_width,
            preview_height: r.preview_height,
            taken_at_ms: r.taken_at_ms.unwrap_or(r.mtime_ms),
            day_key: r.day_key,
            mtime_ms: r.mtime_ms,
            has_thumb: r.thumb_path.is_some(),
            thumb_state: r.thumb_state,
            favorite: r.favorite,
            picked: r.picked,
            duration_ms: r.duration_ms,
            is_video: pictkura_core::video::is_video_path(&r.path),
            plays_in_app: pictkura_core::video::plays_in_webview(&r.path),
            needs_transcode: pictkura_core::thumbs::needs_display_transcode(&r.path),
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
    /// 選別で選んだ件数（⚑。0.2 ②）
    picked: i64,
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
    remember_unreadable(state, &scan.outcome, &config.library.roots);
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

/// 綴りを照合の形へ畳む。**バイト長を変えない**
/// （畳んだ文字列上の位置で、元の文字列をそのまま切り出すため）。
///
/// **Windowsだけ**区切りをバックスラッシュへ寄せ、ASCIIの大小を畳む。
/// macOS/Linuxでは**何もしない**——区切りは `/` しか無く、
/// APFSは**大小を区別する設定にできる**ので、畳むと別のファイルを同一視する。
fn fold_for_compare(s: &str) -> String {
    if cfg!(windows) {
        s.replace('/', "\\").to_ascii_lowercase()
    } else {
        s.to_string()
    }
}

/// このOSのパス区切り。
fn path_separator() -> char {
    if cfg!(windows) {
        '\\'
    } else {
        '/'
    }
}

/// 綴りをこのOSの形へ寄せ、末尾の区切りを落とす。
fn root_spelling_of(raw: &str) -> String {
    let spelled = if cfg!(windows) {
        raw.replace('/', "\\")
    } else {
        raw.to_string()
    };
    spelled.trim_end_matches(path_separator()).to_string()
}

/// `\\?\` 拡張プレフィックスを外す（fs::canonicalizeの戻り値を通常形式へ）。
///
/// **UNCは戻し切らない**: `\\?\UNC\server\share` は `UNC\server\share` になり、
/// `\\server\share` には戻らない。今日は無害——そんな綴りは絶対パスに
/// 決して前方一致しないので、`RootSpec::prefixes` に積まれても不活性な的で
/// 終わる（Windowsの監視は設定綴りで返すので1本目で足り、USNはUNCで無効）。
/// **ただし将来 `prefixes` をUNCのジャンクション解決に使うなら、黙って外れる**
/// （ゲート2の指摘）。
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

/// パスをルートへ照合するための情報。
///
/// **使うのは2か所**——ファイル監視（`handle_fs_events`）と
/// USNの差分（`try_usn_sync`）。片方だけの話と読まないこと
struct RootSpec {
    /// 設定ファイル上の綴り（DBのpath/parent_dir/dirsのキーはこの綴りで始まる）
    spelling: String,
    /// 照合に使うプレフィックス。設定綴りに加え、ジャンクション等を解決した
    /// 実体パスも含める（FRN解決は実体パスを返すため）
    prefixes: Vec<String>,
}

/// 照合の的を作る。
///
/// `spelling` を `to_string_lossy` で作っているが、**実際には損なわれない**
/// ——ルートは `pictkura.toml` を `read_to_string` して読んだものなので、
/// 妥当なUTF-8である（ゲート2が「非UTF-8のルートで壊れる」と見たが、
/// TOMLを通る以上そこへは到達しない）。
///
/// **例外は初回起動の1回だけ**: 設定ファイルがまだ無いときの既定は
/// `picture_dir()` の `PathBuf` が**TOMLを経由せず**ここへ届く（ゲート2の指摘）。
/// 仮に非UTF-8でも、化けた綴りは何のパスにも前方一致しないので
/// `rebase_to_root_spelling` は `None` を返す——**そのまま通す**側に落ちるだけで、
/// 綴りを取り違えることはない。
fn root_specs(roots: &[PathBuf]) -> Vec<RootSpec> {
    roots
        .iter()
        .map(|root| {
            let spelling = root_spelling_of(&root.to_string_lossy());
            let mut prefixes = vec![spelling.clone()];
            // **解決後の綴りも照合の的に入れる。** macOSの `/var` は
            // `/private/var` へのシンボリックリンクで、**FSEventsは解決後を返す**
            // ——ここに入れておかないと、設定の綴りと一致せず揃えられない。
            //
            // **大小もここで揃う。** `std::fs::canonicalize` は `realpath(3)` を
            // 呼ぶので、設定に `pictures` と書いてあっても実物が `Pictures` なら
            // `Pictures` が返る（2026-08-26に実測）。ゲート2は「大小は直らない」と
            // 見たが、それは Python の `os.path.realpath`——あちらは
            // ファイルシステムに聞かない純粋な文字列処理で、挙動が違う
            if let Ok(real) = std::fs::canonicalize(root) {
                let real = root_spelling_of(&strip_verbatim(&real));
                if fold_for_compare(&real) != fold_for_compare(&spelling) {
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

/// OSが返したパスの綴りを、設定ルートの綴りへ揃える。ルート配下でなければNone。
///
/// **入口は2つ**: ファイル監視（macOSのFSEventsは `/var` を `/private/var` に
/// 解決して返す）と、USNの差分（Windows）。
///
/// DBのpath/parent_dir/dirsのキーは「設定ルートの綴り＋ファイルシステムが返した
/// 構成要素名」で保存されている。USNのFRN解決はドライブレターの大小文字や
/// ジャンクション解決後の実体パスなど、設定と違う綴りを返すことがあり、
/// そのまま部分反映するとSQLiteの大小区別の比較で同じファイルが重複レコードになる。
/// 大文字小文字（ASCII）と区切り文字の揺れを無視して**最長一致**のプレフィックスを
/// 探し、その部分を設定の綴りへ置き換える（配下の構成要素名はFS由来なので一致する）。
fn rebase_to_root_spelling(path: &Path, specs: &[RootSpec]) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        rebase_unix(path, specs)
    }
    #[cfg(not(unix))]
    {
        rebase_windows(path, specs)
    }
}

/// Unix側。**バイトのまま照合し、バイトのまま組み直す**。
///
/// `to_string_lossy()` を通さないのが要点——Unixのパスは**UTF-8とは限らない**。
/// APFSは非UTF-8の名前を作らせない（実測で `EILSEQ`）が、**FAT32/exFATは通す**
/// ので、外付けをライブラリのルートにすると入りうる。通すと U+FFFD に化け、
/// その後の `is_file()` も `stat` も**存在しないパス**を見にいく（ゲート1の指摘）。
///
/// 畳みが要らないので、バイトの前方一致で足りる——区切りは `/` だけで、
/// APFSは大小を区別する設定にできるから畳んではいけない。
#[cfg(unix)]
fn rebase_unix(path: &Path, specs: &[RootSpec]) -> Option<PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = path.as_os_str().as_bytes();
    let under = |prefix: &str| {
        let p = prefix.trim_end_matches('/').as_bytes();
        (bytes == p || (bytes.starts_with(p) && bytes.get(p.len()) == Some(&b'/')))
            .then_some(p.len())
    };
    // **設定の綴りに当たるなら、それを最優先で採る。**
    //
    // 同じ場所を指すルートが2つ設定されていると（`/photos` と、そこへの
    // リンク `/library`）、解決後の綴りは**どちらのルートにも当たる**。
    // 長さが同じなので先に見つけた方が勝ち、`/photos/a.jpg` のイベントが
    // `/library/a.jpg` へ書き換わりうる——**`/photos` 側の行が更新されなくなる**
    // （ゲート1の指摘）。既に設定どおりの綴りで来ているものは**触らない**
    let mut best: Option<(usize, &str)> = None; // (一致プレフィックス長, 揃える綴り)
    for spec in specs {
        if let Some(len) = under(&spec.spelling) {
            if best.is_none_or(|(best_len, _)| len > best_len) {
                best = Some((len, &spec.spelling));
            }
        }
    }
    // 設定の綴りに当たらないときだけ、解決後の綴りから探す
    if best.is_none() {
        for spec in specs {
            for prefix in spec.prefixes.iter().skip(1) {
                if let Some(len) = under(prefix) {
                    if best.is_none_or(|(best_len, _)| len > best_len) {
                        best = Some((len, &spec.spelling));
                    }
                }
            }
        }
    }
    let (len, spelling) = best?;
    // **綴りが同じならそのまま返す。** 組み直さなければ、非UTF-8のバイトに
    // 触る機会そのものが無い（監視イベントの大半はこちら）
    if spelling.as_bytes() == &bytes[..len] {
        return Some(path.to_path_buf());
    }
    let mut out = spelling.as_bytes().to_vec();
    out.extend_from_slice(&bytes[len..]);
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}

/// Windows側。区切りと大小の揺れを畳んでから照合する。
///
/// 畳んだ形は**バイト長を変えない**ので、畳んだ文字列上の位置で
/// 元の文字列をそのまま切り出せる。
#[cfg(not(unix))]
fn rebase_windows(path: &Path, specs: &[RootSpec]) -> Option<PathBuf> {
    let path_str = path.to_string_lossy().replace('/', "\\");
    let path_norm = fold_for_compare(&path_str);
    let under = |prefix: &str| {
        let prefix_norm = fold_for_compare(prefix.trim_end_matches('\\'));
        (path_norm == prefix_norm || path_norm.starts_with(&format!("{prefix_norm}\\")))
            .then_some(prefix_norm.len())
    };
    // 設定の綴りに当たるなら最優先（理由は `rebase_unix` を見よ）
    let mut best: Option<(usize, &str)> = None; // (一致プレフィックス長, 揃える綴り)
    for spec in specs {
        if let Some(len) = under(&spec.spelling) {
            if best.is_none_or(|(best_len, _)| len > best_len) {
                best = Some((len, &spec.spelling));
            }
        }
    }
    // 設定の綴りに当たらないときだけ、解決後の綴りから探す
    if best.is_none() {
        for spec in specs {
            for prefix in spec.prefixes.iter().skip(1) {
                if let Some(len) = under(prefix) {
                    if best.is_none_or(|(best_len, _)| len > best_len) {
                        best = Some((len, &spec.spelling));
                    }
                }
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
    remember_unreadable(state, &scan.outcome, &config.library.roots);
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
    filter: pictkura_core::MediaFilter,
) -> Result<Vec<DaySummaryDto>, String> {
    let query = pictkura_core::parse_query(&query, filter);
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
    filter: pictkura_core::MediaFilter,
) -> Result<Vec<MediaItemDto>, String> {
    let query = pictkura_core::parse_query(&query, filter);
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

/// このOS。**UIの案内の文言を分けるための正**。
///
/// フロントにも `navigator.userAgent` を見る判定（`api.ts` の `isMac` /
/// `isWindows`）があるが、あれは修飾キーの表記を選ぶためのもので、
/// **WebViewのUA次第で外れうる**。「買わせる話をするか」「デコーダはあると
/// 断言するか」を取り違えると実害が出るので、コンパイル時に分かるこちらを正とする。
///
/// **`decoder_status` に相乗りさせない。** あちらはHEICを1枚実際に展開するので
/// 高く、しかも「今後表示しない」で丸ごと省かれる。OSの判定はそれとは無関係に
/// 常に要る（動画の案内が使う）ので、**安くて転ばない口**として分けてある。
#[tauri::command]
fn host_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "other"
    }
}

/// 一覧が空の理由（[`empty_library_reason`]）。
#[derive(serde::Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct EmptyLibraryDto {
    /// ライブラリのルートが1つも設定されていない
    no_roots: bool,
    /// 設定にあるのに**そこに無い**ルート（外付けを挿し忘れた等）
    missing: Vec<String>,
    /// 実在するのに**読めなかった**場所（権限・TCC・切れたネットワーク）。
    /// ルート自身に加えて、**走査が奥で開けなかったフォルダ**も入る
    unreadable: Vec<String>,
    /// 開けなかった場所の数。UIは3件まで挙げて「ほか N件」と言うので、
    /// 切ったぶんの数が要る——走査は先頭3件しか名前を控えていない
    /// （[`ScanUnreadable`]）ので、**名前の数では足りない**
    unreadable_total: usize,
    /// 除外で飛ばした項目の名前（先頭3件まで）
    excluded: Vec<String>,
    /// 除外で飛ばした**名前**の数（重複を除く）。UIは3件まで挙げて
    /// 「ほか N件」と言うので、切ったぶんの数が要る——**黙って落とさない**。
    ///
    /// **数えるのは項目ではなく名前。** この数は「例: `.隠しフォルダ`」という
    /// **名前の並びの続き**として読まれる。3つのルートに同じ `.隠しフォルダ` が
    /// あるときに「ほか2件」と出すと、`pictkura.toml` に**在りもしない2つの
    /// 設定を探しに行かせる**——外すべきパターンは1つしか無い（ゲート2の指摘）
    excluded_total: usize,
    /// 写真を管理するアプリのライブラリが直下にあり、**ほかに扱えるものが無い**
    photo_library: bool,
    /// 直下で見つけたライブラリに、**現行の写真.app以外**のもの
    /// （iPhotoの `*.photolibrary` / `*.migratedphotolibrary`、Aperture の
    /// `*.aplibrary`）が含まれる。
    ///
    /// **文言がここで割れる**——「原本の多くはiCloud側にあり、手元には
    /// ありません」は写真.appの話で、あちらの中身は手元にある。iCloudを
    /// 持ち出すと**在りかを取り違えさせる**（ゲート1の指摘）。
    ///
    /// **「1つでも写真.appだった」ではなく「1つでも写真.app以外だった」を持つ。**
    /// 写真.appのライブラリと iPhoto のライブラリが**並んでいる**とき、前者で
    /// 旗を立てると写真.app専用の文言が選ばれ、**隣にある手元のライブラリの話が
    /// 消える**（ゲート1の指摘）。こちらの向きなら、混ざったときは
    /// どちらにも当たる文言に落ちる
    photo_library_legacy: bool,
    /// **ルートそのもの**が写真.appのライブラリ。
    ///
    /// [`Self::photo_library`] と分けてある——あちらは「ここにはライブラリしか
    /// 無い」という推測で、隣に候補があれば覆る。こちらは「このルートは
    /// ライブラリそのもので、走査が丸ごと飛ばすので何も出てこない」という
    /// **覆らない事実**。1つの旗に2つの事実を持たせると、隣に空のフォルダが
    /// あるだけで「ライブラリのほかに見つかりません」という**排他の主張**が
    /// 嘘になる（ゲート2の指摘）
    root_is_package: bool,
    /// ルート自身のライブラリに、**現行の写真.app以外**のものが含まれる
    /// （[`Self::photo_library_legacy`] と同じ向き・同じ理由）
    root_package_legacy: bool,
    /// **まだ確かめ終わっていない。** 探りが返ってこないまま時間切れ。
    /// 刺さったネットワークのフォルダで起きる——**「何も無い」と言わない**
    checking: bool,
    /// **返事がないまま見切ったルート。** [`Self::checking`] と分けてある——
    /// あちらは「まだ見ている」で、放っておけば変わる。こちらは
    /// **探りを諦めた**あとの話で、印が残っている限り二度と探らない
    /// （[`AppState::root_probes`]）。黙って `checking` を出し続けると、
    /// 見てもいないのに「確かめている途中です」と言い続けることになる
    stalled: Vec<String>,
}

/// ルートごとの探りの控え。**返ってこなかったルートの印がここに残る**
/// （[`AppState::root_probes`] に経緯がある）。
type RootProbes = std::sync::Arc<Mutex<ProbeRegistry>>;

/// 探りが返らないまま**これだけ経ったら**、「確かめている途中」ではなく
/// **「応答がない」**と言い切る（探りは1本きりで、待っても増えない）。
const PROBE_STALLED_AFTER: std::time::Duration = std::time::Duration::from_secs(10);

/// **返らない探りを同時に抱える上限。**
///
/// 1本につきブロッキングプールのスレッドを1つ、永久に持っていく。
/// ルートごとの相乗りは「同じ場所へ2本目を飛ばさない」までしか守らないので、
/// **死んだマウントが何本もあると、その数だけ積み上がる**（ゲート1の指摘）。
/// ここで頭を打つと、それ以上のルートは探らずに「まだ確かめていない」と返す
/// ——見てもいないものを「応答がない」と名指ししないため。
///
/// 上限を数えるのは**刺さっている探りだけ**（[`PROBE_STALLED_AFTER`] を超えたもの）。
/// 健康なルートは数ミリ秒で返るので、10本あっても1本も数に入らない
const MAX_STUCK_PROBES: usize = 4;

/// 走っている探りの台帳。
#[derive(Default)]
struct ProbeRegistry {
    running: HashMap<PathBuf, RootProbe>,
    /// 次に配る世代番号（[`RootProbe::generation`] を見よ）
    next_generation: u64,
}

/// 走っている探り1本。
#[derive(Clone)]
struct RootProbe {
    /// この探りの**世代**。同じルートを外して足し直したとき、
    /// **古い探りの後片付けが新しい探りの印を消さない**ようにするための番号
    generation: u64,
    /// 始めた時刻。**返らないまま長く経ったら「応答がない」と言い切る**
    started: std::time::Instant,
    /// 答えの受け口。複製すれば相乗りできる（`watch` は最後の値を配る）
    answer: tokio::sync::watch::Receiver<Option<RootReason>>,
}

/// 探りの後始末——**答えを配り、印を消す**。
///
/// 消すのを `Drop` に置くのが要。[`root_reason_of`] がパニックしたときも
/// 巻き戻りで走るので、**バグで落ちたルートに「応答がない」の印が残らない**。
/// 印は「返ってこない」ことの証拠でなければならず、落ちたことの証拠にすると、
/// 直したはずのルートを二度と探らなくなる
struct ProbeGuard {
    probes: RootProbes,
    root: PathBuf,
    /// 自分が置いた印の世代。**これと違う印は自分のものではない**
    generation: u64,
    answer: tokio::sync::watch::Sender<Option<RootReason>>,
}

impl ProbeGuard {
    /// 見終わった。**待っている全員に配ってから**印を消す（`Drop`）
    fn finish(self, reason: RootReason) {
        let _ = self.answer.send(Some(reason));
    }
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        // **自分の印だけ消す。** ルートを外して足し直すと、刺さったままの
        // 古い探りの後ろで新しい探りが始まっている——世代を見ないと、
        // 古い方が返った拍子に**新しい印を消してしまう**（ゲート1の指摘）
        let mut registry = lock_ok(&self.probes);
        if registry
            .running
            .get(&self.root)
            .is_some_and(|probe| probe.generation == self.generation)
        {
            registry.running.remove(&self.root);
        }
    }
}

/// ルート1本の探りを**始めるか、走っているものに相乗りする**。
///
/// 始める中身を外から受けるのは試験のため（本物は `spawn_blocking`）。
/// 返すのは受け口の複製なので、**呼んだ側は待つだけでスレッドを食わない**。
fn probe_of(
    probes: &RootProbes,
    root: &Path,
    stalled_after: std::time::Duration,
    start: impl FnOnce(ProbeGuard),
) -> Option<RootProbe> {
    let mut registry = lock_ok(probes);
    // **走っているものがあれば、それに乗る。** 同じルートへ2本目を飛ばしても、
    // 刺さったマウントでは**スレッドが1本増えるだけ**で答えは増えない
    if let Some(probe) = registry.running.get(root) {
        return Some(probe.clone());
    }
    // **刺さっている探りが多すぎるなら、これ以上は増やさない**
    // （[`MAX_STUCK_PROBES`]）。数えるのは返らなくなったものだけで、
    // 健康なルートは数ミリ秒で消えるので数に入らない
    let stuck = registry
        .running
        .values()
        .filter(|probe| probe.started.elapsed() >= stalled_after)
        .count();
    if stuck >= MAX_STUCK_PROBES {
        return None;
    }
    let generation = registry.next_generation;
    registry.next_generation += 1;
    let (answer, receiver) = tokio::sync::watch::channel(None);
    let probe = RootProbe {
        generation,
        started: std::time::Instant::now(),
        answer: receiver,
    };
    registry.running.insert(root.to_path_buf(), probe.clone());
    // **印を置いてから始める**（先に始めると、その隙に来た問い合わせが
    // 2本目を飛ばす）。ロックはここで手放す——探りが即座に終わって
    // `ProbeGuard` が印を消しに来ても、待たせない
    drop(registry);
    start(ProbeGuard {
        probes: probes.clone(),
        root: root.to_path_buf(),
        generation,
        answer,
    });
    Some(probe)
}

/// ルートの顔ぶれが変わったとき、**その場所の探りの印を捨てる**。
///
/// 刺さったルートの印は探りが返るまで残る——それが狙いで、二度と触らないため。
/// ところが**外して足し直した**ときまで残ると、貼り直した共有を一度も見ずに
/// 「応答がありません」と言い続ける（ゲート1の指摘）。捨てるのは印だけで、
/// 返らない探り自身はそのまま——その後片付けは世代を見るので、
/// 新しい探りの印を消すことはない。
fn forget_probes_for_changed_roots(probes: &RootProbes, before: &[PathBuf], after: &[PathBuf]) {
    let mut registry = lock_ok(probes);
    // 設定から外れたルート
    registry.running.retain(|root, _| after.contains(root));
    // 足し直された（この変更で新しく入った）ルート
    for root in after.iter().filter(|root| !before.contains(root)) {
        registry.running.remove(root);
    }
}

/// 一覧が空のとき、**なぜ空なのか**をUIへ返す。
///
/// **無言で `0 件` を出さないため**の口。macOSの `~/Pictures` には
/// 写真.appのライブラリしか無いことが普通で、それは既定の除外に入っている
/// （2026-08-14に14,938件を掴んだ事故の後で足した）。結果、**全部正しく動いた
/// うえで初回起動が空になる**——理由がどこにも出ないと「壊れている」と読まれる。
/// Lightroom は取り込みダイアログを、Photo Mechanic は「フォルダを選べ」を必ず出す。
///
/// **走査の数え上げには足さない。** 空のときにしか呼ばれないので、ここで
/// ルートの直下を1回読むだけで足りる——速さが仕様の走査路を太らせる理由が無い。
/// 中も開かない（名前だけ見る）。奥で開けなかったフォルダは走査が控えている
/// （[`ScanUnreadable`]）。
///
/// **ルートは1本ずつ独立して探る。** 直列に回していた頃は、刺さったSMBの
/// 後ろにある `~/Pictures` まで巻き添えになった（[`AppState::root_probes`]）。
#[tauri::command]
async fn empty_library_reason(app: tauri::AppHandle) -> EmptyLibraryDto {
    /// 1回の問い合わせで待つ上限。**フロントの見切り（5秒）より短くする**:
    /// 向こうが先に諦めると、こちらの正直な返事が捨てられる
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);
    let state = app.state::<AppState>();
    let config = lock_ok(&state.config).clone();
    let from_scan = lock_ok(&state.scan_unreadable).clone();
    let roots = config.library.roots.clone();
    let probes = state.root_probes.clone();
    // **ルートごとに独立して探る**（走っているものには相乗りする）
    let running: Vec<Option<RootProbe>> = roots
        .iter()
        .map(|root| {
            // 探る側へ渡す複製（鍵に使う `root` は畳んだあとの名前にも要る）
            let probing = root.clone();
            let config = config.clone();
            probe_of(&probes, root, PROBE_STALLED_AFTER, move |guard| {
                // **ブロッキングプールへ逃がす。** 切れたSMB/NFSのルートでは
                // `metadata` も `read_dir` もマウントのタイムアウトぶん
                // （hardマウントなら際限なく）返ってこない。非同期コマンドは
                // tokioのワーカーを占めるので、走らせる場所を間違えると
                // **他のコマンドごと詰まる**（ゲート2の指摘）。
                // `add_library_root` などが同じ理由でこうしている
                tauri::async_runtime::spawn_blocking(move || {
                    guard.finish(root_reason_of(&probing, &config));
                });
            })
        })
        .collect();
    // **待つのは有限時間まで。** 刺さったhardマウントでは `read_dir` が返らず、
    // `spawn_blocking` の `await` も返らない——Tauriのコマンドは detached で
    // 走るから、フロントが見切っても `Drop` は来ない
    let until = std::time::Instant::now() + DEADLINE;
    let mut answers = Vec::with_capacity(running.len());
    for probe in running {
        // **抱えきれないぶんは探っていない。** 見てもいないものを
        // 「応答がありません」と名指ししない——言えるのは「まだ確かめていない」
        // だけで、刺さっていた探りが1本返れば次の問い合わせで探りに行く
        let Some(RootProbe {
            started,
            mut answer,
            ..
        }) = probe
        else {
            answers.push(RootAnswer::Checking);
            continue;
        };
        // **諦めたルートはもう待たない。** 印が付いている以上、待っても
        // 答えは来ない——待つと**健康なルートの返事まで3秒遅れる**
        let left = if started.elapsed() >= PROBE_STALLED_AFTER {
            std::time::Duration::ZERO
        } else {
            until.saturating_duration_since(std::time::Instant::now())
        };
        let waited = tokio::time::timeout(left, async move {
            loop {
                // **見た印を付けて借りる**ので、下の `changed()` は「次の変化」を
                // 待つ。もう入っていればここで返る
                let current = { (*answer.borrow_and_update()).clone() };
                if current.is_some() {
                    return current;
                }
                // 送り手が落ちた（探りがパニックした）。印は `Drop` が消して
                // いるので、次の問い合わせが探り直す
                if answer.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await;
        answers.push(match waited {
            Ok(Some(reason)) => RootAnswer::Answered(reason),
            // **落ちたときも「無い」と言わない。** 旗が1つも立たない答えを
            // 返すと、UIは「扱える画像が見つかりません」と**断定**する
            // ——エラーから断定を作らない（ゲート2の指摘）
            Ok(None) => RootAnswer::Checking,
            Err(_) if started.elapsed() >= PROBE_STALLED_AFTER => RootAnswer::Stalled,
            Err(_) => RootAnswer::Checking,
        });
    }
    merge_root_reasons(&roots, &answers, &from_scan)
}

/// **OSが自分のために置くフォルダ**。利用者の写真は入らない。
///
/// ボリュームの直下にはこれらが必ず居る。既定の除外は `.*` を含むので、
/// 何も入っていない外付けをルートに足すと**この面々が「除外で全部飛ばして
/// います（例: .Spotlight-V100）」の証拠になり**、`pictkura.toml` を
/// 編集しに行かせることになる——直しても何も出てこない（ゲート2の指摘）。
/// 利用者が自分で隠したフォルダ（`.隠し写真` など）は**候補のまま**にする:
/// あちらは除外を外せば本当に出てくる。
const OS_BOOKKEEPING: &[&str] = &[
    // macOS
    ".Spotlight-V100",
    ".fseventsd",
    ".Trashes",
    ".TemporaryItems",
    ".DocumentRevisions-V100",
    ".apdisk",
    ".Trash",
    // Windows
    "System Volume Information",
    "$RECYCLE.BIN",
    "RECYCLER",
    // Linux
    "lost+found",
];

/// freedesktop の「ボリュームごとのごみ箱」か（`.Trash-1000` のようにuidが付く）。
///
/// 名前を並べただけでは当たらないので別に見る。USBメモリの直下がこれだけの
/// とき、「除外で全部飛ばしています（例: .Trash-1000）」と言って
/// `pictkura.toml` を編集しに行かせることになる（ゲート2の指摘）。
///
/// **uidが数字であることまで見る**。単なる前方一致にすると
/// `.Trash-の写真` のような利用者のフォルダまで飲む
/// （加えて、バイト位置で切ると多バイト文字の途中に当たって落ちる）
fn is_volume_trash(name: &str) -> bool {
    name.strip_prefix(".Trash-")
        .is_some_and(|uid| !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit()))
}

/// ルート直下の1エントリの形。`std::fs::FileType` から起こす。
///
/// テストから直に組めるようにここで畳んである（`FileType` は作れない）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EntryShape {
    Dir,
    File,
    /// リンク（**リンク先は見ない**。走査もそうしている）
    Symlink,
    /// 普通のファイルでもフォルダでもない（FIFO・ソケット・デバイスノード）
    Other,
    /// 種別が取れなかった
    Unknown,
}

impl EntryShape {
    fn of(entry: &std::fs::DirEntry) -> Self {
        match entry.file_type() {
            Ok(t) if t.is_symlink() => Self::Symlink,
            Ok(t) if t.is_dir() => Self::Dir,
            Ok(t) if t.is_file() => Self::File,
            // FIFO・ソケット・デバイスノード。**走査は `is_file()` を要求する**
            // ので、`preview.jpg` という名前のFIFOがあっても索引されない
            // ——リンクと同じで、走査が見ないものを証拠にしない（ゲート2の指摘）
            Ok(_) => Self::Other,
            Err(_) => Self::Unknown,
        }
    }
}

/// ルート直下の1エントリを、**空の理由**の観点で見たときの正体。
#[derive(PartialEq, Eq, Debug)]
enum EntryKind {
    /// 写真を管理するアプリのライブラリ（名前で分かる。中は開かない）。
    /// **どのアプリのものかで案内が変わる**ので、現行の写真.appかどうかも持つ
    Package { photos_app: bool },
    /// 中身になり得たが、除外の設定で飛ばした
    Excluded,
    /// 中身になり得た。**除外を犯人にしてはいけない証拠**
    Candidate,
    /// そもそも中身になり得ない（`.DS_Store`・`desktop.ini` など）
    Ignorable,
}

/// [`empty_library_reason_of`] のエントリ1件ぶんの判断。
///
/// **ファイルシステムを通さずに試験できるよう切り出してある。**
/// UTF-8にならない名前は APFS が受け付けず（実測 `EILSEQ`）、CIに Linux が
/// 無いので、実物のファイルを作る形の試験は**どの環境でも1度も走らない**
/// ——「試験があるのに何も見ていない」状態になっていた（ゲート1の指摘）。
///
/// `shape` は `EntryShape::of` でエントリから起こす（走査と同じくリンクは辿らない）。
fn classify_entry(name: &std::ffi::OsStr, shape: EntryShape, config: &Config) -> EntryKind {
    // **リンクの判断は名前より先。** 走査は `follow_links = false` で
    // `file_type().is_file()` が偽のものを落とすので、**名前が読めるかどうかは
    // 関係なく**リンクは拾われない。下の `Symlink | Other` より後ろで
    // 「読めない名前は候補」に倒すと、**同じリンクが名前の綴りだけで
    // 候補にも除外にも転ぶ**——Latin-1の名前のリンクが1本あるだけで
    // 「写真.appのライブラリしかありません」が消える（ゲート2の指摘）
    if matches!(shape, EntryShape::Symlink | EntryShape::Other) {
        return EntryKind::Ignorable;
    }
    // **読めない名前は「候補あり」に倒す。** 走査本体は UTF-8 にならない名前を
    // 除外に一致させず、そのまま入って行く（`scanner.rs` の
    // `to_str().is_some_and(..)`）。ここで黙って飛ばすと、走査が開いて何も
    // 見つけていないフォルダが**この関数から見えなくなり**、隣の
    // `.隠しフォルダ` が冤罪を着る（ゲート1の指摘）
    let Some(text) = name.to_str() else {
        return EntryKind::Candidate;
    };
    // **パッケージはフォルダ。** 名前だけで見ると、`old.photoslibrary` という
    // ただのファイル（改名した書庫の残骸など）で旗が立ち、写真.appのライブラリが
    // 1つも無い場所に「写真.appのライブラリのほかに見つかりません」と言う。
    // 走査もその名前は**除外されたファイル**として扱う（ゲート2の指摘）
    if shape == EntryShape::Dir {
        if let Some(package) = pictkura_core::scanner::managed_package_name(Path::new(text)) {
            // **配下のパッケージは、走査も利用者の設定に従う。** 既定の
            // `exclude_patterns` に同じ綴りが入っているだけで、消せば走査は
            // 中へ入っていく（`scanner.rs` の `MANAGED_PACKAGE_PATTERNS` の
            // 説明。固定なのは**ルートに名指しされたとき**だけ）。
            // 消している人にここで旗を立てると、走査は既に中を読んで何も
            // 見つけていないのに「写真.appのほかに見つかりません」と言い、
            // **直しても何も出てこない**話へ送ることになる（ゲート1の指摘）
            if config
                .library
                .exclude_patterns
                .iter()
                .any(|p| pictkura_core::scanner::matches_pattern(text, p))
            {
                return EntryKind::Package {
                    photos_app: pictkura_core::scanner::is_photos_app_package(package),
                };
            }
            // 除外していない＝**走査は中を見ている**。中身になり得た側に数える
            return EntryKind::Candidate;
        }
    }
    // OSの置き場は「中身になり得た」に数えない（`OS_BOOKKEEPING` を見よ）
    if OS_BOOKKEEPING.iter().any(|n| n.eq_ignore_ascii_case(text)) || is_volume_trash(text) {
        return EntryKind::Ignorable;
    }
    let could_have_been_content = match shape {
        EntryShape::Dir => true,
        EntryShape::File => {
            pictkura_core::scanner::has_target_extension(Path::new(text), &config.import.extensions)
        }
        // 上で先に落としてある（名前が読めなくても落とすため）
        EntryShape::Symlink | EntryShape::Other => return EntryKind::Ignorable,
        // 種別が取れないときは安全側（＝除外を犯人にしない側）へ
        EntryShape::Unknown => return EntryKind::Candidate,
    };
    if !could_have_been_content {
        return EntryKind::Ignorable;
    }
    if config
        .library
        .exclude_patterns
        .iter()
        .any(|p| pictkura_core::scanner::matches_pattern(text, p))
    {
        EntryKind::Excluded
    } else {
        EntryKind::Candidate
    }
}

/// ルート1本を見た結果。**畳む前の素材**（[`merge_root_reasons`] が畳む）。
///
/// [`EmptyLibraryDto`] と分けてあるのは、**ルートごとに独立して探る**ため
/// ——刺さった1本を待っている間も、返ってきたルートのぶんは畳める。
/// 名前の重複を除くのと「ほか N件」を数えるのは畳む側の仕事で、
/// ここは1本の中で見たままを持つ
#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct RootReason {
    /// そこに無い（外付けを挿し忘れた等）
    missing: bool,
    /// 在るのに読めなかった（権限・TCC・切れたネットワーク）
    unreadable: bool,
    /// ルートそのものが、写真を管理するアプリのライブラリ
    root_is_package: bool,
    /// そのライブラリが**写真.app以外**（iPhoto・Aperture）のものか
    root_package_legacy: bool,
    /// 直下にライブラリがあった
    photo_library: bool,
    /// そのライブラリに**写真.app以外**のものが含まれる
    photo_library_legacy: bool,
    /// 除外に当たらなかった「中身になり得たもの」を見た
    survivor: bool,
    /// 除外で飛ばした名前（**このルートの中では**重複を除いてある）
    excluded: Vec<String>,
}

/// ルート1本について、いま言えること。
enum RootAnswer {
    /// 見終わった
    Answered(RootReason),
    /// まだ返ってきていない。**「無い」ではなく「まだ見ていない」**
    Checking,
    /// 返らないまま長く経った。**探りは諦めていて、印は残したまま**
    /// ——ここまで来たら「確かめている途中です」は嘘になるので名指しする
    Stalled,
}

/// ルート1本を見る。**ファイルシステムを触るのはここだけ**。
///
/// 刺さったマウントで返ってこないのもこの関数で、だからルートごとに
/// 別々の探りへ入れてある（[`AppState::root_probes`]）。
fn root_reason_of(root: &Path, config: &Config) -> RootReason {
    let mut out = RootReason::default();
    // **「無い」と「見せてもらえない」を取り違えない。** `is_dir()` は
    // 内側で `stat` を呼ぶだけなので、**親フォルダに検索権が無い**と
    // `EACCES` で偽を返す（実測: `chmod 000` の下のフォルダで確認）。
    // それを「見つかりません、つないでから再スキャンを」と案内すると、
    // **挿さっている外付けを挿し直させる**ことになる（ゲート1の指摘）。
    // 本当に無いのは `NotFound` のときだけ。
    //
    // **切れたSMBはここを通り抜ける**——`stat` は属性キャッシュで即座に
    // 「在る」と答え、止まるのは下の `read_dir` の方（実測: 応答を止めた
    // サーバで120秒待っても返らなかった）
    match std::fs::metadata(root) {
        Ok(md) if md.is_dir() => {}
        // フォルダを選ぶ画面から来る以上ほぼ起きないが、
        // ファイルを指していれば「そこにフォルダは無い」で正しい
        Ok(_) => {
            out.missing = true;
            return out;
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            out.missing = true;
            return out;
        }
        Err(_) => {
            out.unreadable = true;
            return out;
        }
    }
    // **ルート自身がパッケージのこともある。** いまの `add_library_root` は
    // それを断るので、届くのは**手で書いた `pictkura.toml`**か、断るより
    // 前の版が書いた設定から（ゲート2の指摘。以前ここに「フォルダを選ぶ
    // 画面で普通に起きる」と書いていたのは誤り）。走査はそのルートを
    // 丸ごと飛ばすので、中を読んでも何も出ない。
    //
    // **これは `survivor` で消えない事実**。「この場所には写真.appの
    // ライブラリしか無い」は隣に候補があれば覆るが、「このルートは
    // ライブラリそのもので、何も出てこない」は覆らない
    if let Some(package) = pictkura_core::scanner::managed_package_name(root) {
        out.root_is_package = true;
        out.root_package_legacy = !pictkura_core::scanner::is_photos_app_package(package);
        return out;
    }
    // **「在るのに読めない」を握り潰さない。** `stat` は通るのに
    // `read_dir` が落ちるのは、権限・macOSのTCC（`~/Desktop` や外付けへの
    // アクセスを許していない。**TCCはこちらの形で落ちる**——`stat` は
    // 通り、`opendir` で止まる）・切れたネットワークのとき。ここで黙ると
    // 「扱える画像がありません」と出てしまう——**1万枚あって、必要なのは
    // 許可の話**なのに（ゲート2の指摘）
    let Ok(entries) = std::fs::read_dir(root) else {
        out.unreadable = true;
        return out;
    };
    // **「1つでも除外に当たった」と「全部除外だった」は違う。**
    // macOSは `~/Pictures` に `.localized` を置き、Finderが覗いた場所には
    // `.DS_Store` が残る。既定の除外は `.*` を含むので、**本当に空の
    // フォルダでも「全部除外しています」と言ってしまう**（同）。
    // 中身になり得たもの——フォルダか、扱える拡張子のファイル——だけ数える
    // **列挙の途中で落ちたときも「空」と言わない。** `opendir` は通っても、
    // 共有が途中で切れれば以降の `readdir` が落ちる。黙って抜けると
    // 候補が1つも見つからず「扱える画像がありません」になる——
    // 必要なのは「開けませんでした」の方（ゲート2の指摘。走査本体の
    // `had_error` と揃える）
    let mut seen: HashSet<String> = HashSet::new();
    for entry in entries {
        let Ok(entry) = entry else {
            out.unreadable = true;
            break;
        };
        let name = entry.file_name();
        match classify_entry(&name, EntryShape::of(&entry), config) {
            EntryKind::Package { photos_app } => {
                out.photo_library = true;
                out.photo_library_legacy |= !photos_app;
            }
            EntryKind::Ignorable => {}
            EntryKind::Candidate => out.survivor = true,
            EntryKind::Excluded => {
                // `Excluded` はUTF-8として読めた名前でしか返らない
                let Some(name) = name.to_str() else { continue };
                // 同じ名前を2度並べない（数えるのは畳む側。`excluded_total`）
                if seen.insert(name.to_string()) {
                    out.excluded.push(name.to_string());
                }
            }
        }
    }
    out
}

/// ルートごとの答えを、UIへ返す**1つの理由**へ畳む。
///
/// 文言はライブラリ全体に対して1つしか出ないので、ルートをまたいで見る。
fn merge_root_reasons(
    roots: &[PathBuf],
    answers: &[RootAnswer],
    from_scan: &ScanUnreadable,
) -> EmptyLibraryDto {
    let mut out = EmptyLibraryDto {
        no_roots: roots.is_empty(),
        ..Default::default()
    };
    // 除外に当たらなかった「中身になり得たもの」を1つでも見たか
    let mut survivor = false;
    // **中身が分からないルートがある**（探っている最中・諦めた のどちらも）
    let mut unknown = false;
    // 例示に挙げた名前。**同じ名前を2度並べない**ためだけのもので、
    // 数（`excluded_total`）はここを通さず素直に数える
    let mut excluded_names: HashSet<String> = HashSet::new();
    for (root, answer) in roots.iter().zip(answers) {
        let reason = match answer {
            RootAnswer::Answered(reason) => reason,
            RootAnswer::Checking => {
                out.checking = true;
                unknown = true;
                continue;
            }
            RootAnswer::Stalled => {
                out.stalled.push(root.display().to_string());
                unknown = true;
                continue;
            }
        };
        if reason.missing {
            out.missing.push(root.display().to_string());
        }
        if reason.unreadable {
            out.unreadable.push(root.display().to_string());
        }
        out.root_is_package |= reason.root_is_package;
        out.root_package_legacy |= reason.root_package_legacy;
        out.photo_library |= reason.photo_library;
        out.photo_library_legacy |= reason.photo_library_legacy;
        survivor |= reason.survivor;
        for name in &reason.excluded {
            // **見せる3件と別に数える。** 見せる側で重複を見ると、
            // 4件目以降は毎回数えられ、先頭3件は何ルートに在っても1のまま
            // ——「ほか N件」がどちらにも狂う。数える先は名前の集合
            // （`excluded_total` の説明を見よ）
            if excluded_names.insert(name.clone()) {
                out.excluded_total += 1;
                if out.excluded.len() < 3 {
                    out.excluded.push(name.clone());
                }
            }
        }
    }
    // **走査が奥で開けなかった場所を借りる。** ここはルートの直下しか
    // 読まないので（速さが仕様の走査路を太らせないため）、`~/Pictures/2024/非公開`
    // だけ許可が無い形が見えない——走査はそこで `had_error` を立てているのに、
    // 説明側は「フォルダがある＝候補あり」と数えて「扱える画像がありません」に
    // 落ちる。**写真はその奥に在る**（ゲート1の指摘）
    for dir in &from_scan.dirs {
        let name = dir.display().to_string();
        // ルート自身が読めないと自分で分かっているものは二度並べない
        // （走査も同じ壁で止まっているので、必ず重なる）
        if !out.unreadable.contains(&name) {
            out.unreadable.push(name);
        }
    }
    // **足し算にしない。** 走査の総数にはルート自身も入っている——こちらで
    // 名指しした4本の外付けを、走査も4本として数えているのに、名前で重なりを
    // 消したぶんだけ足すと**5本目の「開けない場所」を探しに行かせる**
    // （ゲート2の指摘）。重なりの全体は数えられない（走査は3件しか名前を
    // 控えていない）ので、**大きい方を採って少なめに倒す**——
    // 「ほか N件」が在りもしない場所を指すよりは、少なく言う方がまし
    out.unreadable_total = out.unreadable.len().max(from_scan.total);
    // **除外を犯人にするのは、他に容疑者がいないときだけ。**
    // `.隠しフォルダ` の隣に空の `写真/` があれば、走査はその `写真/` を
    // ちゃんと開いて何も見つけていない——そこで「全部除外しています」と
    // 言うと、**除外を外せば出てくる**という嘘になる（ゲート1の指摘）。
    //
    // **写真.appの旗も同じ**。`~/Pictures` に写真ライブラリと `富士/` が
    // 並んでいて `富士/` が空なら、「ここには写真.appのライブラリしか
    // ありません」は端的に嘘で、しかも**利用者を真犯人から遠ざける**
    // （ゲート2の指摘。UIは写真.appの文言を除外より上に出す）。
    // 残るのは `emptyNothingHere`——具体性は落ちるが、嘘ではない
    //
    // **まだ見ていないルートがあるときも同じ**。「全部除外しています」も
    // 「ライブラリのほかに見つかりません」も**ライブラリ全体に対する
    // 排他の主張**なので、中身の分からない場所が1つでも残っていれば言えない
    // ——UIの並び順に頼らず、ここで落としておく
    if survivor || unknown {
        out.excluded.clear();
        out.excluded_total = 0;
        // **推測は覆る。** `root_is_package` は覆らないので落とさない（上の説明）
        out.photo_library = false;
        // 旗を1つでも残すと、UIが「どのアプリのライブラリか」だけを見ている
        // 場所で食い違う
        out.photo_library_legacy = false;
    }
    out
}

/// 設定のルートを**順に**見る（試験用の入口）。
///
/// 本番の [`empty_library_reason`] はルートごとに独立して探るので、この形では
/// 呼ばない——刺さった1本が後ろを巻き添えにするのが、この順序の性質そのもの。
/// 畳み方（名前の重複・「ほか N件」・`survivor` の後始末）をファイルシステム
/// 込みで確かめるために残してある
#[cfg(test)]
fn empty_library_reason_of(config: &Config, from_scan: &ScanUnreadable) -> EmptyLibraryDto {
    let answers: Vec<RootAnswer> = config
        .library
        .roots
        .iter()
        .map(|root| RootAnswer::Answered(root_reason_of(root, config)))
        .collect();
    merge_root_reasons(&config.library.roots, &answers, from_scan)
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
    /// 同梱した取扱説明書（日本語）。開発中の実行では見つからないので `None` になりうる
    manual_path: Option<String>,
    /// 同梱した取扱説明書（英語）。表示言語で出し分けるのはフロント側の仕事なので、
    /// **両方の在り処を返す**（片方しか同梱されていない実行でもボタンの有効・無効が出せる）
    manual_en_path: Option<String>,
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
        manual_en_path: resolve("docs/manual.en.html"),
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
        "manual-en" => info.manual_en_path,
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

/// ビューアの先読み候補のうち、**実体がクラウドにしか無い**ものを返す（0.2 ①）。
///
/// 先読みは**利用者の意思ではない**。OneDriveのプレースホルダを裏で読むと
/// その場でダウンロードが始まり、見てもいない写真のために通信とディスクを使う。
/// ビューアが隣を読み込む前にここで聞き、返ってきたidは先読みしない
/// （利用者が実際にそこへ送ったときは、今までどおり普通に開く）。
///
/// 一覧のDTOに載せない理由は [`video_status`] と同じ——判定はファイル属性の
/// 読み出しなので、1000件の日を開くたびに撒くのは割に合わない。聞くのは
/// 隣接の数件だけ。判定そのものはファイルを開かないのでダウンロードは起きない。
#[tauri::command]
fn cloud_only_media(state: tauri::State<'_, AppState>, ids: Vec<i64>) -> Result<Vec<i64>, String> {
    /// 一度に聞ける上限。先読みの隣接はどんなに広げてもこの桁に収まる。
    /// 超える呼びは「全件を運ぼうとしている」UI側の間違いなので、黙って
    /// 切り詰めずに断る（属性読みを大量に撒くのがまさに避けたいこと）
    const MAX_IDS: usize = 64;
    if ids.len() > MAX_IDS {
        return Err(format!("一度に聞けるのは{MAX_IDS}件までです"));
    }
    state.read_pool.with(|db| {
        let mut cloud = Vec::new();
        for id in ids {
            match db.get_by_id(id) {
                Ok(Some(r)) => match std::fs::symlink_metadata(&r.path) {
                    Ok(m) => {
                        if pictkura_core::cloud::is_cloud_only(&m) {
                            cloud.push(id);
                        }
                    }
                    // 属性が読めない＝**分からない**。`is_cloud_only_path` は
                    // 読めないと「クラウドではない」に倒すが、それは一覧の
                    // 表示向けの安全側で、ここでは逆。先読みは開いて困る側なので、
                    // 分からないものは「触らない」に入れる
                    Err(_) => cloud.push(id),
                },
                // 行が消えている: 配信も404になるので、先読みしても何も起きない
                Ok(None) => {}
                // **引けなかったら断る**。返さないidをフロントは「ローカルにある」と
                // 読むので、DBの一時的な失敗が**先読みのダウンロード**に化ける。
                // ここは開いて困る側なので、迷ったら答えない
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(cloud)
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

/// まとめて**ゴミ箱へ**送り、（実体が消えた意味で）片付いたパスと最初のエラーを返す。
///
/// 実体がもう無いものは**先に分ける**。ファイルが1つ欠けるだけで、まとめての操作は
/// 全体が失敗して1件ずつへ落ちてしまう（選択したあとに外から消えるのは、
/// 同期フォルダでは普通に起きる）。無いものはDBからだけ落とす（実体に合わせる）。
///
/// **まとめて1回で渡す**のが要点。Windowsの `trash::delete` は1件ごとにシェルの
/// ファイル操作を起こすので、一覧の全選択（数千〜数万件）では分単位で固まる。
/// 失敗したときだけ1件ずつへ落とし、**消せたぶんだけ**返す。
fn trash_paths(paths: Vec<PathBuf>) -> (Vec<PathBuf>, Option<String>) {
    trash_paths_with_progress(paths, |_, _| {})
}

/// ひと束の大きさは**小さく始めて倍にしていく**（上限まで）。
///
/// 刻む理由は進捗を出すためだけで、刻めば必ず損をする——**1回のシェル呼び出しに
/// 200〜300msの固定費**があり、500件（1MB）の実測は
/// 1束 3675ms / 5束 4848ms / 10束 5406ms だった（2026-08-19）。
/// 一方で「押した直後に数字が動く」ことには意味があるので、最初の束だけ小さくして
/// 早く1回動かし、あとは束を大きくして速さを取り戻す。
/// 500件なら 25→50→100→200→125 の**5束**で、1束のときより約1秒の損で収まる。
/// 25件以下（選別で普通に出る量）は1束のまま＝損はゼロ。
const TRASH_CHUNK_FIRST: usize = 25;
const TRASH_CHUNK_MAX: usize = 200;

/// [`trash_paths`] に**束ごとの進捗**を足したもの。`on_progress(done, total)` は
/// 1束を送り終えるたびに呼ばれる。`done` は**手を付け終えた件数**で、失敗したものも
/// 含む（進捗は必ず `total` まで進む。成否は戻り値の側で受ける）。
/// `total` は実体のあるものだけ——もう無いファイルは待ち時間を生まないので数えない。
fn trash_paths_with_progress(
    paths: Vec<PathBuf>,
    mut on_progress: impl FnMut(usize, usize),
) -> (Vec<PathBuf>, Option<String>) {
    let (mut gone, present): (Vec<PathBuf>, Vec<PathBuf>) =
        paths.into_iter().partition(|p| !p.exists());
    let total = present.len();
    let mut done = 0usize;
    let mut first_err: Option<String> = None;
    let mut size = TRASH_CHUNK_FIRST;
    while done < total {
        let end = (done + size).min(total);
        let chunk = &present[done..end];
        match trash::delete_all(chunk) {
            Ok(()) => gone.extend(chunk.iter().cloned()),
            Err(_) => {
                // **落ちるのは束の中だけ**。1件ずつへ降りるのはこの束に限り、
                // 残りの束はまとめて渡す速さのまま進む
                for path in chunk {
                    // ファイルが既に無い場合も片付いた扱い（実体に合わせる）。
                    // まとめての操作が途中まで通っていたぶんも、ここで拾える
                    match trash::delete(path) {
                        Ok(()) => gone.push(path.clone()),
                        Err(_) if !path.exists() => gone.push(path.clone()),
                        Err(e) => {
                            // **ここで打ち切らない**。既にゴミ箱へ入れたぶんを呼び出し側が
                            // DBへ反映できなくなり、一覧には居るのに実体が無い行になる
                            if first_err.is_none() {
                                first_err = Some(format!("ゴミ箱へ移動できません: {e}"));
                            }
                        }
                    }
                }
            }
        }
        done = end;
        size = size.saturating_mul(2).min(TRASH_CHUNK_MAX);
        on_progress(done, total);
    }
    (gone, first_err)
}

/// ファイルを**ゴミ箱へ**移動し、DBからも取り除く。
///
/// 完全削除はしない（写真は取り返しがつかないため、OSのゴミ箱経由にして
/// 誤操作から戻せるようにする）。ゴミ箱に入れられたものだけをDBから消す。
/// 戻り値は実際に削除できた件数。
///
/// **別スレッドで走らせる**（`export_media` と同じ形）。シェルのゴミ箱APIは
/// 1件あたり中央20ms・最悪215msの固定費で、**実機の500件（2MB×500）で約4.9秒**
/// かかった（2026-08-19の実測。`dev/plan.0.2.research.md` §2-1 の外挿2.3秒より遅い）。
/// 同期コマンドのままだと、その間メインスレッドが塞がって
/// **呼び出し側の「移動中…」表示ごと固まる**。
///
/// 4.9秒は「押して待つ」には長いので、束ごとに `delete-progress` を出して
/// 件数が動くようにしてある（`dev/plan.0.2.rev.md` 3-3 の「3秒超なら進捗表示へ」）。
/// 束を刻むぶんは遅くなるので、[`TRASH_CHUNK_FIRST`] のとおり小さく始めて倍にする。
#[tauri::command]
async fn delete_media(app: tauri::AppHandle, ids: Vec<i64>) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // 消えているIDは黙って飛ばす（選択したあとに外から消された等）
        let paths: Vec<PathBuf> = ids
            .iter()
            .filter_map(|id| path_of(&state, *id).ok())
            .collect();
        // **サイドカーも一緒にゴミ箱へ**（0.2・`dev/loadmap.md` 1.1）。
        // `.xmp` は一覧に出ないので、置いていくと**誰にも見えない迷子**になる
        // ——写真だけ消えたフォルダに現像設定だけが残り続ける。
        // 取り込みで一緒に運んだものを、消すときだけ置いていく道理も無い。
        //
        // ただし**残る相方のものは奪わない**（`Companions`）。RAW+JPGのうち
        // JPGだけを消すとき、`IMG_0001.xmp` はたいていRAWのものだ。
        let sidecar_exts = lock_ok(&state.config).import.sidecar_extensions.clone();
        let mut media: Vec<PathBuf> = Vec::with_capacity(paths.len());
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for path in paths {
            if seen.insert(path.clone()) {
                media.push(path);
            }
        }
        let mut companions = pictkura_core::sidecar::Companions::new(media.iter().cloned());
        let mut sidecars: Vec<PathBuf> = Vec::new();
        // その影が「誰のものか」。**全員をゴミ箱へ送れたときだけ**一緒に送る
        // （下の絞り込みで使う）
        let mut owners: Vec<Vec<PathBuf>> = Vec::new();
        let mut where_is: std::collections::HashMap<PathBuf, usize> =
            std::collections::HashMap::new();
        for path in &media {
            for sidecar in companions.sidecars_of(path, &sidecar_exts) {
                // **同じものを2回積まない**。組を両方選ぶと共有の `.xmp` に2回
                // 行き当たり、束での削除が「もう無い」で落ちて**1件ずつへ降りる**
                // （1件200〜300msの固定費を払い直す）。分母も水増しされる
                match where_is.get(&sidecar) {
                    Some(&i) => owners[i].push(path.clone()),
                    None if seen.insert(sidecar.clone()) => {
                        where_is.insert(sidecar.clone(), sidecars.len());
                        sidecars.push(sidecar);
                        owners.push(vec![path.clone()]);
                    }
                    None => {}
                }
            }
        }
        // 分母は写真＋影。**進む数字が止まらない**ほうを取る
        let shadows = sidecars.len();
        let media_total = std::cell::Cell::new(0usize);
        let (deleted_media, first_err) = trash_paths_with_progress(media, |done, total| {
            media_total.set(total);
            let _ = app.emit(
                "delete-progress",
                DeleteProgress {
                    done,
                    total: total + shadows,
                },
            );
        });
        // **消せなかった写真の影は置いていく**（ゲート2の指摘）。ゴミ箱への移動は
        // 失敗することがある（現像ソフトが握っている等）。写真が元の場所に残ったのに
        // その `.xmp` だけ送ってしまうと、**残った写真から現像設定が剥がれる**
        // ——一覧に出ないファイルなので、戻せるとしても気付く手立てが無い
        let trashed: std::collections::HashSet<&PathBuf> = deleted_media.iter().collect();
        let sidecars: Vec<PathBuf> = sidecars
            .into_iter()
            .zip(owners)
            .filter(|(_, owners)| owners.iter().all(|o| trashed.contains(o)))
            .map(|(sidecar, _)| sidecar)
            .collect();
        // **影は別に送る**。ここでの失敗で写真の結果を書き換えない——
        // 現像ソフトが `.xmp` を握っているだけで「削除できませんでした」を返すと、
        // 写真が全部消えていても画面は失敗の側へ倒れ、選別の関所が閉じない
        let base = media_total.get();
        let (_, sidecar_err) = trash_paths_with_progress(sidecars, |done, total| {
            let _ = app.emit(
                "delete-progress",
                DeleteProgress {
                    done: base + done,
                    total: base + total.max(shadows),
                },
            );
        });
        if let Some(e) = sidecar_err {
            // 写真は消えているのに `.xmp` だけが残る。一覧に出ないので
            // 気付く手立てが無い——せめて記録には残す
            eprintln!("サイドカーをゴミ箱へ移せませんでした（無視して継続）: {e}");
        }
        // **DBから落とすのは写真のぶんだけ**。サイドカーは行を持っていない
        if !deleted_media.is_empty() {
            lock_ok(&state.db)
                .remove_paths(&deleted_media)
                .map_err(|e| e.to_string())?;
        }
        // 数えて返すのも写真だけ——利用者が見ているのは「何枚消えたか」
        let count = deleted_media.len();
        match first_err {
            Some(e) if count == 0 => Err(e),
            // 一部だけ失敗したことは伝える。件数を添えないと、利用者からは
            // 「何枚消えたのか」が分からない
            Some(e) => Err(format!("{e}（{count}枚は移動できました）")),
            None => Ok(count),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 書き出し（コピー／移動）の結果。
#[derive(serde::Serialize)]
struct ExportStatsDto {
    done: usize,
    skipped: usize,
    failed: usize,
    /// コピーはできたが、**元を消せなかった**件数（移動のときだけ）。
    /// 両方に残っているので、黙って「移動しました」と言ってはいけない
    left_behind: usize,
}

/// 選んだものを、指定のフォルダへ**コピー／移動**する。
///
/// 書き出し先は利用者がフォルダ選択ダイアログで選んだ場所（ライブラリの外が普通）。
/// 移動でライブラリから出たぶんは、**DBの行も落とす**——残すと、一覧には居るのに
/// 実体が別の場所にある行になる。移動先がライブラリの中なら、監視が拾い直す。
#[tauri::command]
async fn export_media(
    app: tauri::AppHandle,
    ids: Vec<i64>,
    dest: String,
    move_files: bool,
) -> Result<ExportStatsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // 消えているIDは黙って飛ばす（選択したあとに外から消された等）
        let paths: Vec<PathBuf> = ids
            .iter()
            .filter_map(|id| path_of(&state, *id).ok())
            .collect();
        let mode = if move_files {
            pictkura_core::ExportMode::Move
        } else {
            pictkura_core::ExportMode::Copy
        };
        let progress_app = app.clone();
        // サイドカー（`.xmp` 等）も一緒に運ぶ（0.2）。設定で空にすれば運ばない
        let sidecar_exts = lock_ok(&state.config).import.sidecar_extensions.clone();
        let outcome = pictkura_core::export_files(
            &paths,
            Path::new(&dest),
            mode,
            &sidecar_exts,
            move |done, total, path| emit_export_progress(&progress_app, done, total, path),
        )
        .map_err(|e| e.to_string())?;

        let mut stats = ExportStatsDto {
            done: outcome.stats.done,
            skipped: outcome.stats.skipped,
            failed: outcome.stats.failed,
            left_behind: 0,
        };
        // 別のドライブへ移したぶんは、コピーが済んでいて元が残っている。
        // **元はゴミ箱へ**送る——「アプリがファイルを直接消すことはない」を移動でも守る
        let mut gone = outcome.moved;
        if !outcome.to_remove.is_empty() {
            let asked = outcome.to_remove.len();
            let (trashed, _) = trash_paths(outcome.to_remove);
            stats.left_behind = asked - trashed.len();
            gone.extend(trashed);
        }
        // **サイドカーの元も片付ける**（別ドライブへ移したぶん）。DBに行は無いので
        // 落とすものは無く、**件数にも数えない**——利用者が見ているのは写真の枚数
        if !outcome.sidecars_to_remove.is_empty() {
            let (_, err) = trash_paths(outcome.sidecars_to_remove);
            if let Some(e) = err {
                // 移した先には在るのに、元の `.xmp` も残る。一覧に出ない
                // ファイルなので利用者からは見えない——記録には残す
                eprintln!("移動元のサイドカーを片付けられませんでした: {e}");
            }
        }
        if !gone.is_empty() {
            lock_ok(&state.db)
                .remove_paths(&gone)
                .map_err(|e| e.to_string())?;
            let _ = app.emit("library-updated", ());
        }
        Ok(stats)
    })
    .await
    .map_err(|e| e.to_string())?
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

/// ビューアの選別キー（`P` / `U`）を押したあと、次の絵へ自動で送るかを切り替える（0.2 ②）。
#[tauri::command]
fn set_auto_advance(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    update_config(&state, |c| c.viewer.auto_advance = enabled)
}

/// USB/SDカードを挿したときの「自動再生」の候補に pictkura を出すかを切り替える。
///
/// **レジストリを先に書き、成功したら設定を保存する**。逆順だと、書けなかったときに
/// 設定だけ変わって「切ったはずなのに候補が残る」というずれ方をする。
///
/// 起動時にも同じ処理が走るが、ここでその場で反映するのが要点。**アプリを消す前に
/// 切る**という使い方では、切ったあとに起動し直す機会が無い。
#[tauri::command]
fn set_register_autoplay(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if enabled {
        autoplay::register(&exe).map_err(|e| e.to_string())?;
    } else {
        autoplay::unregister().map_err(|e| e.to_string())?;
    }
    // 設定の保存に失敗したら**レジストリを元へ戻す**。戻さないと、画面と設定ファイルは
    // 元のまま・レジストリだけ変わった状態で残り、しかも次の起動で起動時の同期処理が
    // 設定に合わせて戻す——利用者から見ると「切ったのに戻っている」。
    if let Err(e) = update_config(&state, |c| c.import.register_autoplay = enabled) {
        let rollback = if enabled {
            autoplay::unregister()
        } else {
            autoplay::register(&exe)
        };
        return Err(match rollback {
            Ok(()) => e,
            Err(re) => format!("{e}（レジストリを元に戻すのにも失敗: {re}）"),
        });
    }
    Ok(())
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

/// 起動時の同期が終わったか。**イベントを取りこぼしたフロント用**。
///
/// [`get_startup_report`] とは別に要る——報告が出ないまま終わる道があるため
/// （`AppState::startup_done` の説明）。
#[tauri::command]
fn startup_scan_finished(state: tauri::State<'_, AppState>) -> bool {
    state
        .startup_done
        .load(std::sync::atomic::Ordering::Acquire)
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
                picked: db.count_picked()?,
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

/// 選別の印（⚑ Pick。0.2 ②）を設定する。★とは別の列を触る。
#[tauri::command]
fn set_picked(state: tauri::State<'_, AppState>, id: i64, picked: bool) -> Result<(), String> {
    lock_ok(&state.db)
        .set_picked(id, picked)
        .map_err(|e| e.to_string())
}

/// 選別の印をまとめて付ける・外す（複数選択の一括操作）。
#[tauri::command]
fn set_pickeds(
    state: tauri::State<'_, AppState>,
    ids: Vec<i64>,
    picked: bool,
) -> Result<usize, String> {
    lock_ok(&state.db)
        .set_pickeds(&ids, picked)
        .map_err(|e| e.to_string())
}

/// お気に入り（★）を設定する。
#[tauri::command]
fn set_favorite(state: tauri::State<'_, AppState>, id: i64, favorite: bool) -> Result<(), String> {
    lock_ok(&state.db)
        .set_favorite(id, favorite)
        .map_err(|e| e.to_string())
}

/// お気に入りをまとめて付ける・外す（複数選択の一括操作）。
///
/// 1件ずつ `set_favorite` を呼ぶ形にしないのは、数千件でその数だけコミットが
/// 走るため。DB側で1つのトランザクションにまとめている。
#[tauri::command]
fn set_favorites(
    state: tauri::State<'_, AppState>,
    ids: Vec<i64>,
    favorite: bool,
) -> Result<usize, String> {
    lock_ok(&state.db)
        .set_favorites(&ids, favorite)
        .map_err(|e| e.to_string())
}

/// 検索条件に一致する**IDだけ**を、一覧に並ぶ順で返す。
///
/// 範囲選択（Shift+クリック）と全選択のためのもの。一覧は日ごとに遅延読み込み
/// するので、画面から見えている範囲だけで選択を決めると
/// **まだ読んでいない日の写真が黙って外れる**。IDなら1件8バイトなので、
/// 3万件でも240KB——選択のたびに引き直しても割に合う。
///
/// 読み取り専用プールを使う（書き込みロックを取らない）。
#[tauri::command]
fn list_media_ids(
    state: tauri::State<'_, AppState>,
    query: String,
    filter: pictkura_core::MediaFilter,
) -> Result<Vec<i64>, String> {
    let query = pictkura_core::parse_query(&query, filter);
    state
        .read_pool
        .with(|db| db.search_ids(&query))
        .map_err(|e| e.to_string())
}

/// 範囲選択（Shift+クリック）で、**2点に挟まれたIDだけ**を取る。
///
/// 全IDを返す `list_media_ids` を範囲選択に使うと、隣り合う2枚のために
/// 一覧の全件がIPCを渡る。切り出しはDB側でやる。
#[tauri::command]
fn list_media_ids_between(
    state: tauri::State<'_, AppState>,
    query: String,
    filter: pictkura_core::MediaFilter,
    from_id: i64,
    to_id: i64,
) -> Result<Vec<i64>, String> {
    let query = pictkura_core::parse_query(&query, filter);
    state
        .read_pool
        .with(|db| db.search_ids_between(&query, from_id, to_id))
        .map_err(|e| e.to_string())
}

/// 渡されたIDのうち、**いまの条件で実際に一覧に並んでいるもの**だけを返す。
///
/// 一括操作の直前の確認用。選択したあとに★を外した・撮影日が確定して
/// 検索から外れた等で、画面に出ていないものが選択に残ることがある。
#[tauri::command]
fn visible_media_ids(
    state: tauri::State<'_, AppState>,
    query: String,
    filter: pictkura_core::MediaFilter,
    ids: Vec<i64>,
) -> Result<Vec<i64>, String> {
    let query = pictkura_core::parse_query(&query, filter);
    state
        .read_pool
        .with(|db| db.visible_ids(&query, &ids))
        .map_err(|e| e.to_string())
}

/// 選択をビューアの**スコープ**として固定するために、
/// 「いま並んでいるものだけ」を**一覧と同じ並び**で、その日と一緒に返す（0.2 ②）。
///
/// [`visible_media_ids`] との違いは並びと `day_key`。ビューアは位置を
/// `(day_key, id)` で持つので、隣へ送るにはその日が要る。選択はJS側では
/// 集合（入れた順）なので、並べ直しをフロントでやると一覧とずれる。
#[tauri::command]
fn scope_media(
    state: tauri::State<'_, AppState>,
    query: String,
    filter: pictkura_core::MediaFilter,
    ids: Vec<i64>,
) -> Result<Vec<ScopeItemDto>, String> {
    let query = pictkura_core::parse_query(&query, filter);
    state
        .read_pool
        .with(|db| db.visible_ids_in_order(&query, &ids))
        .map(|rows| {
            rows.into_iter()
                .map(|(id, day_key)| ScopeItemDto { id, day_key })
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// [`scope_media`] が返す1件。
#[derive(serde::Serialize, Clone)]
struct ScopeItemDto {
    id: i64,
    day_key: i64,
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
    /// `DCIM` があれば**その絶対パス**（カメラメディアの可能性が高い）。
    ///
    /// 真偽値ではなくパスを返すのは、**UI側で組み直させないため**。
    /// 以前は `path + "DCIM"` と繋いでいて区切りを足しておらず、
    /// Windowsは `E:\` に末尾の区切りが付くのでたまたま通っていたが、
    /// macOSでは `/Volumes/NO NAME` ＋ `DCIM` ＝ `/Volumes/NO NAMEDCIM` という
    /// 存在しないパスになり、**カードを押すとウィザードが空で開いていた**
    dcim_path: Option<String>,
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
            // **OSが「利用者に見せない」と印を付けた場所は出さない。**
            // macOSの `/System/Volumes/Data` 等がこれで、Finderも隠している。
            // パスの形で当てずに、マウントの旗（`MNT_DONTBROWSE`）を読む
            if !is_browsable(&mount) {
                return None;
            }
            let label = drive_label(&d.name().to_string_lossy(), &mount);
            let kind = drive_kind(&mount, d.is_removable());
            Some(DriveDto {
                label,
                path: mount.to_string_lossy().into_owned(),
                removable: d.is_removable() || kind == "removable",
                kind,
                dcim_path: dcim_under(&mount, kind),
            })
        })
        .collect();
    // **`DCIM` を持つものを先頭に出す。** カメラのカードは必ず `DCIM` を持つので、
    // 「取り外せるか」より確かな合図になる。macOSでは外付けのバックアップ用ディスクも
    // `is_removable()` が真になり、SDカードと見分けが付かない
    // （Windowsの `GetDriveTypeW` に当たる粒度のAPIが無い）。**消すのではなく上げる**
    // ——判定を誤って消すと「挿したのに出ない」になり、そちらの方が困る。
    //
    // 同じ組の中はマウント位置（Windowsならドライブレター）順。OSの列挙順のままだと
    // C: D: E: の並びが起動ごとに入れ替わって見え、目的のドライブを探しにくい
    drives.sort_by(|a, b| {
        b.dcim_path
            .is_some()
            .cmp(&a.dcim_path.is_some())
            .then_with(|| a.path.to_uppercase().cmp(&b.path.to_uppercase()))
    });
    drives
}

/// このドライブ配下の `DCIM` のパス（無ければ `None`）。
///
/// **パスを組むのはここだけ。** 以前はUI側が `path + "DCIM"` と繋いでいて
/// 区切りを足しておらず、Windowsは `E:\` に末尾の区切りが付くのでたまたま
/// 通っていたが、macOSでは `/Volumes/NO NAME` ＋ `DCIM` ＝
/// `/Volumes/NO NAMEDCIM` という存在しないパスになっていた。
///
/// ネットワークドライブは触らない——有無の確認だけで回線越しに待たされる。
fn dcim_under(mount: &Path, kind: &str) -> Option<String> {
    if kind == "network" {
        return None;
    }
    let dcim = mount.join("DCIM");
    dcim.is_dir().then(|| dcim.to_string_lossy().into_owned())
}

/// ドライブ一覧の見出しを組む。
///
/// Windowsは `C:\` → `C:`（ドライブレター）でよいが、**macOSのルートは `/` で、
/// 末尾を削ると空になる**。実際に `Macintosh HD ()` という空の括弧が出ていた
/// （2026-08-26に実機で確認）。削り切ったら削らない。
fn drive_label(name: &str, mount: &Path) -> String {
    let raw = mount.to_string_lossy();
    let trimmed = raw.trim_end_matches(['\\', '/']);
    let location = if trimmed.is_empty() {
        raw.as_ref()
    } else {
        trimmed
    };
    if name.is_empty() {
        location.to_string()
    } else {
        format!("{name} ({location})")
    }
}

/// このマウントを利用者に見せてよいか。
///
/// macOSは「Finderに出さない」ボリュームに `MNT_DONTBROWSE` を立てる
/// （`/System/Volumes/Data` や各種のシステム用ボリューム）。**OS自身の合図**なので、
/// `/System/Volumes/` というパスの形で当てるより確か。
///
/// 他のOSでは判定材料が無いので常に真。Windowsはそもそもドライブレターしか出ない。
fn is_browsable(mount: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(path) = std::ffi::CString::new(mount.as_os_str().as_bytes()) else {
            return true;
        };
        // SAFETY: `statfs` はNUL終端のパスと出力用の構造体を取る。
        // 構造体はゼロ初期化してから渡し、成功したときだけ読む
        let mut buf = unsafe { std::mem::zeroed::<libc::statfs>() };
        if unsafe { libc::statfs(path.as_ptr(), &mut buf) } != 0 {
            // 読めないものは隠さない（消し過ぎるより出し過ぎる方が安全）
            return true;
        }
        buf.f_flags & libc::MNT_DONTBROWSE as u32 == 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mount;
        true
    }
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

/// 書き出しの進捗。**ファイル名だけ**を送る（一覧の外の場所を webview へ渡さない）。
#[derive(Clone, serde::Serialize)]
struct ExportProgress {
    done: usize,
    total: usize,
    name: String,
}

/// ゴミ箱へ移している最中の進み具合。**名前は載せない**——書き出しと違って、
/// 消えていくものの名前が流れても手掛かりにならず、不安を煽るだけになる。
#[derive(Clone, serde::Serialize)]
struct DeleteProgress {
    done: usize,
    total: usize,
}

fn emit_export_progress(app: &tauri::AppHandle, done: usize, total: usize, path: &Path) {
    // 取り込みと同じ間引き（1件ごとに送ると、数千件でイベントが溢れる）
    let step = (total / 200).max(1);
    if !done.is_multiple_of(step) && done != total {
        return;
    }
    let _ = app.emit(
        "export-progress",
        ExportProgress {
            done,
            total,
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
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
            .unwrap_or_else(|_| server_error())
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
            .unwrap_or_else(|_| server_error()),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap_or_else(|_| server_error()),
    }
}

/// 応答を組み立てられなかったときに返すもの（`dev/loadmap.md` 1.3）。
///
/// `http` のビルダーが `Err` を返すのは、直前に置いた状態やヘッダが
/// 壊れているときだけで、ここで渡すのはどれも定数なので通らない。
/// それでも `unwrap` を置かないのは、**ここで落ちると WebView の要求を捌く
/// スレッドがそのまま死ぬ**から——一覧が丸ごと白いまま、何も起きなくなる。
/// 空の500へ均せば、その1枚が出ないだけで済む。
fn server_error() -> Response<Vec<u8>> {
    let mut res = Response::new(Vec::new());
    *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    res
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
        .unwrap_or_else(|_| server_error())
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

    let fail = |status: StatusCode| {
        Response::builder()
            .status(status)
            .body(Vec::new())
            .unwrap_or_else(|_| server_error())
    };
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
                        .unwrap_or_else(|_| server_error()),
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
                    .unwrap_or_else(|_| server_error())
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
            .unwrap_or_else(|_| server_error()),
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
            .unwrap_or_else(|_| server_error())
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
    if kind == ServeKind::Thumb && record.thumb_state == 2 {
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
    // 表示用JPEGキャッシュの鍵に使う。この下で record.path / record.thumb_path を
    // 取り出す（部分ムーブ）ので、先に控えておく
    let mtime_ms = record.mtime_ms;
    // 動画の再生（第9部）。ここだけが原本を丸ごと相手にする経路で、
    // Rangeで刻んで返す。WebViewが扱えないコンテナ（.m2ts/.avi）は渡しても
    // 黒い枠になるだけなので、415を返してUIの「既定のアプリで開く」へ寄せる
    if kind == ServeKind::Video {
        if !is_video {
            return not_found();
        }
        if !pictkura_core::video::plays_in_webview(&record.path) {
            return Response::builder()
                .status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                .body(Vec::new())
                .unwrap_or_else(|_| server_error());
        }
        return video_response(&record.path, range, mime_for_path(&record.path));
    }
    // 動画の**絵**の要求は原本へ流さない。`<img>` に動画を渡しても絵にならず、
    // 下の `std::fs::read` はファイルを丸ごとメモリへ載せる。
    // サムネイル（あれば）で代用し、無ければ空タイルを出す
    let path = match kind {
        ServeKind::Thumb | ServeKind::Full if is_video => match record.thumb_path {
            Some(thumb) => thumb,
            None => return blank_thumb(),
        },
        // 上の `if kind == ServeKind::Video` で必ず返しているので通らない。
        // それでも `unreachable!` を置かないのは、**通ったときに落ちる**のが
        // ここでいちばん高くつくため（要求を捌くスレッドが死ぬ）
        ServeKind::Video => return not_found(),
        ServeKind::Thumb => match record.thumb_path {
            Some(thumb) => thumb,
            None if !can_serve_original(&record) => return blank_thumb(),
            None => record.path,
        },
        ServeKind::Full => record.path,
    };

    // WebViewが描けない形式の原寸表示: RAW（第6部 段階F）・HEIC（第7部 段階G）・
    // TIFF。いずれも原本をそのまま返しても絵にならないのでJPEGへ詰め直す。
    // AVIF・SVG・BMP・GIF はブラウザが直接描けるので、この下で原本を返す
    if kind == ServeKind::Full && pictkura_core::thumbs::needs_display_transcode(&path) {
        // 詰め直しは高い（実測: HEIC 0.6〜1秒 / TIFF 約300ms）ので、できた
        // バイト列をLRUに残す（0.2 ①）。ビューアの先読みが投げる要求も
        // ここを温めるので、フロントの画素キャッシュが崖で全滅しても
        // 戻ってきたときに払うのはファイル読み出しぶんだけになる。
        //
        // **鍵に mtime を含める**——同じidでもファイルが差し替われば別物
        let key = pictkura_core::display_cache::Key { id, mtime_ms };
        let served = |bytes: Vec<u8>| {
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "image/jpeg")
                .header("Cache-Control", "max-age=31536000, immutable")
                .body(bytes)
                .unwrap_or_else(|_| server_error())
        };
        if let Some(hit) = state.display_cache.get(key) {
            return served(hit.as_ref().clone());
        }
        // ここから先は実際に詰め直す。**わざと直列化していない**（0.2 ①）。
        //
        // 一度は「同時に走るのは1枚だけ」と錠を掛けたが、それだと
        // **いま見ている1枚が、もう捨てた先読みの後ろで待つ**——1枚1秒級の
        // HEICでは、先読みを入れたせいで送りがかえって遅くなる。しかも
        // 隠し `<img>` を外しても、走り出した `spawn_blocking` は取り消せない。
        //
        // 同時に走る数は**投げる側で絞る**: 先読みは「表示中の1枚が出てから
        // 隣の1枚だけ」というのが `ui/src/App.tsx` の約束で、押しっぱなしの
        // 送りでも1回の操作につき1枚しか増えない。ここは要求のたびに
        // そのまま作る（この経路はPR #20の前からずっとそうだった）
        return match pictkura_core::thumbs::display_jpeg(&path) {
            Some(bytes) => {
                let bytes = std::sync::Arc::new(bytes);
                state
                    .display_cache
                    .insert(key, std::sync::Arc::clone(&bytes));
                served(bytes.as_ref().clone())
            }
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
            .unwrap_or_else(|_| server_error()),
        Err(_) => not_found(),
    }
}

/// 設定ファイルの置き場。`tauri::path::app_config_dir` と**同じ場所**を、
/// アプリを組み立てる前に知るための計算。
///
/// Windowsでは `%APPDATA%\<identifier>`。identifier が `tauri.conf.json` と
/// ずれないよう、下のテストで突き合わせている。
#[cfg(windows)]
fn config_path_without_app() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join(APP_IDENTIFIER)
            .join("pictkura.toml"),
    )
}

/// AutoPlayの登録を設定に合わせ直す（`--sync-autoplay`）。
///
/// 設定ファイルが無ければ**何もしない**。まだ一度も使っていない人の環境に、
/// 起動前から自動再生の候補を足さないため。
#[cfg(windows)]
fn sync_autoplay_with_config() -> Result<(), String> {
    let Some(path) = config_path_without_app() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let config = Config::load(&path).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if config.import.register_autoplay {
        autoplay::register(&exe).map_err(|e| e.to_string())
    } else {
        autoplay::unregister().map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
fn sync_autoplay_with_config() -> Result<(), String> {
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // アンインストール前の明示的な解除口。AutoPlayの登録は**実行時にHKCUへ**
    // 書くので、アプリを消してもレジストリには残り、挿すたびに居ないpictkuraを
    // 呼ぶ候補が並び続ける。設定のトグルを切っても同じ処理が走るが、
    // ポータブル版を消すときや、アンインストーラから呼ぶときのために
    // 窓を出さずに解除だけできる入口を用意する。
    if std::env::args().any(|a| a == "--unregister-autoplay") {
        if let Err(e) = autoplay::unregister() {
            eprintln!("AutoPlayの解除に失敗: {e}");
            std::process::exit(1);
        }
        return;
    }

    // AutoPlayの登録を**設定に合わせ直す**だけの入口（窓を出さない）。
    //
    // インストーラは「消してから入れる」ので、入れ終わった時点で登録が消えている
    // ことがある——**更新**（古い版のアンインストーラが走る）と、**MSI版からの
    // 乗り換え**（MSI側の掃除 `windows/autoplay-cleanup.wxs` が走る）の2つ。
    // どちらもアプリは在るのに、次に起動するまで自動再生の候補から居なくなる。
    // NSISの `NSIS_HOOK_POSTINSTALL` からここを呼んで揃え直す。
    //
    // **設定を読んでから決める**——自動再生を自分で切った人に、乗り換えを機に
    // 勝手に名乗らせない。**設定ファイルが無ければ何もしない**（＝まだ一度も
    // 使っていない人。起動前から候補に並べるのは越権）
    if std::env::args().any(|a| a == "--sync-autoplay") {
        if let Err(e) = sync_autoplay_with_config() {
            eprintln!("AutoPlayの同期に失敗（無視）: {e}");
        }
        return;
    }

    let run = tauri::Builder::default()
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
                    }
                }
                // **写真フォルダが無くても必ず書く**。設定ファイルの有無を
                // 「一度でも使ったか」の目印にしている場所がある
                // （インストーラから呼ぶ `--sync-autoplay`）。書かないと、
                // 使っている人を「まだ使っていない人」と読み違える。
                //
                // **引き換えに失うもの**: 以前は保存しないことで次回も
                // `first_launch` 扱いになり、「写真フォルダが後から現れたら拾う」
                // 再試行が暗黙に効いていた（OneDriveの移動が済んでいない初回など）。
                // これからは初回に無ければ自動では足さない——設定から手で足せる
                config.save(&config_path)?;
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
            let pending_import =
                import_path_from_args(&std::env::args().collect::<Vec<_>>()).map(narrow_to_dcim);
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
                startup_done: std::sync::atomic::AtomicBool::new(false),
                scan_unreadable: Mutex::new(ScanUnreadable::default()),
                root_probes: RootProbes::default(),
                index_progress: Mutex::new(IndexProgressDto::default()),
                browse_allow: Mutex::new(HashSet::new()),
                pending_import: Mutex::new(pending_import),
                display_cache: pictkura_core::display_cache::DisplayCache::default(),
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
            // 索引が落ちたときに「作成中」を畳むための控え（本体は下の閉包が持っていく）
            let index_fail_handle = app.handle().clone();
            std::thread::spawn(move || {
                let finished =
                    pictkura_core::panics::catching("全文索引の作成", move || {
                        let state = index_handle.state::<AppState>();
                        let Ok(mut db) = Db::open(&index_db_path) else {
                            return;
                        };
                        let publish = |phase: &str,
                                       done: i64,
                                       total: i64,
                                       building: bool,
                                       incomplete: bool| {
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
                                    .filter(|(_, path)| {
                                        !pictkura_core::cloud::is_cloud_only_path(path)
                                    })
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

                        // 第3段: 寸法の後追い補完（第6部 段階F-4）。段階F-4より前に
                        // 取り込んだRAWは `width/height` に埋め込みプレビューの寸法が
                        // 入ったままなので、原本の申告を読んで入れ直す。
                        //
                        // **帯は出さない**。直しても見た目はほぼ変わらない
                        // （UIは `preview_width` がNULLなら `width` へ落とすので、
                        // 配る絵の寸法は前後で同じ。動くのは一覧の枠の縦横比だけで、
                        // 原本とプレビューは同じ絵なので比もほぼ同じ）。
                        // 動いた行は下で `media-updated` を出す。
                        //
                        // **サムネイルは作り直さない**。キューへ投げると絵まで作り直す
                        // ことになるが、絵は既に正しい。ここは絵を1枚も起こさない
                        // （値段は `thumbs::read_exif_declaration` に書いた。
                        // TIFF系RAWはコンテナ読みがファイル全体を読むので安くはない）
                        let mut after_id = 0i64;
                        while let Ok(batch) = db.dimensions_to_backfill(after_id, 200) {
                            if batch.is_empty() {
                                break;
                            }
                            after_id = batch.last().map(|t| t.id).unwrap_or(after_id);
                            type Backfilled = (
                                pictkura_core::db::DimensionTarget,
                                pictkura_core::db::Dimensions,
                            );
                            let results: Vec<Backfilled> = batch
                                .into_iter()
                                // 開けないファイル・クラウドにしか実体が無いファイルは
                                // **印を付けずに飛ばす**（カメラ補完と同じ理由）。
                                // ここで「確かめた」と書くと、外付けを繋いだ日が来ても
                                // 二度と読み直されない
                                .filter(|t| t.path.is_file())
                                .filter(|t| !pictkura_core::cloud::is_cloud_only_path(&t.path))
                                // **開けなかった行は印を付けずに飛ばす**。権限や
                                // 共有ロックで一時的に読めないだけなら、読める日に
                                // 拾い直せる（ゲート1のP2）
                                .filter_map(|t| {
                                    let dims = pictkura_core::thumbs::backfilled_dimensions(
                                        &t.path, t.width, t.height,
                                    )?;
                                    Some((t, dims))
                                })
                                .collect();
                            // 丸ごと飛ばした束（外付けが未接続・クラウドのみ）で
                            // 空の書き込みトランザクションを開かない
                            if results.is_empty() {
                                std::thread::sleep(std::time::Duration::from_millis(20));
                                continue;
                            }
                            match db.set_dimensions(&results) {
                                // **寸法が動いた行はUIへ知らせる**。一覧の枠は
                                // `width/height` から決まるので、知らせないと
                                // 次に開き直すまで古い縦横比のまま（ゲート1のP2）
                                Ok(moved) => {
                                    for id in moved {
                                        if let Ok(Some(rec)) = db.get_by_id(id) {
                                            let _ = index_handle
                                                .emit("media-updated", MediaItemDto::from(rec));
                                        }
                                    }
                                }
                                Err(_) => break, // 印を付けていないので次回起動でやり直せる
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                    });
                // **「作成中」を出したまま消えない**（ゲート1の指摘）。ここで落ちると
                // `building` が真のまま残り、UIは永久に作成中の帯を出し続ける。
                // 途中までの索引は次回起動が続きから作り直す（カーソルは進めていない）
                if finished.is_none() {
                    let state = index_fail_handle.state::<AppState>();
                    let progress = IndexProgressDto {
                        building: false,
                        incomplete: true,
                        phase: "index".into(),
                        done: 0,
                        total: 0,
                    };
                    *lock_ok(&state.index_progress) = progress.clone();
                    let _ = index_fail_handle.emit("index-progress", progress);
                }
            });

            // サムネイルキャッシュのジャニター（段階B-3）: 60秒ごとに
            // タッチ記録をDBへフラッシュし、上限容量超過分をLRU削除する。
            // 順序が安全の要: フラッシュ→選定（キュー内ID除外＋直近利用ガード）→
            // DB更新→ファイル削除。途中でクラッシュしても「state=0だがファイルが
            // 残っている」で済み、再生成時に同名パスへ上書きされる
            let janitor_handle = app.handle().clone();
            let janitor_db_path = db_path.clone();
            std::thread::spawn(move || {
                // **掃除役を絶やさない**（ゲート1の指摘）。ここが死ぬとLRU削除が
                // 永久に止まり、サムネイルが上限を超えて**ディスクを埋め続ける**
                // ——しかも静かなので誰も気付かない。落ちたら開き直して回し直す
                // （DBの接続は中で開き直る。60秒眠ってから始まるので暴走しない）。
                //
                // **外側は `loop`**（`while ... .is_none()` にしない。ゲート2の指摘）。
                // 中の閉包に `break` や `return` を1つ書き足すだけで、正常終了と
                // 見なされて**掃除役が静かに永久停止する**——落ちないぶん誰も気付けない
                loop {
                    let _ = pictkura_core::panics::catching("サムネイルの掃除", || {
                        let cap_bytes = (thumb_cache_mb.min(8 * 1024 * 1024) as i64) * 1024 * 1024;
                        let mut db: Option<Db> = None;
                        loop {
                            std::thread::sleep(std::time::Duration::from_secs(60));
                            let state = janitor_handle.state::<AppState>();
                            let touches: Vec<(i64, i64)> =
                                lock_ok(&state.thumb_touches).drain().collect();
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
                                    let _ = janitor_handle
                                        .emit("media-updated", MediaItemDto::from(rec));
                                }
                            }
                        }
                    });
                }
            });

            // ライブラリルートのファイルシステム監視を開始（アプリ外の操作に追従）
            rebuild_watcher(app.handle());

            // 起動時にバックグラウンドでライブラリを同期し、完了をフロントへ通知する。
            // 方式（USN差分/枝刈り/フル）と所要時間は⚡爆速メーターとして別途知らせる
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let inner = handle.clone();
                let _ = pictkura_core::panics::catching("起動時の同期", move || {
                    let started = std::time::Instant::now();
                    let state = inner.state::<AppState>();
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
                    let _ = inner.emit("library-updated", SyncStatsDto::from(stats));
                    let _ = inner.emit("startup-scan-report", report);
                });
                // **どう終わってもここを通る。** 上は `startup_scan` が `Err` なら
                // 報告を出さずに返るし、パニックは `catching` が飲む。
                // フロントはこれが来るまで「空です」と言わないので、
                // **黙って終わると初回起動が永久に無言になる**（ゲート2の指摘）。
                // 旗を先に立てる: イベントを取りこぼしたフロントは後から聞きに来る
                handle
                    .state::<AppState>()
                    .startup_done
                    .store(true, std::sync::atomic::Ordering::Release);
                let _ = handle.emit("startup-scan-finished", ());
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
                // **1枚の絵で要求の捌き手を死なせない**（`dev/loadmap.md` 1.3）。
                // ここは壊れた写真を実際に展開する場所で、解読器は規格外の
                // バイト列にパニックで応えることがある。そのまま巻き戻すと
                // `responder` へ何も返らず、WebView側はその `<img>` を
                // **永久に待つ**（読み込み中のまま、タイルが1枚白く残る）。
                // 500を返しておけば、そのセルだけが空になって次へ進める
                let response =
                    pictkura_core::panics::catching(&format!("media要求 {uri}"), || {
                        handle_media_request(&state, &uri, range.as_deref())
                    })
                    .unwrap_or_else(|| {
                        // **一覧のタイルには壊れた画像アイコンを出さない**
                        // （ゲート1の指摘）。500も404と同じ絵になるので、
                        // `blank_thumb` と同じ透明PNGへ揃える——落ちた1枚が
                        // 空のセルになるだけで、割れた写真は並ばない
                        match parse_media_url(&uri) {
                            Some(MediaTarget::Library {
                                kind: ServeKind::Thumb,
                                ..
                            }) => blank_thumb(),
                            _ => server_error(),
                        }
                    });
                responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            timeline_summary,
            list_day,
            list_memories,
            get_stats,
            get_startup_report,
            startup_scan_finished,
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
            host_platform,
            empty_library_reason,
            open_decoder_help,
            get_index_progress,
            video_status,
            cloud_only_media,
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
            set_favorites,
            export_media,
            list_media_ids,
            list_media_ids_between,
            visible_media_ids,
            scope_media,
            set_picked,
            set_pickeds,
            set_auto_advance,
            set_register_autoplay,
            take_pending_import,
            update::check_update,
            update::open_download_page,
            update::set_check_update_on_start
        ])
        .run(tauri::generate_context!());
    // **起動できなかったときは黙って消えない**。`expect` で落とすと
    // Windowsでは何も出ないまま終わる（コンソールが無いため）ので、
    // 理由を書いてから終了コードで知らせる
    if let Err(e) = run {
        eprintln!("pictkura を起動できませんでした: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::APP_IDENTIFIER;
    use super::{dcim_under, drive_label, import_path_from_args};
    // 実物のリンクを張る試験は Unix だけ（Windowsでは未使用importが
    // `-D warnings` でエラーになる。ゲート2が実際に再現させて見つけた）
    #[cfg(unix)]
    use super::{rebase_to_root_spelling, root_specs};
    // Windows側は `RootSpec` を直に組む（実FSも `canonicalize` も要らない）
    #[cfg(windows)]
    use super::{rebase_to_root_spelling, Path, PathBuf, RootSpec};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_drive_label_never_loses_its_location() {
        use std::path::Path;

        // Windows: 末尾の区切りを削ってドライブレターにする
        assert_eq!(
            drive_label("ボリューム", Path::new("C:\\")),
            "ボリューム (C:)"
        );
        // **macOSのルート**。削り切ると空になるので削らない
        // （`Macintosh HD ()` が出ていた回帰）
        assert_eq!(
            drive_label("Macintosh HD", Path::new("/")),
            "Macintosh HD (/)"
        );
        // 普通のマウント位置はそのまま
        assert_eq!(
            drive_label("SDカード", Path::new("/Volumes/NO NAME")),
            "SDカード (/Volumes/NO NAME)"
        );
        // 名前が無いときは場所だけ（括弧を出さない）
        assert_eq!(drive_label("", Path::new("/")), "/");
        assert_eq!(drive_label("", Path::new("D:\\")), "D:");
    }

    /// 解決後の綴りで来ても、設定の綴りへ戻す。
    ///
    /// **リンクは自分で張る。** `temp_dir()` に頼ると、macOSでは
    /// `/var/folders/...`（`/private/var` へのリンク）なので回帰を突けるが、
    /// Linuxの `/tmp` はリンクではないため**CIでは素通りの経路しか通らない**
    /// ——直したはずの穴が開いても気付けない（ゲート2の指摘）
    #[cfg(unix)]
    #[test]
    fn a_resolved_symlink_is_rebased_to_the_configured_spelling() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // 設定は**リンクの綴り**。監視はリンクを解決した綴りを返す
        let specs = root_specs(std::slice::from_ref(&link));
        let resolved = std::fs::canonicalize(&link).unwrap();
        assert_ne!(resolved, link, "この題材でリンクが効いていること");

        let rebased = rebase_to_root_spelling(&resolved.join("2020").join("a.jpg"), &specs)
            .expect("ルート配下だと分かること");
        assert_eq!(
            rebased,
            link.join("2020").join("a.jpg"),
            "解決後の綴りで来ても、設定の綴りへ戻す"
        );

        // 設定の綴りのまま来たものはそのまま
        let same = link.join("b.jpg");
        assert_eq!(rebase_to_root_spelling(&same, &specs), Some(same.clone()));

        // ルートの外は「分からない」と返す（呼び出し側が判断する）
        assert_eq!(
            rebase_to_root_spelling(std::path::Path::new("/どこか/よそ/c.jpg"), &specs),
            None
        );

        // **隣のルートを配下と誤らない**（`/x/EXT` と `/x/EXTRA`）
        let sibling = dir.path().join("linkX");
        std::os::unix::fs::symlink(&real, &sibling).unwrap();
        assert_eq!(
            rebase_to_root_spelling(&link.with_file_name("linkX").join("d.jpg"), &specs),
            None,
            "綴りの前方一致だけで配下と決めない"
        );
    }

    /// 大小だけ違う綴りでも揃える（大小を区別しないボリューム）。
    ///
    /// 設定に `pictures` と書いてあって実物が `Pictures` のとき。
    /// `canonicalize` が `realpath(3)` 越しに**大小まで直して**返すので、
    /// 解決後の綴りを的に入れてある時点で当たる（実測で確認）。
    /// ゲート2が「直らない」と見たのは Python の `os.path.realpath` の挙動で、
    /// あちらはファイルシステムに聞かない。**当たることを固定しておく**
    #[cfg(target_os = "macos")]
    #[test]
    fn a_root_written_with_the_wrong_case_still_matches() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("MixedCase");
        std::fs::create_dir(&real).unwrap();
        let as_written = dir.path().join("mixedcase");
        if !as_written.is_dir() {
            // 大小を区別するボリューム（この題材が成り立たない）
            return;
        }

        // 設定には**小文字**で書かれている
        let specs = root_specs(std::slice::from_ref(&as_written));
        // 監視は**解決後かつ実物の大小**で返してくる（FSEventsの実際の形）
        let Ok(from_watcher) = std::fs::canonicalize(&real) else {
            return;
        };
        let event = from_watcher.join("a.jpg");
        assert_eq!(
            rebase_to_root_spelling(&event, &specs),
            Some(as_written.join("a.jpg")),
            "大小が違っても設定の綴りへ戻す"
        );
    }

    /// 同じ場所を指すルートが2つあっても、**設定どおりの綴りは書き換えない**。
    ///
    /// `/photos` と、そこへのリンク `/library` の両方をルートにすると、
    /// 解決後の綴りは**どちらにも当たる**。長さが同じなので順番次第で勝者が変わり、
    /// `/photos/a.jpg` のイベントが `/library/a.jpg` へ化けると
    /// **`/photos` 側の行が二度と更新されない**（ゲート1の指摘）
    #[cfg(unix)]
    #[test]
    fn two_roots_pointing_at_the_same_place_keep_their_own_spelling() {
        let dir = tempfile::tempdir().unwrap();
        // **土台の側がリンク越しだと、この試験は何も見ない。**
        // macOSの `$TMPDIR` は `/var/folders/…` にあり、`/var` 自身が
        // `/private/var` へのリンク。すると `link` の解決後は
        // `/private/var/…/photos` になって、題材のイベントパス
        // `/var/…/photos/a.jpg` と**そもそも当たらない**——勝者を争う場面に
        // 入らないので、1パスへ戻しても通ってしまう（ゲート1が実測で指摘）。
        // 先に土台を解決して、両者を同じ名前空間へ乗せる
        let base = dir.path().canonicalize().unwrap();
        let real = base.join("photos");
        let link = base.join("library");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // **リンクの方を先に**登録する（これで順番の運に頼れなくなる）
        let specs = root_specs(&[link.clone(), real.clone()]);

        // 実体の綴りで来たものは、実体の綴りのまま
        let from_real = real.join("a.jpg");
        assert_eq!(
            rebase_to_root_spelling(&from_real, &specs),
            Some(from_real.clone()),
            "設定どおりの綴りは書き換えない"
        );
        // リンクの綴りで来たものも、そのまま
        let from_link = link.join("b.jpg");
        assert_eq!(
            rebase_to_root_spelling(&from_link, &specs),
            Some(from_link.clone())
        );
    }

    /// 名前がUTF-8として読めなくても、バイトを壊さずに揃える。
    ///
    /// APFSは非UTF-8の名前を作らせない（実測で `EILSEQ`）ので**ファイルは作れない**が、
    /// **FAT32/exFATは通す**——外付けをライブラリのルートにすれば入りうる。
    /// `to_string_lossy()` を通すと U+FFFD に化け、その後の `stat` が
    /// **存在しないパス**を見にいく。ここはファイルを作らずに、
    /// パスの組み立てだけを見る
    #[cfg(unix)]
    #[test]
    fn a_non_utf8_name_survives_the_rebase() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let specs = root_specs(std::slice::from_ref(&link));
        let resolved = std::fs::canonicalize(&link).unwrap();

        // Shift_JISの「あ」＋ UTF-8として不正なバイト
        let odd = std::ffi::OsString::from_vec(b"\x82\xa0\xff.jpg".to_vec());

        let rebased = rebase_to_root_spelling(&resolved.join(&odd), &specs)
            .expect("ルート配下だと分かること");
        assert_eq!(
            rebased,
            link.join(&odd),
            "綴りは揃え、名前のバイトは触らない"
        );
        assert_eq!(
            rebased.file_name().map(|n| n.as_bytes().to_vec()),
            Some(odd.as_bytes().to_vec()),
            "U+FFFD に化けていないこと"
        );

        // 綴りが同じなら、そもそも組み直さない（同じ値が返る）
        let same = link.join(&odd);
        assert_eq!(rebase_to_root_spelling(&same, &specs), Some(same.clone()));
    }

    /// Windows側の綴り揃え。**この差分が2パスへ書き換えた側**なのに、
    /// 実物のリンクを張る試験は `#[cfg(unix)]` なので**1行も通っていなかった**
    /// （ゲート1の指摘）。しかも `rebase_to_root_spelling` のそもそもの
    /// 利用者であるUSNの差分はWindows専用で、綴りがズレると重複行＝★消えになる。
    ///
    /// `RootSpec` を直に組むので、実ファイルもジャンクションも要らない。
    #[cfg(windows)]
    #[test]
    fn windows_paths_come_back_in_the_configured_spelling() {
        let spec = |spelling: &str, resolved: &str| RootSpec {
            spelling: spelling.to_string(),
            prefixes: vec![spelling.to_string(), resolved.to_string()],
        };

        // 大小と区切りの揺れを畳んで、設定の綴りへ戻す
        let specs = vec![spec("C:\\foto\\bar", "D:\\real")];
        assert_eq!(
            rebase_to_root_spelling(Path::new("C:/FOTO/Bar\\a.jpg"), &specs),
            Some(PathBuf::from("C:\\foto\\bar\\a.jpg")),
            "大小も区切りも設定の綴りへ揃える"
        );
        // 解決後（ジャンクションの実体）で来たものも設定の綴りへ
        assert_eq!(
            rebase_to_root_spelling(Path::new("D:\\real\\b.jpg"), &specs),
            Some(PathBuf::from("C:\\foto\\bar\\b.jpg")),
            "実体パスで来ても、DBのキーは設定の綴り"
        );

        // **同じ実体を指す2ルート**。設定どおりに来たものは書き換えない
        // （2パスにした理由。1パスだともう片方の綴りへ化けうる）
        let both = vec![
            spec("C:\\link", "C:\\photos"),
            spec("C:\\photos", "C:\\photos"),
        ];
        assert_eq!(
            rebase_to_root_spelling(Path::new("C:\\photos\\a.jpg"), &both),
            Some(PathBuf::from("C:\\photos\\a.jpg")),
            "設定どおりの綴りは触らない"
        );
        assert_eq!(
            rebase_to_root_spelling(Path::new("C:\\link\\b.jpg"), &both),
            Some(PathBuf::from("C:\\link\\b.jpg")),
            "もう片方も同じ"
        );

        // **隣を配下と誤らない**（`C:\ext` に対する `C:\extra`）
        let ext = vec![spec("C:\\ext", "C:\\ext")];
        assert_eq!(
            rebase_to_root_spelling(Path::new("C:\\extra\\a.jpg"), &ext),
            None,
            "名前が前方一致するだけの別フォルダを配下にしない"
        );
        // ルート自身は配下
        assert_eq!(
            rebase_to_root_spelling(Path::new("c:/EXT"), &ext),
            Some(PathBuf::from("C:\\ext")),
        );
    }

    /// エントリ1件の分類。**実物のファイルを作らない**——UTF-8にならない名前は
    /// APFSが受け付けず（実測 `EILSEQ`）、CIに Linux が無いので、作る形の試験は
    /// **どの環境でも1度も走らない**（ゲート1の指摘）。
    #[test]
    fn a_name_we_cannot_read_is_not_nothing() {
        use super::{classify_entry, EntryKind, EntryShape};
        use std::ffi::{OsStr, OsString};

        let config = pictkura_core::config::Config::default();
        let kind = |name: &OsStr, shape| classify_entry(name, shape, &config);

        // UTF-8として読めない名前。走査本体はこれを除外に一致させず入って行くので、
        // **候補として数える**——数えないと隣の `.隠しフォルダ` が冤罪を着る
        #[cfg(unix)]
        let unreadable = {
            use std::os::unix::ffi::OsStrExt;
            OsString::from(OsStr::from_bytes(b"\xff\xfe.jpg"))
        };
        #[cfg(windows)]
        let unreadable = {
            use std::os::windows::ffi::OsStringExt;
            // 対を成さないサロゲート（"\u{D800}.jpg" に相当）
            OsString::from_wide(&[0xD800, 0x002E, 0x006A, 0x0070, 0x0067])
        };
        assert!(
            unreadable.to_str().is_none(),
            "この名前はUTF-8として読めない"
        );
        assert_eq!(
            kind(&unreadable, EntryShape::File),
            EntryKind::Candidate,
            "読めない名前は「候補あり」に数える"
        );

        // **リンクは、名前が読めても読めなくても拾われない。** 走査が
        // `file_type().is_file()` で落とす以上、名前の綴りで判断が割れては
        // いけない——割れると、Latin-1の名前のリンクが1本あるだけで
        // 「写真.appのライブラリしかありません」が消える（ゲート2の指摘）
        assert_eq!(
            kind(&unreadable, EntryShape::Symlink),
            EntryKind::Ignorable,
            "読めない名前でも、リンクは走査が拾わない"
        );
        assert_eq!(
            kind(&unreadable, EntryShape::Other),
            EntryKind::Ignorable,
            "読めない名前でも、FIFOやソケットは走査が拾わない"
        );

        // 種別が取れないとき（`file_type()` が落ちた）も安全側へ
        assert_eq!(
            kind(OsStr::new(".DS_Store"), EntryShape::Unknown),
            EntryKind::Candidate,
            "種別が分からないものを「中身になり得ない」と決めない"
        );

        // 読める名前はこれまでどおり
        assert_eq!(
            kind(OsStr::new(".DS_Store"), EntryShape::File),
            EntryKind::Ignorable,
            "Finderの痕跡は中身になり得ない"
        );
        assert_eq!(
            kind(OsStr::new(".隠しフォルダ"), EntryShape::Dir),
            EntryKind::Excluded
        );
        assert_eq!(
            kind(OsStr::new("写真"), EntryShape::Dir),
            EntryKind::Candidate
        );
        assert_eq!(
            kind(OsStr::new("a.jpg"), EntryShape::File),
            EntryKind::Candidate
        );
        // **OSの置き場は証拠にしない**（新品の外付けで「除外のせい」と
        // 言わせない。ゲート2の指摘）。利用者が隠したフォルダは候補のまま
        assert_eq!(
            kind(OsStr::new(".Spotlight-V100"), EntryShape::Dir),
            EntryKind::Ignorable
        );
        assert_eq!(
            kind(OsStr::new("System Volume Information"), EntryShape::Dir),
            EntryKind::Ignorable
        );
        // uidが付く形（freedesktop のボリュームごとのごみ箱）
        assert_eq!(
            kind(OsStr::new(".Trash-1000"), EntryShape::Dir),
            EntryKind::Ignorable
        );
        // **拾いすぎない**——uidが数字でなければ利用者のフォルダ。
        // （バイト位置で切る実装だと、ここで多バイト文字の途中に当たって落ちる）
        assert_eq!(
            kind(OsStr::new(".Trash-の写真"), EntryShape::Dir),
            EntryKind::Excluded,
            "`.Trash-` で始まるだけの利用者のフォルダは、除外を外せば出てくる"
        );
        assert_eq!(
            kind(OsStr::new(".Trash-"), EntryShape::Dir),
            EntryKind::Excluded,
            "uidが無いものはOSの置き場ではない"
        );
        assert_eq!(
            kind(OsStr::new(".隠し写真"), EntryShape::Dir),
            EntryKind::Excluded,
            "利用者が隠したフォルダは、除外を外せば出てくるので候補のまま"
        );
        // **リンクは走査が拾わないので、候補に数えない**（ゲート2の指摘）
        assert_eq!(
            kind(OsStr::new("latest.jpg"), EntryShape::Symlink),
            EntryKind::Ignorable,
            "リンクを証拠にすると、写真.appの文言が1本のリンクで消える"
        );
        // **普通のファイルでないものも同じ**（FIFO・ソケット・デバイスノード）
        assert_eq!(
            kind(OsStr::new("preview.jpg"), EntryShape::Other),
            EntryKind::Ignorable
        );
        // **パッケージはフォルダ。** 同じ名前のただのファイルで旗を立てない
        assert_eq!(
            kind(OsStr::new("古い.photoslibrary"), EntryShape::File),
            EntryKind::Ignorable,
            "写真.appのライブラリが無い場所で「ライブラリしかない」と言わせない"
        );
        assert_eq!(
            kind(OsStr::new("写真ライブラリ.photoslibrary"), EntryShape::Dir),
            EntryKind::Package { photos_app: true }
        );
        // **写真.appだけではない。** iPhoto と Aperture のライブラリも同じ扱いだが、
        // **iCloudの話をしてはいけない**——あちらの中身は手元にある（ゲート1の指摘）
        for name in [
            "古い.photolibrary",
            "移行済み.migratedphotolibrary",
            "撮影.aplibrary",
        ] {
            assert_eq!(
                kind(OsStr::new(name), EntryShape::Dir),
                EntryKind::Package { photos_app: false },
                "{name} は写真.app（現行）のライブラリではない"
            );
        }

        // **除外から外している人には、パッケージのせいだと言わない。**
        // 走査は配下のパッケージを利用者の `exclude_patterns` でしか落とさない
        // ので、消してあれば中へ入って何も見つけていない——そこで旗を立てると、
        // **直しても何も出てこない**設定の話へ送ることになる（ゲート1の指摘）
        let mut opted_out = pictkura_core::config::Config::default();
        opted_out
            .library
            .exclude_patterns
            .retain(|p| !p.ends_with("photoslibrary"));
        assert_eq!(
            classify_entry(
                OsStr::new("写真ライブラリ.photoslibrary"),
                EntryShape::Dir,
                &opted_out
            ),
            EntryKind::Candidate,
            "除外を外したなら、走査と同じく「中身になり得た」側に数える"
        );
        assert_eq!(
            classify_entry(OsStr::new("撮影.aplibrary"), EntryShape::Dir, &opted_out),
            EntryKind::Package { photos_app: false },
            "外したのは写真.appの綴りだけ——他の系譜は既定のまま除外に当たる"
        );
    }

    #[test]
    fn an_empty_library_says_why_it_is_empty() {
        use super::{empty_library_reason_of, ScanUnreadable};

        // 走査の控えは**この試験では空**（借りてくる側は別の試験で見る）
        let no_scan = ScanUnreadable::default();
        let dir = tempfile::tempdir().unwrap();
        let mut config = pictkura_core::config::Config::default();

        // ルートが無い
        config.library.roots.clear();
        assert!(empty_library_reason_of(&config, &no_scan).no_roots);

        // 設定にあるのに、そこに無い（外付けを挿し忘れた等）
        let missing = dir.path().join("gone");
        config.library.roots = vec![missing.clone()];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(!r.no_roots);
        assert_eq!(r.missing.len(), 1, "見つからない場所を名指しする");

        // 写真.appのライブラリしか無い（macOSの `~/Pictures` の普通の姿）
        let root = dir.path().join("pictures");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("写真ライブラリ.photoslibrary")).unwrap();
        config.library.roots = vec![root.clone()];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(r.photo_library, "写真.appのライブラリだと分かる");
        assert!(r.missing.is_empty());

        // **「しか無い」が嘘になる形。** 隣に除外されていない空のフォルダが
        // あれば、走査はそこを開いて何も見つけていない（ゲート2の指摘）
        std::fs::create_dir(root.join("富士")).unwrap();
        assert!(
            !empty_library_reason_of(&config, &no_scan).photo_library,
            "隣に別のフォルダがあるなら「写真.appのライブラリしか無い」と言わない"
        );
        std::fs::remove_dir(root.join("富士")).unwrap();

        // 除外で全部飛んでいる（写真.app以外）
        std::fs::remove_dir(root.join("写真ライブラリ.photoslibrary")).unwrap();
        std::fs::create_dir(root.join(".隠しフォルダ")).unwrap();
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(!r.photo_library);
        assert_eq!(
            r.excluded,
            vec![".隠しフォルダ".to_string()],
            "中身になり得たものだけ名前を挙げる"
        );
        assert_eq!(r.excluded_total, 1, "見せた3件と別に、総数を数える");

        // **「ほか N件」の数が狂わないこと。** 見せるのは3件でも数は全部
        // （ゲート2の指摘: 見せる側で重複を見ると数がどちらにも狂う）
        // （$RECYCLE.BIN はOSの置き場なので数に入らない。上の分類の試験を見よ）
        for n in ["Thumbs.db", ".二つ目", ".三つ目", ".四つ目"] {
            std::fs::create_dir(root.join(n)).unwrap();
        }
        let r = empty_library_reason_of(&config, &no_scan);
        assert_eq!(r.excluded.len(), 3, "名前は3件まで");
        assert_eq!(r.excluded_total, 5, "数は打ち切らない");
        for n in ["Thumbs.db", ".二つ目", ".三つ目", ".四つ目"] {
            std::fs::remove_dir(root.join(n)).unwrap();
        }

        // **ルートをまたいで同じ名前が並んでも、数えるのは名前。**
        // この数は名前の並びの続きとして読まれるので（ゲート2の指摘）
        let root2 = dir.path().join("pictures2");
        std::fs::create_dir(&root2).unwrap();
        std::fs::create_dir(root2.join(".隠しフォルダ")).unwrap();
        config.library.roots = vec![root.clone(), root2.clone()];
        let r = empty_library_reason_of(&config, &no_scan);
        assert_eq!(r.excluded, vec![".隠しフォルダ".to_string()], "例示は畳む");
        assert_eq!(
            r.excluded_total, 1,
            "外すべきパターンは1つ——「ほか1件」と言うと在りもしない設定を探させる"
        );
        std::fs::remove_dir_all(&root2).unwrap();
        config.library.roots = vec![root.clone()];

        // **除外の隣に、除外されていない空のフォルダがある。**
        // 走査はその `写真` を開いて何も見つけていないのだから、
        // 「全部除外しています」は嘘になる（ゲート1の指摘）
        std::fs::create_dir(root.join("写真")).unwrap();
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(
            r.excluded.is_empty(),
            "除外されていない候補が1つでもあれば、除外を犯人にしない"
        );
        assert_eq!(r.excluded_total, 0, "名前だけでなく数も落とす");
        std::fs::remove_dir(root.join("写真")).unwrap();
        std::fs::remove_dir(root.join(".隠しフォルダ")).unwrap();

        // **ルート自身が写真.appのライブラリ**（手で書いた設定・古い版の設定）。
        // 走査はそのルートを丸ごと飛ばすので、中を読んでも何も出ない
        let picked = dir.path().join("選んだ.photoslibrary");
        std::fs::create_dir(&picked).unwrap();
        config.library.roots = vec![picked.clone()];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(r.root_is_package, "ルート自身がパッケージだと見分ける");
        assert!(
            !r.photo_library,
            "「ライブラリしか無い」とは別の事実——排他は主張しない"
        );
        // **隣に空のルートがあっても、この事実は消えない。**
        // 「ここにはライブラリしか無い」は覆るが、「このルートは
        // ライブラリそのもので何も出ない」は覆らない（ゲート2の指摘）
        let plain = dir.path().join("空っぽ");
        std::fs::create_dir(&plain).unwrap();
        std::fs::create_dir(plain.join("写真")).unwrap();
        config.library.roots = vec![picked, plain.clone()];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(
            r.root_is_package,
            "隣に候補があっても、ルート自身がパッケージという事実は残す"
        );
        assert!(
            !r.photo_library,
            "隣に候補があるなら「ライブラリのほかに無い」は嘘になる"
        );
        std::fs::remove_dir_all(&plain).unwrap();
        config.library.roots = vec![root.clone()];

        // **`.DS_Store` だけでは「全部除外」と言わない**（中身になり得ない）
        std::fs::write(root.join(".DS_Store"), b"x").unwrap();
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(
            r.excluded.is_empty(),
            "Finderが置いた痕跡を「除外のせい」と言わない"
        );
        std::fs::remove_file(root.join(".DS_Store")).unwrap();

        // **見せてもらえないのと、無いのは違う。**
        // `stat` は親フォルダの検索権を要るので、親が `000` だと
        // 「見つかりません」に落ちる——挿さっている外付けを挿し直させる
        // 案内になる（ゲート1の指摘）。
        //
        // **`cfg(unix)` なのはモードビットがPOSIXの概念だから**で、
        // 検出力を落とすための除外ではない（Windowsは別のACLの話になる）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let outer = dir.path().join("外");
            let inner = outer.join("中");
            std::fs::create_dir_all(&inner).unwrap();
            std::fs::set_permissions(&outer, std::fs::Permissions::from_mode(0o000)).unwrap();
            // **rootで走ると権限が効かない**（CIのコンテナ等）。
            // 仕掛けが効いているかを先に確かめ、効いていなければ飛ばす——
            // 効かない環境で通してしまうと、試験があるのに何も見ていないことになる
            let denied = std::fs::metadata(&inner).is_err();
            config.library.roots = vec![inner];
            let r = empty_library_reason_of(&config, &no_scan);
            std::fs::set_permissions(&outer, std::fs::Permissions::from_mode(0o755)).unwrap();
            if denied {
                assert!(
                    r.missing.is_empty(),
                    "読めないだけの場所を「見つかりません」と言わない"
                );
                assert_eq!(r.unreadable.len(), 1, "読めなかった場所として名指しする");
            }

            // **TCCが実際に落ちる形はこちら**——`stat` は通り、`opendir` で止まる
            // （実測: ルート自身を `000` にすると `metadata` は成功し
            // `read_dir` だけが `EACCES`）。コメントが名指ししている枝なのに
            // 上のケースでは一度も通っていなかった（ゲート1の指摘）
            let locked = dir.path().join("錠");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            let stat_ok = std::fs::metadata(&locked).is_ok();
            let read_denied = std::fs::read_dir(&locked).is_err();
            config.library.roots = vec![locked.clone()];
            let r = empty_library_reason_of(&config, &no_scan);
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            if stat_ok && read_denied {
                assert!(
                    r.missing.is_empty(),
                    "在るものを「見つかりません」と言わない"
                );
                assert_eq!(
                    r.unreadable.len(),
                    1,
                    "`stat` は通って `read_dir` で落ちる形（TCC）を拾う"
                );
            }

            config.library.roots = vec![root.clone()];
        }

        // 本当に空（何も言うことが無い＝どの旗も立たない）
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(!r.no_roots && r.missing.is_empty() && !r.photo_library);
        assert!(r.unreadable.is_empty());
        assert!(r.excluded.is_empty());
        // **数の側も見る**——`survivor` の後始末で `excluded_total` を
        // 落とし忘れても、名前だけ見ていると気づけない（ゲート1の指摘）
        assert_eq!(r.excluded_total, 0);
    }

    /// 走査が奥で開けなかった場所を、空の理由が**借りてくる**。
    ///
    /// この関数はルートの直下しか読まない（速さが仕様の走査路を太らせない
    /// ため）ので、`~/Pictures/2024/非公開` の許可だけが無い形が見えない。
    /// 走査はそこで止まっているのに、説明側は「フォルダがある＝候補あり」と
    /// 数えて「扱える画像がありません」に落ちる——**写真はその奥に在る**
    /// **どのアプリのライブラリかで案内が変わる**——混ざっているときも含めて。
    ///
    /// 「原本の多くはiCloud側にあり、手元にはありません」は写真.appの話。
    /// iPhotoやApertureのライブラリの中身は手元にあるので、そちらに向かって
    /// 言うと**在りかを取り違えさせる**（ゲート1の指摘）
    #[test]
    fn a_legacy_library_next_to_a_photos_one_does_not_get_the_icloud_wording() {
        use super::{empty_library_reason_of, ScanUnreadable};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pictures");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("写真ライブラリ.photoslibrary")).unwrap();
        let mut config = pictkura_core::config::Config::default();
        config.library.roots = vec![root.clone()];
        let no_scan = ScanUnreadable::default();

        let r = empty_library_reason_of(&config, &no_scan);
        assert!(
            r.photo_library && !r.photo_library_legacy,
            "写真.appのライブラリだけなら、iCloudの話をしてよい"
        );

        // **隣に Aperture のライブラリがある。** ここで写真.app専用の文言に
        // 落ちると、手元に中身があるライブラリの話がまるごと消える
        std::fs::create_dir(root.join("撮影.aplibrary")).unwrap();
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(
            r.photo_library && r.photo_library_legacy,
            "混ざっているなら、どちらにも当たる文言へ落とす"
        );

        // ルート自身がライブラリのときも同じ向き
        config.library.roots = vec![root.join("撮影.aplibrary")];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(r.root_is_package && r.root_package_legacy);
        config.library.roots = vec![root.join("写真ライブラリ.photoslibrary")];
        let r = empty_library_reason_of(&config, &no_scan);
        assert!(r.root_is_package && !r.root_package_legacy);
    }

    #[test]
    fn the_empty_reason_borrows_what_the_scan_could_not_open() {
        use super::{empty_library_reason_of, ScanUnreadable};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pictures");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("2024")).unwrap();
        let mut config = pictkura_core::config::Config::default();
        config.library.roots = vec![root.clone()];

        // 走査が1件も転んでいないなら、これまでどおり何も言わない
        let r = empty_library_reason_of(&config, &ScanUnreadable::default());
        assert!(r.unreadable.is_empty());
        assert_eq!(r.unreadable_total, 0);

        // 転んでいたら、その場所を名指しする
        let deep = root.join("2024").join("非公開");
        let from_scan = ScanUnreadable {
            dirs: vec![deep.clone()],
            total: 1,
        };
        let r = empty_library_reason_of(&config, &from_scan);
        assert_eq!(
            r.unreadable,
            vec![deep.display().to_string()],
            "直下からは見えない奥の場所を名指しする"
        );
        assert_eq!(r.unreadable_total, 1);

        // **数は控えた件数ではなく、走査が数えた総数から出す。**
        // 「ほか N件」が狂うと、在りもしない場所を探しに行かせる
        let from_scan = ScanUnreadable {
            dirs: vec![root.join("a"), root.join("b"), root.join("c")],
            total: 9,
        };
        let r = empty_library_reason_of(&config, &from_scan);
        assert_eq!(r.unreadable.len(), 3, "名前は走査が控えたぶんだけ");
        assert_eq!(r.unreadable_total, 9, "数は打ち切らない");

        // **同じ場所を2度並べない。** ルート自身が読めないときは、走査も
        // 同じ壁で止まっているので必ず重なる
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = dir.path().join("錠");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            let denied = std::fs::read_dir(&locked).is_err();
            config.library.roots = vec![locked.clone()];
            let from_scan = ScanUnreadable {
                dirs: vec![locked.clone()],
                total: 1,
            };
            let r = empty_library_reason_of(&config, &from_scan);
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            if denied {
                assert_eq!(r.unreadable.len(), 1, "同じ場所を2度並べない");
                assert_eq!(r.unreadable_total, 1, "重なったぶんを二重に数えない");
            }
        }
    }

    /// **枝刈り走査は、開けなかったフォルダを二度と見に行かない。**
    ///
    /// 開けなかった場所は `seen_dirs` に載らず、次回の `known_dirs` にも
    /// 入らない。親のmtimeは変わっていないので親ごと飛ばされ、**今回の結果は
    /// 空**になる——丸ごと置き換えると、正しかった控えを空で上書きして
    /// **2回目の起動から穴が元通り**になる（ゲート2が実測で示した）
    #[test]
    fn a_pruned_scan_that_never_looked_again_does_not_erase_what_we_knew() {
        use super::{merged_unreadable, ScanUnreadable};
        use pictkura_core::PrunedScanOutcome;

        let root = std::path::PathBuf::from("/home/me/Pictures");
        let locked = root.join("2024").join("非公開");
        let known = ScanUnreadable {
            dirs: vec![locked.clone()],
            total: 1,
        };

        // 枝刈りで親ごと飛ばした回。**見ていないのだから、控えは残す**
        let skipped = PrunedScanOutcome {
            enumerated_dirs: vec![root.clone()],
            ..Default::default()
        };
        let after = merged_unreadable(&known, &skipped, std::slice::from_ref(&root));
        assert_eq!(
            after.dirs,
            vec![locked.clone()],
            "見ていない場所の控えは残す"
        );
        assert_eq!(after.total, 1);

        // 親を列挙し直した回に何も出なかった＝**直った**。控えは落とす
        let looked = PrunedScanOutcome {
            enumerated_dirs: vec![root.clone(), root.join("2024")],
            ..Default::default()
        };
        let after = merged_unreadable(&known, &looked, std::slice::from_ref(&root));
        assert!(
            after.dirs.is_empty() && after.total == 0,
            "踏み直して何も出なかったなら、古い控えは捨てる"
        );

        // ルートは毎回歩くので、ルート自身の控えは今回の結果で置き換わる
        let known_root = ScanUnreadable {
            dirs: vec![root.clone()],
            total: 1,
        };
        let after = merged_unreadable(
            &known_root,
            &PrunedScanOutcome::default(),
            std::slice::from_ref(&root),
        );
        assert!(after.dirs.is_empty(), "ルートは毎回見直している");

        // 今回の結果は足される（重複は増やさない）
        let found_again = PrunedScanOutcome {
            enumerated_dirs: vec![root.clone()],
            unreadable_dirs: vec![locked.clone()],
            unreadable_total: 1,
            ..Default::default()
        };
        let after = merged_unreadable(&known, &found_again, std::slice::from_ref(&root));
        assert_eq!(after.dirs, vec![locked], "同じ場所を2度並べない");
        assert_eq!(after.total, 1);
    }

    /// **「ほか N件」は在りもしない場所を指してはいけない。**
    ///
    /// 走査の総数にはルート自身も入っている。名前で重なりを消したぶんだけ
    /// 足すと、4本しか無いのに5本目を探しに行かせる（ゲート2の指摘）。
    ///
    /// **`cfg(unix)` なのはモードビットがPOSIXの概念だから**で、検出力を
    /// 落とすための除外ではない
    #[cfg(unix)]
    #[test]
    fn the_count_of_places_we_could_not_open_never_invents_one() {
        use super::{empty_library_reason_of, ScanUnreadable};
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let mut roots = Vec::new();
        for i in 1..=4 {
            let root = dir.path().join(format!("外付け{i}"));
            std::fs::create_dir(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
            roots.push(root);
        }
        // **rootで走ると権限が効かない**（CIのコンテナ等）。効いていなければ飛ばす
        let denied = std::fs::read_dir(&roots[0]).is_err();
        let mut config = pictkura_core::config::Config::default();
        config.library.roots = roots.clone();
        // 走査も同じ4本で転んでいる（名前は先頭3件しか控えていない）
        let from_scan = ScanUnreadable {
            dirs: roots[..3].to_vec(),
            total: 4,
        };
        let r = empty_library_reason_of(&config, &from_scan);
        for root in &roots {
            std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        if denied {
            assert_eq!(r.unreadable.len(), 4, "同じ場所を2度並べない");
            assert_eq!(r.unreadable_total, 4, "5本目を作らない");
        }
    }

    /// 走っている探りには**相乗りする**（同じルートへ2本目を飛ばさない）。
    ///
    /// 刺さったマウントでは `read_dir` が返らず、探りはスレッドを持ったまま
    /// 残る。同じ状況を聞き直すたびに新しい探りを飛ばすと、**そのぶんだけ
    /// スレッドが増える**——答えは1つも増えないのに
    #[test]
    fn a_second_question_about_the_same_root_joins_the_probe_already_running() {
        use super::{probe_of, ProbeGuard, RootProbes, RootReason};

        let probes = RootProbes::default();
        let root = std::path::PathBuf::from("/nas/photos");
        let mut started = 0;
        // 1本目。**わざと終わらせない**（＝刺さったマウントと同じ形）
        let mut held: Option<ProbeGuard> = None;
        let first = probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |guard| {
            started += 1;
            held = Some(guard);
        })
        .expect("1本目は始まる");
        assert_eq!(started, 1);

        // 2本目は同じルート——**始めない**。受け口は同じ探りの複製
        let second = probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |_| started += 1)
            .expect("走っているものに相乗りする");
        assert_eq!(started, 1, "刺さったルートへ2本目を飛ばさない");
        assert!(second.answer.borrow().is_none(), "まだ答えは無い");

        // 1本目が答えを配ると、相乗りしている側にも同じものが届く
        held.take().unwrap().finish(RootReason {
            survivor: true,
            ..Default::default()
        });
        assert!(first.answer.borrow().as_ref().is_some_and(|r| r.survivor));
        assert!(
            second.answer.borrow().as_ref().is_some_and(|r| r.survivor),
            "待っていた側も**自分で本当の答えを受け取る**"
        );

        // 終わった探りの印は消えている——次に聞かれたら探り直す
        let mut restarted = 0;
        probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |_| {
            restarted += 1
        });
        assert_eq!(restarted, 1, "返ってきたルートは次にまた探る");
    }

    /// 探りがパニックしても、**印は残らない**。
    ///
    /// 印は「返ってこない」ことの証拠でなければならない。落ちたことの証拠に
    /// してしまうと、バグで1回落ちたルートを**二度と探らなくなる**
    #[test]
    fn a_probe_that_panics_leaves_no_mark() {
        use super::{probe_of, RootProbes};

        let probes = RootProbes::default();
        let root = std::path::PathBuf::from("/nas/photos");
        // パニックの出力で試験のログを汚さない（この試験は落ちることが前提）
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let fell = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |_guard| {
                panic!("探りが落ちた")
            });
        }));
        std::panic::set_hook(previous);
        assert!(fell.is_err(), "落ちること自体は試験の前提");

        let mut started = 0;
        probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |_| started += 1);
        assert_eq!(started, 1, "落ちたルートは次に探り直す");
    }

    /// **外して足し直したルートは、もう一度探る。**
    ///
    /// 刺さった印は探りが返るまで残す（それが狙い）が、貼り直した共有を
    /// 一度も見ずに「応答がありません」と言い続けるのは、直しても直らない
    /// 案内になる（ゲート1の指摘）
    #[test]
    fn a_root_taken_out_and_put_back_gets_a_fresh_probe() {
        use super::{forget_probes_for_changed_roots, probe_of, ProbeGuard, RootProbes};

        let probes = RootProbes::default();
        let root = std::path::PathBuf::from("/nas/photos");
        // 刺さったまま返らない探り
        let mut stuck: Option<ProbeGuard> = None;
        probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |guard| {
            stuck = Some(guard)
        });

        // ルートを外す → 印を捨てる
        forget_probes_for_changed_roots(&probes, std::slice::from_ref(&root), &[]);
        // 足し直す → 新しい探りが始まる（古いのは返らないまま）
        forget_probes_for_changed_roots(&probes, &[], std::slice::from_ref(&root));
        let mut started = 0;
        let mut fresh: Option<ProbeGuard> = None;
        probe_of(&probes, &root, super::PROBE_STALLED_AFTER, |guard| {
            started += 1;
            fresh = Some(guard);
        });
        assert_eq!(started, 1, "貼り直した共有は、もう一度見に行く");

        // **古い探りが後から片付いても、新しい印を巻き添えにしない**
        drop(stuck.take());
        assert!(
            super::lock_ok(&probes).running.contains_key(&root),
            "古い後片付けが新しい探りの印を消さない（世代で見分ける）"
        );
        drop(fresh.take());
        assert!(
            !super::lock_ok(&probes).running.contains_key(&root),
            "自分の印は自分で消す"
        );
    }

    /// **刺さったマウントの数だけスレッドを増やさない。**
    ///
    /// 相乗りは「同じ場所へ2本目を飛ばさない」までしか守らない。死んだ共有が
    /// 何本もあると、その数だけブロッキングプールのスレッドが永久に減る
    /// ——同じプールを使う `add_library_root` などが止まる（ゲート1の指摘）
    #[test]
    fn a_pile_of_dead_mounts_stops_taking_new_threads() {
        use super::{probe_of, ProbeGuard, RootProbes, MAX_STUCK_PROBES};

        let probes = RootProbes::default();
        // **時間で待たずに「刺さっている」を作る**——上限の数え方だけを見る
        let at_once = std::time::Duration::ZERO;
        let mut held: Vec<ProbeGuard> = Vec::new();
        for i in 0..MAX_STUCK_PROBES {
            let root = std::path::PathBuf::from(format!("/nas/{i}"));
            probe_of(&probes, &root, at_once, |guard| held.push(guard));
        }
        assert_eq!(held.len(), MAX_STUCK_PROBES, "上限までは始める");

        let mut started = 0;
        let more = probe_of(
            &probes,
            std::path::Path::new("/nas/another"),
            at_once,
            |_| started += 1,
        );
        assert!(more.is_none() && started == 0, "上限を超えたら手を出さない");

        // 1本返れば、また探れる（返らないぶんだけが枠を食う）
        held.pop();
        let mut after = 0;
        probe_of(
            &probes,
            std::path::Path::new("/nas/another"),
            at_once,
            |_| after += 1,
        );
        assert_eq!(after, 1, "空いたぶんは探りに行く");
    }

    /// **刺さった1本が、他のルートの答えを飲み込まない。**
    ///
    /// 直列に見ていた頃は、刺さったSMBの後ろにある `~/Pictures` の結果まで
    /// 出てこなかった。いまは返ってきたぶんを畳める——ただし
    /// **「ほかに無い」という排他の主張だけは、分からない場所が残る限り言わない**
    #[test]
    fn one_stalled_root_does_not_speak_for_the_others() {
        use super::{merge_root_reasons, RootAnswer, RootReason, ScanUnreadable};

        let roots = vec![
            std::path::PathBuf::from("/nas/photos"),
            std::path::PathBuf::from("/home/me/Pictures"),
        ];
        // 2本目は見終わっていて、「写真.appのライブラリしか無い」形
        let answered = RootReason {
            photo_library: true,
            excluded: vec![".隠しフォルダ".to_string()],
            ..Default::default()
        };
        let r = merge_root_reasons(
            &roots,
            &[RootAnswer::Stalled, RootAnswer::Answered(answered.clone())],
            &ScanUnreadable::default(),
        );
        assert_eq!(
            r.stalled,
            vec!["/nas/photos".to_string()],
            "返事の無い場所を名指しする"
        );
        assert!(!r.checking, "諦めたことと、まだ見ている最中は別の話");
        assert!(
            !r.photo_library && r.excluded.is_empty() && r.excluded_total == 0,
            "中身の分からない場所が残っているなら「ほかに無い」とは言えない"
        );

        // まだ見ている最中なら `checking`。**名指しはしない**——変わるかもしれない
        let r = merge_root_reasons(
            &roots,
            &[RootAnswer::Checking, RootAnswer::Answered(answered)],
            &ScanUnreadable::default(),
        );
        assert!(r.checking, "まだ見ている");
        assert!(r.stalled.is_empty(), "見切る前に名指ししない");

        // 全部見終わったなら、これまでどおり言い切る
        let both_seen = RootReason::default();
        let r = merge_root_reasons(
            &roots,
            &[
                RootAnswer::Answered(both_seen.clone()),
                RootAnswer::Answered(RootReason {
                    photo_library: true,
                    ..Default::default()
                }),
            ],
            &ScanUnreadable::default(),
        );
        assert!(r.photo_library, "分からない場所が無いなら主張してよい");
        assert!(!r.checking && r.stalled.is_empty());
    }

    #[test]
    fn the_dcim_path_gets_a_separator_on_every_platform() {
        // 後始末まで含めて `narrow_to_dcim` のテストと同じ流儀
        let mount = std::env::temp_dir().join("pictkura_dcim_under");
        std::fs::create_dir_all(&mount).unwrap();

        // まだ `DCIM` が無い
        assert_eq!(dcim_under(&mount, "removable"), None);

        let dcim = mount.join("DCIM");
        std::fs::create_dir_all(&dcim).unwrap();
        let found = dcim_under(&mount, "removable").expect("DCIMを見つける");
        // **区切りが入っていること**が要点。以前はUI側が文字列を繋いでいて、
        // `/Volumes/NO NAME` ＋ `DCIM` ＝ `/Volumes/NO NAMEDCIM` になっていた
        assert_eq!(found, dcim.display().to_string());
        assert!(
            std::path::Path::new(&found).is_dir(),
            "組んだパスが実在すること"
        );

        // ネットワークドライブは見に行かない（回線越しに待たされる）
        assert_eq!(dcim_under(&mount, "network"), None);

        std::fs::remove_dir_all(&mount).ok();
    }

    #[test]
    fn lifts_the_drive_out_of_the_autoplay_argument() {
        // AutoPlayは `exe --import E:\` の形で渡す（argv[0]は実行ファイル）
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "E:\\"])),
            Some("E:\\".to_string())
        );
    }

    #[test]
    fn a_bare_drive_letter_becomes_the_root() {
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "E:"])),
            Some("E:\\".to_string())
        );
    }

    #[test]
    fn no_import_yields_none() {
        assert_eq!(import_path_from_args(&args(&["pictkura.exe"])), None);
    }

    #[test]
    fn nothing_after_import_yields_none() {
        // 末尾に値が無い（壊れた呼び出し）ときにパニックしない
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import"])),
            None
        );
    }

    #[test]
    fn an_empty_value_yields_none() {
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "   "])),
            None
        );
    }

    #[test]
    fn a_quote_broken_by_a_trailing_backslash_is_restored() {
        // レジストリの `"%L"` は `E:\` を渡すと `"E:\"` になり、`\"` が
        // エスケープと解釈されて `E:"` の形で届く。元のルートに直す
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "E:\""])),
            Some("E:\\".to_string())
        );
    }

    #[test]
    fn a_path_holding_spaces_arrives_uncut() {
        // MTPやフォルダにマウントされたボリュームだと `%L` にスペースが入る。
        // 囲んであるのでargvでは1つに収まり、末尾の引用符だけ戻せばよい
        assert_eq!(
            import_path_from_args(&args(&["pictkura.exe", "--import", "C:\\My Photos\""])),
            Some("C:\\My Photos\\".to_string())
        );
    }

    #[test]
    fn with_a_dcim_present_we_move_up_to_it() {
        let dir = std::env::temp_dir().join("pictkura_narrow_to_dcim");
        let dcim = dir.join("DCIM");
        std::fs::create_dir_all(&dcim).unwrap();
        assert_eq!(
            super::narrow_to_dcim(dir.display().to_string()),
            dcim.display().to_string()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_a_dcim_it_stays_as_it_is() {
        let dir = std::env::temp_dir().join("pictkura_narrow_no_dcim");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            super::narrow_to_dcim(dir.display().to_string()),
            dir.display().to_string()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `APP_IDENTIFIER` が `tauri.conf.json` の `identifier` と一致している。
    ///
    /// ずれると [`config_path_without_app`] が**空のフォルダ**を指し、
    /// `--sync-autoplay` は「設定ファイルが無い」と読んで黙って何もしなくなる
    /// （MSIから乗り換えた人のAutoPlay登録が戻らない。PR #24 のゲート1 P2）
    #[cfg(windows)]
    #[test]
    fn the_identifier_matches_the_tauri_config() {
        let conf = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tauri.conf.json"))
            .expect("tauri.conf.json が読めない");
        let value: serde_json::Value = serde_json::from_str(&conf).expect("JSONとして読めない");
        assert_eq!(
            value["identifier"].as_str(),
            Some(APP_IDENTIFIER),
            "tauri.conf.json の identifier と APP_IDENTIFIER がずれている"
        );
    }
}
