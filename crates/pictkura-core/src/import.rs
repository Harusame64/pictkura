//! USB等からの取り込み（コピー）処理。
//!
//! - 取り込み元をスキャンし、`[routing]` の設定（コピー先ルート＋日付フォルダパターン）に
//!   従ってコピーする
//! - 日付はEXIF撮影日時を優先、なければファイルのmtime
//! - 同名・同サイズのファイルが既にあればスキップ（再取り込みの重複防止）
//! - 同名・別サイズなら `名前-1.jpg` 形式で衝突回避
//! - コピー後にサイズ比較で検証する（`verify_after_copy`）

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, TimeZone};

use crate::config::Config;
use crate::scanner;
use crate::thumbs::read_exif;

pub use scanner::is_managed_package_path;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("コピー先フォルダが設定されていません")]
    NoDestination,
    #[error("取り込み元フォルダが読めません: {0}")]
    SourceUnreadable(PathBuf),
    /// 写真.appのライブラリのような、アプリが管理するパッケージの中を
    /// 取り込み元に指定した。中身はUUID名の内部ファイルなので、
    /// 取り込むと派生画像を数千枚コピーすることになる
    #[error(
        "アプリが管理するライブラリの中は取り込めません（中身は内部ファイルです）。\
         中の写真を取り出したいときは、フォルダ名から拡張子（.photoslibrary など）を\
         外してから選び直してください: {0}"
    )]
    SourceIsManagedPackage(PathBuf),
}

/// 取り込み結果の件数サマリ。
#[derive(Debug, Default, PartialEq)]
pub struct ImportStats {
    /// コピーしたファイル数
    pub copied: usize,
    /// 既に存在していてスキップした数
    pub skipped: usize,
    /// コピーまたは検証に失敗した数
    pub failed: usize,
    /// 取り込み元の走査中にエラーがあった（**取りこぼしの可能性あり**）。
    /// trueの場合、UIは「すべて取り込めた」と表示してはならない
    pub scan_incomplete: bool,
}

/// 撮影日（またはmtime）の年月日。フォルダパターンの置換に使う。
#[derive(Debug, Clone, Copy)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// エポックミリ秒からローカルタイムゾーンの年月日を取り出す。
/// EXIF撮影日時（ローカル壁時計としてエポック化）とmtimeの両方に使う。
fn date_from_local_ms(ms: i64) -> Option<CivilDate> {
    let dt = Local.timestamp_millis_opt(ms).single()?;
    Some(CivilDate {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
    })
}

/// 今日（取り込み実行日）のローカル年月日。日付が全く取れないファイルのフォールバック。
fn today_local() -> CivilDate {
    let now = Local::now();
    CivilDate {
        year: now.year(),
        month: now.month(),
        day: now.day(),
    }
}

/// コピー先のフォルダ構成の候補。
///
/// Lightroom Classic が「日付で整理」に用意しているのと同じ発想で、
/// 自由記述に頼らず**選ぶだけ**で決められるようにする。既定（先頭）の
/// `{year}/{year}-{month}-{day}` は、年で束ねつつ日フォルダが名前順＝時系列に
/// 並ぶため、写真管理で最も広く薦められている形。
pub const FOLDER_PATTERN_PRESETS: &[&str] = &[
    "{year}/{year}-{month}-{day}",         // 2026/2026-08-12
    "{year}/{month}/{day}",                // 2026/08/12
    "{year}/{month}-{day}",                // 2026/08-12
    "{year}-{month}/{year}-{month}-{day}", // 2026-08/2026-08-12
    "{year}-{month}-{day}",                // 2026-08-12（年の階層は作らない）
    "{month}/{year}-{month}-{day}",        // 08/2026-08-12
    "{year}/{month}",                      // 2026/08（月単位）
    "{year}",                              // 2026（年単位）
    "",                                    // 振り分けない（コピー先直下）
];

