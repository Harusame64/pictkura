//! pictkura-core: pictkura のコアライブラリ。
//!
//! バックエンド(Rust)が重い処理（ファイルI/O、サムネイル生成、DB管理）を担い、
//! フロントエンドは表示のみを行う、という役割分担の「重い側」を実装する。
//!
//! - フェーズ1: [`config`] — 設定の定義とTOML入出力
//! - フェーズ2: [`scanner`] / [`db`] / [`sync`] — 爆速スキャンと差分検知、SQLite管理
//! - フェーズ3以降: カスタムプロトコル / サムネイル（順次追加）

pub mod av1;
pub mod browse;
pub mod cloud;
pub mod config;
pub mod db;
pub mod display_cache;
pub mod export;
pub mod heif;
pub mod import;
pub mod jpeg;
pub mod namedate;
pub mod panics;
pub mod paths;
pub mod protocol;
pub mod raw;
pub mod resize;
pub mod scanner;
pub mod search;
pub mod shell;
pub mod sidecar;
pub mod svg;
pub mod sync;
pub mod thumbs;
pub mod update;
pub mod usn;
pub mod video;
pub mod watch;

pub use browse::{list_dir, list_tree, DirListing, SourceDir, SourceFile, TreeListing};
pub use config::{Config, ConfigError};
pub use db::{Db, DbError, DirSnapshot, MediaRecord, ReadPool};
pub use export::{export_files, ExportError, ExportMode, ExportOutcome, ExportStats};
pub use import::{import_files, import_from, is_already_imported, ImportError, ImportStats};
pub use scanner::{PrunedScanOutcome, ScanOutcome, ScannedFile};
pub use search::{parse_query, MediaFilter, MediaKind, SearchQuery};
pub use sync::{
    apply_scan, scan_library, scan_library_pruned, sync_library, LibraryScan, SyncStats,
};
pub use thumbs::{ThumbQueue, ThumbnailService};
