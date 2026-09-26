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

use notify_debouncer_mini::notify::RecommendedWatcher;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult, Debouncer};

#[cfg(windows)]
mod device;

/// 束を受け取る口。Windows では取り外しに合わせて監視を作り直すので、共有して持ち回る
type OnBatch = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync + 'static>;

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

/// いま張っている監視と、張り直すのに要るもの
struct Watching {
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    debounce: Duration,
    on_batch: OnBatch,
}

impl Watching {
    /// `roots` を監視し直す。**先に今の監視を丸ごと落とす**——Windows ではルートごとにディレクトリの
    /// ハンドルを握っていて、落とせば同期で手放す（win の spike で drop は 14〜34 µs、そのあと
    /// 取り外しが通った）。1本だけ `unwatch` する道は、手放すのが監視の糸の側で遅れるかもしれず、
    /// 測っていないので使わない。返すのは実際に監視できたルート
    fn rebuild(
        &mut self,
        roots: &[PathBuf],
    ) -> Result<Vec<PathBuf>, notify_debouncer_mini::notify::Error> {
        self.debouncer = None;
        let on_batch = Arc::clone(&self.on_batch);
        let mut debouncer = new_debouncer(self.debounce, move |result: DebounceEventResult| {
            if let Ok(events) = result {
                let mut paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                paths.sort();
                paths.dedup();
                if !paths.is_empty() {
                    on_batch(paths);
                }
            }
        })?;
        let mut watched = Vec::new();
        for root in roots {
            // 存在しないルート（未接続のUSB等）は監視できないのでスキップ
            if root.is_dir()
                && debouncer
                    .watcher()
                    .watch(root, RecursiveMode::Recursive)
                    .is_ok()
            {
                watched.push(root.clone());
            }
        }
        self.debouncer = Some(debouncer);
        Ok(watched)
    }
}

/// ルート群の再帰監視を開始する。
/// イベントはデバウンス（既定800ms）後に、重複除去済みのパス一覧で `on_batch` へ渡される。
///
/// **Windows では、取り外せるドライブの上のルートを「安全な取り外し」の求めで手放し、
/// 差し直されたら張り直す**（dev #35）。握ったままだと、アプリを開いている間は取り外しが
/// 必ず断られた（Kernel-PnP 225 が pictkura を名指し）。起動時に無かったルートも、
/// 差し込まれた時点で監視に入る
pub fn watch_roots(
    roots: &[PathBuf],
    debounce: Duration,
    on_batch: impl Fn(Vec<PathBuf>) + Send + Sync + 'static,
) -> Result<LibraryWatcher, notify_debouncer_mini::notify::Error> {
    let mut watching = Watching {
        debouncer: None,
        debounce,
        on_batch: Arc::new(on_batch),
    };
    let watched_roots = watching.rebuild(roots)?;
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

    /// 作り直すと、外したルートのイベントは止まり、戻したルートのイベントはまた届く
    /// （Windows の取り外しの糸が使う入口。dev #35）。対照に、外さないルートは届き続ける
    #[test]
    fn rebuild_drops_a_root_and_takes_it_back() {
        let kept = tempfile::tempdir().unwrap();
        let removable = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
        let mut w = Watching {
            debouncer: None,
            debounce: Duration::from_millis(150),
            on_batch: Arc::new(move |paths| {
                let _ = tx.send(paths);
            }),
        };
        let both = [kept.path().to_path_buf(), removable.path().to_path_buf()];
        assert_eq!(w.rebuild(&both).unwrap().len(), 2);
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
        let write = |dir: &tempfile::TempDir, name: &str| {
            std::thread::sleep(Duration::from_millis(300));
            std::fs::write(dir.path().join(name), b"x").unwrap();
        };

        // 取り外し可能な側を外す: そちらは止まり、残した側は届く
        assert_eq!(w.rebuild(&both[..1]).unwrap(), both[..1].to_vec());
        write(&removable, "while_away.jpg");
        write(&kept, "kept_1.jpg");
        let all = collect();
        assert!(has(&all, "kept_1.jpg"), "残した側は届く（対照）: {all:?}");
        assert!(!has(&all, "while_away.jpg"), "外した側は届かない: {all:?}");

        // 戻す: また届く
        assert_eq!(w.rebuild(&both).unwrap().len(), 2);
        write(&removable, "back.jpg");
        let all = collect();
        assert!(has(&all, "back.jpg"), "戻した側がまた届く: {all:?}");
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
