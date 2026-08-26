//! ライブラリルートのファイルシステム監視。
//!
//! アプリ外でのファイル操作（エクスプローラーでの追加・削除・移動）を検知して
//! DBへ追従させるための土台。notifyがOS別のAPI
//! （Windows: ReadDirectoryChangesW / macOS: FSEvents / Linux: inotify）を抽象化する。
//!
//! 爆速の原則: イベントのあったパスだけを処理する（全ルートの再スキャンはしない）。
//! イベントは短時間デバウンスしてまとめ、コピー中の連続書き込みで嵐にならないようにする。

use std::path::PathBuf;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

/// 監視ハンドル。dropすると監視が止まる。
pub struct LibraryWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
    /// 監視できているルート（存在しないルートはスキップされる）
    pub watched_roots: Vec<PathBuf>,
}

/// ルート群の再帰監視を開始する。
/// イベントはデバウンス（既定800ms）後に、重複除去済みのパス一覧で `on_batch` へ渡される。
pub fn watch_roots(
    roots: &[PathBuf],
    debounce: Duration,
    on_batch: impl Fn(Vec<PathBuf>) + Send + 'static,
) -> Result<LibraryWatcher, notify_debouncer_mini::notify::Error> {
    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        if let Ok(events) = result {
            let mut paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
            paths.sort();
            paths.dedup();
            if !paths.is_empty() {
                on_batch(paths);
            }
        }
    })?;

    let mut watched_roots = Vec::new();
    for root in roots {
        // 存在しないルート（未接続のUSB等）は監視できないのでスキップ
        if root.is_dir()
            && debouncer
                .watcher()
                .watch(root, RecursiveMode::Recursive)
                .is_ok()
        {
            watched_roots.push(root.clone());
        }
    }

    Ok(LibraryWatcher {
        _debouncer: debouncer,
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
