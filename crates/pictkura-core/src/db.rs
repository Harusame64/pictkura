//! SQLiteによるメディアライブラリDB。
//!
//! 爆速の原則:
//! - WALモードで開く（読み書き並行・書き込み高速化）
//! - INSERT/UPDATE/DELETEはすべてトランザクションでバッチ実行
//! - ファイル更新検知はサイズとmtimeの比較のみ（ハッシュ計算は禁止）
//!
//! スケールの原則（plan.md 第3部 段階A）:
//! - UIへ全件を渡さない。「日付→枚数」のサマリ（[`Db::timeline_summary`]）と
//!   日単位の取得（[`Db::list_day`]）の2段APIで、可視範囲だけを転送する
//! - `day_key`（ローカル日付のYYYYMMDD整数）を書き込み時に維持し、
//!   `(day_key, 表示時刻, id)` の複合インデックスでサマリも日取得もインデックスだけで返す
//! - 差分検知は一時テーブル＋外部結合（[`Db::apply_scan`]）。全件をRustのメモリへ読まない

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rusqlite::{params, Connection, OpenFlags};

use crate::scanner::ScannedFile;
use crate::search::{index_text, SearchQuery};

/// `media` を [`Db::row_to_record`] へ渡すときの列並び。**4か所のSELECTで共有する**
/// ——別々に書くと、列を足したときに片方だけ直して添字がずれる。
const MEDIA_COLUMNS: &str = "id, path, size, mtime_ms, width, height, taken_at_ms,
     day_key, thumb_path, thumb_state, favorite, picked, duration_ms,
     preview_width, preview_height";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("DB操作に失敗: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// `media` テーブルの1行。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaRecord {
    pub id: i64,
    pub path: PathBuf,
    /// ファイルサイズ（バイト）
    pub size: i64,
    /// 更新日時（Unixエポックミリ秒）
    pub mtime_ms: i64,
    /// **原本**の幅（メタデータ抽出後に設定。グリッドの枠確保に使う）。
    /// RAWはセンサーの原寸で、配信する絵の寸法ではない（[`Self::preview_width`]）
    pub width: Option<i64>,
    /// 原本の高さ
    pub height: Option<i64>,
    /// **埋め込みプレビュー**の幅。RAWで一覧とビューアへ配るのはこの絵で、
    /// 原本より小さいことが多い（HDR PQのCR3は 6000x4000 に対して 1620x1080）。
    ///
    /// **NULLは「まだ確かめていない」**。RAWなら確かめた時点で必ず入る
    /// （原本と同じ値のこともある）ので、`NULL` のまま残る行を後追いで拾える
    /// ——[`Db::dimensions_to_backfill`]。RAW以外は配るのが原本そのものなので、
    /// 確かめてもNULLのまま。読む側はNULLなら `width` へ落とせばよい。
    ///
    /// **ビューアが配る絵と一致する保証は無い**（一覧は長辺512、ビューアは1600で
    /// 探すので、原寸プレビューを後ろに置く形式では後者のほうが大きい）。
    /// 見当を付けるための値で、UIは絵が届いた時点で実寸に取り直す
    pub preview_width: Option<i64>,
    /// 埋め込みプレビューの高さ。入る条件は [`Self::preview_width`] と同じ
    pub preview_height: Option<i64>,
    /// 撮影日時（EXIF DateTimeOriginal、Unixエポックミリ秒）。未抽出はNULL、表示側はmtimeへフォールバック
    pub taken_at_ms: Option<i64>,
    /// 表示日（ローカルタイムゾーンのYYYYMMDD整数）。撮影日時（なければmtime）から書き込み時に計算
    pub day_key: i64,
    /// 生成済みサムネイルのパス
    pub thumb_path: Option<PathBuf>,
    /// サムネイルの品質段階: 0=なし, 1=即席（EXIF埋め込み）, 2=高品質生成済み
    pub thumb_state: i64,
    /// お気に入り（★）。ファイル更新でも維持される
    pub favorite: bool,
    /// 選別で選んだ印（⚑ Pick。0.2 ②）。★とは**別の棚**で、
    /// 「あとで見返したい写真」と「この連写から残す1枚」を混ぜないための列
    pub picked: bool,
    /// 動画の長さ（ミリ秒）。画像はNULL（第9部）
    pub duration_ms: Option<i64>,
}

/// [`Db::update_metadata`] へ渡す寸法。
///
/// 原本と、一覧が掴んだ埋め込みプレビューの2組。**数字を4つ並べて渡すと
/// 取り違える**ので型にしてある（幅と高さ、原本とプレビューの4通り）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    /// 原本（RAWならセンサー）の幅。読めなければ0
    pub width: i64,
    /// 原本の高さ
    pub height: i64,
    /// 掴んだ埋め込みプレビューの幅・高さ。**原本をそのまま配るなら `None`**
    /// （RAW以外はすべてこちら）。RAWは原本と同じ値でも入れる——`None` が
    /// 「まだ確かめていない」を意味するため
    pub preview: Option<(i64, i64)>,
}

impl Dimensions {
    /// 原本をそのまま配る形式（RAW以外はすべてこちら）。
    pub fn original(width: i64, height: i64) -> Self {
        Self {
            width,
            height,
            preview: None,
        }
    }
}

/// DBに保存済みのファイルメタデータ（差分検知用の軽量ビュー）。
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMeta {
    pub id: i64,
    pub size: i64,
    pub mtime_ms: i64,
}

/// タイムライン索引の1日分（スパースタイムラインの骨組み）。
#[derive(Debug, Clone, PartialEq)]
pub struct DaySummary {
    /// ローカル日付のYYYYMMDD整数（例: 20260812）
    pub day_key: i64,
    pub count: i64,
    /// その日の代表（最新）レコード。カレンダーの日セルのカバー表示用
    pub cover_id: i64,
    pub cover_mtime_ms: i64,
    pub cover_thumb_state: i64,
}

/// 表示時刻（撮影日時、なければmtime）のSQL式。
/// 複合インデックスと各クエリで**文字列として完全一致**させること
/// （SQLiteの式インデックスは式のテキスト一致で適用可否を判定する）。
const SORT_TS: &str = "COALESCE(taken_at_ms, mtime_ms)";

/// 検索索引の初期構築で「どこまで索引化したか」を持つmetaキー。
const FTS_CURSOR_KEY: &str = "fts_cursor";
/// 初期構築の対象上限ID（これより新しい行はトリガが直接索引化する）。
const FTS_BUILD_MAX_KEY: &str = "fts_build_max";
/// 索引の構成（列・トークナイザ・索引語の作り方）の版。変えると索引を作り直す。
/// v2: CJKの連続に末尾1文字の単独トークンを追加（1文字検索がどの位置でも効くように）
/// v3: bigram展開の対象を「分かち書きしない文字体系」へ拡大（タイ・ラオ・クメール・ビルマ・ハングル）
/// パスの綴りを揃えた版。上げると一度だけDBを書き換える。
/// v1: 区切り文字をバックスラッシュへ統一し、ドライブ文字を大文字にする
const PATH_SCHEMA_KEY: &str = "path_schema";
const PATH_SCHEMA_VERSION: &str = "v1";

const FTS_SCHEMA_KEY: &str = "fts_schema";
const FTS_SCHEMA_VERSION: &str = "v3";

/// `media.camera_id` の「EXIFを確認したがカメラ情報が無かった」印。
/// NULL（＝未確認）と区別することで、後追いのカメラ補完スイープが
/// 同じファイルを毎起動読み直すのを防ぐ。`cameras` に0番の行は作らないので、
/// カメラ別集計（JOIN）や `camera:` 絞り込み（IN）には現れない。
const CAMERA_NONE: i64 = 0;

/// [`Db::rows_with_fallback_taken_at`] が返す行（ID・パス・mtime）。
pub type FallbackDateRow = (i64, PathBuf, i64);

/// mtime等のエポックミリ秒からローカル日付のYYYYMMDD整数を作るSQL式。
/// day_keyの計算はすべてSQLite側（strftime + localtime）に統一する。
fn day_key_expr(ms_expr: &str) -> String {
    format!("CAST(strftime('%Y%m%d', ({ms_expr})/1000, 'unixepoch', 'localtime') AS INTEGER)")
}

/// パス文字列から親ディレクトリを取り出すSQL式（`\` と `/` の両対応）。
///
/// SQLiteの `rtrim(x, y)` は「yに含まれる文字」を右端から剥がす。
/// y = パスから区切り文字を除いた文字列にすると、ファイル名の文字はすべて
/// yに含まれる（パス自身の部分文字列だから）ため右端からすべて剥がれ、
/// 最初に現れる区切り文字で必ず止まる。仕上げに末尾の区切り文字も落とす。
/// 例: 'D:\photos\a.jpg' → 'D:\photos'、'a.jpg' → ''（区切りなし）
fn parent_dir_expr(path_expr: &str) -> String {
    format!(
        r#"rtrim(rtrim({p}, replace(replace({p}, '\', ''), '/', '')), '\/')"#,
        p = path_expr
    )
}

/// `IN (...)` 用のプレースホルダ列（`?,?,?`）を作る。
/// 範囲選択と選択の確認で使うFTSの形。**測って決めた**
/// （`検索語つきの選択はどちらの形が速いか` のベンチ。2026-08-17に3万件で実測）。
///
/// 実行計画だけ見ると `Probe` が良く見える——日の索引で駆動し、一時表も
/// 並べ直しも出ない。**が、実際は桁で遅い**。FTSを行ごとに引く定数が乗るうえ、
/// このアプリの検索語は必ず末尾が前方一致（`term_to_match`）なので、
/// 行ごとに辞書の範囲展開が走る。3万件・一致1.5万件での実測:
///
/// | | List | Probe |
/// |---|---:|---:|
/// | 範囲選択（151件ぶん） | 6.7ms | 636.8ms |
/// | 範囲選択（全域） | 9.2ms | 26.1秒 |
/// | 選択の確認（500枚） | 8.4ms | 529.1ms |
/// | 選択の確認（1.5万枚） | 295.5ms | 26.7秒 |
///
/// 一致集合を1回作る側は1行あたりの定数が極端に小さいので、
/// **一致件数に比例しても十分速い**。もっと大きなライブラリで問題になるなら、
/// 行ごとに引くのではなく「一致集合を操作ごとに1回だけ一時表へ実体化して
/// 使い回す」形が筋。`Probe` は計測用に残してある。
const FTS_FOR_SELECTION: FtsMode = FtsMode::List;

/// 検索語の条件をどう書くか。**実行計画がこれで決まる**。
#[derive(Clone, Copy, Debug)]
enum FtsMode {
    /// 一致するrowidの集合を1回作り、それへの `IN` で絞る。
    /// 日の一覧・サマリのように「索引でその日へシークしてから絞る」問い合わせ向き。
    List,
    /// 候補の行ごとにFTSをrowidで引く（相関サブクエリ）。
    /// **範囲や選択の確認のように、候補が先に小さく決まる**問い合わせ向き。
    /// 一致集合を作らないので、ライブラリ全体の一致件数に引きずられない。
    /// 表を `media` の名前で参照するので、別名を付けた文では使えない。
    ///
    /// **製品では使っていない**。計画は綺麗になるが実測では桁で遅い
    /// （[`FTS_FOR_SELECTION`] の表）。比べ直せるように残してある
    #[allow(dead_code)]
    Probe,
}

impl FtsMode {
    fn cond(self) -> &'static str {
        match self {
            FtsMode::List => "id IN (SELECT rowid FROM media_fts WHERE media_fts MATCH ?)",
            FtsMode::Probe => {
                "EXISTS (SELECT 1 FROM media_fts WHERE media_fts MATCH ? AND rowid = media.id)"
            }
        }
    }
}

/// 全ID取得の文。**並びは索引がそのまま供給する**（`idx_media_day_sort`）。
/// 実行計画のテストと同じ文を共有するために切り出してある。
fn all_ids_sql(filter: &str) -> String {
    format!("SELECT id FROM media {filter} ORDER BY day_key DESC, {SORT_TS} DESC, id DESC")
}

/// タイムライン索引（日付→枚数＋代表）のSQL。
///
/// **全行のGROUP BY**なので、絞り込みを足したときにここが表スキャンへ落ちると
/// 骨組みの取得がそのまま遅くなる。実行計画に釘を打てるよう、組み立てを1か所に置く。
fn summary_sql(filter: &str) -> String {
    format!(
        "SELECT day_key, COUNT(*), id, mtime_ms, thumb_state, MAX({SORT_TS})
             FROM media {filter}
             GROUP BY day_key ORDER BY day_key DESC"
    )
}

/// 範囲取得の条件。**`day_key` を先頭に置く**ので索引でシークでき、
/// 端の日の中だけを表示時刻とidで削る。
fn between_conds() -> Vec<String> {
    vec![
        "day_key BETWEEN ? AND ?".to_string(),
        format!("(day_key < ? OR {SORT_TS} < ? OR ({SORT_TS} = ? AND id <= ?))"),
        format!("(day_key > ? OR {SORT_TS} > ? OR ({SORT_TS} = ? AND id >= ?))"),
    ]
}

/// 範囲取得の文。条件は [`between_conds`] を含んだもの。
fn between_sql(conds: &[String]) -> String {
    format!(
        "SELECT id FROM media WHERE {} ORDER BY day_key DESC, {SORT_TS} DESC, id DESC",
        conds.join(" AND ")
    )
}

/// `IN (...)` のプレースホルダ列（`?,?,?`）を作る。
fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// ディレクトリパスの正規化: 末尾の区切り文字を落とす
/// （parent_dir_exprの出力と、スキャナーが報告するディレクトリパスを一致させる）。
fn normalize_dir(path: &Path) -> String {
    crate::paths::normalize_dir_str(path)
}

/// 「このパスを含む最も深い設定ルートが走査成功していれば1、失敗なら0、
/// どのルートにも属さなければ1（削除可）」を返すCASE式とそのパラメータを組む。
/// media.path にも dirs.path にも使えるよう対象カラムを引数で受ける。
/// パラメータはプレースホルダ ?1.. を使うため、クエリの他の部分に
/// パラメータを持たないこと。
fn root_case_sql(
    configured_roots: &[PathBuf],
    ok_roots: &[PathBuf],
    column: &str,
) -> (String, Vec<String>) {
    let mut roots: Vec<&PathBuf> = configured_roots.iter().collect();
    roots.sort_by_key(|r| std::cmp::Reverse(r.components().count()));
    let mut case_sql = String::from("CASE\n");
    let mut params: Vec<String> = Vec::new();
    for root in &roots {
        let ok = ok_roots.iter().any(|r| r == *root);
        // 末尾の区切り文字を落としてからパターンを組む。`D:\` のようなルートを
        // そのまま使うと `D:\\%` になり配下にマッチせず、保護ルールが効かなくなる
        // ルートの綴りもDBのパスと同じ形に揃える。ここを揃えないと
        // 「どのルートにも属さない」と判定され、**保護が外れて消える**
        let escaped = escape_like(&crate::paths::normalize_dir_str(root));
        let a = params.len() + 1;
        let b = params.len() + 2;
        case_sql.push_str(&format!(
            "WHEN {column} LIKE ?{a} ESCAPE '!' OR {column} LIKE ?{b} ESCAPE '!' THEN {}\n",
            if ok { 1 } else { 0 }
        ));
        params.push(format!("{escaped}\\%"));
        params.push(format!("{escaped}/%"));
    }
    case_sql.push_str("ELSE 1 END"); // どのルートにも属さない → 削除可
                                     // ルート未設定時はWHEN句のないCASEになりSQL構文エラーになるため、
                                     // 「常に削除可」の定数条件へ置き換える（＝全レコードがどのルートにも属さない）
    if roots.is_empty() {
        ("1".to_string(), params)
    } else {
        (case_sql, params)
    }
}

/// 部分スキャン反映用のディレクトリ情報スナップショット（段階B-2）。
pub struct DirSnapshot<'a> {
    /// 存在を確認した全ディレクトリとそのmtime（スキップ分も含む）
    pub seen: &'a [(PathBuf, i64)],
    /// 実際に中身を列挙したディレクトリ（削除判定はこの直下に限定される）
    pub enumerated: &'a [PathBuf],
}

pub struct Db {
    conn: Connection,
}

