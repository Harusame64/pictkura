//! スキャン→差分検知→DB反映 を束ねる同期処理。
//!
//! スキャン（ファイルシステム走査）はDBを必要としないため、
//! [`scan_library`] と [`apply_scan`] に分割している。呼び出し側は
//! スキャンをDBロックの外で実行でき、走査中も画像配信を止めない。
//!
//! 差分検知はSQL（一時テーブル＋外部結合、[`Db::apply_scan`]）で行い、
//! 保存済みレコードをRustのメモリへ全件読み込まない（メモリO(1)）。

use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::Config;
use crate::db::{Db, DbError, DirSnapshot};
use crate::scanner::{self, PrunedScanOutcome};

/// 同期結果の件数サマリ。
#[derive(Debug, Default, PartialEq)]
pub struct SyncStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    /// mtime一致でファイル列挙をスキップしたディレクトリ数（枝刈りの効き具合）
    pub skipped_dirs: usize,
}

/// スキャン結果と、削除判定に必要な設定ルートのスナップショット。
pub struct LibraryScan {
    pub outcome: PrunedScanOutcome,
    pub roots: Vec<PathBuf>,
}

/// ライブラリのルート群をフルスキャンする。**DBを必要としない**（ロック外で実行可能）。
/// すべてのディレクトリを列挙する（枝刈りなし＝取りこぼしの修復手段でもある）。
pub fn scan_library(config: &Config) -> LibraryScan {
    scan_library_pruned(config, &HashMap::new())
}

/// ディレクトリmtime枝刈り付きでスキャンする（段階B-2）。
/// `known_dirs` は前回スキャンの記録（[`Db::load_dirs`]）。空ならフルスキャンと等価。
pub fn scan_library_pruned(config: &Config, known_dirs: &HashMap<PathBuf, i64>) -> LibraryScan {
    LibraryScan {
        outcome: scanner::scan_roots_pruned(
            &config.library.roots,
            &config.import.extensions,
            &config.library.exclude_patterns,
            known_dirs,
        ),
        roots: config.library.roots.clone(),
    }
}

/// スキャン結果をDBと突き合わせ、差分だけを反映する。
/// ディレクトリのmtime記録（dirsテーブル）もあわせて更新する。
pub fn apply_scan(db: &mut Db, scan: &LibraryScan) -> Result<SyncStats, DbError> {
    let (added, changed, removed) = db.apply_scan(
        &scan.outcome.files,
        &scan.roots,
        &scan.outcome.ok_roots,
        Some(DirSnapshot {
            seen: &scan.outcome.seen_dirs,
            enumerated: &scan.outcome.enumerated_dirs,
        }),
    )?;
    Ok(SyncStats {
        added,
        changed,
        removed,
        skipped_dirs: scan.outcome.skipped_dirs,
    })
}

/// スキャン＋反映を一括で行うヘルパー（テスト・小規模ライブラリ向け）。
pub fn sync_library(db: &mut Db, config: &Config) -> Result<SyncStats, DbError> {
    let scan = scan_library(config);
    apply_scan(db, &scan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn 同期の一連の流れ_追加_変更_削除() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        let b = dir.path().join("b.jpg");
        fs::write(&a, b"aaa").unwrap();
        fs::write(&b, b"bbb").unwrap();

        let mut config = Config::default();
        config.library.roots = vec![dir.path().to_path_buf()];

        let mut db = Db::open_in_memory().unwrap();

        // 初回: 2件追加
        let stats = sync_library(&mut db, &config).unwrap();
        assert_eq!(
            stats,
            SyncStats {
                added: 2,
                changed: 0,
                removed: 0,
                skipped_dirs: 0
            }
        );
        assert_eq!(db.count().unwrap(), 2);

        // 変更なしで再同期: 差分ゼロ
        let stats = sync_library(&mut db, &config).unwrap();
        assert_eq!(stats, SyncStats::default());

        // aを更新（サイズ変更）、bを削除、cを追加
        fs::write(&a, b"aaaa-longer").unwrap();
        fs::remove_file(&b).unwrap();
        fs::write(dir.path().join("c.jpg"), b"ccc").unwrap();

        let stats = sync_library(&mut db, &config).unwrap();
        assert_eq!(stats.added, 1);
        assert_eq!(stats.changed, 1);
        assert_eq!(stats.removed, 1);
        assert_eq!(db.count().unwrap(), 2);
    }

    #[test]
    fn ルートが消えてもレコードは削除されない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("usb");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.jpg"), b"aaa").unwrap();

        let mut config = Config::default();
        config.library.roots = vec![root.clone()];

        let mut db = Db::open_in_memory().unwrap();
        assert_eq!(sync_library(&mut db, &config).unwrap().added, 1);

        // USBが抜かれた想定: ルートごと消す
        fs::remove_dir_all(&root).unwrap();
        let stats = sync_library(&mut db, &config).unwrap();
        assert_eq!(stats.removed, 0, "切断されたルートの配下を誤削除しない");
        assert_eq!(db.count().unwrap(), 1);
    }
}
