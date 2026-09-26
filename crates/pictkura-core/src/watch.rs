//! ライブラリルートのファイルシステム監視。
//!
//! アプリ外でのファイル操作（エクスプローラーでの追加・削除・移動）を検知して
//! DBへ追従させるための土台。notifyがOS別のAPI
//! （Windows: ReadDirectoryChangesW / macOS: FSEvents / Linux: inotify）を抽象化する。
//!
//! 爆速の原則: イベントのあったパスだけを処理する（全ルートの再スキャンはしない）。
//! イベントは短時間デバウンスしてまとめ、コピー中の連続書き込みで嵐にならないようにする。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify_debouncer_mini::{
    new_debouncer_opt, notify::RecursiveMode, DebounceEventResult, Debouncer,
};

#[cfg(windows)]
mod device;

/// 監視器。**Windows では、落としたときにハンドルを閉じ終えるまで待つ形**（`device::AckedWatcher`）
/// ——notify の素の監視器は停止を送るだけで、閉じるのは監視の糸の側で遅れる（#161 の codex の P1）。
/// 取り外しを許す前に閉じ終えていないと、取り外しが時々断られる
#[cfg(windows)]
type PlatformWatcher = device::AckedWatcher;
#[cfg(not(windows))]
type PlatformWatcher = notify_debouncer_mini::notify::RecommendedWatcher;

/// 監視ハンドル。dropすると監視が止まる。
pub struct LibraryWatcher {
    /// 監視の本体。**Windows では取り外しの知らせを受ける糸と共有する**（`device`）
    _watching: Arc<Mutex<Watching>>,
    /// 始めたときに監視できたルート（存在しないルートはスキップされる）
    pub watched_roots: Vec<PathBuf>,
    /// Windows: 取り外せるドライブ上のルートを、取り外しの求めに合わせて手放す（dev #35）
    #[cfg(windows)]
    _device: Option<device::DeviceGuard>,
}

/// いま張っている監視
struct Watching {
    debouncer: Debouncer<PlatformWatcher>,
}

impl Watching {
    fn start(
        debounce: Duration,
        on_batch: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> Result<Self, notify_debouncer_mini::notify::Error> {
        let config = notify_debouncer_mini::Config::default().with_timeout(debounce);
        let debouncer =
            new_debouncer_opt::<_, PlatformWatcher>(config, move |result: DebounceEventResult| {
                if let Ok(events) = result {
                    let mut paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                    paths.sort();
                    paths.dedup();
                    if !paths.is_empty() {
                        on_batch(paths);
                    }
                }
            })?;
        Ok(Self { debouncer })
    }

    /// ルートを1つ監視に入れる。存在しないルート（未接続のUSB等）は監視できないので偽
    fn watch_root(&mut self, root: &std::path::Path) -> bool {
        root.is_dir()
            && self
                .debouncer
                .watcher()
                .watch(root, RecursiveMode::Recursive)
                .is_ok()
    }

    /// ルートを1つ監視から外す。**ほかのルートの監視には触らない**——まるごと作り直すと、
    /// 束ねる前のイベントが全ルートぶん捨てられる（#161 のゲート2）。Windows では
    /// `AckedWatcher::unwatch` がハンドルを閉じ終えるまで待ってから戻る
    #[cfg_attr(not(windows), allow(dead_code))]
    fn unwatch_root(&mut self, root: &std::path::Path) {
        let _ = self.debouncer.watcher().unwatch(root);
    }
}

/// ルートの置き場所の形（Windows の取り外しの糸が、ネットワークのルートを確かめから外すのに使う）
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
enum RootLocation {
    /// `\\server\share`（`\\?\UNC\server\share` も）
    Unc,
    /// ドライブ文字（`Z:\` も `\\?\Z:\` も）。ネットワークかどうかは OS に訊く
    Drive(char),
    Other,
}

/// パスの綴りだけからルートの置き場所の形を読む。**長いパスの書き方**（`\\?\`）を先に剥がす
/// ——剥がさないと `\\?\UNC\…` も `\\?\Z:\…` も「ネットワークではない」と読んでいた
/// （#161 の codex、2周目の P2）。先頭の4バイトに多バイト文字が来ても切らない
#[cfg_attr(not(windows), allow(dead_code))]
fn root_location(path: &str) -> RootLocation {
    let s = match path.strip_prefix(r"\\?\") {
        Some(rest)
            if rest
                .get(..4)
                .is_some_and(|p| p.eq_ignore_ascii_case(r"UNC\")) =>
        {
            return RootLocation::Unc
        }
        Some(rest) => rest,
        None => path,
    };
    if s.starts_with(r"\\") {
        return RootLocation::Unc;
    }
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(d), Some(':')) if d.is_ascii_alphabetic() => {
            RootLocation::Drive(d.to_ascii_uppercase())
        }
        _ => RootLocation::Other,
    }
}

/// ルート群の再帰監視を開始する。
/// イベントはデバウンス（既定800ms）後に、重複除去済みのパス一覧で `on_batch` へ渡される。
///
/// **Windows では、取り外せるドライブの上のルートを「安全な取り外し」の求めで手放し、
/// 差し直されたら監視に戻す**（dev #35）。握ったままだと、アプリを開いている間は取り外しが
/// 必ず断られた（Kernel-PnP 225 が pictkura を名指し）。起動時に無かったルートも、
/// 差し込まれた時点で監視に入る。**監視に戻すのは、それ以降の変化だけ**——離れていた間に
/// 増えたファイルは、再スキャンで入る（今までと同じ）
pub fn watch_roots(
    roots: &[PathBuf],
    debounce: Duration,
    on_batch: impl Fn(Vec<PathBuf>) + Send + 'static,
) -> Result<LibraryWatcher, notify_debouncer_mini::notify::Error> {
    let mut watching = Watching::start(debounce, on_batch)?;
    let watched_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|root| watching.watch_root(root))
        .cloned()
        .collect();
    let watching = Arc::new(Mutex::new(watching));
    Ok(LibraryWatcher {
        #[cfg(windows)]
        _device: device::DeviceGuard::start(Arc::clone(&watching), roots.to_vec(), &watched_roots),
        _watching: watching,
        watched_roots,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn added_file_events_arrive_in_a_batch() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let watcher = watch_roots(
            &[dir.path().to_path_buf()],
            Duration::from_millis(200),
            move |paths| {
                let _ = tx.send(paths);
            },
        )
        .unwrap();
        assert_eq!(watcher.watched_roots.len(), 1);

        // 監視開始が安定するまで少し待ってからファイルを作る
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.path().join("new.jpg"), b"data").unwrap();

        let batch = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("イベントが届かなかった");
        // **親と比べるときは両側を解決する。** macOSの `/var` は `/private/var` への
        // シンボリックリンクで、FSEventsは**解決後の綴り**を返す。`dir.path()`
        // （＝`/var/folders/...`）と直に比べると必ず外れるため、ここを解決しないと
        // 「ファイル単体のイベントが来たときだけ通る」テストになる。
        // どちらが来るかはFSEventsの束ね方次第（CIでは親、開発機ではファイルが来た）。
        //
        // **製品側は `handle_fs_events` の入口で設定ルートの綴りへ揃えている**
        // （`rebase_to_root_spelling`）。ここで両側を解決するのは、この層が
        // 綴りを揃えないまま返すことを確かめるためであって、
        // 「揃わないまま入る」という意味ではない
        let root = dir.path().canonicalize().expect("一時フォルダを解決できる");
        assert!(
            batch.iter().any(|p| p.ends_with("new.jpg")
                || p.canonicalize().is_ok_and(|resolved| resolved == root)),
            "new.jpg（またはその親）のイベントが含まれる: {batch:?}（親: {root:?}）"
        );
        drop(watcher);
    }