impl Db {
    /// ファイルパスを指定して開く。存在しなければ作成し、スキーマを初期化する。
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// 書き込みトランザクションを始める（**必ずIMMEDIATE**）。
    ///
    /// 既定のDEFERREDだと、最初のSELECTで読み取りスナップショットを取ってから
    /// 書き込みロックへ昇格する。その間に他の接続（スキャン反映・サムネイル
    /// ワーカー・ファイル監視・ジャニター）がコミットしていると
    /// `SQLITE_BUSY_SNAPSHOT` が**ビジーハンドラを通らず即座に**返り、
    /// `busy_timeout` を設定していても「database is locked」で落ちる。
    /// 最初から書き込みロックを取れば、混み合っていても待って順番に通る。
    fn write_tx(&mut self) -> Result<rusqlite::Transaction<'_>, DbError> {
        Ok(self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?)
    }

    /// インメモリDBを開く（テスト用）。
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    /// リードオンリーで開く（読み取り接続プール用）。
    /// スキーマ初期化は行わない。呼び出し側が先に読み書き接続でDBを作っていること。
    pub fn open_read_only(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI,
        )?;
        // WALのリーダーは書き込みをブロックしないが、チェックポイント等の
        // 一瞬のロックと重なった場合に備えて待機させる
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self { conn })
    }

    fn init(conn: Connection) -> Result<Self, DbError> {
        // WALモード＋パフォーマンス設定（インメモリDBではjournal_modeは"memory"のまま）
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // サムネイルワーカーが並行書き込みするため、ロック競合時は待機させる
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS media (
                id          INTEGER PRIMARY KEY,
                path        TEXT NOT NULL UNIQUE,
                size        INTEGER NOT NULL,
                mtime_ms    INTEGER NOT NULL,
                width       INTEGER,
                height      INTEGER,
                taken_at_ms INTEGER,
                thumb_path  TEXT,
                thumb_state INTEGER NOT NULL DEFAULT 0,
                favorite    INTEGER NOT NULL DEFAULT 0,
                picked      INTEGER NOT NULL DEFAULT 0,
                day_key     INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_media_taken_at ON media(taken_at_ms DESC);
            "#,
        )?;
        // 旧スキーマからの移行: 列がなければ追加する（既にあれば失敗を無視）
        let _ = conn.execute(
            "ALTER TABLE media ADD COLUMN thumb_state INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE media ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE media ADD COLUMN day_key INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // 0.2 ②: 選別の印（★とは別の棚）
        let _ = conn.execute(
            "ALTER TABLE media ADD COLUMN picked INTEGER NOT NULL DEFAULT 0",
            [],
        );
        // 段階F-4: 一覧が掴んだ埋め込みプレビューの寸法。**RAWなら必ず入る**
        // （原本と同じ値のこともある）。NULLは「まだ確かめていない」印で、
        // 起動後の後追い（[`Db::dimensions_to_backfill`]）が拾う
        let _ = conn.execute("ALTER TABLE media ADD COLUMN preview_width INTEGER", []);
        let _ = conn.execute("ALTER TABLE media ADD COLUMN preview_height INTEGER", []);
        // 段階B-3: 高品質サムネイルのLRUキャッシュ管理用
        let _ = conn.execute("ALTER TABLE media ADD COLUMN thumb_bytes INTEGER", []);
        let _ = conn.execute("ALTER TABLE media ADD COLUMN thumb_used_ms INTEGER", []);
        // 段階B-2: 親ディレクトリ列（部分スキャン時の削除判定を「実際に列挙した
        // ディレクトリ直下」に限定するための結合キー）
        let _ = conn.execute("ALTER TABLE media ADD COLUMN parent_dir TEXT", []);
        conn.execute(
            &format!(
                "UPDATE media SET parent_dir = {} WHERE parent_dir IS NULL",
                parent_dir_expr("path")
            ),
            [],
        )?;
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_media_parent ON media(parent_dir);
            -- 段階B-2: スキャン済みディレクトリのmtime記録（枝刈りの照合元）
            CREATE TABLE IF NOT EXISTS dirs (
                path     TEXT PRIMARY KEY,
                mtime_ms INTEGER NOT NULL
            ) WITHOUT ROWID;
            -- 汎用キーバリュー（スキャン設定のフィンガープリント等）
            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            ) WITHOUT ROWID;
            "#,
        )?;
        // タイムライン索引: サマリはこのインデックスのスキャンだけで、
        // 日単位取得はシーク＋整列済み読み出しだけで返る
        conn.execute_batch(&format!(
            r#"
            CREATE INDEX IF NOT EXISTS idx_media_day_sort
                ON media(day_key DESC, {SORT_TS} DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_media_fav_day
                ON media(day_key DESC, {SORT_TS} DESC, id DESC) WHERE favorite = 1;
            CREATE INDEX IF NOT EXISTS idx_media_picked_day
                ON media(day_key DESC, {SORT_TS} DESC, id DESC) WHERE picked = 1;
            CREATE INDEX IF NOT EXISTS idx_media_thumb_lru
                ON media(thumb_used_ms, thumb_bytes) WHERE thumb_state = 2;
            "#
        ))?;
        // 移行: day_key未計算（=0）の既存レコードを埋める
        conn.execute(
            &format!(
                "UPDATE media SET day_key = {} WHERE day_key = 0",
                day_key_expr(SORT_TS)
            ),
            [],
        )?;
        Self::init_search(&conn)?;
        Ok(Self { conn })
    }

    /// 検索インデックス（第4部 段階D）のスキーマを用意する。
    ///
    /// - `cameras`: カメラ名の正規化表（`media.camera_id` から参照）。
    ///   カメラ名を本表に持たせないのは、1000万行×数十バイトの重複を避けるため
    /// - `media_fts`: FTS5の全文索引。`content=''`（contentless）なので**索引だけ**を
    ///   持ちDBの肥大を抑える。`contentless_delete=1` により rowid 指定の削除ができる
    /// - トリガ: mediaの**追加と削除**に索引を自動追随させる（索引の中身＝
    ///   ファイル名と親フォルダは行の生涯で変わらないので更新のトリガは不要）。
    ///   スキャン経路（apply_scan / apply_partial_scan / upsert_files）が増えても
    ///   索引の同期を書き忘れないよう、DB側で閉じている
    fn init_search(conn: &Connection) -> Result<(), DbError> {
        Self::register_functions(conn)?;
        let _ = conn.execute("ALTER TABLE media ADD COLUMN camera_id INTEGER", []);
        // 動画の長さ（ミリ秒。第9部）。画像はNULLのまま
        let _ = conn.execute("ALTER TABLE media ADD COLUMN duration_ms INTEGER", []);
        // 種類（0=画像 / 1=RAW / 2=動画。[`crate::MediaKind`]）。
        //
        // 拡張子から決まる派生値だが**列に持たせる**。サマリは全行のGROUP BYなので、
        // 行ごとに拡張子を切り出していては「日付→枚数」が索引だけで返らなくなる。
        // 綴りはパスと一緒に決まって以後変わらないので、入れるのは追加のときだけでよい
        let _ = conn.execute("ALTER TABLE media ADD COLUMN kind INTEGER", []);
        conn.execute(
            "UPDATE media SET kind = pk_kind(path) WHERE kind IS NULL",
            [],
        )?;
        conn.execute_batch(&format!(
            r#"
            -- 種類での絞り込み（画像 / RAW / 動画）。サマリは日ごとの集計なので、
            -- 種類を先頭に置いた複合索引にすると、絞ったままでも索引のスキャンで返る
            CREATE INDEX IF NOT EXISTS idx_media_kind_day
                ON media(kind, day_key DESC, {SORT_TS} DESC, id DESC);
            "#
        ))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS cameras (
                id   INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );
            -- カメラ別の集計（GROUP BY camera_id）と絞り込みを索引だけで返すため
            CREATE INDEX IF NOT EXISTS idx_media_camera ON media(camera_id);
            "#,
        )?;

        // パスの綴りを揃える（一度きり）。索引はパスから作るので、
        // 書き換えたらFTSも作り直させる
        if Self::migrate_path_spelling(conn)? > 0 {
            conn.execute("DELETE FROM meta WHERE key = ?1", params![FTS_SCHEMA_KEY])?;
        }

        // 索引は派生データなので、構成（列・トークナイザ）を変えたら作り直す。
        // 版が違えば丸ごと捨てて、初期構築のカーソルも巻き戻す
        let version = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![FTS_SCHEMA_KEY],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if version.as_deref() != Some(FTS_SCHEMA_VERSION) {
            conn.execute_batch(
                "DROP TRIGGER IF EXISTS media_fts_ai;
                 DROP TRIGGER IF EXISTS media_fts_ad;
                 DROP TRIGGER IF EXISTS media_fts_au;
                 DROP TABLE IF EXISTS media_fts;",
            )?;
            conn.execute(
                "DELETE FROM meta WHERE key IN (?1, ?2)",
                params![FTS_CURSOR_KEY, FTS_BUILD_MAX_KEY],
            )?;
        }

        conn.execute_batch(
            r#"
            -- 全文索引。content='' なので**索引だけ**を持ち（本文の重複保存なし）、
            -- contentless_delete=1 により rowid 指定の削除ができる。
            -- カメラは列に入れない: 機種数は数十しかない低カーディナリティの
            -- ファセットで、全行ぶんの索引エントリを持つのは容量も速度も無駄。
            -- `camera:` は cameras 表を引いて camera_id のインデックスシークにする。
            --
            -- prefix索引は**張らない**。打鍵途中の前方一致（"沖"* や "IMG_00000"*）は
            -- 用語辞書のレンジスキャンで足り、100万件の実測でも prefix='2 3' ありと
            -- 速度差がなかった（1文字 "沖" で170ms、いずれもヒット件数が支配的）。
            -- 一方でprefix索引は容量+48%・構築時間2倍のコストがある
            CREATE VIRTUAL TABLE IF NOT EXISTS media_fts USING fts5(
                name, folder,
                tokenize = "unicode61 remove_diacritics 2",
                content = '',
                contentless_delete = 1
            );
            CREATE TRIGGER IF NOT EXISTS media_fts_ai AFTER INSERT ON media BEGIN
                INSERT INTO media_fts(rowid, name, folder)
                VALUES (new.id, pk_idx(pk_name(new.path)), pk_idx(pk_folder(new.parent_dir)));
            END;
            -- pathとparent_dirは行の生涯で変わらない（pathはON CONFLICTのキー）ため、
            -- 索引の張り替えが要るのは追加と削除だけ。カメラはFTSに載せないので
            -- camera_idの更新では何もしない
            CREATE TRIGGER IF NOT EXISTS media_fts_ad AFTER DELETE ON media BEGIN
                DELETE FROM media_fts WHERE rowid = old.id;
            END;
            "#,
        )?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![FTS_SCHEMA_KEY, FTS_SCHEMA_VERSION],
        )?;
        Ok(())
    }

    /// パスの綴りを揃える一度きりの移行。書き換えた行数を返す。
    ///
    /// 同じファイルが「通常スキャン経由（設定ルートの綴りを引きずる）」と
    /// 「USNジャーナル経由（Win32が返す全部バックスラッシュ）」で
    /// **別の行として2件入る**ため、綴りを揃えて重複を1件に統合する。
    /// 実際に、クラウドのファイルを取り寄せただけで32件の重複ができた。
    fn migrate_path_spelling(conn: &Connection) -> Result<usize, DbError> {
        let done = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![PATH_SCHEMA_KEY],
                |r| r.get::<_, String>(0),
            )
            .ok();
        if done.as_deref() == Some(PATH_SCHEMA_VERSION) {
            return Ok(0);
        }

        let tx = conn.unchecked_transaction()?;

        // 全行の「揃えた綴り」を一時テーブルへ。**Rustのメモリへは読まない**
        // （1000万件を想定するので、ここで全件を持つと起動できなくなる）
        tx.execute_batch(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS path_fix_tmp (
                id   INTEGER PRIMARY KEY,
                norm TEXT NOT NULL
            );
            DELETE FROM path_fix_tmp;
            INSERT INTO path_fix_tmp (id, norm) SELECT id, pk_norm(path) FROM media;
            CREATE INDEX IF NOT EXISTS ix_path_fix_norm ON path_fix_tmp(norm);

            -- 綴りを揃えるとぶつかる組。残すのは「サムネイルが進んでいる方」、
            -- 同じなら若いid（先に見つかった方）
            CREATE TEMP TABLE IF NOT EXISTS path_dup_tmp (
                norm     TEXT PRIMARY KEY,
                keep_id  INTEGER NOT NULL,
                donor_id INTEGER,
                favorite INTEGER NOT NULL DEFAULT 0,
                picked   INTEGER NOT NULL DEFAULT 0
            );
            DELETE FROM path_dup_tmp;
            INSERT INTO path_dup_tmp (norm, keep_id)
            SELECT t.norm,
                   (SELECT m.id FROM media m JOIN path_fix_tmp t2 ON t2.id = m.id
                    WHERE t2.norm = t.norm
                    ORDER BY m.thumb_state DESC, m.id ASC LIMIT 1)
            FROM path_fix_tmp t GROUP BY t.norm HAVING COUNT(*) > 1;

            -- 消える側のうち、**メタデータが埋まっている行**を1つ選んで引き継ぎ元にする。
            -- お気に入り（★）と選別の印（⚑）は片方でも付いていれば残す
            -- （ユーザーが付けた情報は消さない）。**★と⚑は対称に扱うこと**——
            -- 移行を作り直して再実行したときに、片方だけ黙って落ちるのを防ぐ
            UPDATE path_dup_tmp SET
                donor_id = (SELECT o.id FROM media o JOIN path_fix_tmp ot ON ot.id = o.id
                            WHERE ot.norm = path_dup_tmp.norm AND o.id <> path_dup_tmp.keep_id
                            ORDER BY (o.width IS NOT NULL) DESC, o.id ASC LIMIT 1),
                favorite = COALESCE((SELECT MAX(o.favorite) FROM media o
                                     JOIN path_fix_tmp ot ON ot.id = o.id
                                     WHERE ot.norm = path_dup_tmp.norm), 0),
                picked   = COALESCE((SELECT MAX(o.picked) FROM media o
                                     JOIN path_fix_tmp ot ON ot.id = o.id
                                     WHERE ot.norm = path_dup_tmp.norm), 0);
            "#,
        )?;

        // 引き継ぎ: 残す行に足りない情報を、消える行から埋める
        tx.execute(
            r#"
            UPDATE media SET
                favorite    = COALESCE((SELECT d.favorite FROM path_dup_tmp d WHERE d.keep_id = media.id), favorite),
                picked      = COALESCE((SELECT d.picked   FROM path_dup_tmp d WHERE d.keep_id = media.id), picked),
                taken_at_ms = COALESCE(taken_at_ms, (SELECT o.taken_at_ms FROM media o
                                WHERE o.id = (SELECT d.donor_id FROM path_dup_tmp d WHERE d.keep_id = media.id))),
                camera_id   = COALESCE(camera_id,   (SELECT o.camera_id FROM media o
                                WHERE o.id = (SELECT d.donor_id FROM path_dup_tmp d WHERE d.keep_id = media.id))),
                width       = COALESCE(width,       (SELECT o.width FROM media o
                                WHERE o.id = (SELECT d.donor_id FROM path_dup_tmp d WHERE d.keep_id = media.id))),
                height      = COALESCE(height,      (SELECT o.height FROM media o
                                WHERE o.id = (SELECT d.donor_id FROM path_dup_tmp d WHERE d.keep_id = media.id)))
            WHERE id IN (SELECT keep_id FROM path_dup_tmp)
            "#,
            [],
        )?;

        // 消える行のサムネイルファイルは孤児になるので、消す前に控える
        // （対象は重複した行だけなので件数は少ない）
        let orphans: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT m.thumb_path FROM media m JOIN path_fix_tmp t ON t.id = m.id
                 JOIN path_dup_tmp d ON d.norm = t.norm
                 WHERE m.id <> d.keep_id AND m.thumb_path IS NOT NULL",
            )?;
            let mapped = stmt.query_map([], |r| r.get(0))?;
            mapped.collect::<Result<Vec<_>, _>>()?
        };

        let dropped = tx.execute(
            "DELETE FROM media WHERE id IN (
                 SELECT m.id FROM media m JOIN path_fix_tmp t ON t.id = m.id
                 JOIN path_dup_tmp d ON d.norm = t.norm
                 WHERE m.id <> d.keep_id)",
            [],
        )?;

        // 残った行の綴りを揃える
        let updated = tx.execute(
            &format!(
                "UPDATE media SET path = (SELECT t.norm FROM path_fix_tmp t WHERE t.id = media.id),
                                  parent_dir = {}
                 WHERE path <> (SELECT t.norm FROM path_fix_tmp t WHERE t.id = media.id)",
                parent_dir_expr("(SELECT t.norm FROM path_fix_tmp t WHERE t.id = media.id)")
            ),
            [],
        )?;

        // dirs も同じ綴りへ寄せる（重複は片方だけ残す）
        let dirs = tx.execute(
            "DELETE FROM dirs WHERE path <> pk_norm(path)
               AND EXISTS (SELECT 1 FROM dirs o WHERE o.path = pk_norm(dirs.path))",
            [],
        )?;
        let dirs = dirs
            + tx.execute(
                "UPDATE dirs SET path = pk_norm(path) WHERE path <> pk_norm(path)",
                [],
            )?;

        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![PATH_SCHEMA_KEY, PATH_SCHEMA_VERSION],
        )?;
        tx.execute_batch("DROP TABLE IF EXISTS path_fix_tmp; DROP TABLE IF EXISTS path_dup_tmp;")?;
        tx.commit()?;

        for thumb in &orphans {
            let _ = std::fs::remove_file(thumb);
        }

        Ok(updated + dropped + dirs)
    }

    /// 索引用テキストを作るSQL関数を登録する（トリガと初期構築SQLから呼ばれる）。
    /// **接続ごと**の登録なので、mediaへ書き込む接続では必ず呼ばれること
    /// （未登録の接続で書くとトリガが "no such function" で失敗する）。
    fn register_functions(conn: &Connection) -> Result<(), DbError> {
        use rusqlite::functions::FunctionFlags;
        let flags = FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_UTF8;

        // CJKをbigramへ展開する（日本語の中間一致用。search::index_text 参照）
        conn.create_scalar_function("pk_idx", 1, flags, |ctx| {
            let s: Option<String> = ctx.get(0)?;
            Ok(s.map(|s| index_text(&s)))
        })?;
        // パスの綴りを揃える（paths::normalize_str と同じ規則を使う）。
        // 移行SQLから呼ぶことで、Rust側と規則がずれない
        conn.create_scalar_function("pk_norm", 1, flags, |ctx| {
            let s: Option<String> = ctx.get(0)?;
            Ok(s.map(|s| crate::paths::normalize_str(&s)))
        })?;
        // パスからファイル名を取り出す
        conn.create_scalar_function("pk_name", 1, flags, |ctx| {
            let s: Option<String> = ctx.get(0)?;
            Ok(s.map(|s| {
                s.rsplit(['\\', '/'])
                    .next()
                    .unwrap_or(s.as_str())
                    .to_string()
            }))
        })?;
        // パスの拡張子から種類（0=画像 / 1=RAW / 2=動画）を決める。
        // **判定の規則はRust側（[`crate::MediaKind`]）に1つだけ**置き、
        // 移行SQLとINSERTの両方からこの関数を呼ぶ——SQLに拡張子の一覧を
        // 書き写すと、対応形式を足したときに片方だけ古くなる
        conn.create_scalar_function("pk_kind", 1, flags, |ctx| {
            let s: Option<String> = ctx.get(0)?;
            Ok(s.map(|s| crate::MediaKind::from_path(std::path::Path::new(&s)) as i64))
        })?;
        // 親ディレクトリのうち**末尾3階層**だけを索引対象にする。
        // ライブラリルートの共通部分（C:\Users\...\Pictures）は全行で同じで
        // 検索の役に立たないうえ、行数ぶんの索引が無駄に増えるため落とす
        conn.create_scalar_function("pk_folder", 1, flags, |ctx| {
            let s: Option<String> = ctx.get(0)?;
            Ok(s.map(|s| {
                let mut parts: Vec<&str> = s
                    .rsplit(['\\', '/'])
                    .filter(|p| !p.is_empty())
                    .take(3)
                    .collect();
                parts.reverse();
                parts.join(" ")
            }))
        })?;
        Ok(())
    }

    /// 新規・変更ファイルをトランザクションでまとめてupsertする。
    /// 変更されたファイルは幅・高さ・撮影日時・サムネイルを無効化（NULL化）する。
    pub fn upsert_files(&mut self, files: &[ScannedFile]) -> Result<(), DbError> {
        let tx = self.write_tx()?;
        {
            let mut stmt = tx.prepare_cached(&format!(
                r#"
                INSERT INTO media (path, size, mtime_ms, day_key, parent_dir, kind)
                VALUES (?1, ?2, ?3, {}, {}, pk_kind(?1))
                ON CONFLICT(path) DO UPDATE SET
                    size = excluded.size,
                    mtime_ms = excluded.mtime_ms,
                    day_key = excluded.day_key,
                    width = NULL,
                    height = NULL,
                    preview_width = NULL,
                    preview_height = NULL,
                    taken_at_ms = NULL,
                    thumb_path = NULL,
                    thumb_state = 0,
                    thumb_bytes = NULL,
                    thumb_used_ms = NULL,
                    camera_id = NULL,
                    duration_ms = NULL
                "#,
                day_key_expr("?3"),
                parent_dir_expr("?1")
            ))?;
            for f in files {
                stmt.execute(params![
                    crate::paths::normalize_str(&f.path.to_string_lossy()),
                    f.size,
                    f.mtime_ms
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// スキャン結果を一時テーブルへ投入し、SQLの外部結合で差分（追加・変更・削除）を
    /// 検知して反映する。全件をRustのメモリへ読み込まない（メモリO(1)）。
    ///
    /// 削除判定のルール（旧compute_diffと同一）:
    /// - スキャンで見つからなかった保存済みパスのうち、
    ///   - どの設定ルートにも属さないもの → 削除（ルートが設定から外された）
    ///   - 正常に走査できたルート（ok_roots）に属するもの → 削除
    ///   - 走査に失敗したルートに属するもの → **保持**（USB切断等の誤削除防止）
    /// - 入れ子のルートは**最も深いルート**の成否で判定する
    ///
    /// 削除判定の追加ルール（段階B-2、`dirs` がSomeの場合）:
    /// - 削除できるのは「実際に列挙したディレクトリ（enumerated）直下」のレコードか、
    ///   親ディレクトリ自体が消えた（dirsテーブルの掃除で消えた）レコードだけ。
    ///   枝刈りでスキップしたディレクトリ配下は削除判定の対象にしない
    /// - あわせてdirsテーブルを更新する（見えたディレクトリをupsert、消えたものを削除）
    ///
    /// 戻り値は (added, changed, removed) の件数。
    pub fn apply_scan(
        &mut self,
        files: &[ScannedFile],
        configured_roots: &[PathBuf],
        ok_roots: &[PathBuf],
        dirs: Option<DirSnapshot<'_>>,
    ) -> Result<(usize, usize, usize), DbError> {
        let tx = self.write_tx()?;
        let (added, changed) = Self::stage_scan_tmp(&tx, files)?;

        // 段階B-2: ディレクトリ情報があれば、dirsテーブルを先に更新する
        // （「親ディレクトリ自体が消えた」の判定は更新後のdirsを参照するため）
        if let Some(snapshot) = &dirs {
            Self::stage_dir_tmps(&tx, snapshot.seen, snapshot.enumerated)?;
            // 消えたディレクトリを掃除する。走査に失敗したルートの配下は保持
            // （メディアの削除保護と同じルール）
            let (dir_case, dir_params) = root_case_sql(configured_roots, ok_roots, "dirs.path");
            tx.execute(
                &format!(
                    "DELETE FROM dirs
                     WHERE NOT EXISTS (SELECT 1 FROM dirs_seen_tmp s WHERE s.path = dirs.path)
                     AND {dir_case} = 1"
                ),
                rusqlite::params_from_iter(dir_params.iter()),
            )?;
            tx.execute(
                // WHERE true はSELECT+ON CONFLICT併用時のパース曖昧性回避（SQLiteの仕様）
                "INSERT INTO dirs (path, mtime_ms)
                 SELECT path, mtime_ms FROM dirs_seen_tmp WHERE true
                 ON CONFLICT(path) DO UPDATE SET mtime_ms = excluded.mtime_ms",
                [],
            )?;
        }

        // 削除: スキャンに現れなかったパスのうち、削除してよいものだけを消す。
        // 「このパスを含む最も深い設定ルート」を深い順のCASEで判定する
        let (condition, case_params) = root_case_sql(configured_roots, ok_roots, "media.path");
        // 枝刈りスキャン時の追加ガード: 実際に列挙したディレクトリ直下か、
        // 親ディレクトリ自体が消えたレコードだけを削除対象にする
        let dir_guard = if dirs.is_some() {
            "AND (EXISTS (SELECT 1 FROM dirs_enum_tmp e WHERE e.path = media.parent_dir)
                  OR NOT EXISTS (SELECT 1 FROM dirs d WHERE d.path = media.parent_dir))"
        } else {
            ""
        };
        let removed = tx.execute(
            &format!(
                "DELETE FROM media
                 WHERE NOT EXISTS (SELECT 1 FROM scan_tmp s WHERE s.path = media.path)
                 AND {condition} = 1 {dir_guard}"
            ),
            rusqlite::params_from_iter(case_params.iter()),
        )?;

        tx.execute("DELETE FROM scan_tmp", [])?;
        if dirs.is_some() {
            tx.execute("DELETE FROM dirs_seen_tmp", [])?;
            tx.execute("DELETE FROM dirs_enum_tmp", [])?;
        }
        tx.commit()?;
        Ok((added as usize, changed as usize, removed))
    }

    /// スキャン結果を一時テーブルへ投入し、新規・変更をupsertする（apply系の共通部）。
    /// 戻り値は (added, changed) の件数。
    fn stage_scan_tmp(
        tx: &rusqlite::Transaction<'_>,
        files: &[ScannedFile],
    ) -> Result<(i64, i64), DbError> {
        tx.execute_batch(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS scan_tmp (
                path     TEXT PRIMARY KEY,
                size     INTEGER NOT NULL,
                mtime_ms INTEGER NOT NULL
            ) WITHOUT ROWID;
            DELETE FROM scan_tmp;
            "#,
        )?;
        {
            let mut ins = tx.prepare_cached(
                "INSERT OR REPLACE INTO scan_tmp (path, size, mtime_ms) VALUES (?1, ?2, ?3)",
            )?;
            for f in files {
                ins.execute(params![
                    crate::paths::normalize_str(&f.path.to_string_lossy()),
                    f.size,
                    f.mtime_ms
                ])?;
            }
        }

        let added: i64 = tx.query_row(
            "SELECT COUNT(*) FROM scan_tmp s LEFT JOIN media m ON m.path = s.path
             WHERE m.id IS NULL",
            [],
            |r| r.get(0),
        )?;
        let changed: i64 = tx.query_row(
            "SELECT COUNT(*) FROM scan_tmp s JOIN media m ON m.path = s.path
             WHERE m.size <> s.size OR m.mtime_ms <> s.mtime_ms",
            [],
            |r| r.get(0),
        )?;

        // 新規・変更だけをupsertする（変更なしの行に触れるとサムネイルが無効化されてしまう）
        tx.execute(
            &format!(
                r#"
                INSERT INTO media (path, size, mtime_ms, day_key, parent_dir, kind)
                SELECT s.path, s.size, s.mtime_ms, {}, {}, pk_kind(s.path)
                FROM scan_tmp s LEFT JOIN media m ON m.path = s.path
                WHERE m.id IS NULL OR m.size <> s.size OR m.mtime_ms <> s.mtime_ms
                ON CONFLICT(path) DO UPDATE SET
                    size = excluded.size,
                    mtime_ms = excluded.mtime_ms,
                    day_key = excluded.day_key,
                    width = NULL,
                    height = NULL,
                    preview_width = NULL,
                    preview_height = NULL,
                    taken_at_ms = NULL,
                    thumb_path = NULL,
                    thumb_state = 0,
                    thumb_bytes = NULL,
                    thumb_used_ms = NULL,
                    camera_id = NULL,
                    duration_ms = NULL
                "#,
                day_key_expr("s.mtime_ms"),
                parent_dir_expr("s.path")
            ),
            [],
        )?;
        Ok((added, changed))
    }

    /// ディレクトリ情報を一時テーブルへ投入する（apply系の共通部）。
    fn stage_dir_tmps(
        tx: &rusqlite::Transaction<'_>,
        seen: &[(PathBuf, i64)],
        enumerated: &[PathBuf],
    ) -> Result<(), DbError> {
        tx.execute_batch(
            r#"
            CREATE TEMP TABLE IF NOT EXISTS dirs_seen_tmp (
                path     TEXT PRIMARY KEY,
                mtime_ms INTEGER NOT NULL
            ) WITHOUT ROWID;
            CREATE TEMP TABLE IF NOT EXISTS dirs_enum_tmp (
                path TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            DELETE FROM dirs_seen_tmp;
            DELETE FROM dirs_enum_tmp;
            "#,
        )?;
        let mut ins = tx.prepare_cached(
            "INSERT OR REPLACE INTO dirs_seen_tmp (path, mtime_ms) VALUES (?1, ?2)",
        )?;
        for (path, mtime_ms) in seen {
            ins.execute(params![normalize_dir(path), mtime_ms])?;
        }
        let mut ins =
            tx.prepare_cached("INSERT OR REPLACE INTO dirs_enum_tmp (path) VALUES (?1)")?;
        for path in enumerated {
            ins.execute(params![normalize_dir(path)])?;
        }
        Ok(())
    }

    /// USNジャーナル等で特定した「ダーティディレクトリ」だけを走査した結果を
    /// 反映する（段階B-1）。[`Db::apply_scan`] との違いは影響範囲の限定:
    ///
    /// - 削除判定は「今回列挙したディレクトリ直下」のレコードに限る
    /// - 列挙したディレクトリの既知の子ディレクトリのうち今回見えなかったものは
    ///   「ディレクトリごと消えた（またはリネームされた）」とみなし、
    ///   配下のレコードとdirs記録をまとめて削除する
    /// - それ以外（今回走査していない場所）のレコード・dirs記録には一切触れない
    ///
    /// 呼び出し側は走査が**1件のエラーもなく**成功した場合のみ呼ぶこと
    /// （このメソッドは走査失敗ルートの保護ルールを持たない）。
    /// 戻り値は (added, changed, removed)。
    pub fn apply_partial_scan(
        &mut self,
        files: &[ScannedFile],
        seen_dirs: &[(PathBuf, i64)],
        enumerated_dirs: &[PathBuf],
    ) -> Result<(usize, usize, usize), DbError> {
        let tx = self.write_tx()?;
        let (added, changed) = Self::stage_scan_tmp(&tx, files)?;
        Self::stage_dir_tmps(&tx, seen_dirs, enumerated_dirs)?;

        // 列挙したディレクトリの既知の子ディレクトリで、今回見えなかったもの
        // = ディレクトリごと消えた。配下をプレフィックスで削除する
        let vanished: Vec<String> = {
            let mut stmt = tx.prepare_cached(&format!(
                "SELECT path FROM dirs
                 WHERE {} IN (SELECT path FROM dirs_enum_tmp)
                 AND NOT EXISTS (SELECT 1 FROM dirs_seen_tmp s WHERE s.path = dirs.path)",
                parent_dir_expr("dirs.path")
            ))?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let mut removed = 0usize;
        for dir in &vanished {
            // 前方一致はバイト厳密に。大小文字だけのリネーム（summer→Summer）では
            // 旧綴りだけが「消えた」扱いになるが、LIKE単独だと新綴りの行まで
            // 巻き込んで削除してしまう（SQLiteのLIKEはASCII大小文字を区別しない）
            let (media_sql, media_params) = binary_prefix_sql("path", dir, 1);
            removed += tx.execute(
                &format!("DELETE FROM media WHERE {media_sql}"),
                rusqlite::params_from_iter(media_params.iter()),
            )?;
            let (dirs_sql, dirs_params) = binary_prefix_sql("path", dir, 2);
            let mut all_params: Vec<String> = vec![dir.clone()];
            all_params.extend(dirs_params);
            tx.execute(
                &format!("DELETE FROM dirs WHERE path = ?1 OR {dirs_sql}"),
                rusqlite::params_from_iter(all_params.iter()),
            )?;
        }

        // 列挙したディレクトリ直下で、スキャンに現れなかったレコードを削除
        removed += tx.execute(
            "DELETE FROM media
             WHERE NOT EXISTS (SELECT 1 FROM scan_tmp s WHERE s.path = media.path)
             AND EXISTS (SELECT 1 FROM dirs_enum_tmp e WHERE e.path = media.parent_dir)",
            [],
        )?;

        // 見えたディレクトリのmtime記録を更新
        tx.execute(
            "INSERT INTO dirs (path, mtime_ms)
             SELECT path, mtime_ms FROM dirs_seen_tmp WHERE true
             ON CONFLICT(path) DO UPDATE SET mtime_ms = excluded.mtime_ms",
            [],
        )?;

        tx.execute_batch(
            "DELETE FROM scan_tmp; DELETE FROM dirs_seen_tmp; DELETE FROM dirs_enum_tmp;",
        )?;
        tx.commit()?;
        Ok((added as usize, changed as usize, removed))
    }

    /// サイズ未記録（thumb_bytes NULL）の高品質サムネイルを補完する（段階B-3）。
    /// 対象は列追加前に生成された既存サムネイル。これを放置すると
    /// キャッシュ使用量に数えられず、LRUの上限管理から漏れてしまう。
    /// 一度に `limit` 件だけ処理し、ジャニターが毎サイクル少しずつ進める。
    /// ファイルが消えているもの（削除途中のクラッシュ等）は未生成状態へ戻して
    /// 自己修復する（可視領域に入ればオンデマンド再生成される）。
    /// 戻り値は処理した件数（0なら補完すべきものなし）。
    pub fn backfill_thumb_sizes(&mut self, limit: usize) -> Result<usize, DbError> {
        let rows: Vec<(i64, Option<String>)> = {
            let mut stmt = self.conn.prepare_cached(
                "SELECT id, thumb_path FROM media
                 WHERE thumb_state = 2 AND thumb_bytes IS NULL LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        if rows.is_empty() {
            return Ok(0);
        }
        let now = now_ms();
        let tx = self.write_tx()?;
        {
            let mut fill = tx.prepare_cached(
                "UPDATE media SET thumb_bytes = ?2,
                        thumb_used_ms = MAX(COALESCE(thumb_used_ms, 0), ?3)
                 WHERE id = ?1",
            )?;
            let mut reset = tx.prepare_cached(
                "UPDATE media SET thumb_path = NULL, thumb_state = 0,
                        thumb_bytes = NULL, thumb_used_ms = NULL
                 WHERE id = ?1",
            )?;
            for (id, path) in &rows {
                let size = path
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len() as i64);
                match size {
                    Some(bytes) => fill.execute(params![id, bytes, now])?,
                    None => reset.execute(params![id])?,
                };
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// 記録済みのディレクトリmtime一覧（枝刈りスキャンの照合元）を読む。
    pub fn load_dirs(&self) -> Result<std::collections::HashMap<PathBuf, i64>, DbError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT path, mtime_ms FROM dirs")?;
        let rows = stmt.query_map([], |r| {
            Ok((PathBuf::from(r.get::<_, String>(0)?), r.get::<_, i64>(1)?))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, mtime) = row?;
            out.insert(path, mtime);
        }
        Ok(out)
    }

    /// 汎用メタデータの読み出し（スキャン設定のフィンガープリント等）。
    pub fn get_meta(&self, key: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    }

    /// 汎用メタデータの書き込み。
    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// メタデータ抽出結果（幅・高さ・撮影日時・カメラ）を書き込む。
    /// 撮影日時が確定したら表示日（day_key）も撮影日時基準で更新する。
    /// カメラ名は `cameras` 表へ正規化し、`camera_id` の更新でFTS索引が張り替わる。
    ///
    /// `width`/`height` は**原本**の寸法、`preview_*` は**掴んだ埋め込みプレビュー**の
    /// 寸法で、原本と同じなら `None`。`None` も毎回書き戻す——プレビューが原寸に
    /// 変わったファイル（RAWを別のソフトで書き出し直した等）で古い値が残らないように
    pub fn update_metadata(
        &mut self,
        id: i64,
        dims: Dimensions,
        taken_at_ms: Option<i64>,
        camera: Option<&str>,
    ) -> Result<(), DbError> {
        let camera_id = match camera.map(str::trim).filter(|c| !c.is_empty()) {
            Some(name) => self.camera_id_of(name)?,
            // 「確認済みだがカメラ情報なし」の印（NULLは未確認を意味する）
            None => CAMERA_NONE,
        };
        self.conn.execute(
            &format!(
                "UPDATE media SET width = ?2, height = ?3, taken_at_ms = ?4,
                        day_key = {}, camera_id = ?5,
                        preview_width = ?6, preview_height = ?7
                 WHERE id = ?1",
                day_key_expr("COALESCE(?4, mtime_ms)")
            ),
            params![
                id,
                dims.width,
                dims.height,
                taken_at_ms,
                camera_id,
                dims.preview.map(|(w, _)| w),
                dims.preview.map(|(_, h)| h)
            ],
        )?;
        Ok(())
    }

    /// OSのプロパティから借りた素性を書き込む（第9部 段階H）。
    ///
    /// [`Self::update_metadata`] との違いが2つある:
    ///
    /// - **`camera_id` に触らない**。Shellはカメラ名を返さないので、ここで
    ///   「確認済みだが情報なし」を立てると、後で実物を読める日が来たときに
    ///   読み直さなくなる（`camera_id` のNULLは「未確認」の意味）
    /// - **読めなかった項目で既存の値を消さない**。Shellは項目ごとに
    ///   返ったり返らなかったりする（クラウドのみのファイルでは長さが返らない）ので、
    ///   0 や `None` は「読めなかった」として素通しする
    pub fn update_shell_metadata(
        &mut self,
        id: i64,
        width: i64,
        height: i64,
        taken_at_ms: Option<i64>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            &format!(
                "UPDATE media SET width = COALESCE(NULLIF(?2, 0), width),
                        height = COALESCE(NULLIF(?3, 0), height),
                        taken_at_ms = COALESCE(?4, taken_at_ms),
                        day_key = {}
                 WHERE id = ?1",
                day_key_expr("COALESCE(?4, taken_at_ms, mtime_ms)")
            ),
            params![id, width, height, taken_at_ms],
        )?;
        Ok(())
    }

    /// 動画の長さを記録する（第9部）。
    ///
    /// 寸法・撮影日時は [`Self::update_metadata`] と共通なので、
    /// ここは長さだけを持つ。画像では呼ばれない。
    ///
    pub fn update_duration(&mut self, id: i64, duration_ms: Option<i64>) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET duration_ms = ?2 WHERE id = ?1",
            params![id, duration_ms],
        )?;
        Ok(())
    }

    /// カメラ名に対応するIDを取得する（なければ作る）。
    fn camera_id_of(&self, name: &str) -> Result<i64, DbError> {
        // まず読むのは、毎回INSERTするとidが飛んで採番が無駄に進むため
        let existing: Option<i64> = self
            .conn
            .prepare_cached("SELECT id FROM cameras WHERE name = ?1")?
            .query_row(params![name], |r| r.get(0))
            .ok();
        if let Some(id) = existing {
            return Ok(id);
        }
        self.conn.execute(
            "INSERT INTO cameras (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![name],
        )?;
        Ok(self.conn.query_row(
            "SELECT id FROM cameras WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?)
    }

    /// サムネイルの段階だけを書き換える（パスは触らない）。
    ///
    /// SVGのように「サムネイルを作らないと決めた」行へ終わった印を付けるために使う。
    /// `thumb_path` を書かないので、原本をサムネイル置き場と誤認して消す事故が起きない。
    pub fn update_thumb_state(&mut self, id: i64, thumb_state: i64) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET thumb_state = ?2 WHERE id = ?1",
            params![id, thumb_state],
        )?;
        Ok(())
    }

    /// サムネイルのパスと品質段階（1=即席, 2=高品質）を書き込む。
    /// 高品質（state=2）はLRU管理対象なのでファイルサイズと利用時刻も記録する
    /// （生成直後をmost-recently-usedにして、直後の削除競合を避ける）。
    pub fn update_thumb_path(
        &mut self,
        id: i64,
        thumb_path: &Path,
        thumb_state: i64,
        thumb_bytes: Option<i64>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET thumb_path = ?2, thumb_state = ?3, thumb_bytes = ?4,
                    thumb_used_ms = CASE WHEN ?4 IS NULL THEN thumb_used_ms ELSE ?5 END
             WHERE id = ?1",
            params![
                id,
                thumb_path.to_string_lossy(),
                thumb_state,
                thumb_bytes,
                now_ms()
            ],
        )?;
        Ok(())
    }

    /// サムネイル利用時刻（LRUのタッチ）をまとめて記録する（段階B-3）。
    /// media://thumb配信時のタッチはメモリに集約し、ジャニターが定期フラッシュする。
    pub fn touch_thumbs(&mut self, touches: &[(i64, i64)]) -> Result<(), DbError> {
        let tx = self.write_tx()?;
        {
            let mut stmt = tx.prepare_cached(
                "UPDATE media SET thumb_used_ms = MAX(COALESCE(thumb_used_ms, 0), ?2)
                 WHERE id = ?1",
            )?;
            for &(id, used_ms) in touches {
                stmt.execute(params![id, used_ms])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 高品質サムネイルの合計ディスク使用量（バイト）。部分インデックスで返る。
    pub fn thumb_cache_usage(&self) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(thumb_bytes), 0) FROM media WHERE thumb_state = 2",
            [],
            |r| r.get(0),
        )?)
    }

    /// 高品質サムネイルのLRU削除（段階B-3）。合計が `cap_bytes` を超えていたら、
    /// 利用時刻の古い順にキャッシュ扱いを解除して（state=0へ戻す）削除対象を返す。
    /// 実ファイルの削除は呼び出し側が行う（DB更新→ファイル削除の順なら、
    /// 途中でクラッシュしても「state=0だがファイルが残っている」で済み、
    /// 再生成時に同名パスへ上書きされるため壊れた状態にならない）。
    ///
    /// - `exclude`: 現在生成キューにあるID（削除と再生成の競合を避ける）
    /// - 生成・利用から1時間以内のものは**なるべく**削除しない（表示中を守る優先則）。
    ///   ただしガード内だけでは上限に収まらない場合は、古い順にガードを破って削除する
    ///   （上限は守る: キャッシュがディスクを食い潰す方が害が大きい）
    /// - 超過時はcapの9割まで解放する（境界での削除・再生成の往復を防ぐ）
    pub fn evict_final_thumbs(
        &mut self,
        cap_bytes: i64,
        exclude: &std::collections::HashSet<i64>,
    ) -> Result<Vec<(i64, PathBuf)>, DbError> {
        if cap_bytes <= 0 {
            return Ok(Vec::new());
        }
        let total = self.thumb_cache_usage()?;
        if total <= cap_bytes {
            return Ok(Vec::new());
        }
        let mut to_free = total - cap_bytes / 10 * 9;
        let recent_guard_ms = now_ms() - 60 * 60 * 1000;

        let tx = self.write_tx()?;
        let mut victims: Vec<(i64, PathBuf)> = Vec::new();
        let mut selected: std::collections::HashSet<i64> = std::collections::HashSet::new();
        // 1パス目はガード付き（直近利用を守る）、それでも足りなければ
        // 2パス目でガードなし（古い順は維持）。連続スクロール中でも上限は守る
        for guard in [Some(recent_guard_ms), None] {
            if to_free <= 0 {
                break;
            }
            let filter = if guard.is_some() {
                "AND COALESCE(thumb_used_ms, 0) < ?1"
            } else {
                "AND ?1 IS NOT NULL" // プレースホルダ数を揃えるだけのダミー条件
            };
            let mut stmt = tx.prepare_cached(&format!(
                "SELECT id, thumb_path, thumb_bytes FROM media
                 WHERE thumb_state = 2 AND thumb_bytes IS NOT NULL {filter}
                 ORDER BY thumb_used_ms ASC"
            ))?;
            let rows = stmt.query_map(params![guard.unwrap_or(recent_guard_ms)], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                if to_free <= 0 {
                    break;
                }
                let (id, path, bytes) = row?;
                if exclude.contains(&id) || !selected.insert(id) {
                    continue;
                }
                if let Some(path) = path {
                    victims.push((id, PathBuf::from(path)));
                    to_free -= bytes;
                }
            }
        }
        {
            let mut stmt = tx.prepare_cached(
                "UPDATE media SET thumb_path = NULL, thumb_state = 0,
                        thumb_bytes = NULL, thumb_used_ms = NULL
                 WHERE id = ?1",
            )?;
            for (id, _) in &victims {
                stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(victims)
    }

    /// 選別の印（⚑ Pick。0.2 ②）を設定する。
    ///
    /// ★（[`Db::set_favorite`]）とは**別の列**。連写から1枚を選ぶ作業の結果が
    /// 「あとで見返したい写真」の棚へ流れ込まないように分けてある
    pub fn set_picked(&mut self, id: i64, picked: bool) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET picked = ?2 WHERE id = ?1",
            params![id, picked as i64],
        )?;
        Ok(())
    }

    /// 選別の印をまとめて付ける・外す（[`Db::set_favorites`] と同じ書き方）。
    pub fn set_pickeds(&mut self, ids: &[i64], picked: bool) -> Result<usize, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tx = self.write_tx()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare_cached("UPDATE media SET picked = ?2 WHERE id = ?1")?;
            for id in ids {
                changed += stmt.execute(params![id, picked as i64])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// お気に入り（★）を設定する。
    pub fn set_favorite(&mut self, id: i64, favorite: bool) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE media SET favorite = ?2 WHERE id = ?1",
            params![id, favorite as i64],
        )?;
        Ok(())
    }

    /// お気に入りをまとめて付ける・外す。
    ///
    /// **1つのトランザクションで書く**。1件ずつ `set_favorite` を呼ぶと、
    /// 数千件でその数だけコミットが走る。途中で落ちたときに半端に付いた状態が
    /// 残るのも避けたい。
    ///
    /// トランザクションは**必ずIMMEDIATE**（`write_tx`）。DEFERREDだと
    /// `SQLITE_BUSY_SNAPSHOT` が `busy_timeout` を無視して即返る。
    pub fn set_favorites(&mut self, ids: &[i64], favorite: bool) -> Result<usize, DbError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tx = self.write_tx()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare_cached("UPDATE media SET favorite = ?2 WHERE id = ?1")?;
            for id in ids {
                changed += stmt.execute(params![id, favorite as i64])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// IDでレコードを1件取得する（カスタムプロトコルの配信元）。
    pub fn get_by_id(&self, id: i64) -> Result<Option<MediaRecord>, DbError> {
        let mut stmt = self
            .conn
            .prepare_cached(&format!("SELECT {MEDIA_COLUMNS} FROM media WHERE id = ?1"))?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::row_to_record(row)?)),
            None => Ok(None),
        }
    }

    /// タイムライン索引: 「日付→枚数＋代表レコード」のサマリを新しい日付順で返す。
    /// 集計は複合インデックスのスキャンで行い、行本体は代表1件分しか読まない。
    /// （MAX()と同時に選択したベア列は最大値の行から取られる: SQLiteの保証仕様）
    pub fn timeline_summary(&self, filter: crate::MediaFilter) -> Result<Vec<DaySummary>, DbError> {
        self.search_summary(&SearchQuery::filtered(filter))
    }

    /// 指定日（YYYYMMDD整数）のレコードを表示順（新しい順）で返す。
    /// 複合インデックスへのシークで、その日の行だけを読む。
    pub fn list_day(
        &self,
        day_key: i64,
        filter: crate::MediaFilter,
    ) -> Result<Vec<MediaRecord>, DbError> {
        self.search_day(day_key, &SearchQuery::filtered(filter))
    }

    /// 検索条件をWHERE句の条件リストとパラメータへ変換する（第4部 段階D）。
    ///
    /// 爆速の原則: どの条件も**インデックスシークに落ちる形**だけを組み立てる。
    /// - 自由語・フォルダ → FTS5索引が返すrowid集合への `IN`
    ///   （`IN (SELECT ...)` は集合として評価されるため、索引側に同一rowidの
    ///   重複エントリがあっても結果は重複しない）
    /// - カメラ → `cameras` 表（数十行）を部分一致で引いて `camera_id IN (...)`
    /// - お気に入り → 部分インデックス、日付 → day_keyの複合インデックス
    ///
    /// 条件が空（＝絞り込みなし）なら空のリストを返し、呼び出し側は
    /// WHERE句そのものを省いて既存のタイムラインと同じ実行計画になる。
    /// プレースホルダは無名の `?` なので、パラメータは条件と同じ順で渡すこと。
    ///
    /// FTSの形は [`FtsMode`] で選ぶ。**既定は一致集合を1回作る形**（日の一覧・
    /// サマリのように、その日ぶんを索引シークで取る問い合わせに向く）。
    fn query_filter(
        &self,
        query: &SearchQuery,
    ) -> Result<(Vec<String>, Vec<rusqlite::types::Value>), DbError> {
        self.query_filter_mode(query, FtsMode::List)
    }

    /// [`Db::query_filter`] の、FTSの形を選べる版。
    fn query_filter_mode(
        &self,
        query: &SearchQuery,
        fts_mode: FtsMode,
    ) -> Result<(Vec<String>, Vec<rusqlite::types::Value>), DbError> {
        use rusqlite::types::Value;
        let fts = fts_mode.cond();
        let mut conds: Vec<String> = Vec::new();
        let mut args: Vec<Value> = Vec::new();

        // 自由語: 「名前・フォルダに一致」または「カメラ名に一致」。
        // カメラに当たらない語（大多数）は純粋なFTSシークのままで、
        // 当たる語のときだけカメラ側のインデックス条件をORで足す
        for (term, matcher) in query.term_matches() {
            let ids = self.camera_ids_like(term)?;
            // 索引語を作れない語（記号・絵文字だけ等）は「常に偽」を土台にする。
            // 条件ごと落とすと絞り込みが消えて全件が返ってしまう
            let seek = match matcher {
                Some(m) => {
                    args.push(Value::Text(m));
                    fts
                }
                None => "0",
            };
            if ids.is_empty() {
                conds.push(seek.to_string());
                continue;
            }
            let holes = placeholders(ids.len());
            conds.push(format!("({seek} OR camera_id IN ({holes}))"));
            args.extend(ids.into_iter().map(Value::Integer));
        }
        for matcher in query.folder_matches() {
            conds.push(fts.to_string());
            args.push(Value::Text(matcher));
        }
        // `camera:` 指定はカメラ名だけに効く（名前やフォルダの一致は拾わない）
        for name in &query.camera {
            let ids = self.camera_ids_like(name)?;
            if ids.is_empty() {
                // 該当する機種がない → 結果は必ず空。常に偽の条件を置く
                conds.push("0".to_string());
                continue;
            }
            conds.push(format!("camera_id IN ({})", placeholders(ids.len())));
            args.extend(ids.into_iter().map(Value::Integer));
        }
        if query.picked_only {
            conds.push("picked = 1".to_string());
        }
        if query.favorites_only {
            conds.push("favorite = 1".to_string());
        }
        // 種類（画像 / RAW / 動画）。`idx_media_kind_day` の先頭列なのでシークに落ちる。
        // **空なら「何にも当たらない」**（`kind:` に知らない値が来たとき。search.rs 参照）
        if let Some(kinds) = &query.kinds {
            if kinds.is_empty() {
                conds.push("0".to_string());
            } else {
                conds.push(format!("kind IN ({})", placeholders(kinds.len())));
                args.extend(kinds.iter().map(|k| Value::Integer(*k as i64)));
            }
        }
        if let Some(from) = query.day_from {
            conds.push("day_key >= ?".to_string());
            args.push(Value::Integer(from));
        }
        if let Some(to) = query.day_to {
            conds.push("day_key <= ?".to_string());
            args.push(Value::Integer(to));
        }
        Ok((conds, args))
    }

    /// カメラ名の部分一致でIDを引く（`cameras` は数十行なのでスキャンで十分速い）。
    fn camera_ids_like(&self, needle: &str) -> Result<Vec<i64>, DbError> {
        let pattern = format!("%{}%", escape_like(needle));
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM cameras WHERE name LIKE ?1 ESCAPE '!'")?;
        let rows = stmt.query_map(params![pattern], |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 検索条件つきのタイムライン索引（第4部 段階D）。
    /// 絞り込みなしなら [`Db::timeline_summary`] と同一のクエリ（＝同一の実行計画）になる。
    pub fn search_summary(&self, query: &SearchQuery) -> Result<Vec<DaySummary>, DbError> {
        let (conds, args) = self.query_filter(query)?;
        let filter = if conds.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conds.join(" AND "))
        };
        let mut stmt = self.conn.prepare_cached(&summary_sql(&filter))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok(DaySummary {
                day_key: r.get(0)?,
                count: r.get(1)?,
                cover_id: r.get(2)?,
                cover_mtime_ms: r.get(3)?,
                cover_thumb_state: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 検索条件つきの日単位取得（第4部 段階D）。
    pub fn search_day(
        &self,
        day_key: i64,
        query: &SearchQuery,
    ) -> Result<Vec<MediaRecord>, DbError> {
        let (mut conds, mut args) = self.query_filter(query)?;
        // 日の指定を先頭に置き、パラメータの順序を合わせる
        conds.insert(0, "day_key = ?".to_string());
        args.insert(0, rusqlite::types::Value::Integer(day_key));
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {MEDIA_COLUMNS} FROM media WHERE {}
             ORDER BY {SORT_TS} DESC, id DESC",
            conds.join(" AND ")
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), Self::row_to_record)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 検索条件に一致する**IDだけ**を、一覧に並ぶ順で返す。
    ///
    /// 全選択（Ctrl+A）のためのもの。一覧は日ごとに遅延読み込みするので、
    /// **まだ読んでいない日の写真も選択の対象になる**。かといってレコードごと
    /// 引くと万単位で無駄になる——IDなら1件8バイトで、3万件でも240KB。
    ///
    /// **範囲選択と、選択の絞り込みにはこれを使わないこと**。
    /// 数枚のために全件をUIへ送ることになる（このモジュールの「UIへ全件を渡さない」
    /// 原則に反する）。[`Db::search_ids_between`] と [`Db::visible_ids`] を使う。
    ///
    /// 並びは `search_day` と同じ（`SORT_TS DESC, id DESC`）。
    pub fn search_ids(&self, query: &SearchQuery) -> Result<Vec<i64>, DbError> {
        let (conds, args) = self.query_filter(query)?;
        let filter = if conds.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conds.join(" AND "))
        };
        // 並びは表示と同じ。**`day_key` を先頭に置く**ので `idx_media_day_sort` が
        // そのまま順序を供給でき、全件の並べ直しが起きない
        let mut stmt = self.conn.prepare_cached(&all_ids_sql(&filter))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 並びの中で、**2つのIDに挟まれた範囲のIDだけ**を返す（Shift+クリック用）。
    ///
    /// [`Db::search_ids`] と違い、返るのは範囲のぶんだけ。隣り合う2枚を選ぶために
    /// 全件を送っていては、1000万件を想定した設計が成り立たない。順番の割り出しは
    /// SQL側でやる（`ROW_NUMBER`）ので、Rustにもメモリへ全件は載らない。
    ///
    /// 端のIDが条件から外れていたら、**見つかったほうだけ**の範囲になる。
    /// 両方とも無ければ空。並びは [`Db::search_ids`] と同じ。
    pub fn search_ids_between(
        &self,
        query: &SearchQuery,
        from_id: i64,
        to_id: i64,
    ) -> Result<Vec<i64>, DbError> {
        self.search_ids_between_mode(query, from_id, to_id, FTS_FOR_SELECTION)
    }

    /// [`Db::search_ids_between`] の、FTSの形を選べる版（速さの計測用）。
    fn search_ids_between_mode(
        &self,
        query: &SearchQuery,
        from_id: i64,
        to_id: i64,
        fts_mode: FtsMode,
    ) -> Result<Vec<i64>, DbError> {
        use rusqlite::types::Value;
        let (conds, args) = self.query_filter_mode(query, fts_mode)?;

        // **端の並びの鍵を先に引く**。順位を数え上げる形（ROW_NUMBER）にすると、
        // 隣り合う2枚のためにライブラリ全体を並べ直すことになる。
        // 条件から外れているIDはここで落ちる
        let mut ends_conds = conds.clone();
        ends_conds.push("id IN (?, ?)".to_string());
        let mut ends_args = args.clone();
        ends_args.push(Value::Integer(from_id));
        ends_args.push(Value::Integer(to_id));
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT day_key, {SORT_TS}, id FROM media WHERE {}",
            ends_conds.join(" AND ")
        ))?;
        let mut keys: Vec<(i64, i64, i64)> = Vec::new();
        let rows = stmt.query_map(rusqlite::params_from_iter(ends_args.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        for row in rows {
            keys.push(row?);
        }
        let (Some(lo), Some(hi)) = (keys.iter().min().copied(), keys.iter().max().copied()) else {
            return Ok(Vec::new());
        };

        // 鍵で挟んで引く。`day_key` を先頭に置くのが要点で、これで
        // `idx_media_day_sort` がシークと並びの両方を賄う（並べ直しが起きない）。
        // `day_key` は表示時刻の単調な関数なので、日をまたぐ並びも表示と一致する
        let mut conds = conds;
        conds.extend(between_conds());
        let mut args = args;
        args.extend([Value::Integer(lo.0), Value::Integer(hi.0)]);
        args.extend([
            Value::Integer(hi.0),
            Value::Integer(hi.1),
            Value::Integer(hi.1),
            Value::Integer(hi.2),
        ]);
        args.extend([
            Value::Integer(lo.0),
            Value::Integer(lo.1),
            Value::Integer(lo.1),
            Value::Integer(lo.2),
        ]);
        let mut stmt = self.conn.prepare_cached(&between_sql(&conds))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 渡されたIDのうち、**いまの条件で実際に並んでいるもの**を、
    /// **一覧と同じ並び**で、その日（`day_key`）と一緒に返す（0.2 ②）。
    ///
    /// ビューアの選択スコープ用。ビューアは位置を `(day_key, id)` で持つので、
    /// 隣へ送るには「次のid」だけでなく**その日**が要る。選択は入れた順の集合で
    /// あって並び順を持たないため、並べ直しもここでやる。
    ///
    /// 並びは [`Db::search_ids`] と同じ（`day_key` → 表示時刻 → id の降順）。
    /// 渡された順を保つ [`Db::visible_ids`] とはそこが違う。
    pub fn visible_ids_in_order(
        &self,
        query: &SearchQuery,
        ids: &[i64],
    ) -> Result<Vec<(i64, i64)>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let (conds, base_args) = self.query_filter_mode(query, FTS_FOR_SELECTION)?;
        // 並べる鍵ごと引く。SQLの変数上限があるので [`Db::visible_ids`] と
        // 同じように分けて問い合わせ、並べ直しはこちらで1回だけやる
        let mut rows_out: Vec<(i64, i64, i64)> = Vec::new();
        for chunk in ids.chunks(400) {
            let mut conds = conds.clone();
            conds.push(format!("id IN ({})", placeholders(chunk.len())));
            let mut args = base_args.clone();
            args.extend(chunk.iter().copied().map(rusqlite::types::Value::Integer));
            let mut stmt = self.conn.prepare_cached(&format!(
                "SELECT day_key, {SORT_TS}, id FROM media WHERE {}",
                conds.join(" AND ")
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
            for row in rows {
                rows_out.push(row?);
            }
        }
        // 同じIDを2度渡されても2度並ばないようにしておく（選択は集合だが、
        // 呼ぶ側の作り方に依存したくない）
        rows_out.sort_unstable_by(|a, b| b.cmp(a));
        rows_out.dedup_by_key(|r| r.2);
        Ok(rows_out.into_iter().map(|(day, _, id)| (id, day)).collect())
    }

    /// 渡されたIDのうち、**いまの条件で実際に並んでいるもの**だけを返す。
    ///
    /// 一括操作の直前に、選択が画面の中身とズレていないか確かめるためのもの。
    /// 全IDを引いて突き合わせると、数枚の確認のために全件を送ることになる。
    /// 返す順番は渡された順。SQLの変数上限があるので分けて問い合わせる。
    pub fn visible_ids(&self, query: &SearchQuery, ids: &[i64]) -> Result<Vec<i64>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.visible_ids_mode(query, ids, FTS_FOR_SELECTION)
    }

    /// [`Db::visible_ids`] の、FTSの形を選べる版（速さの計測用）。
    fn visible_ids_mode(
        &self,
        query: &SearchQuery,
        ids: &[i64],
        fts_mode: FtsMode,
    ) -> Result<Vec<i64>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let (conds, base_args) = self.query_filter_mode(query, fts_mode)?;
        let mut found: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for chunk in ids.chunks(400) {
            let mut conds = conds.clone();
            conds.push(format!("id IN ({})", placeholders(chunk.len())));
            let mut args = base_args.clone();
            args.extend(chunk.iter().copied().map(rusqlite::types::Value::Integer));
            let mut stmt = self.conn.prepare_cached(&format!(
                "SELECT id FROM media WHERE {}",
                conds.join(" AND ")
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
            for row in rows {
                found.insert(row?);
            }
        }
        Ok(ids
            .iter()
            .copied()
            .filter(|id| found.contains(id))
            .collect())
    }

    /// 検索条件に一致する総枚数（検索結果の件数表示用）。
    pub fn search_count(&self, query: &SearchQuery) -> Result<i64, DbError> {
        let (conds, args) = self.query_filter(query)?;
        let filter = if conds.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conds.join(" AND "))
        };
        Ok(self.conn.query_row(
            &format!("SELECT COUNT(*) FROM media {filter}"),
            rusqlite::params_from_iter(args.iter()),
            |r| r.get(0),
        )?)
    }

    /// 検索索引の初期構築を開始し、対象の上限ID（構築の分母）を返す。
    ///
    /// 既存ライブラリ（索引導入前に取り込んだレコード）を後追いで索引化するための
    /// 仕組み。**起動を1msも遅らせない**ため、ここでは範囲を決めるだけにして、
    /// 実際の投入は [`Db::fts_build_step`] をバックグラウンドで少しずつ回す。
    ///
    /// 構築中に新しく追加された行（id > 上限）はトリガが即座に索引へ入れるため、
    /// 掃き寄せの対象外にしてよい（＝二重投入を避けられる）。
    /// 戻り値は (開始カーソル, 上限ID)。開始カーソル >= 上限IDなら構築済み。
    pub fn fts_build_range(&mut self) -> Result<(i64, i64), DbError> {
        let cursor: i64 = self
            .get_meta(FTS_CURSOR_KEY)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let max_id: i64 = match self
            .get_meta(FTS_BUILD_MAX_KEY)?
            .and_then(|v| v.parse().ok())
        {
            Some(v) => v,
            None => {
                // 初回のみ範囲を決めて固定する（以後の起動でも同じ範囲を掃き終える）
                let max: i64 =
                    self.conn
                        .query_row("SELECT COALESCE(MAX(id), 0) FROM media", [], |r| r.get(0))?;
                self.set_meta(FTS_BUILD_MAX_KEY, &max.to_string())?;
                max
            }
        };
        Ok((cursor, max_id))
    }

    /// 検索索引の初期構築を1バッチ進める（[`Db::fts_build_range`] とセットで使う）。
    /// 戻り値は (投入件数, 進んだ後のカーソル)。投入件数0で完了。
    pub fn fts_build_step(&mut self, max_id: i64, batch: usize) -> Result<(usize, i64), DbError> {
        let cursor: i64 = self
            .get_meta(FTS_CURSOR_KEY)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if cursor >= max_id {
            return Ok((0, cursor));
        }
        // **IMMEDIATE で始める**こと。既定のDEFERREDだと最初のSELECTで読み取り
        // スナップショットを取ってから書き込みロックへ昇格するため、その間に
        // 他の接続（サムネイルワーカー・スキャン反映・ジャニター）がコミットすると
        // SQLITE_BUSY_SNAPSHOT が**ビジーハンドラを通らずに即座に**返る
        // （busy_timeout が効かない）。最初から書き込みロックを取れば待機できる
        let tx = self.write_tx()?;
        // 対象範囲の上端を先に決める（LIMITだけだと進んだ位置が分からないため）
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(id), ?2) FROM (
                 SELECT id FROM media WHERE id > ?1 AND id <= ?2 ORDER BY id LIMIT ?3
             )",
            params![cursor, max_id, batch as i64],
            |r| r.get(0),
        )?;
        let inserted = tx.execute(
            "INSERT INTO media_fts(rowid, name, folder)
             SELECT m.id, pk_idx(pk_name(m.path)), pk_idx(pk_folder(m.parent_dir))
             FROM media m WHERE m.id > ?1 AND m.id <= ?2",
            params![cursor, next],
        )?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![FTS_CURSOR_KEY, next.to_string()],
        )?;
        tx.commit()?;
        Ok((inserted, next))
    }

    /// カメラ未確認（`camera_id IS NULL`）でメタデータ抽出済みの行を返す（第4部 段階D）。
    ///
    /// 検索導入前に取り込んだ既存ライブラリを後追いでカメラ対応させるための
    /// スイープ。呼び出し側はEXIFヘッダだけを読んで [`Db::set_cameras`] へ渡す
    /// （画像デコードは不要）。メタデータ未抽出の行は通常のサムネイル処理が
    /// カメラごと書くので、ここでは対象にしない（二重処理を避ける）。
    ///
    /// `after_id` より大きいIDだけを返す**カーソル方式**。読み取れなかった
    /// ファイル（未マウントの外付けドライブ等）は印を付けずに飛ばしたいが、
    /// 印が付かない行を毎回先頭から拾うと同じ200件で無限に足踏みするため。
    /// **呼び出し側は「いま開けないファイル」を落とすこと**。この問い合わせは
    /// DBの列しか見ないので、未マウントのドライブや実体がクラウドにしか無い
    /// ファイルもそのまま返る。EXIFを読むにはファイルを開く必要があり、
    /// クラウドのみのファイルを開けば実体のダウンロードが走る（段階H）。
    pub fn cameras_to_backfill(
        &self,
        after_id: i64,
        limit: usize,
    ) -> Result<Vec<(i64, PathBuf)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path FROM media
             WHERE camera_id IS NULL AND width IS NOT NULL AND id > ?1
             ORDER BY id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after_id, limit as i64], |r| {
            Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// カメラ補完スイープの結果をまとめて書く。
    /// `None`（EXIFにカメラ情報なし）も「確認済み」として記録し、再走査を防ぐ。
    pub fn set_cameras(&mut self, results: &[(i64, Option<String>)]) -> Result<(), DbError> {
        // カメラ名→IDの解決はトランザクションの外で先に済ませる
        // （camera_id_of は自前でINSERTするため、トランザクション中に呼べない）
        let mut ids: Vec<(i64, i64)> = Vec::with_capacity(results.len());
        for (id, camera) in results {
            let camera_id = match camera.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                Some(name) => self.camera_id_of(name)?,
                None => CAMERA_NONE,
            };
            ids.push((*id, camera_id));
        }
        let tx = self.write_tx()?;
        {
            let mut stmt = tx.prepare_cached("UPDATE media SET camera_id = ?2 WHERE id = ?1")?;
            for (id, camera_id) in &ids {
                stmt.execute(params![id, camera_id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 寸法を確かめ直す行を返す（段階F-4の後追い）。
    ///
    /// 段階F-4より前に取り込んだRAWは、`width/height` に**埋め込みプレビューの
    /// 寸法**が入ったままで、原本の寸法（CR3なら `CMT1` の申告）を持っていない。
    /// メタデータ抽出済みなので [`Self::ids_missing_metadata`] からは漏れる。
    ///
    /// **済んだ印は持たない。** 確かめた行は `preview_width` が埋まって条件から
    /// 外れる——RAWは原本と同じ寸法でも書き込むので、**確かめた行が必ず抜ける**。
    /// 印を先に書くと、仕事が終わる前にアプリが落ちたときに拾い直せなくなる。
    ///
    /// 絞り込みの4つは、それぞれ別の穴を塞いでいる:
    ///
    /// - **拡張子**: RAW以外は確かめることが無い（配るのが原本そのものなので、
    ///   確かめても `preview_width` はNULLのまま＝毎回引っ掛かる）
    /// - **`width IS NOT NULL`**: まだ一度も読んでいない行を外す。そちらは
    ///   [`Self::ids_missing_metadata`] が拾って、抽出のついでに寸法も入れる
    /// - **`height IS NOT NULL`**: 幅だけ入っている行を外す。[`Self::update_shell_metadata`]
    ///   は幅と高さを**別々に**書くので、OSが幅しか返さなかった行が作れる。
    ///   高さをNULLのまま返すと呼び出し側の `i64` への読み出しが失敗し、
    ///   掃き寄せが**そこで黙って止まって二度と先へ進まない**（ゲート2のP2）
    /// - **`thumb_path IS NOT NULL`**: 寸法を**自分で測った行だけ**にする。
    ///   クラウドにしか実体が無いファイルは [`Self::update_shell_metadata`] が
    ///   OSから借りた**センサーの寸法**を `width/height` に入れており、それを
    ///   「掴んだプレビューの寸法」として `preview_*` へ写すと嘘になる
    ///   （実際に配るのは 1620x1080 なのに 6000x4000 と名乗る。ゲート2のP2）。
    ///   サムネイルがLRUで消された行もここで外れるが、次に見えたときに
    ///   [`crate::thumbs::process_one`] が両方の列を正しく入れ直す
    pub fn dimensions_to_backfill(
        &self,
        after_id: i64,
        limit: usize,
    ) -> Result<Vec<(i64, PathBuf, i64, i64)>, DbError> {
        // 拡張子はDBに正規化して持っていないので、末尾一致で見る
        // （[`Self::count_by_extensions`] と同じやり方）。`?1` は after_id、
        // `?2` は limit なので、拡張子は `?3` から始まる
        let clause = (0..crate::raw::RAW_EXTENSIONS.len())
            .map(|i| format!("LOWER(path) LIKE ?{}", i + 3))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT id, path, width, height FROM media
             WHERE preview_width IS NULL AND id > ?1
             AND width IS NOT NULL AND height IS NOT NULL AND thumb_path IS NOT NULL
             AND ({clause})
             ORDER BY id LIMIT ?2"
        ))?;
        let mut params: Vec<rusqlite::types::Value> = vec![after_id.into(), (limit as i64).into()];
        params.extend(
            crate::raw::RAW_EXTENSIONS
                .iter()
                .map(|ext| format!("%.{ext}").into()),
        );
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            Ok((
                r.get(0)?,
                PathBuf::from(r.get::<_, String>(1)?),
                r.get(2)?,
                r.get(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 寸法の後追い補完の結果をまとめて書く（段階F-4）。
    /// 実際に寸法が動いた行のIDを返す。
    ///
    /// 触るのは寸法の4列だけ。撮影日時・`day_key`・カメラは既に入っているので、
    /// [`Self::update_metadata`] のように書き直すと**正しい値を上書きしかねない**。
    ///
    /// **読んだときのまま残っている行にだけ書く**（`expected` は
    /// [`Self::dimensions_to_backfill`] が返した width/height）。この掃き寄せは
    /// 起動同期やサムネイル生成と**同時に走る**ので、途中でファイルが差し替わると
    /// スキャンが列を落とし、サムネイル生成が新しい寸法を入れる。素直に `id` だけで
    /// 書くと、古い寸法で上書きしたうえ**確かめた印まで付けて**しまい、
    /// 二度と直らない（ゲート1のP2）。
    pub fn set_dimensions(
        &mut self,
        results: &[(i64, (i64, i64), Dimensions)],
    ) -> Result<Vec<i64>, DbError> {
        let mut moved = Vec::new();
        let tx = self.write_tx()?;
        {
            let mut stmt = tx.prepare_cached(
                "UPDATE media SET width = ?2, height = ?3,
                        preview_width = ?4, preview_height = ?5
                 WHERE id = ?1 AND preview_width IS NULL
                   AND width = ?6 AND height = ?7",
            )?;
            for (id, (expect_w, expect_h), dims) in results {
                let n = stmt.execute(params![
                    id,
                    dims.width,
                    dims.height,
                    dims.preview.map(|(w, _)| w),
                    dims.preview.map(|(_, h)| h),
                    expect_w,
                    expect_h
                ])?;
                // 寸法が動いた行だけ返す（UIへ知らせるのはそれだけでよい）
                if n > 0 && (dims.width, dims.height) != (*expect_w, *expect_h) {
                    moved.push(*id);
                }
            }
        }
        tx.commit()?;
        Ok(moved)
    }

    /// カメラ未確認の残数（補完スイープの進捗表示用）。
    pub fn cameras_pending(&self) -> Result<i64, DbError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM media WHERE camera_id IS NULL AND width IS NOT NULL",
            [],
            |r| r.get(0),
        )?)
    }

    /// カメラ別の枚数（左ペインの「カメラとメディア」用）を多い順で返す。
    /// `camera_id` のインデックスだけで集計できる。
    pub fn list_cameras(&self) -> Result<Vec<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT c.name, COUNT(*) AS n FROM media m JOIN cameras c ON c.id = m.camera_id
             GROUP BY m.camera_id ORDER BY n DESC, c.name",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 「〇年前の今日」の思い出を返す（過去の各年の同じ月日をインデックスシークで探す）。
    /// 戻り値は (何年前か, レコード)。新しい年から並び、最大 `limit` 件。
    pub fn list_memories(&self, limit: usize) -> Result<Vec<(i64, MediaRecord)>, DbError> {
        let (this_year, mmdd): (i64, i64) = self.conn.query_row(
            "SELECT CAST(strftime('%Y', 'now', 'localtime') AS INTEGER),
                    CAST(strftime('%m%d', 'now', 'localtime') AS INTEGER)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let min_year: Option<i64> = self.conn.query_row(
            "SELECT MIN(day_key) / 10000 FROM media WHERE day_key > 0",
            [],
            |r| r.get(0),
        )?;
        let Some(min_year) = min_year else {
            return Ok(Vec::new());
        };

        let mut out = Vec::new();
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {MEDIA_COLUMNS} FROM media WHERE day_key = ?1
             ORDER BY {SORT_TS} DESC, id DESC LIMIT ?2"
        ))?;
        for year in (min_year..this_year).rev() {
            let remaining = limit.saturating_sub(out.len());
            if remaining == 0 {
                break;
            }
            let day_key = year * 10000 + mmdd;
            let rows = stmt.query_map(params![day_key, remaining as i64], Self::row_to_record)?;
            for row in rows {
                out.push((this_year - year, row?));
            }
        }
        Ok(out)
    }

    /// 選別で選んだ（⚑）件数。部分インデックスのスキャンで返る（0.2 ②）。
    pub fn count_picked(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM media WHERE picked = 1", [], |r| {
                r.get(0)
            })?)
    }

    /// お気に入り（★）の総数。部分インデックスのスキャンで返る。
    pub fn count_favorites(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM media WHERE favorite = 1", [], |r| {
                r.get(0)
            })?)
    }

    /// 全レコードを表示順（撮影日時降順、フォールバックはmtime降順）で取得する。
    ///
    /// **注意**: 全件をメモリへ載せるため大規模ライブラリでは使わないこと。
    /// テスト・ベンチマーク（新旧比較）用に残している。
    pub fn list_all(&self) -> Result<Vec<MediaRecord>, DbError> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT {MEDIA_COLUMNS} FROM media ORDER BY {SORT_TS} DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], Self::row_to_record)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// パスで1件のメタデータを引く（ウォッチャーの差分判定用）。
    pub fn get_meta_by_path(&self, path: &Path) -> Result<Option<StoredMeta>, DbError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, size, mtime_ms FROM media WHERE path = ?1")?;
        let mut rows = stmt.query(params![crate::paths::normalize(path).to_string_lossy()])?;
        match rows.next()? {
            Some(row) => Ok(Some(StoredMeta {
                id: row.get(0)?,
                size: row.get(1)?,
                mtime_ms: row.get(2)?,
            })),
            None => Ok(None),
        }
    }

    /// パスそのもの、または配下（ディレクトリ削除時）のレコードをまとめて削除する。
    /// 前方一致はバイト厳密（大小文字だけ違う別綴りを巻き込まない）。
    /// 削除した件数を返す。
    pub fn remove_by_prefix(&mut self, prefix: &Path) -> Result<usize, DbError> {
        let p = crate::paths::normalize_dir_str(prefix);
        let (prefix_sql, prefix_params) = binary_prefix_sql("path", &p, 2);
        let mut all_params: Vec<String> = vec![p.to_string()];
        all_params.extend(prefix_params);
        let n = self.conn.execute(
            &format!("DELETE FROM media WHERE path = ?1 OR {prefix_sql}"),
            rusqlite::params_from_iter(all_params.iter()),
        )?;
        Ok(n)
    }

    /// 消えたファイルのレコードをトランザクションでまとめて削除する。
    pub fn remove_paths(&mut self, paths: &[PathBuf]) -> Result<(), DbError> {
        let tx = self.write_tx()?;
        {
            let mut stmt = tx.prepare_cached("DELETE FROM media WHERE path = ?1")?;
            for p in paths {
                stmt.execute(params![crate::paths::normalize(p).to_string_lossy()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// メタデータ（幅・高さ・撮影日時）未抽出のIDを新しい順で返す。
    ///
    /// 段階B-3: 事前の自動処理はこの「メタデータ抽出＋即席サムネイル」までに絞る。
    /// 高品質サムネイルの生成は可視領域の要求時のみ行う（オンデマンド化）。
    /// LRUで削除されたもの（width有り・state=0）もここには含まれない。
    /// 撮影日時が mtime へ落ちている行（段階H-2の拾い直し用）。
    ///
    /// 撮影日時が読めなかったとき、この列には mtime がそのまま入る。
    /// ファイル名から日時を拾えるようにする前に走査したDBには、
    /// 「同期した日」が撮影日として並んだ行が残っている（実測965件、うち212件は
    /// 名前が別の日を知っていた）。**ここでは直さず、投げ直すだけ**にして、
    /// EXIF → OSのプロパティ → 名前 → mtime の順で決め直させる
    /// （EXIFが読めるならそちらが勝つので、名前で上書きすることはない）。
    ///
    /// 撮影日時が**NULLの行も拾う**。段階Hで寸法だけ埋まった行
    /// （OSが寸法は返したが日付は返さなかった）はここに落ちるが、
    /// `width` は入っているので [`Self::ids_missing_metadata`] からも漏れる。
    /// 両方の網から落ちて mtime で並び続けるのを防ぐ。
    ///
    /// パスとmtimeも返すのは、呼び出し側が**投げ直す意味のある行だけ**に
    /// 絞れるようにするため。「済んだ印」を持たずに済ませたい——印を先に書くと、
    /// 投げた仕事が終わる前にアプリが落ちたときに二度と拾い直せなくなる。
    /// 代わりに「名前から拾える日時が mtime と違う行」だけを投げれば、
    /// 直った行は条件から外れ、直らない行はファイルを開かない文字列判定で弾ける。
    pub fn rows_with_fallback_taken_at(&self) -> Result<Vec<FallbackDateRow>, DbError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path, mtime_ms FROM media
             WHERE taken_at_ms IS NULL OR taken_at_ms = mtime_ms
             ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, PathBuf::from(r.get::<_, String>(1)?), r.get(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// `width = 0` も対象に含める（第9部 段階H）。0は「読もうとして読めなかった」印で、
    /// コンテナを自前で解釈できない動画（`.m2ts` / `.avi`）がここに落ちる。OSのプロパティを
    /// 聞く道ができた後は取れるようになるので、既に走査済みのDBでも拾い直せるようにする。
    /// どうしても取れないファイルは毎回聞き直すことになるが、1件あたり10ms程度で、
    /// そもそも「自前でもOSでも読めない動画」はほとんど無い（実測のライブラリで1件）。
    pub fn ids_missing_metadata(&self) -> Result<Vec<i64>, DbError> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT id FROM media WHERE width IS NULL OR width = 0
             ORDER BY thumb_state ASC, {SORT_TS} DESC"
        ))?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 拡張子で数え、見本のパスも少しだけ返す。
    ///
    /// 「この環境ではHEIFを展開できない」と知らせるかどうかの判断に使う
    /// （ライブラリに1枚も無いなら黙っているべきなので、まず数を見る）。
    /// 見本を複数返すのは、クラウドにしか実体が無いものを避けて選ぶため。
    pub fn count_by_extensions(
        &self,
        extensions: &[&str],
        samples: usize,
    ) -> Result<(i64, Vec<PathBuf>), DbError> {
        if extensions.is_empty() {
            return Ok((0, Vec::new()));
        }
        // 拡張子はDBに正規化して持っていないので、末尾一致で見る。
        // 件数は最大でもライブラリ全体の走査1回で済む
        let clause = extensions
            .iter()
            .map(|_| "LOWER(path) LIKE ?")
            .collect::<Vec<_>>()
            .join(" OR ");
        let patterns: Vec<String> = extensions.iter().map(|e| format!("%.{e}")).collect();
        let params = rusqlite::params_from_iter(patterns.iter());
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM media WHERE {clause}"),
            params,
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT path FROM media WHERE {clause} LIMIT {samples}"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(patterns.iter()), |r| {
            r.get::<_, String>(0)
        })?;
        Ok((total, rows.flatten().map(PathBuf::from).collect()))
    }

    /// レコード総数。
    pub fn count(&self) -> Result<i64, DbError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))?)
    }

    fn row_to_record(row: &rusqlite::Row<'_>) -> Result<MediaRecord, rusqlite::Error> {
        Ok(MediaRecord {
            id: row.get(0)?,
            path: PathBuf::from(row.get::<_, String>(1)?),
            size: row.get(2)?,
            mtime_ms: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            taken_at_ms: row.get(6)?,
            day_key: row.get(7)?,
            thumb_path: row.get::<_, Option<String>>(8)?.map(PathBuf::from),
            thumb_state: row.get(9)?,
            favorite: row.get::<_, i64>(10)? != 0,
            picked: row.get::<_, i64>(11)? != 0,
            duration_ms: row.get(12)?,
            preview_width: row.get(13)?,
            preview_height: row.get(14)?,
        })
    }
}