/// フォルダパターンの `{year}` `{month}` `{day}` を置換する（月日はゼロ埋め2桁）。
///
/// 置換後は**必ず** [`sanitize_relative`] を通す。パターンは設定ファイルを
/// 直接編集して壊せてしまうため、コピー先の外へ書き出さないことをここで担保する。
pub fn render_folder_pattern(pattern: &str, date: CivilDate) -> String {
    let rendered = pattern
        .replace("{year}", &format!("{:04}", date.year))
        .replace("{month}", &format!("{:02}", date.month))
        .replace("{day}", &format!("{:02}", date.day));
    sanitize_relative(&rendered)
}

/// パターンから作った相対パスを安全な形へ正規化する。
///
/// - `..` と絶対パス（`/foo` `C:\foo`）を落として**コピー先の外へ出さない**
/// - 各階層からファイル名に使えない文字（`:*?"<>|`）を除く
/// - 空になった階層は詰める
///
/// 危険な入力は「エラーにする」のではなく「無害化する」。取り込みの最中に
/// 設定不備で止まるより、コピー先直下へでも確実に取り込めた方が写真は失われない。
fn sanitize_relative(rendered: &str) -> String {
    /// `C:` のようなWindowsのドライブ指定か（そのまま残すと "C" フォルダになる）。
    fn is_drive_spec(part: &str) -> bool {
        let mut chars = part.chars();
        matches!((chars.next(), chars.next(), chars.next()),
            (Some(c), Some(':'), None) if c.is_ascii_alphabetic())
    }
    rendered
        .split(['/', '\\'])
        .map(|part| part.trim())
        // "." と ".." は階層移動、"C:" はドライブ指定。いずれも相対パスには残さない
        .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !is_drive_spec(part))
        .map(|part| {
            part.chars()
                .filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                .collect::<String>()
        })
        // 末尾のドット・空白はWindowsで開けないフォルダ名になるため落とす
        .map(|part| part.trim_end_matches(['.', ' ']).to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// コピー先パスの決定結果。
pub(crate) enum DestResolution {
    /// このパスへコピーする
    CopyTo(PathBuf),
    /// 同名・同サイズのファイルが既にある（取り込み済み）
    AlreadyImported,
    /// 連番の衝突回避が尽きた（コピーできない＝失敗として扱う）
    Exhausted,
}

/// コピー先のフルパスを決める。同名・別内容の場合は `-1`, `-2` … で衝突回避。
pub(crate) fn resolve_dest_path(dest_dir: &Path, file_name: &str, src_size: u64) -> DestResolution {
    resolve_dest_path_avoiding(dest_dir, file_name, src_size, &HashSet::new())
}

/// 上と同じだが、**この操作で自分が書いたばかりのパス**（`taken`）は
/// 「同じもの」と見なさずに連番へ回す。
///
/// 「同名・同サイズなら同じもの」は取り込みでは成り立つ（日付でフォルダが分かれるので、
/// 別の日の同名ファイルが同じフォルダへ来ない）。**平置きの書き出しでは成り立たない**
/// ——カメラの連番が一周すると別の日の `DSC00001.ARW` が同じフォルダへ落ちるし、
/// 非圧縮RAWは中身が違ってもサイズが同じになる。**選んだ写真が黙って1枚欠ける**ので、
/// 自分が書いたものとの衝突は必ず連番で避ける。
pub(crate) fn resolve_dest_path_avoiding(
    dest_dir: &Path,
    file_name: &str,
    src_size: u64,
    taken: &HashSet<PathBuf>,
) -> DestResolution {
    let candidate = dest_dir.join(file_name);
    if !candidate.exists() && !taken.contains(&candidate) {
        return DestResolution::CopyTo(candidate);
    }
    if !taken.contains(&candidate) && candidate.metadata().map(|m| m.len()).ok() == Some(src_size) {
        return DestResolution::AlreadyImported;
    }
    let (stem, ext) = match (
        Path::new(file_name).file_stem().and_then(|s| s.to_str()),
        Path::new(file_name).extension().and_then(|e| e.to_str()),
    ) {
        (Some(s), Some(e)) => (s.to_string(), format!(".{e}")),
        _ => (file_name.to_string(), String::new()),
    };
    for i in 1..1000 {
        let alt = dest_dir.join(format!("{stem}-{i}{ext}"));
        if taken.contains(&alt) {
            continue;
        }
        if !alt.exists() {
            return DestResolution::CopyTo(alt);
        }
        if alt.metadata().map(|m| m.len()).ok() == Some(src_size) {
            return DestResolution::AlreadyImported;
        }
    }
    DestResolution::Exhausted
}

/// 取り込み元フォルダをスキャンし、設定に従ってコピーする。
///
/// `on_progress(処理済み件数, 総件数, いま処理したファイル)` は1件ごとに呼ばれる。
/// 進捗表示に「今どれを入れているか」を出せるよう、パスも渡す。
pub fn import_from(
    source: &Path,
    config: &Config,
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ImportStats, ImportError> {
    let dest_root = config
        .routing
        .destination
        .as_ref()
        .ok_or(ImportError::NoDestination)?;
    if !source.is_dir() {
        return Err(ImportError::SourceUnreadable(source.to_path_buf()));
    }
    // **取り込み元がパッケージの中**のときは、下の除外では止まらない
    // （`scan_roots` はルート自身を除外判定しない）。ここで断る——
    // 黙って0件を返すと「USBに写真が無い」と同じ見え方になり、
    // 何が起きたのか分からないため
    if is_managed_package_path(source) {
        return Err(ImportError::SourceIsManagedPackage(source.to_path_buf()));
    }

    // 取り込み元の除外はドットフォルダ（.Trashes等）と、アプリが管理する
    // パッケージ（写真.app等）だけ。**固定の一覧**で、ライブラリ用の
    // exclude_patterns はここに適用しない——ユーザーの除外設定が
    // DCIMフォルダに誤マッチして写真を静かに取りこぼす恐れがあるため。
    // パッケージを落とすのは、中身が内部ファイルだから: 外付けHDDに
    // 写真ライブラリがあると、派生JPEGを数千枚**コピーしてしまう**
    let source_buf = source.to_path_buf();
    let exclude: Vec<String> = std::iter::once(".*")
        .chain(scanner::MANAGED_PACKAGE_PATTERNS.iter().copied())
        .map(|p| p.to_string())
        .collect();
    let outcome = scanner::scan_roots(
        std::slice::from_ref(&source_buf),
        &config.import.extensions,
        &exclude,
    );

    let total = outcome.files.len();
    let mut stats = ImportStats {
        // 走査中にエラーがあった（ok_rootsに入らなかった）＝取りこぼしの可能性
        scan_incomplete: !outcome.ok_roots.contains(&source_buf),
        ..ImportStats::default()
    };

    for (i, file) in outcome.files.iter().enumerate() {
        let result = import_one(
            &file.path,
            file.size as u64,
            file.mtime_ms,
            dest_root,
            config,
        );
        match result {
            ImportOneResult::Copied => stats.copied += 1,
            ImportOneResult::Skipped => stats.skipped += 1,
            ImportOneResult::Failed => stats.failed += 1,
        }
        on_progress(i + 1, total, &file.path);
    }
    Ok(stats)
}

/// 選んだファイルだけを取り込む（第5部 段階E: 取り込みウィザード）。
///
/// [`import_from`] と違い**走査しない**。ウィザードが既に一覧を持っており、
/// ユーザーがチェックを外した分を取り込まないことが本質なので、
/// 渡されたパスをそのまま順に処理する。
pub fn import_files(
    files: &[PathBuf],
    config: &Config,
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ImportStats, ImportError> {
    let dest_root = config
        .routing
        .destination
        .as_ref()
        .ok_or(ImportError::NoDestination)?;
    let total = files.len();
    let mut stats = ImportStats::default();
    for (i, path) in files.iter().enumerate() {
        // ウィザードで一覧を出した後にファイルが消えている可能性があるので
        // ここで改めてstatする（読めなければ失敗として数え、他は続行する）。
        //
        // アプリが管理するパッケージの中身も同じ扱いで**1件ずつ落とす**。
        // 一覧の経路（`list_source_dir` / `list_source_tree`）が既に断っているので
        // ここへ来ることは無いはずだが、入口ごとに守る。ただし**一括中止はしない**
        // ——紛れ込んだ1件で1000件の取り込みが丸ごと消えるのは、
        // 読めないファイルを1件ずつ数えるこの関数の作法に合わない
        let result = if is_managed_package_path(path) {
            ImportOneResult::Failed
        } else {
            match std::fs::metadata(path) {
                Ok(meta) => import_one(path, meta.len(), mtime_ms_of(&meta), dest_root, config),
                Err(_) => ImportOneResult::Failed,
            }
        };
        match result {
            ImportOneResult::Copied => stats.copied += 1,
            ImportOneResult::Skipped => stats.skipped += 1,
            ImportOneResult::Failed => stats.failed += 1,
        }
        on_progress(i + 1, total, path);
    }
    Ok(stats)
}

/// メタデータからmtime（Unixエポックミリ秒）を取り出す。取れなければ0。
fn mtime_ms_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// そのファイルが既にコピー先へ取り込まれているか（コピーはしない）。
///
/// ウィザードで「済」バッジを出し、**未取り込みだけを初期選択する**ために使う。
/// 判定は取り込み本体と同じ経路（同じ日付決定＋同じ衝突回避）を通るので、
/// 「済と出たのにもう一度コピーされた」というズレが起きない。
pub fn is_already_imported(path: &Path, config: &Config) -> bool {
    let Some(dest_root) = config.routing.destination.as_ref() else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let dest_dir = dest_dir_for(path, mtime_ms_of(&meta), dest_root, config);
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        resolve_dest_path(&dest_dir, file_name, meta.len()),
        DestResolution::AlreadyImported
    )
}

enum ImportOneResult {
    Copied,
    Skipped,
    Failed,
}

/// このファイルのコピー先フォルダ（作成はしない）。
///
/// 日付決定: 撮影日時 → mtime（正の値のみ。0はメタデータ欠損とみなす）→ 取り込み実行日。
/// mtime=0を有効値として扱うと1970年のフォルダが生まれてしまう。
///
/// 撮影日時の出どころは形式で変わる。**動画はEXIFを持たない**のでコンテナ
///（`moov`）を読む——ここでmtimeへ落とすと、フォルダはカードからコピーした日、
/// 一覧は撮影日になり、**同じファイルの置き場所と表示日がずれる**（第9部）。
fn dest_dir_for(path: &Path, mtime_ms: i64, dest_root: &Path, config: &Config) -> PathBuf {
    let taken_at_ms = if crate::video::is_video_path(path) {
        crate::video::read_info(path).and_then(|i| i.taken_at_ms)
    } else {
        read_exif(path).taken_at_ms
    };
    let date = taken_at_ms
        // 撮影日時を持たないファイルはファイル名に聞く（段階H-2）。
        // ここを mtime へ落とすと、コピー先フォルダが「取り込んだ日」になり、
        // 一覧の表示日（同じ順で名前を見る）とずれる
        .or_else(|| crate::namedate::guess_taken_at(path))
        .and_then(date_from_local_ms)
        .or_else(|| {
            (mtime_ms > 0)
                .then(|| date_from_local_ms(mtime_ms))
                .flatten()
        })
        .unwrap_or_else(today_local);
    dest_root.join(render_folder_pattern(&config.routing.folder_pattern, date))
}

fn import_one(
    path: &Path,
    size: u64,
    mtime_ms: i64,
    dest_root: &Path,
    config: &Config,
) -> ImportOneResult {
    let dest_dir = dest_dir_for(path, mtime_ms, dest_root, config);
    if std::fs::create_dir_all(&dest_dir).is_err() {
        return ImportOneResult::Failed;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return ImportOneResult::Failed;
    };
    let dest_path = match resolve_dest_path(&dest_dir, file_name, size) {
        DestResolution::CopyTo(p) => p,
        DestResolution::AlreadyImported => return ImportOneResult::Skipped,
        // コピーしていないのにSkippedと報告すると「取り込み済み」と誤認される
        DestResolution::Exhausted => return ImportOneResult::Failed,
    };

    match std::fs::copy(path, &dest_path) {
        Ok(copied_bytes) => {
            // Unixのfs::copyはmtimeを保持しないため明示的に引き継ぐ
            // （日付フォルダの振り分けとグリッドの日付グルーピングを一致させる）
            let src_meta = std::fs::metadata(path);
            if let Ok(meta) = &src_meta {
                if let Ok(mtime) = meta.modified() {
                    let _ = filetime::set_file_mtime(
                        &dest_path,
                        filetime::FileTime::from_system_time(mtime),
                    );
                }
            }
            if config.import.verify_after_copy {
                // スキャン時点ではなく「今の」ソースサイズと比較する
                // （スキャン後にファイルが変化したケースで正しいコピーを消さない）
                let src_now = src_meta.map(|m| m.len()).unwrap_or(copied_bytes);
                let dest_len = std::fs::metadata(&dest_path).map(|m| m.len()).ok();
                if dest_len != Some(src_now) {
                    // 検証失敗: 中途半端なファイルを残さない
                    let _ = std::fs::remove_file(&dest_path);
                    return ImportOneResult::Failed;
                }
            }
            ImportOneResult::Copied
        }
        Err(_) => ImportOneResult::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config(dest: &Path) -> Config {
        let mut config = Config::default();
        config.routing.destination = Some(dest.to_path_buf());
        config
    }

    #[test]
    fn パターン置換はゼロ埋めされる() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 5,
        };
        assert_eq!(
            render_folder_pattern("{year}/{year}-{month}-{day}", date),
            "2026/2026-08-05"
        );
        assert_eq!(render_folder_pattern("{year}/{month}", date), "2026/08");
    }

    #[test]
    fn 取り込みで日付フォルダへコピーされる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(src.join("b.jpg"), b"bbbb").unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 2);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.failed, 0);

        // mtimeは今日 → 今日の日付フォルダに入っている
        let copied: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();
        assert_eq!(copied.len(), 2);
        // パターン {year}/{year}-{month}-{day} の2階層下にある
        for p in &copied {
            let rel = p.strip_prefix(&dest).unwrap();
            assert_eq!(rel.components().count(), 3);
        }
    }

    /// **取り込み元そのもの**にパッケージを指定したら断る。
    ///
    /// 取り込み元はネイティブのフォルダ選択ダイアログで選ぶので、一覧から
    /// 隠しても名指しで選べる。`scan_roots` はルート自身を除外判定しないため、
    /// ここで止めないと内部の派生画像を全部コピーする
    #[test]
    fn 取り込み元そのものがパッケージなら断る() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("写真ライブラリ.photoslibrary");
        let dest = dir.path().join("photos");
        let inner = src.join("resources/derivatives");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("derived.jpg"), b"xxx").unwrap();

        let config = test_config(&dest);
        let err = import_from(&src, &config, |_, _, _| {}).unwrap_err();
        assert!(
            matches!(err, ImportError::SourceIsManagedPackage(_)),
            "err={err}"
        );
        // 1枚もコピーしていない
        assert!(!dest.exists());

        // **中のフォルダを名指しされても断る**。ネイティブのダイアログは
        // パッケージの中へ入って選べるので、葉の名前だけでは守れない
        let err = import_from(&inner, &config, |_, _, _| {}).unwrap_err();
        assert!(
            matches!(err, ImportError::SourceIsManagedPackage(_)),
            "err={err}"
        );
        assert!(!dest.exists());
    }

    #[test]
    fn パッケージの中を指していても判る() {
        assert!(is_managed_package_path(Path::new(
            "/x/写真ライブラリ.photoslibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            r"E:\Photos Library.photoslibrary"
        )));
        // **中を名指しした場合**。葉の名前だけの判定ではここが抜ける
        // （ネイティブのダイアログはパッケージの中へ入って選べる）
        assert!(is_managed_package_path(Path::new(
            "/x/写真ライブラリ.photoslibrary/originals/0"
        )));

        // 写真.appの系譜はまとめて落とす（どれも `~/Pictures` に住む）
        assert!(is_managed_package_path(Path::new(
            "/x/iPhoto Library.photolibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            "/x/iPhoto Library.migratedphotolibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            "/x/Aperture Library.aplibrary"
        )));

        assert!(!is_managed_package_path(Path::new("/x/DCIM")));
        assert!(!is_managed_package_path(Path::new("/x/photoslibrary/a")));
    }

    /// アプリが管理するパッケージの中身は取り込まない。
    ///
    /// ここは**ファイルをコピーする**経路なので、漏らすと索引より重い
    /// ——外付けHDDに写真ライブラリがあると、内部の派生JPEGを数千枚
    /// コピー先へ書いてしまう
    #[test]
    fn 写真ライブラリのパッケージは取り込まない() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        let pkg = src.join("写真ライブラリ.photoslibrary/resources/derivatives");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("derived.jpg"), b"xxx").unwrap();
        fs::create_dir_all(src.join("DCIM")).unwrap();
        fs::write(src.join("DCIM/a.jpg"), b"aaa").unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 1, "DCIMの1枚だけ");

        let copied: Vec<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(copied, vec!["a.jpg"]);
    }

    #[test]
    fn 同名同サイズはスキップされる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        let config = test_config(&dest);
        let first = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(first.copied, 1);

        // 再取り込み → スキップ
        let second = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.skipped, 1);
    }

    #[test]
    fn 同名別サイズは連番で衝突回避される() {
        let dir = tempfile::tempdir().unwrap();
        let src1 = dir.path().join("usb1");
        let src2 = dir.path().join("usb2");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src1).unwrap();
        fs::create_dir_all(&src2).unwrap();
        // 同じファイル名で中身が違う（別のカメラのDSC_0001.jpg想定）
        fs::write(src1.join("DSC_0001.jpg"), b"first").unwrap();
        fs::write(src2.join("DSC_0001.jpg"), b"second-longer").unwrap();

        let config = test_config(&dest);
        assert_eq!(import_from(&src1, &config, |_, _, _| {}).unwrap().copied, 1);
        assert_eq!(import_from(&src2, &config, |_, _, _| {}).unwrap().copied, 1);

        let names: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"DSC_0001.jpg".to_string()));
        assert!(names.contains(&"DSC_0001-1.jpg".to_string()));
    }

    #[test]
    fn mtime欠損のファイルは1970ではなく今日のフォルダへ入る() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let f = src.join("broken.jpg");
        fs::write(&f, b"data").unwrap();
        // mtime=Unixエポック0（壊れたFATタイムスタンプ相当）
        filetime::set_file_mtime(&f, filetime::FileTime::from_unix_time(0, 0)).unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 1);
        let year_dir = fs::read_dir(&dest).unwrap().next().unwrap().unwrap();
        let name = year_dir.file_name().to_string_lossy().into_owned();
        assert_ne!(name, "1970", "エポック0は日付として扱わない");
        assert_ne!(name, "1969");
    }

    #[test]
    fn コピー後のmtimeはソースと一致する() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let f = src.join("old.jpg");
        fs::write(&f, b"data").unwrap();
        let past = filetime::FileTime::from_unix_time(1_600_000_000, 0); // 2020-09
        filetime::set_file_mtime(&f, past).unwrap();

        let config = test_config(&dest);
        assert_eq!(import_from(&src, &config, |_, _, _| {}).unwrap().copied, 1);
        let copied = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .find(|e| e.file_type().is_file())
            .unwrap();
        let copied_mtime =
            filetime::FileTime::from_last_modification_time(&copied.metadata().unwrap());
        assert_eq!(copied_mtime.unix_seconds(), past.unix_seconds());
    }

    #[test]
    fn コピー先未設定はエラー() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let result = import_from(dir.path(), &config, |_, _, _| {});
        assert!(matches!(result, Err(ImportError::NoDestination)));
    }

    #[test]
    fn 進捗コールバックが呼ばれる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        for i in 0..3 {
            fs::write(src.join(format!("p{i}.jpg")), b"x").unwrap();
        }
        let config = test_config(&dest);
        let calls = std::sync::Mutex::new(Vec::new());
        import_from(&src, &config, |done, total, path| {
            calls
                .lock()
                .unwrap()
                .push((done, total, path.file_name().unwrap().to_owned()));
        })
        .unwrap();
        let calls = calls.lock().unwrap();
        // 件数の進みと「いま処理したファイル」が毎回そろって届く
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls.iter().map(|(d, t, _)| (*d, *t)).collect::<Vec<_>>(),
            vec![(1, 3), (2, 3), (3, 3)]
        );
        let mut names: Vec<_> = calls
            .iter()
            .map(|(_, _, n)| n.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["p0.jpg", "p1.jpg", "p2.jpg"]);
    }

    #[test]
    fn 選んだファイルだけを取り込む() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            fs::write(src.join(name), b"x").unwrap();
        }

        let config = test_config(&dest);
        // チェックを外した c.jpg は渡さない → コピーされない
        let picked = vec![src.join("a.jpg"), src.join("b.jpg")];
        let stats = import_files(&picked, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 2);
        let names: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(!names.contains(&"c.jpg".to_string()));
    }

    #[test]
    fn 消えたファイルは失敗として数え残りは続行する() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"x").unwrap();

        let config = test_config(&dest);
        let picked = vec![src.join("消えた.jpg"), src.join("a.jpg")];
        let stats = import_files(&picked, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.copied, 1, "1件失敗しても後続は取り込む");
    }

    #[test]
    fn 取り込み済み判定は取り込み本体と一致する() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let a = src.join("a.jpg");
        fs::write(&a, b"aaa").unwrap();

        let config = test_config(&dest);
        assert!(!is_already_imported(&a, &config), "取り込む前は未取り込み");
        import_files(std::slice::from_ref(&a), &config, |_, _, _| {}).unwrap();
        assert!(is_already_imported(&a, &config), "取り込んだ後は済");
        // 「済」と出るファイルを再度取り込んでもコピーは増えない
        let again = import_files(std::slice::from_ref(&a), &config, |_, _, _| {}).unwrap();
        assert_eq!(again.copied, 0);
        assert_eq!(again.skipped, 1);
    }

    #[test]
    fn コピー先未設定なら取り込み済み判定はfalse() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.jpg");
        fs::write(&f, b"x").unwrap();
        assert!(!is_already_imported(&f, &Config::default()));
    }

    #[test]
    fn プリセットは全て安全な相対パスを作る() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 12,
        };
        let rendered: Vec<String> = FOLDER_PATTERN_PRESETS
            .iter()
            .map(|p| render_folder_pattern(p, date))
            .collect();
        assert_eq!(rendered[0], "2026/2026-08-12", "既定は年/年-月-日");
        assert_eq!(rendered[1], "2026/08/12");
        assert_eq!(rendered[2], "2026/08-12");
        assert_eq!(rendered[3], "2026-08/2026-08-12");
        // 年の階層を自分で作る人向け（コピー先を年フォルダにして、この下に日付だけ）
        assert_eq!(rendered[4], "2026-08-12");
        assert_eq!(rendered[5], "08/2026-08-12");
        assert_eq!(rendered[6], "2026/08");
        assert_eq!(rendered[7], "2026");
        assert_eq!(rendered[8], "", "空パターンはコピー先直下");
        assert_eq!(rendered.len(), FOLDER_PATTERN_PRESETS.len());
        for r in &rendered {
            assert!(!Path::new(r).is_absolute(), "絶対パスにならない: {r}");
        }
    }

    #[test]
    fn 危険なパターンは無害化される() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 12,
        };
        // 設定ファイルを直接編集して壊せてしまうため、コピー先の外へ出さない
        assert_eq!(render_folder_pattern("../../{year}", date), "2026");
        assert_eq!(render_folder_pattern("/etc/{year}", date), "etc/2026");
        assert_eq!(render_folder_pattern(r"C:\evil\{year}", date), "evil/2026");
        assert_eq!(render_folder_pattern("{year}/../..", date), "2026");
        // ファイル名に使えない文字は落とす
        assert_eq!(render_folder_pattern("a*b?{year}", date), "ab2026");
        // Windowsで開けなくなる「末尾のドット・空白」も落とす
        assert_eq!(render_folder_pattern("{year}. / x ", date), "2026/x");
        // 全部落ちたら空（コピー先直下へ取り込む。止めるより失わない方を選ぶ）
        assert_eq!(render_folder_pattern("../..", date), "");
    }
}