    /// ルートを1つ外すと、そのルートのイベントは止まり、戻すとまた届く（Windows の取り外しの糸が
    /// 使う入口。dev #35）。**外さないルートは届き続け、外す直前に起きた変化も捨てない**
    /// ——まるごと作り直していたときは、束ねる前のイベントが全ルートぶん消えた（#161 のゲート2）。
    /// 最後の点は Windows でだけ見る（下の注記）
    #[test]
    fn unwatching_one_root_leaves_the_others_and_their_pending_events() {
        let kept = tempfile::tempdir().unwrap();
        let removable = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
        let mut w = Watching::start(Duration::from_millis(400), move |paths| {
            let _ = tx.send(paths);
        })
        .unwrap();
        assert!(w.watch_root(kept.path()));
        assert!(w.watch_root(removable.path()));
        // 一定時間ぶんの束を**全部**集める（見たい名前以外を捨てると、外した側のイベントが
        // 先に来ていたときに見逃す）
        let collect = || -> Vec<PathBuf> {
            let mut all = Vec::new();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
                match rx.recv_timeout(left) {
                    Ok(batch) => all.extend(batch),
                    Err(_) => break,
                }
            }
            all
        };
        let has =
            |all: &[PathBuf], name: &str| all.iter().any(|p| p.to_string_lossy().contains(name));
        let settle = || std::thread::sleep(Duration::from_millis(300));

        // 残す側に書いた**直後**（束ねる前）に、もう片方を外す
        settle();
        std::fs::write(kept.path().join("pending.jpg"), b"x").unwrap();
        w.unwatch_root(removable.path());
        settle();
        std::fs::write(removable.path().join("while_away.jpg"), b"x").unwrap();
        std::fs::write(kept.path().join("kept_1.jpg"), b"x").unwrap();
        let all = collect();
        // **これは Windows でだけ見る**。macOS の FSEvents は1本外すとストリームごと作り直すので、
        // その間のイベントは監視器の側で消える（手元で実測）。外す道を使うのは Windows の取り外しだけ
        #[cfg(windows)]
        assert!(has(&all, "pending.jpg"), "外す直前の変化も届く: {all:?}");
        assert!(has(&all, "kept_1.jpg"), "残した側は届く（対照）: {all:?}");
        assert!(!has(&all, "while_away.jpg"), "外した側は届かない: {all:?}");

        // 戻す: また届く
        assert!(w.watch_root(removable.path()));
        settle();
        std::fs::write(removable.path().join("back.jpg"), b"x").unwrap();
        let all = collect();
        assert!(has(&all, "back.jpg"), "戻した側がまた届く: {all:?}");
    }

    /// ネットワークのルートを見分ける綴りの形（長いパスの書き方を含む。#161 の codex、2周目）
    #[test]
    fn root_location_reads_unc_and_drive_letters_in_both_spellings() {
        use RootLocation::*;
        for (path, want) in [
            (r"\\nas\photos", Unc),
            (r"\\?\UNC\nas\photos", Unc),
            (r"\\?\unc\nas\photos", Unc),
            (r"Z:\photos", Drive('Z')),
            (r"z:\photos", Drive('Z')),
            (r"\\?\Z:\photos", Drive('Z')),
            (r"\\?\写真\x", Other),
            ("/Users/me/Pictures", Other),
            ("", Other),
        ] {
            assert_eq!(root_location(path), want, "{path}");
        }
    }

    #[test]
    fn a_root_that_does_not_exist_is_skipped() {
        let watcher = watch_roots(
            &[PathBuf::from("Z:/no/such/dir")],
            Duration::from_millis(100),
            |_| {},
        )
        .unwrap();
        assert!(watcher.watched_roots.is_empty());
    }
}