/// 現在時刻のUnixエポックミリ秒（サムネイルLRUの利用時刻用）。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// LIKEパターン中のワイルドカード（% _）とエスケープ文字（!）を無効化する
/// （IMG_0001 等の `_` を含むパス対策）。
fn escape_like(s: &str) -> String {
    s.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

/// 「`column` が `prefix` 配下（区切りは \\ か /）」を**バイト厳密**に判定する
/// SQL条件とパラメータを組む。プレースホルダは ?N から4つ使う。
///
/// SQLiteのLIKEはASCII大文字小文字を区別しないため、LIKE単独の前方一致は
/// `summer` と `Summer` を同一視してしまう。大小文字だけのフォルダ名リネームで
/// 「消えた旧綴りの削除」が新綴りの行まで巻き込む事故を防ぐため、
/// LIKE（インデックスでの絞り込み用）に substr のバイト一致を重ねる。
fn binary_prefix_sql(column: &str, prefix: &str, first_param: usize) -> (String, Vec<String>) {
    let escaped = escape_like(prefix);
    let (a, b, c, d) = (
        first_param,
        first_param + 1,
        first_param + 2,
        first_param + 3,
    );
    let sql = format!(
        "(({column} LIKE ?{a} ESCAPE '!' OR {column} LIKE ?{b} ESCAPE '!')
          AND (substr({column}, 1, length(?{c})) = ?{c}
               OR substr({column}, 1, length(?{d})) = ?{d}))"
    );
    let params = vec![
        format!("{escaped}\\%"),
        format!("{escaped}/%"),
        format!("{prefix}\\"),
        format!("{prefix}/"),
    ];
    (sql, params)
}

/// 読み取り専用接続のプール（plan.md 第3部 段階B-4）。
///
/// media://配信とタイムライン系クエリを複数のリードオンリー接続へ分散し、
/// 書き込み（スキャン反映・サムネイル更新）中もUIの読み取りを止めない。
/// WALモードではリーダーはライターと並行に動ける。
pub struct ReadPool {
    dbs: Vec<Mutex<Db>>,
    next: AtomicUsize,
}

impl ReadPool {
    /// リードオンリー接続を `size` 本開く。DBファイルは既に存在していること。
    pub fn open(path: &Path, size: usize) -> Result<Self, DbError> {
        let size = size.max(1);
        let mut dbs = Vec::with_capacity(size);
        for _ in 0..size {
            dbs.push(Mutex::new(Db::open_read_only(path)?));
        }
        Ok(Self {
            dbs,
            next: AtomicUsize::new(0),
        })
    }

    /// 空いている接続でクロージャを実行する。
    /// ラウンドロビン起点からtry_lockで空き接続を探し、全接続が使用中なら
    /// 起点の接続の解放を待つ（読み取りは短時間なので待ちも短い）。
    pub fn with<T>(&self, f: impl FnOnce(&Db) -> T) -> T {
        use std::sync::TryLockError;
        let n = self.dbs.len();
        let start = self.next.fetch_add(1, Ordering::Relaxed);
        for i in 0..n {
            match self.dbs[(start + i) % n].try_lock() {
                Ok(db) => return f(&db),
                Err(TryLockError::Poisoned(p)) => return f(&p.into_inner()),
                Err(TryLockError::WouldBlock) => continue,
            }
        }
        let db = self.dbs[start % n]
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        f(&db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanned(path: &str, size: i64, mtime_ms: i64) -> ScannedFile {
        ScannedFile {
            path: PathBuf::from(path),
            size,
            mtime_ms,
        }
    }

    #[test]
    fn 寸法の後追いはrawの未確認だけを拾う() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("a.cr3", 100, 1000),
            scanned("b.jpg", 100, 1000),
            scanned("c.NEF", 100, 1000), // 拡張子は大文字でも拾う
            scanned("d.cr2", 100, 1000), // まだ一度も読んでいない（width が空）
        ])
        .unwrap();
        let id_of = |db: &Db, name: &str| {
            db.list_all()
                .unwrap()
                .into_iter()
                .find(|r| r.path.to_string_lossy().ends_with(name))
                .unwrap()
                .id
        };
        for name in ["a.cr3", "b.jpg", "c.NEF"] {
            let id = id_of(&db, name);
            db.update_metadata(id, Dimensions::original(640, 480), Some(1), None)
                .unwrap();
            // 寸法を**自分で測った**印。これが無い行はOSから借りた可能性がある
            db.update_thumb_path(id, Path::new("t/x.webp"), 2, Some(1))
                .unwrap();
        }

        let picked: Vec<String> = db
            .dimensions_to_backfill(0, 100)
            .unwrap()
            .into_iter()
            .map(|(_, path, ..)| path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            picked.len(),
            2,
            "RAWで、読んだのに preview_width が空の行だけ: {picked:?}"
        );
        assert!(picked.iter().any(|p| p.ends_with("a.cr3")));
        assert!(picked.iter().any(|p| p.ends_with("c.NEF")));

        // 確かめた行は次から抜ける（済んだ印を別に持たなくて済む）
        let a = id_of(&db, "a.cr3");
        assert_eq!(
            db.set_dimensions(&[(
                a,
                (640, 480),
                Dimensions {
                    width: 6000,
                    height: 4000,
                    preview: Some((640, 480)),
                },
            )])
            .unwrap(),
            vec![a],
            "寸法が動いた行だけ返る"
        );
        let rec = db.get_by_id(a).unwrap().unwrap();
        assert_eq!((rec.width, rec.height), (Some(6000), Some(4000)));
        assert_eq!(
            (rec.preview_width, rec.preview_height),
            (Some(640), Some(480))
        );
        assert_eq!(db.dimensions_to_backfill(0, 100).unwrap().len(), 1);
    }

    #[test]
    fn 後追いは自分で測っていない行を拾わない() {
        // クラウドのみのファイルはOSから**センサーの寸法**を借りて width に入れる
        // （サムネイルは作れないので thumb_path は空のまま）。これを「掴んだ
        // プレビューの寸法」として写すと、6000x4000 を配ると名乗ることになる
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("cloud.cr3", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_shell_metadata(id, 6000, 4000, Some(1)).unwrap();
        assert!(db.dimensions_to_backfill(0, 100).unwrap().is_empty());

        // 実体が落ちてきて process_one が通ると、そのときは両方の列が正しく入る
        // ——つまりこの行が後追いの対象になることは一度も無い
        db.update_metadata(
            id,
            Dimensions {
                width: 6000,
                height: 4000,
                preview: Some((1620, 1080)),
            },
            Some(1),
            None,
        )
        .unwrap();
        db.update_thumb_path(id, Path::new("t/x.webp"), 2, Some(1))
            .unwrap();
        assert!(db.dimensions_to_backfill(0, 100).unwrap().is_empty());
    }

    #[test]
    fn 後追いは高さが空の行を拾わない() {
        // 拾うと呼び出し側の i64 への読み出しが失敗し、掃き寄せが黙って止まる
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.cr3", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_thumb_path(id, Path::new("t/x.webp"), 2, Some(1))
            .unwrap();
        // OSが幅しか返さなかった行（update_shell_metadata は別々に書く）
        db.update_shell_metadata(id, 6000, 0, Some(1)).unwrap();
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!((rec.width, rec.height), (Some(6000), None));
        assert!(db.dimensions_to_backfill(0, 100).unwrap().is_empty());
    }

    #[test]
    fn 後追いは読んだときから動いた行に書かない() {
        // 掃き寄せの途中でスキャンが列を落とし、サムネイル生成が新しい寸法を
        // 入れた行。古い値で上書きすると、確かめた印まで付いて二度と直らない
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.cr3", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_metadata(id, Dimensions::original(640, 480), Some(1), None)
            .unwrap();
        db.update_thumb_path(id, Path::new("t/x.webp"), 2, Some(1))
            .unwrap();
        assert_eq!(db.dimensions_to_backfill(0, 100).unwrap().len(), 1);

        // ここでファイルが差し替わり、新しい寸法が入った
        db.upsert_files(&[scanned("a.cr3", 200, 2000)]).unwrap();
        db.update_metadata(
            id,
            Dimensions {
                width: 100,
                height: 200,
                preview: Some((100, 200)),
            },
            Some(2),
            None,
        )
        .unwrap();

        let moved = db
            .set_dimensions(&[(
                id,
                (640, 480),
                Dimensions {
                    width: 6000,
                    height: 4000,
                    preview: Some((640, 480)),
                },
            )])
            .unwrap();
        assert!(moved.is_empty(), "書き込みは丸ごと落ちる");
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!((rec.width, rec.height), (Some(100), Some(200)));
        assert_eq!(
            (rec.preview_width, rec.preview_height),
            (Some(100), Some(200))
        );
    }

    #[test]
    fn 中身が変わった行はプレビューの寸法も落とす() {
        // 落とさないと、前の中身の寸法で下敷きと先読みの予算が決まる
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.cr3", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_metadata(
            id,
            Dimensions {
                width: 6000,
                height: 4000,
                preview: Some((1620, 1080)),
            },
            Some(1),
            None,
        )
        .unwrap();

        db.upsert_files(&[scanned("a.cr3", 200, 2000)]).unwrap();
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!((rec.width, rec.height), (None, None));
        assert_eq!((rec.preview_width, rec.preview_height), (None, None));
    }

    /// サムネイル利用時刻を任意の過去へ設定する（LRUテスト用。
    /// touch_thumbsはMAXで単調増加のみのため直接更新する）。
    fn set_thumb_used(db: &Db, id: i64, used_ms: i64) {
        db.conn
            .execute(
                "UPDATE media SET thumb_used_ms = ?2 WHERE id = ?1",
                params![id, used_ms],
            )
            .unwrap();
    }

    #[test]
    fn upsertで新規追加できる() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 100, 1000), scanned("b.jpg", 200, 2000)])
            .unwrap();
        assert_eq!(db.count().unwrap(), 2);
    }

    #[test]
    fn upsertで既存レコードが更新されメタデータが無効化される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_metadata(id, Dimensions::original(640, 480), Some(123), None)
            .unwrap();
        db.update_thumb_path(id, Path::new("thumb/a.webp"), 2, Some(1000))
            .unwrap();
        assert_eq!(db.get_by_id(id).unwrap().unwrap().thumb_state, 2);

        // サイズ・mtimeが変わった（＝ファイル更新）→ メタデータは再抽出が必要
        db.upsert_files(&[scanned("a.jpg", 150, 3000)]).unwrap();
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.size, 150);
        assert_eq!(rec.mtime_ms, 3000);
        assert_eq!(rec.width, None);
        assert_eq!(rec.taken_at_ms, None);
        assert_eq!(rec.thumb_path, None);
        assert_eq!(rec.thumb_state, 0);
        assert_eq!(db.count().unwrap(), 1);
    }

    /// 動画の長さもファイル更新で無効化される（第9部）。
    ///
    /// 消し忘れると、差し替えられた動画に**前のファイルの長さ**が
    /// 残り続ける（クラウドのみで保留されたままなら永久に）
    #[test]
    fn 動画の長さもファイル更新で無効化される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.mp4", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_duration(id, Some(12_000)).unwrap();
        assert_eq!(db.get_by_id(id).unwrap().unwrap().duration_ms, Some(12_000));

        db.upsert_files(&[scanned("a.mp4", 200, 2000)]).unwrap();
        assert_eq!(
            db.get_by_id(id).unwrap().unwrap().duration_ms,
            None,
            "差し替え後に前の長さが残っている"
        );
    }

    #[test]
    fn お気に入りはファイル更新でも維持される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 100, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        assert!(!db.get_by_id(id).unwrap().unwrap().favorite);

        db.set_favorite(id, true).unwrap();
        assert!(db.get_by_id(id).unwrap().unwrap().favorite);
        assert_eq!(db.count_favorites().unwrap(), 1);

        // ファイル更新（サイズ変化）でもお気に入りは消えない
        db.upsert_files(&[scanned("a.jpg", 200, 2000)]).unwrap();
        assert!(db.get_by_id(id).unwrap().unwrap().favorite);

        db.set_favorite(id, false).unwrap();
        assert!(!db.get_by_id(id).unwrap().unwrap().favorite);
        assert_eq!(db.count_favorites().unwrap(), 0);
    }

    #[test]
    fn remove_pathsで削除できる() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 100, 1000), scanned("b.jpg", 200, 2000)])
            .unwrap();
        db.remove_paths(&[PathBuf::from("a.jpg")]).unwrap();
        assert_eq!(db.count().unwrap(), 1);
        assert_eq!(db.list_all().unwrap()[0].path, PathBuf::from("b.jpg"));
    }

    #[test]
    fn list_allは撮影日時降順でmtimeフォールバックする() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("old.jpg", 1, 1000),
            scanned("new.jpg", 1, 9000),
            scanned("exif.jpg", 1, 500),
        ])
        .unwrap();
        // exif.jpg だけ撮影日時あり（mtimeより新しい）
        let exif_id = db
            .list_all()
            .unwrap()
            .iter()
            .find(|r| r.path == Path::new("exif.jpg"))
            .unwrap()
            .id;
        db.update_metadata(exif_id, Dimensions::original(640, 480), Some(10000), None)
            .unwrap();

        let names: Vec<_> = db
            .list_all()
            .unwrap()
            .into_iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["exif.jpg", "new.jpg", "old.jpg"]);
    }

    #[test]
    fn 拡張子で数えて見本も返す() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("a.HEIC", 1, 0),
            scanned("b.heic", 2, 0),
            scanned("c.hif", 3, 0),
            scanned("d.jpg", 4, 0),
        ])
        .unwrap();
        // 大文字の拡張子も同じ形式として数える（実ファイルは大文字が普通にある）
        let (total, samples) = db.count_by_extensions(&["heic", "heif", "hif"], 8).unwrap();
        assert_eq!(total, 3);
        assert_eq!(samples.len(), 3);

        // 「この形式は1枚も無い」を取り違えると、要らない案内が出てしまう
        let (none, _) = db.count_by_extensions(&["avif"], 8).unwrap();
        assert_eq!(none, 0);

        // 見本の数は上限で頭打ちにする（全件は要らない）
        let (_, capped) = db.count_by_extensions(&["heic"], 1).unwrap();
        assert_eq!(capped.len(), 1);

        // 拡張子を1つも渡さないときに全件が当たらないこと
        assert_eq!(db.count_by_extensions(&[], 8).unwrap().0, 0);
    }

    #[test]
    fn get_by_idは存在しなければnone() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.get_by_id(999).unwrap().is_none());
    }

    /// ローカル日付 y-m-d のエポックミリ秒（テストデータ用、正午で安全側）
    fn local_noon_ms(y: i32, m: u32, d: u32) -> i64 {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(y, m, d, 12, 0, 0)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn day_keyは書き込み時に維持される() {
        let mut db = Db::open_in_memory().unwrap();
        let ms = local_noon_ms(2024, 8, 11);
        db.upsert_files(&[scanned("a.jpg", 1, ms)]).unwrap();
        let rec = &db.list_all().unwrap()[0];
        assert_eq!(rec.day_key, 20240811, "mtime基準のday_key");

        // 撮影日時が確定したらday_keyは撮影日基準へ
        let taken = local_noon_ms(2020, 1, 5);
        db.update_metadata(rec.id, Dimensions::original(640, 480), Some(taken), None)
            .unwrap();
        assert_eq!(db.get_by_id(rec.id).unwrap().unwrap().day_key, 20200105);
    }

    #[test]
    fn タイムラインサマリと日単位取得() {
        let mut db = Db::open_in_memory().unwrap();
        let d1 = local_noon_ms(2024, 8, 11);
        let d2 = local_noon_ms(2024, 8, 12);
        db.upsert_files(&[
            scanned("a.jpg", 1, d1),
            scanned("b.jpg", 1, d1 + 1000),
            scanned("c.jpg", 1, d2),
        ])
        .unwrap();

        let summary = db.timeline_summary(crate::MediaFilter::All).unwrap();
        let keys: Vec<_> = summary.iter().map(|d| (d.day_key, d.count)).collect();
        assert_eq!(
            keys,
            vec![(20240812, 1), (20240811, 2)],
            "新しい日付順のサマリ"
        );
        // 代表（カバー）はその日の最新レコード
        let b_id = db.get_meta_by_path(Path::new("b.jpg")).unwrap().unwrap().id;
        assert_eq!(summary[1].cover_id, b_id, "日内最新がカバーになる");

        let day = db.list_day(20240811, crate::MediaFilter::All).unwrap();
        let names: Vec<_> = db
            .list_day(20240811, crate::MediaFilter::All)
            .unwrap()
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["b.jpg", "a.jpg"], "日内は新しい順");
        assert_eq!(day.len(), 2);
        assert!(
            db.list_day(20240812, crate::MediaFilter::All)
                .unwrap()
                .len()
                == 1
        );
        assert!(db
            .list_day(19990101, crate::MediaFilter::All)
            .unwrap()
            .is_empty());
    }

    /// 種類（画像 / RAW / 動画）で絞れる。判定は拡張子だけで、
    /// 追加のときに `kind` 列へ入る（既存の行は起動時の移行で埋まる）。
    #[test]
    fn 種類での絞り込み() {
        use crate::MediaKind;
        let mut db = Db::open_in_memory().unwrap();
        let d1 = local_noon_ms(2024, 8, 11);
        db.upsert_files(&[
            scanned("a.jpg", 1, d1),
            scanned("b.CR2", 1, d1 + 1000),
            scanned("c.MOV", 1, d1 + 2000),
            scanned("d.heic", 1, d1 + 3000),
        ])
        .unwrap();

        let names = |kinds: Option<Vec<MediaKind>>| -> Vec<String> {
            let query = SearchQuery {
                kinds,
                ..Default::default()
            };
            db.search_day(20240811, &query)
                .unwrap()
                .iter()
                .map(|r| r.path.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(
            names(Some(vec![MediaKind::Raw])),
            vec!["b.CR2"],
            "大文字の綴りも拾う"
        );
        assert_eq!(names(Some(vec![MediaKind::Video])), vec!["c.MOV"]);
        assert_eq!(
            names(Some(vec![MediaKind::Photo])),
            vec!["d.heic", "a.jpg"],
            "RAWでも動画でもなければ画像"
        );
        assert!(
            names(Some(vec![])).is_empty(),
            "空の指定（kind:に知らない値）は何にも当たらない"
        );
        assert_eq!(names(None).len(), 4, "指定なしは絞らない");

        // サマリの枚数も種類で絞られる（一覧と骨組みがずれない）
        let summary = db
            .search_summary(&SearchQuery {
                kinds: Some(vec![MediaKind::Photo]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].count, 2);
    }

    #[test]
    fn お気に入りフィルタ付きのサマリと日単位取得() {
        let mut db = Db::open_in_memory().unwrap();
        let d1 = local_noon_ms(2024, 8, 11);
        db.upsert_files(&[scanned("a.jpg", 1, d1), scanned("b.jpg", 1, d1 + 1000)])
            .unwrap();
        let id = db
            .list_day(20240811, crate::MediaFilter::All)
            .unwrap()
            .iter()
            .find(|r| r.path == Path::new("a.jpg"))
            .unwrap()
            .id;
        db.set_favorite(id, true).unwrap();

        let summary = db.timeline_summary(crate::MediaFilter::Fav).unwrap();
        assert_eq!(summary.len(), 1);
        assert_eq!((summary[0].day_key, summary[0].count), (20240811, 1));
        assert_eq!(summary[0].cover_id, id, "★のみの場合のカバーも★の行");
        let day = db.list_day(20240811, crate::MediaFilter::Fav).unwrap();
        assert_eq!(day.len(), 1);
        assert_eq!(day[0].path, PathBuf::from("a.jpg"));
    }

    #[test]
    fn 思い出は過去の同じ月日から新しい年順で返る() {
        use chrono::Datelike;
        let mut db = Db::open_in_memory().unwrap();
        let today = chrono::Local::now();
        let (y, m, d) = (today.year(), today.month(), today.day());
        // うるう日生まれのテスト不安定を避ける（2/29なら実行しても意味のある日付にならない）
        if m == 2 && d == 29 {
            return;
        }
        db.upsert_files(&[
            scanned("one_year_ago.jpg", 1, local_noon_ms(y - 1, m, d)),
            scanned("three_years_ago.jpg", 1, local_noon_ms(y - 3, m, d)),
            scanned("today.jpg", 1, local_noon_ms(y, m, d)),
            scanned(
                "other_day.jpg",
                1,
                local_noon_ms(y - 1, m, if d > 15 { d - 1 } else { d + 1 }),
            ),
        ])
        .unwrap();

        let memories = db.list_memories(24).unwrap();
        let got: Vec<_> = memories
            .iter()
            .map(|(years, r)| (*years, r.path.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            got,
            vec![
                (1, "one_year_ago.jpg".to_string()),
                (3, "three_years_ago.jpg".to_string()),
            ],
            "今日・別の日は含まれず、新しい年から並ぶ"
        );

        // limitで打ち切られる
        assert_eq!(db.list_memories(1).unwrap().len(), 1);
    }

    /// 綴り違いで2件入ってしまった行を、開き直したときに1件へ統合する。
    ///
    /// 通常スキャン（設定ルートの綴り）とUSNジャーナル（全部バックスラッシュ）で
    /// 同じファイルが別行になっていた。実環境で32件の重複が出た。
    #[cfg(windows)]
    #[test]
    fn 綴り違いで重複した行は開き直しで統合される() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("dup.db");

        let kept_thumb = dir.path().join("kept.webp");
        let dropped_thumb = dir.path().join("dropped.webp");
        std::fs::write(&dropped_thumb, b"x").unwrap();

        {
            let mut db = Db::open(&db_path).unwrap();
            db.upsert_files(&[
                ScannedFile {
                    path: PathBuf::from("C:\\lib\\a.jpg"),
                    size: 1,
                    mtime_ms: 10,
                },
                ScannedFile {
                    path: PathBuf::from("C:\\lib\\b.jpg"),
                    size: 2,
                    mtime_ms: 20,
                },
            ])
            .unwrap();
            let ids: Vec<(i64, String)> = db
                .list_all()
                .unwrap()
                .into_iter()
                .map(|r| (r.id, r.path.to_string_lossy().into_owned()))
                .collect();
            // a.jpg 側に高品質サムネイル、b.jpg 側には即席サムネイルを持たせる
            for (id, path) in &ids {
                if path.ends_with("a.jpg") {
                    db.update_thumb_path(*id, &kept_thumb, 2, Some(100))
                        .unwrap();
                } else {
                    db.update_thumb_path(*id, &dropped_thumb, 1, None).unwrap();
                }
            }
        }

        // b.jpg を「a.jpg の別綴り」へ書き換える（UPDATEにはFTSトリガが無い）。
        // あわせて移行済みの印を消し、旧バージョンのDBを再現する
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute(
                "UPDATE media SET path = ?1 WHERE path LIKE '%b.jpg'",
                params!["C:/lib\\a.jpg"],
            )
            .unwrap();
            conn.execute("DELETE FROM meta WHERE key = ?1", params![PATH_SCHEMA_KEY])
                .unwrap();
        }

        // 開き直すと移行が走る
        let db = Db::open(&db_path).unwrap();
        let all = db.list_all().unwrap();
        assert_eq!(all.len(), 1, "同じファイルは1件に統合される");
        assert_eq!(
            all[0].path.to_string_lossy(),
            "C:\\lib\\a.jpg",
            "綴りは揃えた形になる"
        );
        assert_eq!(all[0].thumb_state, 2, "進んでいる方の行を残す");
        assert!(
            !dropped_thumb.exists(),
            "消した行のサムネイルファイルも片付ける"
        );
    }

    /// 統合で消える行に付いていた「お気に入り」と撮影情報を残す行へ引き継ぐ。
    ///
    /// ユーザーが自分で付けた情報を移行で失わせない（レビュー指摘）。
    #[cfg(windows)]
    #[test]
    fn 統合でお気に入りと撮影情報を引き継ぐ() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("merge.db");
        let thumb = dir.path().join("t.webp");

        {
            let mut db = Db::open(&db_path).unwrap();
            db.upsert_files(&[
                ScannedFile {
                    path: PathBuf::from("C:\\lib\\a.jpg"),
                    size: 1,
                    mtime_ms: 10,
                },
                ScannedFile {
                    path: PathBuf::from("C:\\lib\\b.jpg"),
                    size: 2,
                    mtime_ms: 20,
                },
            ])
            .unwrap();
            for r in db.list_all().unwrap() {
                let name = r.path.to_string_lossy().into_owned();
                if name.ends_with("a.jpg") {
                    // 残る側: サムネイルは進んでいるが、寸法も撮影日時も無い
                    db.update_thumb_path(r.id, &thumb, 2, Some(10)).unwrap();
                } else {
                    // 消える側: ユーザーが★を付け、撮影情報も埋まっている
                    db.set_favorite(r.id, true).unwrap();
                    db.update_metadata(
                        r.id,
                        Dimensions::original(640, 480),
                        Some(1_700_000_000_000),
                        Some("Camera X"),
                    )
                    .unwrap();
                }
            }
        }

        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute(
                "UPDATE media SET path = ?1 WHERE path LIKE '%b.jpg'",
                params!["C:/lib\\a.jpg"],
            )
            .unwrap();
            conn.execute("DELETE FROM meta WHERE key = ?1", params![PATH_SCHEMA_KEY])
                .unwrap();
        }

        let db = Db::open(&db_path).unwrap();
        let all = db.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].favorite, "★は消える側に付いていても残る");
        assert_eq!(all[0].width, Some(640), "寸法を引き継ぐ");
        assert_eq!(
            all[0].taken_at_ms,
            Some(1_700_000_000_000),
            "撮影日時を引き継ぐ"
        );
    }

    #[test]
    fn apply_scanの差分検知_新規_変更_削除() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("root");
        db.upsert_files(&[
            scanned("root/unchanged.jpg", 10, 100),
            scanned("root/changed.jpg", 20, 200),
            scanned("root/deleted.jpg", 30, 300),
        ])
        .unwrap();
        let unchanged_id = db
            .get_meta_by_path(Path::new("root/unchanged.jpg"))
            .unwrap()
            .unwrap()
            .id;
        db.update_metadata(
            unchanged_id,
            Dimensions::original(640, 480),
            Some(100),
            None,
        )
        .unwrap();
        db.update_thumb_path(unchanged_id, Path::new("t/1.webp"), 2, Some(1000))
            .unwrap();

        let files = vec![
            scanned("root/unchanged.jpg", 10, 100),
            scanned("root/changed.jpg", 20, 999), // mtimeだけ変化
            scanned("root/new.jpg", 40, 400),
        ];
        let (added, changed, removed) = db
            .apply_scan(
                &files,
                std::slice::from_ref(&root),
                std::slice::from_ref(&root),
                None,
            )
            .unwrap();
        assert_eq!((added, changed, removed), (1, 1, 1));
        assert_eq!(db.count().unwrap(), 3);
        assert!(db
            .get_meta_by_path(Path::new("root/deleted.jpg"))
            .unwrap()
            .is_none());

        // 変更なしの行はサムネイルが維持される
        let rec = db.get_by_id(unchanged_id).unwrap().unwrap();
        assert_eq!(rec.thumb_state, 2, "無変更行のサムネイルを無効化しない");
        // 変更行はメタデータが無効化される
        let changed_rec = db
            .get_meta_by_path(Path::new("root/changed.jpg"))
            .unwrap()
            .unwrap();
        let changed_full = db.get_by_id(changed_rec.id).unwrap().unwrap();
        assert_eq!(changed_full.mtime_ms, 999);
        assert_eq!(changed_full.thumb_state, 0);
    }

    #[test]
    fn apply_scan_走査に失敗したルート配下は削除されない() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("usb");
        db.upsert_files(&[scanned("usb/photo.jpg", 10, 100)])
            .unwrap();

        // ルートが読めなかった（ok_roots空）→ スキャン結果も空
        let (added, changed, removed) = db
            .apply_scan(&[], std::slice::from_ref(&root), &[], None)
            .unwrap();
        assert_eq!((added, changed, removed), (0, 0, 0));
        assert_eq!(db.count().unwrap(), 1, "誤削除しない");
    }

    #[test]
    fn apply_scan_設定から外されたルート配下は削除される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("old-root/photo.jpg", 10, 100)])
            .unwrap();

        let current = PathBuf::from("current-root");
        let (_, _, removed) = db
            .apply_scan(
                &[],
                std::slice::from_ref(&current),
                std::slice::from_ref(&current),
                None,
            )
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn apply_scan_入れ子ルートは最も深いルートの成否で判定する() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("outer/inner/photo.jpg", 10, 100)])
            .unwrap();

        let outer = PathBuf::from("outer");
        let inner = PathBuf::from("outer/inner");
        let roots = vec![outer.clone(), inner.clone()];

        // 深い方(inner)が失敗 → outerが成功していても保持
        let (_, _, removed) = db
            .apply_scan(&[], &roots, std::slice::from_ref(&outer), None)
            .unwrap();
        assert_eq!(removed, 0, "深いルートの失敗が優先され保持される");

        // 深い方(inner)も成功 → 削除される
        let (_, _, removed) = db.apply_scan(&[], &roots, &roots, None).unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn apply_scan_ルート未設定なら全レコードが削除される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("old/photo.jpg", 10, 100)])
            .unwrap();

        // 最後のライブラリルートを外した直後の再同期（ルート0件）
        let (added, changed, removed) = db.apply_scan(&[], &[], &[], None).unwrap();
        assert_eq!((added, changed, removed), (0, 0, 1));
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn apply_scan_区切り文字で終わるルートでも保護される() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("D:\\photos\\a.jpg", 10, 100)])
            .unwrap();

        // ルートが `D:\` のように区切り文字で終わっていて、走査に失敗した場合
        let root = PathBuf::from("D:\\");
        let (_, _, removed) = db
            .apply_scan(&[], std::slice::from_ref(&root), &[], None)
            .unwrap();
        assert_eq!(removed, 0, "区切り文字終わりのルート配下も保護される");

        // 同じルートが走査に成功した場合は削除される
        let (_, _, removed) = db
            .apply_scan(
                &[],
                std::slice::from_ref(&root),
                std::slice::from_ref(&root),
                None,
            )
            .unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn apply_scan_枝刈りでスキップした配下は削除されない() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("root");
        let sub = PathBuf::from("root/sub");
        let roots = vec![root.clone()];

        // 初回フル相当: root と root/sub を列挙して1ファイル登録
        let seen = vec![(root.clone(), 100), (sub.clone(), 200)];
        let enumerated = vec![root.clone(), sub.clone()];
        db.apply_scan(
            &[scanned("root/sub/a.jpg", 10, 100)],
            &roots,
            &roots,
            Some(DirSnapshot {
                seen: &seen,
                enumerated: &enumerated,
            }),
        )
        .unwrap();
        assert_eq!(db.count().unwrap(), 1);
        let dirs = db.load_dirs().unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[&sub], 200);

        // 2回目: subはmtime一致でスキップ（列挙はrootのみ・ファイル報告なし）
        // → sub配下のa.jpgは「見えなかった」だけであり削除してはいけない
        let enumerated_root_only = vec![root.clone()];
        let (_, _, removed) = db
            .apply_scan(
                &[],
                &roots,
                &roots,
                Some(DirSnapshot {
                    seen: &seen,
                    enumerated: &enumerated_root_only,
                }),
            )
            .unwrap();
        assert_eq!(removed, 0, "スキップしたディレクトリ配下は保持");
        assert_eq!(db.count().unwrap(), 1);

        // 3回目: subディレクトリ自体が消えた（seenに現れない）
        // → dirsから掃除され、「親ディレクトリが消えた」判定で配下も削除される
        let seen_gone = vec![(root.clone(), 300)];
        let (_, _, removed) = db
            .apply_scan(
                &[],
                &roots,
                &roots,
                Some(DirSnapshot {
                    seen: &seen_gone,
                    enumerated: &enumerated_root_only,
                }),
            )
            .unwrap();
        assert_eq!(removed, 1, "消えたディレクトリ配下は削除");
        assert_eq!(db.count().unwrap(), 0);
        let dirs = db.load_dirs().unwrap();
        assert!(!dirs.contains_key(&sub), "dirsからも掃除される");
        assert_eq!(dirs[&root], 300, "mtimeは最新へ更新される");
    }

    #[test]
    fn apply_scan_枝刈り時も走査失敗ルートのdirsとメディアは保持される() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("usb");
        let roots = vec![root.clone()];
        let seen = vec![(root.clone(), 100)];
        let enumerated = vec![root.clone()];
        db.apply_scan(
            &[scanned("usb/a.jpg", 10, 100)],
            &roots,
            &roots,
            Some(DirSnapshot {
                seen: &seen,
                enumerated: &enumerated,
            }),
        )
        .unwrap();

        // USB切断: ok_roots空・何も見えない
        let (_, _, removed) = db
            .apply_scan(
                &[],
                &roots,
                &[],
                Some(DirSnapshot {
                    seen: &[],
                    enumerated: &[],
                }),
            )
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(db.count().unwrap(), 1, "メディアは誤削除しない");
        // ルート自身のdirs行はLIKEパターン（root配下）に含まれないため消えるが、
        // 子ディレクトリの記録は保持される → 次回スキャンでルートだけ再列挙される
        assert!(db.load_dirs().unwrap().len() <= 1);
    }

    #[test]
    fn apply_partial_scanは列挙範囲だけを反映する() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("root");
        let (dir_a, dir_b) = (PathBuf::from("root/a"), PathBuf::from("root/b"));
        let roots = vec![root.clone()];
        let seen = vec![(root.clone(), 1), (dir_a.clone(), 2), (dir_b.clone(), 3)];
        let enumerated = vec![root.clone(), dir_a.clone(), dir_b.clone()];
        db.apply_scan(
            &[
                scanned("root/a/1.jpg", 1, 100),
                scanned("root/a/2.jpg", 1, 100),
                scanned("root/b/3.jpg", 1, 100),
            ],
            &roots,
            &roots,
            Some(DirSnapshot {
                seen: &seen,
                enumerated: &enumerated,
            }),
        )
        .unwrap();

        // ダーティなのは a だけ: 2.jpgが消え、4.jpgが増えた
        let (added, changed, removed) = db
            .apply_partial_scan(
                &[
                    scanned("root/a/1.jpg", 1, 100),
                    scanned("root/a/4.jpg", 1, 200),
                ],
                &[(dir_a.clone(), 20)],
                std::slice::from_ref(&dir_a),
            )
            .unwrap();
        assert_eq!((added, changed, removed), (1, 0, 1));
        assert!(db
            .get_meta_by_path(Path::new("root/a/2.jpg"))
            .unwrap()
            .is_none());
        assert!(db
            .get_meta_by_path(Path::new("root/a/4.jpg"))
            .unwrap()
            .is_some());
        // 走査していない b には触れない
        assert!(db
            .get_meta_by_path(Path::new("root/b/3.jpg"))
            .unwrap()
            .is_some());
        let dirs = db.load_dirs().unwrap();
        assert_eq!(dirs[&dir_a], 20, "列挙したaのmtimeは更新");
        assert_eq!(dirs[&dir_b], 3, "bの記録はそのまま");
    }

    #[test]
    fn apply_partial_scan_大小文字だけのリネームで新綴りの行を巻き込まない() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("root");
        let old_dir = PathBuf::from("root/summer");
        let roots = vec![root.clone()];
        let seen = vec![(root.clone(), 1), (old_dir.clone(), 2)];
        let enumerated = vec![root.clone(), old_dir.clone()];
        db.apply_scan(
            &[scanned("root/summer/a.jpg", 1, 100)],
            &roots,
            &roots,
            Some(DirSnapshot {
                seen: &seen,
                enumerated: &enumerated,
            }),
        )
        .unwrap();

        // summer → Summer へ大小文字だけのリネーム（NTFSでは正当な操作）。
        // USN経由の部分反映: rootとSummerを列挙、旧綴りsummerは消えた扱い。
        // SQLiteのLIKEはASCII大小文字を区別しないため、バイト厳密の
        // ガードが無いと旧綴りの掃除が新綴りの行まで削除してしまう
        let new_dir = PathBuf::from("root/Summer");
        let seen2 = vec![(root.clone(), 9), (new_dir.clone(), 10)];
        let enumerated2 = vec![root.clone(), new_dir.clone()];
        let (added, _, removed) = db
            .apply_partial_scan(
                &[scanned("root/Summer/a.jpg", 1, 100)],
                &seen2,
                &enumerated2,
            )
            .unwrap();
        assert_eq!((added, removed), (1, 1), "旧綴りの1件だけが削除される");
        assert_eq!(db.count().unwrap(), 1);
        assert!(
            db.get_meta_by_path(Path::new("root/Summer/a.jpg"))
                .unwrap()
                .is_some(),
            "新綴りの行が残る"
        );
        let dirs = db.load_dirs().unwrap();
        assert!(dirs.contains_key(&new_dir), "新綴りのdirs記録が残る");
        assert!(!dirs.contains_key(&old_dir), "旧綴りのdirs記録は掃除される");
    }

    #[test]
    fn remove_by_prefixは大小文字違いの別綴りを巻き込まない() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("root/summer/a.jpg", 1, 100),
            scanned("root/Summer/b.jpg", 1, 200),
        ])
        .unwrap();
        let n = db.remove_by_prefix(Path::new("root/summer")).unwrap();
        assert_eq!(n, 1);
        assert!(db
            .get_meta_by_path(Path::new("root/Summer/b.jpg"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn apply_partial_scanは消えた子ディレクトリを配下ごと削除する() {
        let mut db = Db::open_in_memory().unwrap();
        let root = PathBuf::from("root");
        let sub = PathBuf::from("root/sub");
        let deep = PathBuf::from("root/sub/deep");
        let roots = vec![root.clone()];
        let seen = vec![(root.clone(), 1), (sub.clone(), 2), (deep.clone(), 3)];
        let enumerated = vec![root.clone(), sub.clone(), deep.clone()];
        db.apply_scan(
            &[
                scanned("root/sub/x.jpg", 1, 100),
                scanned("root/sub/deep/y.jpg", 1, 100),
            ],
            &roots,
            &roots,
            Some(DirSnapshot {
                seen: &seen,
                enumerated: &enumerated,
            }),
        )
        .unwrap();

        // subがリネーム等で消えた: rootを列挙したがsubは見えなかった
        let (_, _, removed) = db
            .apply_partial_scan(&[], &[(root.clone(), 9)], std::slice::from_ref(&root))
            .unwrap();
        assert_eq!(removed, 2, "sub配下の2ファイルが削除される");
        assert_eq!(db.count().unwrap(), 0);
        let dirs = db.load_dirs().unwrap();
        assert!(!dirs.contains_key(&sub));
        assert!(!dirs.contains_key(&deep), "孫ディレクトリの記録も掃除");
        assert_eq!(dirs[&root], 9);
    }

    #[test]
    fn metaの読み書きができる() {
        let mut db = Db::open_in_memory().unwrap();
        assert_eq!(db.get_meta("scan_fingerprint").unwrap(), None);
        db.set_meta("scan_fingerprint", "v1|ext=jpg").unwrap();
        assert_eq!(
            db.get_meta("scan_fingerprint").unwrap().as_deref(),
            Some("v1|ext=jpg")
        );
        db.set_meta("scan_fingerprint", "v1|ext=jpg,png").unwrap();
        assert_eq!(
            db.get_meta("scan_fingerprint").unwrap().as_deref(),
            Some("v1|ext=jpg,png")
        );
    }

    #[test]
    fn サムネイルlru削除は古い順に解放し除外集合を守る() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("a.jpg", 1, 1000),
            scanned("b.jpg", 1, 2000),
            scanned("c.jpg", 1, 3000),
        ])
        .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().rev().map(|r| r.id).collect();
        for (i, &id) in ids.iter().enumerate() {
            db.update_thumb_path(id, Path::new(&format!("t/{id}.webp")), 2, Some(1000))
                .unwrap();
            // 利用時刻を過去へ巻き戻す: a が最も古く c が最も新しい
            // （touch_thumbsはMAXで単調増加のみなので、テストでは直接更新する）
            set_thumb_used(&db, id, 1_000_000 + i as i64);
        }
        assert_eq!(db.thumb_cache_usage().unwrap(), 3000);

        // 上限2000B → 9割(1800B)まで解放 = 2件削除（古い順: a, b）
        let empty = std::collections::HashSet::new();
        let evicted = db.evict_final_thumbs(2000, &empty).unwrap();
        assert_eq!(evicted.len(), 2);
        assert_eq!(db.thumb_cache_usage().unwrap(), 1000);
        // 解放された行はstate=0へ戻り、パスも消える
        let a = db.get_by_id(ids[0]).unwrap().unwrap();
        assert_eq!((a.thumb_state, a.thumb_path), (0, None));
        // 最新のcは残る
        let c = db.get_by_id(ids[2]).unwrap().unwrap();
        assert_eq!(c.thumb_state, 2);

        // 上限内なら何もしない
        assert!(db.evict_final_thumbs(2000, &empty).unwrap().is_empty());
    }

    #[test]
    fn サムネイルlru削除は生成キュー内のidを避ける() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 1, 1000), scanned("b.jpg", 1, 2000)])
            .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().rev().map(|r| r.id).collect();
        for (i, &id) in ids.iter().enumerate() {
            db.update_thumb_path(id, Path::new(&format!("t/{id}.webp")), 2, Some(1000))
                .unwrap();
            set_thumb_used(&db, id, 1_000_000 + i as i64);
        }

        // 最古のaはキュー内 → スキップされ、次点のbが削除される
        let exclude: std::collections::HashSet<i64> = [ids[0]].into();
        let evicted = db.evict_final_thumbs(1000, &exclude).unwrap();
        assert_eq!(
            evicted,
            vec![(ids[1], PathBuf::from(format!("t/{}.webp", ids[1])))]
        );
        assert_eq!(db.get_by_id(ids[0]).unwrap().unwrap().thumb_state, 2);
    }

    #[test]
    fn 直近利用のサムネイルはガード外で足りる限り削除されない() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("old.jpg", 1, 1000), scanned("hot.jpg", 1, 2000)])
            .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().rev().map(|r| r.id).collect();
        for &id in &ids {
            db.update_thumb_path(id, Path::new(&format!("t/{id}.webp")), 2, Some(1000))
                .unwrap();
        }
        set_thumb_used(&db, ids[0], 1_000_000); // old: 1時間ガード外
                                                // hot(ids[1])はupdate_thumb_pathで利用時刻=現在 → ガード内

        // cap 1500B: 超過500B → ガード外のoldだけで足りる → hotは守られる
        let empty = std::collections::HashSet::new();
        let evicted = db.evict_final_thumbs(1500, &empty).unwrap();
        assert_eq!(
            evicted.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![ids[0]]
        );
        assert_eq!(db.get_by_id(ids[1]).unwrap().unwrap().thumb_state, 2);
    }

    #[test]
    fn ガード内だけでは上限を満たせない場合はガードを破って削除する() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 1, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        // 生成直後（利用時刻=現在、ガード内）の5000Bだけがキャッシュにある
        db.update_thumb_path(id, Path::new("t/a.webp"), 2, Some(5000))
            .unwrap();

        // cap 1000B: ガード内しか候補がない → 上限維持を優先して削除される
        let empty = std::collections::HashSet::new();
        let evicted = db.evict_final_thumbs(1000, &empty).unwrap();
        assert_eq!(
            evicted.len(),
            1,
            "ディスク上限の維持が直近利用の保護より優先"
        );
    }

    #[test]
    fn 複数接続から同時に書いてもロックで失敗しない() {
        // 取り込み直後に起きる実際の混み合いを再現する:
        // 「コピー先の反映スキャン」と「ファイル監視のupsert」が別接続で
        // 同時に書きに来る。書き込みトランザクションがDEFERREDだと
        // SQLITE_BUSY_SNAPSHOT が busy_timeout を無視して即座に返り、
        // 取り込みが「database is locked」で失敗して見える
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("busy.db");
        Db::open(&path).unwrap(); // スキーマを作っておく

        let handles: Vec<_> = (0..3)
            .map(|worker| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut db = Db::open(&path).unwrap();
                    let root = PathBuf::from(format!("/root{worker}"));
                    for round in 0..30 {
                        let file = scanned(&format!("root{worker}/{round}.jpg"), 1, 1000);
                        // 反映スキャン: 読んでから書く（＝スナップショットを取る側）
                        db.apply_scan(
                            std::slice::from_ref(&file),
                            std::slice::from_ref(&root),
                            std::slice::from_ref(&root),
                            None,
                        )
                        .expect("反映スキャンがロックで失敗した");
                        // ファイル監視のupsert（＝先に書く側）
                        db.upsert_files(std::slice::from_ref(&file))
                            .expect("upsertがロックで失敗した");
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn backfillはサイズ未記録のサムネイルを補完し消失分を自己修復する() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 1, 1000), scanned("b.jpg", 1, 2000)])
            .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().rev().map(|r| r.id).collect();

        // 移行前の状態を再現: state=2 だが thumb_bytes が NULL
        let real = dir.path().join("real.webp");
        std::fs::write(&real, vec![0u8; 1234]).unwrap();
        for (id, path) in [(ids[0], real.as_path()), (ids[1], Path::new("gone.webp"))] {
            db.conn
                .execute(
                    "UPDATE media SET thumb_path = ?2, thumb_state = 2 WHERE id = ?1",
                    params![id, path.to_string_lossy()],
                )
                .unwrap();
        }
        assert_eq!(
            db.thumb_cache_usage().unwrap(),
            0,
            "補完前は使用量に見えない"
        );

        assert_eq!(db.backfill_thumb_sizes(10).unwrap(), 2);
        assert_eq!(db.thumb_cache_usage().unwrap(), 1234, "実在分はサイズ記録");
        let gone = db.get_by_id(ids[1]).unwrap().unwrap();
        assert_eq!(
            (gone.thumb_state, gone.thumb_path),
            (0, None),
            "消失ファイルは未生成へ戻して自己修復"
        );
        assert_eq!(db.backfill_thumb_sizes(10).unwrap(), 0, "2回目は対象なし");
    }

    #[test]
    fn touch_thumbsは利用時刻を単調増加で更新する() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[scanned("a.jpg", 1, 1000)]).unwrap();
        let id = db.list_all().unwrap()[0].id;
        let used = |db: &Db| -> Option<i64> {
            db.conn
                .query_row("SELECT thumb_used_ms FROM media WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .unwrap()
        };

        db.touch_thumbs(&[(id, 100)]).unwrap();
        assert_eq!(used(&db), Some(100));
        // 古いタッチでは巻き戻らない（フラッシュ順序が前後しても安全）
        db.touch_thumbs(&[(id, 50)]).unwrap();
        assert_eq!(used(&db), Some(100));
        db.touch_thumbs(&[(id, 200)]).unwrap();
        assert_eq!(used(&db), Some(200));
    }

    #[test]
    fn メタデータ未抽出のidだけが自動投入対象になる() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned("new.jpg", 1, 1000),
            scanned("provisional.jpg", 1, 2000),
            scanned("evicted.jpg", 1, 3000),
        ])
        .unwrap();
        let by_name =
            |db: &Db, name: &str| db.get_meta_by_path(Path::new(name)).unwrap().unwrap().id;
        // provisional: メタデータ抽出済み・即席サムネイルあり
        let prov = by_name(&db, "provisional.jpg");
        db.update_metadata(prov, Dimensions::original(640, 480), Some(2000), None)
            .unwrap();
        db.update_thumb_path(prov, Path::new("t/p.jpg"), 1, None)
            .unwrap();
        // evicted: メタデータ抽出済み・LRU削除でstate=0へ戻った状態
        let evicted = by_name(&db, "evicted.jpg");
        db.update_metadata(evicted, Dimensions::original(640, 480), Some(3000), None)
            .unwrap();

        let missing = db.ids_missing_metadata().unwrap();
        assert_eq!(
            missing,
            vec![by_name(&db, "new.jpg")],
            "未抽出のみ。即席止まり・LRU削除済みは可視要求時のみ再生成"
        );
    }

    #[test]
    fn 読み取りプールは書き込み後のデータを読める() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("pool.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[scanned("a.jpg", 100, 1000)]).unwrap();

        let pool = ReadPool::open(&db_path, 3).unwrap();
        assert_eq!(pool.with(|db| db.count().unwrap()), 1);

        // 書き込み接続でのコミットは、既存のプール接続からも即座に見える（WALスナップショット）
        db.upsert_files(&[scanned("b.jpg", 200, 2000)]).unwrap();
        assert_eq!(pool.with(|db| db.count().unwrap()), 2);

        // プール接続は書き込みを拒否する
        let err = pool.with(|db| {
            db.conn
                .execute("UPDATE media SET favorite = 1", [])
                .unwrap_err()
        });
        assert!(err.to_string().contains("readonly"), "err={err}");
    }

    #[test]
    fn apply_scan_ワイルドカードを含むパスで誤削除しない() {
        let mut db = Db::open_in_memory().unwrap();
        // root_x は root_a とLIKE `root_%` で誤マッチしうる名前
        db.upsert_files(&[scanned("root_x/photo.jpg", 10, 100)])
            .unwrap();

        let root_a = PathBuf::from("root_a");
        // root_a配下は走査成功・空。root_x はどのルートにも属さない → 削除対象
        // （エスケープが壊れていると root_a のLIKEに root_x が巻き込まれ得ることの逆試験:
        //  root_x がルート root_x として保護されるケース）
        let root_x = PathBuf::from("root_x");
        let (_, _, removed) = db
            .apply_scan(
                &[],
                &[root_a.clone(), root_x.clone()],
                std::slice::from_ref(&root_a),
                None,
            )
            .unwrap();
        assert_eq!(removed, 0, "走査に失敗した root_x 配下は保持される");
    }

    // ---- 第4部 段階D: 爆速検索 ----

    /// 検索テスト用のライブラリを作る（パス・カメラ名つき）。
    fn seed_search_db() -> Db {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned(
                r"D:\写真\2019-08 沖縄旅行\DSC00123.JPG",
                1,
                1_565_000_000_000,
            ),
            scanned(
                r"D:\写真\2019-08 沖縄旅行\DSC00124.JPG",
                1,
                1_565_000_100_000,
            ),
            scanned(r"D:\写真\家族\IMG_1234.jpg", 1, 1_600_000_000_000),
            scanned(r"D:\写真\2020 夏\花火大会.jpg", 1, 1_596_000_000_000),
        ])
        .unwrap();
        let ids: Vec<(i64, PathBuf)> = db
            .list_all()
            .unwrap()
            .into_iter()
            .map(|r| (r.id, r.path))
            .collect();
        for (id, path) in ids {
            let camera = if path.to_string_lossy().contains("IMG_") {
                "Apple iPhone 15 Pro"
            } else {
                "SONY ILCE-7M3"
            };
            db.update_metadata(id, Dimensions::original(400, 300), None, Some(camera))
                .unwrap();
        }
        db
    }

    /// 検索して、ヒットしたファイル名を昇順で返す。
    ///
    /// 名前の切り出しは `Path::file_name` ではなく**両方の区切り文字**で行う。
    /// テストデータはWindowsの綴り（`D:\写真\...`）だが、macOS/Linuxの
    /// `Path` は `\` を区切りと見ないため、パス全体が1つのファイル名になる。
    /// DB側（`pk_name`）も同じく文字で切っており、そちらに合わせる
    fn search_names(db: &Db, input: &str) -> Vec<String> {
        let query = crate::search::parse_query(input, crate::MediaFilter::All);
        let mut names: Vec<String> = db
            .search_summary(&query)
            .unwrap()
            .iter()
            .flat_map(|d| db.search_day(d.day_key, &query).unwrap())
            .map(|r| {
                let path = r.path.to_string_lossy();
                path.rsplit(['\\', '/']).next().unwrap_or("").to_string()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn 日本語は中間一致で検索できる() {
        let db = seed_search_db();
        // unicode61そのままでは引けない「語の途中」がbigram索引で引ける
        assert_eq!(
            search_names(&db, "旅行"),
            ["DSC00123.JPG", "DSC00124.JPG"],
            "フォルダ名の中間一致"
        );
        assert_eq!(
            search_names(&db, "大会"),
            ["花火大会.jpg"],
            "ファイル名の中間一致"
        );
        assert_eq!(search_names(&db, "沖縄"), ["DSC00123.JPG", "DSC00124.JPG"]);
        // 1文字はどの位置でも引ける（語頭・語中・**語末**）
        assert_eq!(search_names(&db, "花"), ["花火大会.jpg"]);
        assert_eq!(search_names(&db, "火"), ["花火大会.jpg"]);
        assert_eq!(
            search_names(&db, "会"),
            ["花火大会.jpg"],
            "末尾の1文字（bigramの先頭には現れない）も引ける"
        );
        assert_eq!(
            search_names(&db, "行"),
            ["DSC00123.JPG", "DSC00124.JPG"],
            "フォルダ名末尾の1文字"
        );
        // 語の連続はフレーズ照合なので、順序違いの誤検出は出ない
        assert!(search_names(&db, "行旅").is_empty());
        // 末尾ユニグラムを足しても部分一致は壊れない
        assert_eq!(
            search_names(&db, "沖縄旅行"),
            ["DSC00123.JPG", "DSC00124.JPG"]
        );
        assert_eq!(search_names(&db, "火大会"), ["花火大会.jpg"]);
    }

    #[test]
    fn 索引語を作れない検索語は全件を返さない() {
        let db = seed_search_db();
        assert_eq!(search_names(&db, "").len(), 4, "空クエリは絞り込みなし");
        // 記号・絵文字だけの語は「一致なし」。条件が消えて全件になってはいけない
        assert!(search_names(&db, "!!!").is_empty());
        assert!(search_names(&db, "📷").is_empty());
        // 有効な語と組み合わせてもAND（絞り込み）なので結果は空
        assert!(search_names(&db, "沖縄 !!!").is_empty());
    }

    #[test]
    fn ファイル名とカメラ名で検索できる() {
        let db = seed_search_db();
        assert_eq!(search_names(&db, "dsc"), ["DSC00123.JPG", "DSC00124.JPG"]);
        assert_eq!(search_names(&db, "1234"), ["IMG_1234.jpg"]);
        // camera: は列を限定する。フォルダ名やファイル名の一致は拾わない
        assert_eq!(search_names(&db, "camera:iPhone"), ["IMG_1234.jpg"]);
        assert_eq!(search_names(&db, "folder:家族"), ["IMG_1234.jpg"]);
        assert!(search_names(&db, "camera:沖縄").is_empty());
        // 自由語はカメラ名にも当たる
        assert_eq!(search_names(&db, "ILCE").len(), 3);
    }

    #[test]
    fn 検索条件はandで重なる() {
        let db = seed_search_db();
        assert_eq!(
            search_names(&db, "沖縄 camera:SONY"),
            ["DSC00123.JPG", "DSC00124.JPG"]
        );
        assert!(search_names(&db, "沖縄 camera:iPhone").is_empty());
        // 日付での絞り込み（day_keyは撮影日時未確定なのでmtime基準）
        let all = search_names(&db, "");
        assert_eq!(all.len(), 4);
        let y2019 = search_names(&db, "2019年");
        assert_eq!(y2019, ["DSC00123.JPG", "DSC00124.JPG"]);
        assert!(search_names(&db, "2019年 花火").is_empty());
    }

    #[test]
    fn search_idsは一覧と同じ並びで全件返す() {
        let db = seed_search_db();
        let q = crate::search::parse_query("", crate::MediaFilter::All);
        let ids = db.search_ids(&q).unwrap();
        assert_eq!(ids.len(), 4, "条件なしなら全件");

        // **一覧に出る順と一致すること**が肝。日をまたいでも、2点のIDが決まれば
        // 添字で範囲を切り出せる、という前提がここで担保される
        let mut expected = Vec::new();
        for day in db.search_summary(&q).unwrap() {
            for rec in db.search_day(day.day_key, &q).unwrap() {
                expected.push(rec.id);
            }
        }
        assert_eq!(ids, expected, "日ごとに引いた順と同じ");

        // 絞り込みも効く
        let q = crate::search::parse_query("沖縄", crate::MediaFilter::All);
        assert_eq!(db.search_ids(&q).unwrap().len(), 2);
        let q = crate::search::parse_query("見つからない語", crate::MediaFilter::All);
        assert!(db.search_ids(&q).unwrap().is_empty());
    }

    #[test]
    fn search_ids_betweenは範囲のぶんだけ返す() {
        let db = seed_search_db();
        let q = crate::search::parse_query("", crate::MediaFilter::All);
        let all = db.search_ids(&q).unwrap();
        assert_eq!(all.len(), 4);

        // **全件を引かずに、同じ添字の切り出しになる**ことが肝
        let mid = db.search_ids_between(&q, all[1], all[2]).unwrap();
        assert_eq!(mid, all[1..3].to_vec());
        // 端の順序はどちらから指しても同じ
        assert_eq!(db.search_ids_between(&q, all[2], all[1]).unwrap(), mid);
        // 同じIDを2回指したら1枚
        assert_eq!(db.search_ids_between(&q, all[0], all[0]).unwrap(), [all[0]]);
        // 全体
        assert_eq!(db.search_ids_between(&q, all[0], all[3]).unwrap(), all);

        // 絞り込みの外にあるIDは、範囲の端として効かない。
        // 片方だけ生き残っていれば、その1枚ぶんの範囲になる
        let oki = crate::search::parse_query("沖縄", crate::MediaFilter::All);
        let oki_ids = db.search_ids(&oki).unwrap();
        assert_eq!(oki_ids.len(), 2);
        let outside = all
            .iter()
            .find(|id| !oki_ids.contains(id))
            .copied()
            .unwrap();
        assert_eq!(
            db.search_ids_between(&oki, oki_ids[0], outside).unwrap(),
            [oki_ids[0]]
        );
        // 両方とも条件の外なら空
        assert!(db.search_ids_between(&oki, outside, -1).unwrap().is_empty());
    }

    /// 実行計画を1行にまとめて返す（`EXPLAIN QUERY PLAN`）。
    fn plan_of(db: &Db, sql: &str) -> String {
        let mut stmt = db
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        // 変数は値によらないので、数だけ合わせて NULL を渡す
        let holes = vec![rusqlite::types::Value::Null; sql.matches('?').count()];
        let rows = stmt
            .query_map(rusqlite::params_from_iter(holes.iter()), |r| {
                r.get::<_, String>(3)
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>();
        rows.join(" / ")
    }

    #[test]
    fn 選択に使う問い合わせは索引の並びで引く() {
        // **測って釘を打つ**。`ORDER BY` が索引の並びと食い違うと、
        // 1000万件では全件を並べ直すことになる（Shift+クリックのたびに数秒）
        let db = seed_search_db();

        let plan = plan_of(&db, &super::all_ids_sql(""));
        assert!(plan.contains("idx_media_day_sort"), "全件: {plan}");
        assert!(
            !plan.contains("TEMP B-TREE"),
            "全件が並べ直しになっている: {plan}"
        );

        let plan = plan_of(&db, &super::between_sql(&super::between_conds()));
        assert!(plan.contains("idx_media_day_sort"), "範囲: {plan}");
        assert!(
            !plan.contains("TEMP B-TREE"),
            "範囲が並べ直しになっている: {plan}"
        );
        // 端の日だけを見るシークになっていること（全件走査に落ちていない）
        assert!(
            plan.contains("SEARCH"),
            "範囲がシークになっていない: {plan}"
        );

        // ★の絞り込みは部分索引に乗る。ここも並べ直しにならないこと
        let mut fav = vec!["favorite = 1".to_string()];
        fav.extend(super::between_conds());
        let plan = plan_of(&db, &super::between_sql(&fav));
        assert!(plan.contains("idx_media_fav_day"), "★の範囲: {plan}");
        assert!(
            !plan.contains("TEMP B-TREE"),
            "★の範囲が並べ直しになっている: {plan}"
        );

        // **検索語つきは一致集合の側から駆動される**。計画としては見栄えが悪い
        // （一時表と並べ直しが出る）が、行ごとにFTSを引く形は桁で遅い——
        // 測った結果は `FTS_FOR_SELECTION` の表に残してある。
        // ここでは「そういう計画になる」ことだけ確かめて、釘は打たない
        let q = crate::search::parse_query("沖縄", crate::MediaFilter::All);
        let (conds, _) = db.query_filter(&q).unwrap();
        let mut with_term = conds.clone();
        with_term.extend(super::between_conds());
        let plan = plan_of(&db, &super::between_sql(&with_term));
        assert!(plan.contains("media_fts"), "検索語つきの範囲: {plan}");
    }

    /// 種類の絞り込みも索引でシークする。
    ///
    /// **サマリは全行のGROUP BY**なので、ここが表スキャンへ落ちると
    /// 「日付→枚数」の骨組みがそのまま遅くなる。`kind` を先頭に置いた
    /// 複合索引なら、絞ったままでも day_key の並びが索引から供給される。
    #[test]
    fn 種類の絞り込みは索引でシークする() {
        let db = seed_search_db();
        let q = crate::search::parse_query("kind:raw", crate::MediaFilter::All);
        let (conds, _) = db.query_filter(&q).unwrap();
        let filter = format!("WHERE {}", conds.join(" AND "));

        let plan = plan_of(&db, &super::summary_sql(&filter));
        assert!(plan.contains("idx_media_kind_day"), "サマリ: {plan}");
        assert!(
            !plan.contains("TEMP B-TREE"),
            "サマリが並べ直しになっている: {plan}"
        );

        let plan = plan_of(&db, &super::all_ids_sql(&filter));
        assert!(plan.contains("idx_media_kind_day"), "全件: {plan}");
        assert!(
            !plan.contains("TEMP B-TREE"),
            "全件が並べ直しになっている: {plan}"
        );

        // **種類を選ばなければ従来どおり**（既存の計画を退化させていない）
        let plan = plan_of(&db, &super::all_ids_sql(""));
        assert!(plan.contains("idx_media_day_sort"), "指定なし: {plan}");
        // **正の形で釘を打つ**。「kindの索引を使っていない」だけだと、
        // 表スキャン＋並べ直しへ落ちても通ってしまう
        let plan = plan_of(&db, &super::summary_sql(""));
        assert!(
            plan.contains("idx_media_day_sort"),
            "指定なしのサマリ: {plan}"
        );
        assert!(
            !plan.contains("TEMP B-TREE"),
            "指定なしのサマリが並べ直しになっている: {plan}"
        );
    }

    /// 検索語つきの範囲選択・選択の確認で、FTSの2つの形のどちらが速いかを測る。
    ///
    /// **実行計画の見た目ではなく時間で決める**ためのもの。索引で駆動する形
    /// （`Probe`）は計画が綺麗に見えるが、FTSを行ごとに引く定数が乗る。
    /// CIでは走らせない（時間がかかるため `#[ignore]`）。
    ///
    /// ```text
    /// cargo test --release -p pictkura-core -- --ignored --nocapture 速さ
    /// ```
    #[test]
    #[ignore]
    fn 検索語つきの選択はどちらの形が速いか() {
        use std::time::Instant;
        let mut db = Db::open_in_memory().unwrap();
        // 3万件・600日ぶん。名前は実際のカメラと同じ連番
        let mut files = Vec::with_capacity(30_000);
        for i in 0..30_000i64 {
            let day = i / 50;
            files.push(scanned(
                &format!(r"D:\写真0-{:02}\IMG_{:05}.JPG", day % 12 + 1, i),
                1,
                1_500_000_000_000 + day * 86_400_000 + (i % 50) * 60_000,
            ));
        }
        db.upsert_files(&files).unwrap();

        let all = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::All))
            .unwrap();
        assert_eq!(all.len(), 30_000);

        for term in ["img_1", "img_12345"] {
            let q = crate::search::parse_query(term, crate::MediaFilter::All);
            let hits = db.search_ids(&q).unwrap();
            // **端は一致集合の中から取る**（画面に出ているものしか押せない）
            for (label, from, to) in [
                ("狭い範囲（150件ぶん）", hits[0], hits[150]),
                ("広い範囲（全域）", hits[0], hits[hits.len() - 1]),
            ] {
                for mode in [FtsMode::List, FtsMode::Probe] {
                    let t = Instant::now();
                    let got = db.search_ids_between_mode(&q, from, to, mode).unwrap();
                    println!(
                        "範囲 {label} 「{term}」（一致{}件）{:?}: {:.1}ms / {}件",
                        hits.len(),
                        mode,
                        t.elapsed().as_secs_f64() * 1000.0,
                        got.len()
                    );
                }
            }
            for (label, ids) in [("選択500枚", &all[..500]), ("全選択のあと", &all[..])] {
                for mode in [FtsMode::List, FtsMode::Probe] {
                    let t = Instant::now();
                    let got = db.visible_ids_mode(&q, ids, mode).unwrap();
                    println!(
                        "確認 {label} 「{term}」（一致{}件）{:?}: {:.1}ms / {}件",
                        hits.len(),
                        mode,
                        t.elapsed().as_secs_f64() * 1000.0,
                        got.len()
                    );
                }
            }
        }
    }

    #[test]
    fn visible_idsは並んでいるものだけを渡した順で返す() {
        let mut db = seed_search_db();
        let all = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::All))
            .unwrap();
        let oki = crate::search::parse_query("沖縄", crate::MediaFilter::All);
        let oki_ids = db.search_ids(&oki).unwrap();

        // 渡した順を保つ（選択の順序をそのまま一括操作へ渡すため）
        let mixed: Vec<i64> = all.iter().rev().copied().collect();
        let kept = db.visible_ids(&oki, &mixed).unwrap();
        let expected: Vec<i64> = mixed
            .iter()
            .copied()
            .filter(|id| oki_ids.contains(id))
            .collect();
        assert_eq!(kept, expected);

        // 居ないIDは落ちる／空の指定は空
        assert_eq!(
            db.visible_ids(
                &crate::search::parse_query("", crate::MediaFilter::All),
                &[all[0], -1]
            )
            .unwrap(),
            [all[0]]
        );
        assert!(db
            .visible_ids(
                &crate::search::parse_query("", crate::MediaFilter::All),
                &[]
            )
            .unwrap()
            .is_empty());

        // ★の絞り込みも条件のうち
        db.set_favorites(&all[..1], true).unwrap();
        let fav = crate::search::parse_query("", crate::MediaFilter::Fav);
        assert_eq!(db.visible_ids(&fav, &all).unwrap(), [all[0]]);
    }

    #[test]
    fn visible_ids_in_orderは一覧と同じ並びで日付を添えて返す() {
        let mut db = seed_search_db();
        let all = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::All))
            .unwrap();

        // 選択は集合なので渡る順はばらばら。逆順でも重複つきでも、
        // 返るのは**一覧と同じ並び**で1件ずつ（ビューアが隣へ歩く順序になる）
        let mixed: Vec<i64> = all
            .iter()
            .rev()
            .copied()
            .chain(all.iter().copied())
            .collect();
        let got = db
            .visible_ids_in_order(
                &crate::search::parse_query("", crate::MediaFilter::All),
                &mixed,
            )
            .unwrap();
        assert_eq!(got.iter().map(|(id, _)| *id).collect::<Vec<_>>(), all);

        // 添える日は行のもの（ビューアはこれで日をまたぐ）
        for (id, day) in &got {
            assert_eq!(db.get_by_id(*id).unwrap().unwrap().day_key, *day);
        }

        // 条件から外れたもの・居ないIDは落ちる／空の指定は空
        let oki = crate::search::parse_query("沖縄", crate::MediaFilter::All);
        let oki_ids = db.search_ids(&oki).unwrap();
        assert_eq!(
            db.visible_ids_in_order(&oki, &all)
                .unwrap()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            oki_ids
        );
        assert_eq!(
            db.visible_ids_in_order(
                &crate::search::parse_query("", crate::MediaFilter::All),
                &[-1]
            )
            .unwrap(),
            []
        );
        assert!(db
            .visible_ids_in_order(
                &crate::search::parse_query("", crate::MediaFilter::All),
                &[]
            )
            .unwrap()
            .is_empty());

        // ★の絞り込みも条件のうち
        db.set_favorites(&all[..1], true).unwrap();
        let fav = crate::search::parse_query("", crate::MediaFilter::Fav);
        assert_eq!(
            db.visible_ids_in_order(&fav, &all)
                .unwrap()
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            [all[0]]
        );
    }

    /// 選別の印は★と**別の棚**であること（0.2 ②）。片方を触っても
    /// もう片方は動かない——ここが混ざると、連写の選別で★の棚が荒れる
    #[test]
    fn 選別の印はお気に入りとは別に付け外しできる() {
        let mut db = seed_search_db();
        let all = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::All))
            .unwrap();

        db.set_picked(all[0], true).unwrap();
        let row = db.get_by_id(all[0]).unwrap().unwrap();
        assert!(row.picked, "⚑が付く");
        assert!(!row.favorite, "★は動かない");

        db.set_favorite(all[1], true).unwrap();
        assert!(
            !db.get_by_id(all[1]).unwrap().unwrap().picked,
            "⚑は動かない"
        );

        // 絞り込みも別々に効く
        let picked = crate::search::parse_query("", crate::MediaFilter::Picked);
        assert_eq!(db.search_ids(&picked).unwrap(), [all[0]]);
        let fav = crate::search::parse_query("", crate::MediaFilter::Fav);
        assert_eq!(db.search_ids(&fav).unwrap(), [all[1]]);

        // まとめて外す
        assert_eq!(db.set_pickeds(&all, false).unwrap(), all.len());
        assert!(db.search_ids(&picked).unwrap().is_empty());
        assert_eq!(db.search_ids(&fav).unwrap(), [all[1]], "★は残る");
    }

    #[test]
    fn set_favoritesはまとめて付け外しできる() {
        let mut db = seed_search_db();
        let all = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::All))
            .unwrap();
        let two = &all[..2];

        assert_eq!(db.set_favorites(two, true).unwrap(), 2);
        let favs = db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::Fav))
            .unwrap();
        assert_eq!(favs.len(), 2);
        assert!(two.iter().all(|id| favs.contains(id)));

        // 外すのも同じ経路
        assert_eq!(db.set_favorites(two, false).unwrap(), 2);
        assert!(db
            .search_ids(&crate::search::parse_query("", crate::MediaFilter::Fav))
            .unwrap()
            .is_empty());

        // 空の指定は何もしない（トランザクションも開かない）
        assert_eq!(db.set_favorites(&[], true).unwrap(), 0);
        // 居ないIDを混ぜても、居るぶんだけ付く
        assert_eq!(db.set_favorites(&[all[0], -1], true).unwrap(), 1);
    }

    #[test]
    fn 検索件数とカメラ別集計が取れる() {
        let db = seed_search_db();
        assert_eq!(
            db.search_count(&crate::search::parse_query("沖縄", crate::MediaFilter::All))
                .unwrap(),
            2
        );
        assert_eq!(
            db.list_cameras().unwrap(),
            vec![
                ("SONY ILCE-7M3".to_string(), 3),
                ("Apple iPhone 15 Pro".to_string(), 1),
            ]
        );
    }

    #[test]
    fn 索引はレコードの追加削除に追随する() {
        let mut db = seed_search_db();
        assert_eq!(search_names(&db, "運動会"), Vec::<String>::new());

        db.upsert_files(&[scanned(r"D:\写真\2021\運動会.jpg", 1, 1_620_000_000_000)])
            .unwrap();
        assert_eq!(
            search_names(&db, "運動会"),
            ["運動会.jpg"],
            "追加で索引に入る"
        );

        db.remove_paths(&[PathBuf::from(r"D:\写真\2021\運動会.jpg")])
            .unwrap();
        assert!(
            search_names(&db, "運動会").is_empty(),
            "削除で索引から消える"
        );

        // ディレクトリごとの削除でも索引が追随する
        db.remove_by_prefix(Path::new(r"D:\写真\2019-08 沖縄旅行"))
            .unwrap();
        assert!(search_names(&db, "沖縄").is_empty());
    }

    #[test]
    fn カメラの張り替えで索引も更新される() {
        let mut db = seed_search_db();
        let id = db
            .list_all()
            .unwrap()
            .iter()
            .find(|r| r.path.to_string_lossy().contains("IMG_1234"))
            .unwrap()
            .id;
        assert_eq!(search_names(&db, "camera:iPhone"), ["IMG_1234.jpg"]);

        db.update_metadata(
            id,
            Dimensions::original(400, 300),
            None,
            Some("Canon EOS R5"),
        )
        .unwrap();
        assert!(
            search_names(&db, "camera:iPhone").is_empty(),
            "古いカメラ名では引けなくなる"
        );
        assert_eq!(search_names(&db, "camera:EOS"), ["IMG_1234.jpg"]);
    }

    #[test]
    fn 既存レコードは初期構築で索引化される() {
        let mut db = seed_search_db();
        // 索引導入前のライブラリを再現する: 索引の中身だけを消す
        db.conn.execute("DELETE FROM media_fts", []).unwrap();
        assert!(search_names(&db, "沖縄").is_empty());

        let (cursor, max_id) = db.fts_build_range().unwrap();
        assert_eq!(cursor, 0);
        assert_eq!(max_id, 4);
        // 1件ずつのバッチでも最後まで進み、完了後は0件を返す
        let mut steps = 0;
        loop {
            let (n, cur) = db.fts_build_step(max_id, 1).unwrap();
            if n == 0 && cur >= max_id {
                break;
            }
            steps += 1;
            assert!(steps <= 10, "構築が終わらない");
        }
        assert_eq!(search_names(&db, "沖縄"), ["DSC00123.JPG", "DSC00124.JPG"]);
        assert_eq!(search_names(&db, "camera:iPhone"), ["IMG_1234.jpg"]);

        // 構築完了後に開き直しても再構築されない（カーソルが永続化されている）
        let (cursor, max_id2) = db.fts_build_range().unwrap();
        assert_eq!((cursor, max_id2), (4, 4));
    }

    #[test]
    fn 構築中に追加された行は二重投入されない() {
        let mut db = seed_search_db();
        db.conn.execute("DELETE FROM media_fts", []).unwrap();
        let (_, max_id) = db.fts_build_range().unwrap();

        // 構築の途中で新しいレコードが増える（トリガが即座に索引化する）
        db.fts_build_step(max_id, 1).unwrap();
        db.upsert_files(&[scanned(r"D:\写真\2021\運動会.jpg", 1, 1_620_000_000_000)])
            .unwrap();
        while db.fts_build_step(max_id, 1).unwrap().0 > 0 {}

        // 掃き寄せは上限IDまでなので、新しい行は1度しか索引に入らない
        assert_eq!(search_names(&db, "運動会"), ["運動会.jpg"]);
        assert_eq!(search_names(&db, "沖縄"), ["DSC00123.JPG", "DSC00124.JPG"]);
    }

    #[test]
    fn カメラ未確認の行だけが後追い補完の対象になる() {
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[
            scanned(r"D:\写真\a.jpg", 1, 1000),
            scanned(r"D:\写真\b.jpg", 1, 2000),
            scanned(r"D:\写真\c.jpg", 1, 3000),
        ])
        .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().map(|r| r.id).collect();

        // 取り込み直後（メタデータ未抽出）は対象外。通常のサムネイル処理が
        // カメラごと書くので、ここで拾うと二重処理になる
        assert_eq!(db.cameras_to_backfill(0, 10).unwrap().len(), 0);
        assert_eq!(db.cameras_pending().unwrap(), 0);

        // 検索導入前のライブラリを再現: メタデータだけ抽出済み・カメラは未確認
        for id in &ids {
            db.conn
                .execute(
                    "UPDATE media SET width = 100, height = 100 WHERE id = ?1",
                    [id],
                )
                .unwrap();
        }
        assert_eq!(db.cameras_pending().unwrap(), 3);
        let batch = db.cameras_to_backfill(0, 2).unwrap();
        assert_eq!(batch.len(), 2, "バッチ上限が効く");

        // カメラありと「EXIFにカメラなし」を混ぜて書く
        db.set_cameras(&[
            (ids[0], Some("SONY ILCE-7M3".into())),
            (ids[1], None),
            (ids[2], Some("  ".into())), // 空白だけの値もカメラなし扱い
        ])
        .unwrap();

        // 「確認済み」の印が付くので、同じ行は二度と読み直されない
        assert_eq!(db.cameras_pending().unwrap(), 0);
        assert_eq!(db.cameras_to_backfill(0, 10).unwrap().len(), 0);
        // カメラなしの行は集計にも `camera:` 絞り込みにも現れない
        assert_eq!(
            db.list_cameras().unwrap(),
            vec![("SONY ILCE-7M3".into(), 1)]
        );
        assert_eq!(
            db.search_count(&crate::search::parse_query(
                "camera:SONY",
                crate::MediaFilter::All
            ))
            .unwrap(),
            1
        );
    }

    #[test]
    fn 内容が変わったファイルはカメラ情報が無効化される() {
        let mut db = seed_search_db();
        assert_eq!(search_names(&db, "camera:iPhone"), ["IMG_1234.jpg"]);
        // 同じパスで内容が変わる（サイズ違い）→ メタデータは再抽出待ちになる
        db.upsert_files(&[scanned(
            r"D:\写真\家族\IMG_1234.jpg",
            999,
            1_600_000_000_000,
        )])
        .unwrap();
        assert!(
            search_names(&db, "camera:iPhone").is_empty(),
            "古いカメラ情報は索引から外れる"
        );
        assert_eq!(
            search_names(&db, "1234"),
            ["IMG_1234.jpg"],
            "名前では引ける"
        );
    }
}
