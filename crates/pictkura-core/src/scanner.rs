//! 爆速ディレクトリスキャナー。
//!
//! 爆速の原則:
//! - 除外パターンに一致したディレクトリは丸ごと枝刈り（中に入らない）
//! - 収集するのはOSのメタデータ（サイズ・mtime）のみ。ファイルの中身は一切読まない
//! - 更新検知はサイズとmtimeの比較のみ（ハッシュ計算は禁止）
//!
//! 安全の原則:
//! - ルート単位で走査の成否を記録する。読めなかったルート（USB切断・権限エラー等）の
//!   配下は「削除された」と誤判定せず、DBのレコードを保持する。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

/// スキャンで見つかった1ファイルのメタデータ。
#[derive(Debug, Clone, PartialEq)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub size: i64,
    /// 更新日時（Unixエポックミリ秒）
    pub mtime_ms: i64,
}

/// スキャン結果。ルート単位の走査成否を持つ。
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub files: Vec<ScannedFile>,
    /// 1件のエラーもなく走査しきれたルート。
    /// ここに含まれないルートの配下は削除判定の対象にしない。
    pub ok_roots: Vec<PathBuf>,
}

/// アプリが管理するパッケージ。**見た目はフォルダだが中身は内部ファイル**。
///
/// 効き方が2種類あるので、混同しないこと:
///
/// - **入口では固定**（利用者の設定に関係なく断る）。ライブラリのルート登録、
///   コピー先の設定、取り込み元の指定と一覧がこれ。パッケージを**ルートに
///   名指しされた**ときだけは、走査の「ルート自身は除外判定しない」例外を
///   打ち消す必要がある——索引はできても監視とUSNは更新を全部落とすので、
///   **索引されたまま永久に古い**という壊れ方をするため
/// - **走査の途中では設定に従う**。既定の `exclude_patterns` に同じものが
///   入っており、利用者が消せば配下のパッケージは索引される（`.*` を消せる
///   のと同じ扱い）。ここを固定にすると、TOMLの手編集という唯一の逃げ道が
///   塞がる
///
/// 取り込み元側が固定なのは、利用者の除外設定を取り込み元へ流用すると
/// DCIMに誤マッチして「USBに写真が無い」ように見える事故になるため。
/// **1箇所にまとめてあるのは片方だけに足す事故を防ぐため**——取り込み元は
/// ファイルを**コピーする**経路なので、漏らすと索引より重い結果になる。
/// 一覧はAppleの写真アプリの系譜。**どれも既定のルート（`~/Pictures`）に住む**ので、
/// 事故の形は完全に同じ。**タグを打つ前でないと既定に足す意味が薄れる**
/// （初回起動で設定が固まり、走査途中の除外は既存の環境へ届かなくなる）ため、
/// 実物を踏んだのは写真.appだけだが、同じ系譜はまとめて入れた。
pub const MANAGED_PACKAGE_PATTERNS: &[&str] = &[
    // 写真.app（現行）。実測で 14,938件・サムネイル5,399枚を索引した
    "*.photoslibrary",
    // iPhoto（写真.appへ移行すると .migratedphotolibrary になる）
    "*.photolibrary",
    "*.migratedphotolibrary",
    // Aperture
    "*.aplibrary",
];

/// パス上のどこかが、アプリが管理するパッケージか（**全構成要素**を見る）。
///
/// 葉の名前だけでは足りない。ルートも取り込み元もネイティブのフォルダ選択
/// ダイアログで選ぶので、`E:\Photos Library.photoslibrary\originals` のように
/// **中を名指しできる**ため。
///
/// これは利用者が編集できる `exclude_patterns` とは**別の固定の判定**で、
/// 走査が「ルート自身は除外判定しない」としている例外（`.*` で始まるフォルダを
/// ルートに指定したい人のため）を、パッケージにだけは適用しないために使う。
/// 索引だけできても、監視とUSNは全構成要素を見るので更新が永久に届かず、
/// **索引されたまま古い**という壊れた状態になる。
pub fn is_managed_package_path(path: &Path) -> bool {
    path.components().any(|c| match c {
        std::path::Component::Normal(name) => name.to_str().is_some_and(|n| {
            MANAGED_PACKAGE_PATTERNS
                .iter()
                .any(|p| matches_pattern(n, p))
        }),
        _ => false,
    })
}

/// 名前が除外パターンに一致するか。
/// パターンは `*` をワイルドカードとする単純グロブ（例: `.*`, `Thumbs.db`, `*.tmp`）。
/// Windowsのファイル名は大文字小文字を区別しないため、比較は小文字化して行う。
/// 反復2ポインタ法でO(名前長×パターン長)を保証する（再帰バックトラックの指数爆発を回避）。
pub fn matches_pattern(name: &str, pattern: &str) -> bool {
    matches_pattern_lower(&name.to_lowercase(), &pattern.to_lowercase())
}

/// [`matches_pattern`] の中身。**両方とも小文字化済み**であることが前提。
///
/// 走査の内側のループは1エントリごとにパターンの数だけここを通る。
/// 素の [`matches_pattern`] は呼ぶたびに両側を `to_lowercase()` するので、
/// 既定が7パターンあると1エントリで14回の確保になる——
/// 「爆速」を掲げる枝刈りの高速路に乗せる重さではない。
/// 呼び出し側はパターンを一度だけ小文字化し、名前も1回で済ませること。
fn matches_pattern_lower(name: &str, pattern: &str) -> bool {
    let (n, p) = (name.as_bytes(), pattern.as_bytes());
    let (mut ni, mut pi) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == n[ni]) {
            ni += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(sp) = star {
            // 直前の '*' にもう1文字吸わせてやり直す
            pi = sp + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// 除外パターンを小文字化して持ち直す（走査の前に一度だけ）。
fn lower_patterns(patterns: &[String]) -> Vec<String> {
    patterns.iter().map(|p| p.to_lowercase()).collect()
}

/// 拡張子（小文字）が対象リストに含まれるか。
pub fn has_target_extension(path: &Path, extensions: &[String]) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)),
        None => false,
    }
}

/// パスのいずれかの構成要素が除外パターンに一致するか（ウォッチャーのイベント判定用）。
pub fn is_excluded_path(path: &Path, exclude_patterns: &[String]) -> bool {
    path.components().any(|c| match c {
        std::path::Component::Normal(name) => name
            .to_str()
            .is_some_and(|n| exclude_patterns.iter().any(|p| matches_pattern(n, p))),
        _ => false,
    })
}

/// ルート群を走査し、対象拡張子のファイルのメタデータを収集する。
///
/// - `exclude_patterns` に一致するディレクトリは丸ごとスキップ（枝刈り）
/// - `exclude_patterns` に一致するファイルもスキップ
/// - 走査中にエラーが1件でもあったルートは `ok_roots` に入れない
///   （そのルート配下の削除判定を保留させるため）
pub fn scan_roots(
    roots: &[PathBuf],
    extensions: &[String],
    exclude_patterns: &[String],
) -> ScanOutcome {
    // パターンは一度だけ小文字化する（内側のループで確保を繰り返さない）
    let lowered = lower_patterns(exclude_patterns);
    let excluded = |name: &str| {
        let name = name.to_lowercase();
        lowered.iter().any(|p| matches_pattern_lower(&name, p))
    };
    let mut outcome = ScanOutcome::default();

    for root in roots {
        let mut had_error = !root.is_dir();
        if !had_error {
            let walker = WalkDir::new(root).into_iter().filter_entry(|e| {
                // ルート自身は除外判定しない（ドット始まりのルート指定などを許す）。
                //
                // **ここに固定のパッケージ判定は置かない。** この関数に渡るのは
                // 設定されたルートではなく、監視が拾った「移動されてきたフォルダ」
                // （`lib.rs`）と取り込み元（そちらは呼ぶ手前で判定済み）。
                // ここで固定判定をすると、opt-outした人の監視経路だけが索引せず、
                // 次のフルスキャンは索引する、という食い違いになる。
                // 設定されたルートの判定は `scan_roots_pruned` 側にある
                if e.depth() == 0 {
                    return true;
                }
                match e.file_name().to_str() {
                    Some(name) => !excluded(name),
                    None => true,
                }
            });

            for entry in walker {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        had_error = true;
                        continue;
                    }
                };
                if !entry.file_type().is_file() {
                    continue;
                }
                if !has_target_extension(entry.path(), extensions) {
                    continue;
                }
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => {
                        had_error = true;
                        continue;
                    }
                };
                let mtime_ms = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                outcome.files.push(ScannedFile {
                    path: entry.into_path(),
                    size: meta.len() as i64,
                    mtime_ms,
                });
            }
        }
        if !had_error {
            outcome.ok_roots.push(root.clone());
        }
    }
    outcome
}

/// 枝刈り付きスキャンの結果（plan.md 第3部 段階B-2）。
#[derive(Debug, Default)]
pub struct PrunedScanOutcome {
    /// 実際に列挙したディレクトリで見つかったファイル。
    /// スキップしたディレクトリ配下のファイルは**含まれない**（未変更とみなす）
    pub files: Vec<ScannedFile>,
    /// 実際に中身を列挙したディレクトリ（mtimeが変わった・新規・枝刈り情報なし）。
    /// 削除判定はこのディレクトリ直下のファイルに限定される
    pub enumerated_dirs: Vec<PathBuf>,
    /// 存在を確認した全ディレクトリとそのmtime（スキップ分も含む。dirsテーブル更新用）
    pub seen_dirs: Vec<(PathBuf, i64)>,
    /// mtime一致でファイル列挙をスキップしたディレクトリ数（統計・爆速メーター用）
    pub skipped_dirs: usize,
    /// 1件のエラーもなく走査しきれたルート
    pub ok_roots: Vec<PathBuf>,
}

/// ディレクトリmtimeによる枝刈り付きスキャン（段階B-2）。
///
/// `known_dirs`（前回スキャンで記録したディレクトリ→mtime）と一致するディレクトリは
/// ファイルの列挙・statをスキップし、既知の子ディレクトリへの再帰だけを行う。
/// NTFS等では直下のファイル・フォルダの追加/削除/改名が親のmtimeを変えるため、
/// mtime一致なら「直下の名前の集合は不変」= 子ディレクトリ一覧はDBの記録と一致する。
///
/// **既知の限界**（フルスキャンで補う前提。手動の「今すぐ同期」は常にフル）:
/// - ファイルの**内容だけの上書き**は親ディレクトリのmtimeを変えないため検知できない
///   （起動中の変更はファイルシステム監視が拾う）
/// - FAT/exFAT はディレクトリmtimeの更新規則が緩く、枝刈りの前提が成り立たないことがある
///
/// `known_dirs` が空ならすべて列挙される（＝フルスキャンと等価）。
pub fn scan_roots_pruned(
    roots: &[PathBuf],
    extensions: &[String],
    exclude_patterns: &[String],
    known_dirs: &HashMap<PathBuf, i64>,
) -> PrunedScanOutcome {
    // パターンは一度だけ小文字化する（内側のループで確保を繰り返さない）
    let lowered = lower_patterns(exclude_patterns);
    let excluded = |name: &std::ffi::OsStr| {
        name.to_str().is_some_and(|n| {
            let n = n.to_lowercase();
            lowered.iter().any(|p| matches_pattern_lower(&n, p))
        })
    };
    // 既知ディレクトリの親→子インデックス（スキップ時の再帰先）
    let mut children: HashMap<&Path, Vec<&PathBuf>> = HashMap::new();
    for dir in known_dirs.keys() {
        if let Some(parent) = dir.parent() {
            children.entry(parent).or_default().push(dir);
        }
    }

    let mut outcome = PrunedScanOutcome::default();
    for root in roots {
        let mut had_error = false;
        // ルート自身の判定は**ここでだけ**行う（`scan_roots` の `depth 0` と同じ）。
        // `walk_pruned` の中に置くと再帰のたびに全構成要素を舐めることになり、
        // mtime枝刈りの高速路（`metadata` 1回）に釣り合わない。
        // 配下でパッケージに出会う場合は `excluded`（利用者の除外パターン。
        // 既定に入っている）が名前だけで落とす
        if !is_managed_package_path(root) {
            walk_pruned(
                root,
                false,
                extensions,
                &excluded,
                known_dirs,
                &children,
                &mut outcome,
                &mut had_error,
            );
        }
        // 「読めなかった」ではなく「見ないと決めた」なので had_error は立てず、
        // ok_roots に入れる（既に索引されていた行は apply_scan が掃き出す）
        if !had_error {
            outcome.ok_roots.push(root.clone());
        }
    }
    outcome
}

/// ダーティディレクトリ群だけを枝刈り付きで走査する（段階B-1のUSN差分反映用）。
///
/// 各ダーティディレクトリ自身は、mtimeが既知の記録と一致していても**必ず列挙する**
/// （ファイル内容だけの上書きは親ディレクトリのmtimeを変えないため、
/// USNが変更ありと言っている以上は中身を確かめる必要がある）。
/// 配下の子ディレクトリは通常の枝刈りルールで走査される。
///
/// 戻り値の bool は「1件のエラーもなく走査できたか」。false の場合、
/// 呼び出し側は結果を捨ててフルスキャンへフォールバックすること
/// （部分反映は走査失敗時の保護ルール（ok_roots）を持たないため）。
pub fn scan_dirty_dirs(
    dirty_dirs: &[PathBuf],
    extensions: &[String],
    exclude_patterns: &[String],
    known_dirs: &HashMap<PathBuf, i64>,
) -> (PrunedScanOutcome, bool) {
    // パターンは一度だけ小文字化する（内側のループで確保を繰り返さない）
    let lowered = lower_patterns(exclude_patterns);
    let excluded = |name: &std::ffi::OsStr| {
        name.to_str().is_some_and(|n| {
            let n = n.to_lowercase();
            lowered.iter().any(|p| matches_pattern_lower(&n, p))
        })
    };
    let mut children: HashMap<&Path, Vec<&PathBuf>> = HashMap::new();
    for dir in known_dirs.keys() {
        if let Some(parent) = dir.parent() {
            children.entry(parent).or_default().push(dir);
        }
    }

    let mut outcome = PrunedScanOutcome::default();
    let mut had_error = false;
    // **ここでは固定の判定を当てない。** dirty dir は利用者が設定したルートではなく、
    // ジャーナルが報告してきた「走査の途中のディレクトリ」なので、
    // 設定に従う側（`excluded`）だけが効く。当ててしまうと、
    // `*.photoslibrary` を消して opt-out した人の差分更新だけが黙って捨てられ、
    // フルスキャンでは索引されるのに差分では古いまま、という食い違いになる。
    // 呼び出し元（`lib.rs`）が `is_excluded_path` で設定どおりに絞り込み済み
    for dir in dirty_dirs {
        walk_pruned(
            dir,
            true,
            extensions,
            &excluded,
            known_dirs,
            &children,
            &mut outcome,
            &mut had_error,
        );
    }
    (outcome, !had_error)
}

/// 1ディレクトリ分の枝刈り走査（scan_roots_pruned / scan_dirty_dirsの本体）。
/// `force_enumerate` はこのディレクトリ自身のmtime枝刈りを無効化する（再帰先には効かない）。
#[allow(clippy::too_many_arguments)]
fn walk_pruned(
    dir: &Path,
    force_enumerate: bool,
    extensions: &[String],
    excluded: &impl Fn(&std::ffi::OsStr) -> bool,
    known_dirs: &HashMap<PathBuf, i64>,
    children: &HashMap<&Path, Vec<&PathBuf>>,
    outcome: &mut PrunedScanOutcome,
    had_error: &mut bool,
) {
    let mtime_ms = match std::fs::metadata(dir) {
        Ok(m) if m.is_dir() => mtime_of(&m),
        _ => {
            *had_error = true;
            return;
        }
    };

    // mtime一致 → 直下は不変。ファイルを見ずに既知の子ディレクトリだけへ再帰する
    if !force_enumerate && known_dirs.get(dir) == Some(&mtime_ms) {
        // スキップ時の記録は前回記録した値の再確認なので安全
        outcome.seen_dirs.push((dir.to_path_buf(), mtime_ms));
        outcome.skipped_dirs += 1;
        if let Some(subdirs) = children.get(dir) {
            for sub in subdirs {
                // 除外パターンは現在の設定で再判定する（設定変更後の入り込み防止）
                if sub.file_name().is_some_and(excluded) {
                    continue;
                }
                walk_pruned(
                    sub, false, extensions, excluded, known_dirs, children, outcome, had_error,
                );
            }
        }
        return;
    }

    // mtime不一致・新規 → 実際に列挙する
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            *had_error = true;
            return;
        }
    };
    // このディレクトリ自身の列挙が1件でも欠けたら、mtimeを記録しない（dir_error）。
    // 欠けたまま記録すると次回から「変更なし」と誤判定され、拾えなかった
    // ファイルが手動フルスキャンまで永久に見つからなくなる（枝刈りキャッシュの汚染）
    let mut dir_error = false;
    for entry in entries {
        let Ok(entry) = entry else {
            dir_error = true;
            continue;
        };
        if excluded(&entry.file_name()) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            dir_error = true;
            continue;
        };
        // シンボリックリンクは辿らない（walkdirの既定と同じ。循環防止）
        if file_type.is_dir() {
            // 子ディレクトリ内のエラーはこのディレクトリの記録には影響しない
            // （子は子で自分の記録を保留する）
            walk_pruned(
                &entry.path(),
                false,
                extensions,
                excluded,
                known_dirs,
                children,
                outcome,
                had_error,
            );
        } else if file_type.is_file() {
            let path = entry.path();
            if !has_target_extension(&path, extensions) {
                continue;
            }
            match entry.metadata() {
                Ok(meta) => outcome.files.push(ScannedFile {
                    path,
                    size: meta.len() as i64,
                    mtime_ms: mtime_of(&meta),
                }),
                Err(_) => dir_error = true,
            }
        }
    }
    if dir_error {
        *had_error = true;
        // seen/enumeratedに載せない: mtime未記録なら次回また列挙される（安全側）。
        // 削除判定も「列挙したディレクトリ」に限られるため、このディレクトリ直下は
        // 誤削除されない（フルスキャンではok_rootsの保護も重なる）
    } else {
        outcome.seen_dirs.push((dir.to_path_buf(), mtime_ms));
        outcome.enumerated_dirs.push(dir.to_path_buf());
    }
}

/// メタデータからUnixエポックミリ秒のmtimeを取り出す（取得不能は0）。
fn mtime_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(dir: &Path, rel: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn jpg_extensions() -> Vec<String> {
        vec!["jpg".into(), "jpeg".into()]
    }

    #[test]
    fn pattern_matching_basics() {
        assert!(matches_pattern(".hidden", ".*"));
        assert!(matches_pattern("Thumbs.db", "thumbs.db"));
        assert!(matches_pattern("foo.tmp", "*.tmp"));
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("a.b.c.tmp", "*.tmp"));
        assert!(matches_pattern("abc", "a*b*c"));
        assert!(!matches_pattern("visible", ".*"));
        assert!(!matches_pattern("foo.jpg", "*.tmp"));
        assert!(!matches_pattern("ab", "a*b*c"));
    }

    #[test]
    fn even_a_hostile_pattern_finishes_fast() {
        // 旧再帰実装では指数時間になっていた形
        let name = "a".repeat(60);
        let pattern = format!("{}b", "a*".repeat(30));
        let start = std::time::Instant::now();
        assert!(!matches_pattern(&name, &pattern));
        assert!(start.elapsed().as_millis() < 200);
    }

    #[test]
    fn only_files_with_a_wanted_extension_are_collected() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.jpg", b"aaa");
        write_file(dir.path(), "b.JPG", b"bbbb");
        write_file(dir.path(), "c.txt", b"ccc");
        write_file(dir.path(), "sub/d.jpeg", b"dd");

        let outcome = scan_roots(&[dir.path().to_path_buf()], &jpg_extensions(), &[]);
        let mut names: Vec<_> = outcome
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a.jpg", "b.JPG", "d.jpeg"]);
        assert_eq!(outcome.ok_roots, vec![dir.path().to_path_buf()]);
        let a = outcome
            .files
            .iter()
            .find(|f| f.path.file_name().unwrap() == "a.jpg")
            .unwrap();
        assert_eq!(a.size, 3);
        assert!(a.mtime_ms > 0);
    }

    #[test]
    fn a_root_that_does_not_exist_stays_out_of_ok_roots() {
        let missing = PathBuf::from("Z:/no/such/root");
        let outcome = scan_roots(std::slice::from_ref(&missing), &jpg_extensions(), &[]);
        assert!(outcome.files.is_empty());
        assert!(outcome.ok_roots.is_empty());
    }

    #[test]
    fn an_exclude_pattern_prunes_the_whole_directory() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "keep/a.jpg", b"a");
        write_file(dir.path(), ".git/b.jpg", b"b");
        write_file(dir.path(), "backup/c.jpg", b"c");
        write_file(dir.path(), "keep/Thumbs.db", b"t");

        let outcome = scan_roots(
            &[dir.path().to_path_buf()],
            &jpg_extensions(),
            &[".*".into(), "backup".into(), "thumbs.db".into()],
        );
        assert_eq!(outcome.files.len(), 1);
        let p = &outcome.files[0].path;
        assert!(p.ends_with("keep/a.jpg") || p.ends_with("keep\\a.jpg"));
    }

    /// 既定の除外が写真.appのパッケージを弾くこと。**実際に走査させて**確かめる。
    ///
    /// 実測で踏んだ事故（14,938件を索引し、サムネイルを5,399枚作った）の回帰試験。
    /// `matches_pattern` や `is_excluded_path` を直接叩くのでは、
    /// 本番が通る `filter_entry` の枝が変わったときに緑のまま通ってしまう。
    /// **OS非依存**——Windowsでも外付けHDD経由で同じ名前のフォルダに出会う
    #[test]
    fn the_default_excludes_keep_photo_library_packages_out() {
        let dir = tempfile::tempdir().unwrap();
        // パッケージの中身（日本語環境と英語環境の両方の名前）
        write_file(
            dir.path(),
            "写真ライブラリ.photoslibrary/originals/0/x.jpg",
            b"x",
        );
        write_file(
            dir.path(),
            "Photos Library.photoslibrary/resources/derivatives/y.jpg",
            b"y",
        );
        // 普通の写真と、拡張子ではない同名フォルダは通す
        write_file(dir.path(), "2020/a.jpg", b"a");
        write_file(dir.path(), "photoslibrary/b.jpg", b"b");

        let patterns = crate::config::LibraryConfig::default().exclude_patterns;
        let outcome = scan_roots(&[dir.path().to_path_buf()], &jpg_extensions(), &patterns);

        let mut names: Vec<String> = outcome
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.jpg", "b.jpg"]);
    }

    /// **ルートに指定されても**索引しない。
    ///
    /// 走査は「ルート自身は除外判定しない」を通例にしている（`.*` で始まる
    /// フォルダをルートにしたい人のため）が、パッケージだけは例外。
    /// 設定ファイルへ直接書かれた場合もここを通る。索引だけできても
    /// 監視とUSNが更新を全部落とすので、永久に古い状態になるだけだから。
    ///
    /// 判定があるのは**設定されたルートを受け取る `scan_roots_pruned`** の側。
    /// `scan_roots` には置かない（あちらに渡るのは監視が拾った移動先フォルダと
    /// 取り込み元で、どちらも設定されたルートではない）
    #[test]
    fn a_package_is_not_indexed_even_when_named_as_a_root() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "写真ライブラリ.photoslibrary/originals/0/x.jpg",
            b"x",
        );
        let pkg = dir.path().join("写真ライブラリ.photoslibrary");
        let patterns = crate::config::LibraryConfig::default().exclude_patterns;

        let known = HashMap::new();
        let pruned = scan_roots_pruned(
            std::slice::from_ref(&pkg),
            &jpg_extensions(),
            &patterns,
            &known,
        );
        assert!(pruned.files.is_empty());
        assert!(
            pruned.seen_dirs.is_empty(),
            "見ないと決めたので記録もしない"
        );
        // 「読めなかった」ではないので掃き出しは効く（ok_rootsに入る）
        assert_eq!(pruned.ok_roots, vec![pkg.clone()]);

        // パッケージの**中**をルートにしても同じ
        let inner = pkg.join("originals");
        let pruned = scan_roots_pruned(
            std::slice::from_ref(&inner),
            &jpg_extensions(),
            &patterns,
            &known,
        );
        assert!(pruned.files.is_empty());
        assert_eq!(pruned.ok_roots, vec![inner]);
    }

    // 差分検知（追加・変更・削除、ルート成否による保持）のテストは
    // SQL化に伴い db.rs の apply_scan 系テストへ移動した

    /// seen_dirs を known_dirs 形式（パス→mtime）へ変換する。
    fn to_known(outcome: &PrunedScanOutcome) -> HashMap<PathBuf, i64> {
        outcome.seen_dirs.iter().cloned().collect()
    }

    #[test]
    fn a_pruned_scan_with_nothing_to_prune_returns_what_a_full_scan_does() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.jpg", b"aaa");
        write_file(dir.path(), "sub/b.jpg", b"bb");
        write_file(dir.path(), ".git/c.jpg", b"c");

        let roots = vec![dir.path().to_path_buf()];
        let excludes = vec![".*".to_string()];
        let full = scan_roots(&roots, &jpg_extensions(), &excludes);
        let pruned = scan_roots_pruned(&roots, &jpg_extensions(), &excludes, &HashMap::new());

        let names = |files: &[ScannedFile]| {
            let mut v: Vec<_> = files
                .iter()
                .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(&pruned.files), names(&full.files));
        assert_eq!(pruned.ok_roots, full.ok_roots);
        assert_eq!(pruned.skipped_dirs, 0, "既知情報なし → 全列挙");
        // ルートとsubの2ディレクトリを列挙・記録（.gitは除外）
        assert_eq!(pruned.enumerated_dirs.len(), 2);
        assert_eq!(pruned.seen_dirs.len(), 2);
    }

    #[test]
    fn a_directory_with_a_matching_mtime_skips_listing_its_files() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "stable/a.jpg", b"aaa");
        write_file(dir.path(), "stable/deep/b.jpg", b"bb");
        let roots = vec![dir.path().to_path_buf()];

        let first = scan_roots_pruned(&roots, &jpg_extensions(), &[], &HashMap::new());
        assert_eq!(first.files.len(), 2);

        // 変更なしの再スキャン: 全ディレクトリがスキップされ、ファイルは1件も返らない
        let known = to_known(&first);
        let second = scan_roots_pruned(&roots, &jpg_extensions(), &[], &known);
        assert!(
            second.files.is_empty(),
            "未変更ディレクトリのファイルは読まない"
        );
        assert_eq!(second.skipped_dirs, 3, "root/stable/deep の3つ");
        assert!(second.enumerated_dirs.is_empty());
        // スキップしてもseen_dirsには全ディレクトリが載る（dirsテーブル維持用）
        assert_eq!(second.seen_dirs.len(), 3);
        assert_eq!(second.ok_roots, roots);
    }

    #[test]
    fn only_changed_directories_are_listed_again() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "stable/a.jpg", b"aaa");
        write_file(dir.path(), "hot/b.jpg", b"bb");
        let roots = vec![dir.path().to_path_buf()];
        let known = to_known(&scan_roots_pruned(
            &roots,
            &jpg_extensions(),
            &[],
            &HashMap::new(),
        ));

        // hot にファイルを追加（ディレクトリmtimeが変わる）。
        // ファイルシステムのmtime分解能でも差が出るよう明示的に進める
        std::thread::sleep(std::time::Duration::from_millis(30));
        write_file(dir.path(), "hot/c.jpg", b"cc");
        let hot = dir.path().join("hot");
        filetime::set_file_mtime(
            &hot,
            filetime::FileTime::from_system_time(std::time::SystemTime::now()),
        )
        .unwrap();

        let rescan = scan_roots_pruned(&roots, &jpg_extensions(), &[], &known);
        let names: Vec<_> = rescan
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        // hotは再列挙（b, c両方が返る）。stableはスキップ
        assert!(names.contains(&"b.jpg".to_string()));
        assert!(names.contains(&"c.jpg".to_string()));
        assert!(!names.contains(&"a.jpg".to_string()));
        // rootとstableはmtime不変でスキップ（rootのmtimeはhot内の変更では変わらない）
        assert_eq!(rescan.skipped_dirs, 2, "rootとstableをスキップ");
        assert!(rescan
            .enumerated_dirs
            .iter()
            .any(|d| d.file_name().is_some_and(|n| n == "hot")));
    }

    #[test]
    fn a_known_child_gone_during_a_skip_fails_the_root() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "keep/a.jpg", b"aaa");
        write_file(dir.path(), "gone/b.jpg", b"bb");
        let roots = vec![dir.path().to_path_buf()];
        let known = to_known(&scan_roots_pruned(
            &roots,
            &jpg_extensions(),
            &[],
            &HashMap::new(),
        ));

        // gone を丸ごと削除し、ルートのmtimeを既知の値へ偽装する
        // （FATなど「親mtimeが変わらない」ファイルシステムの再現）
        std::fs::remove_dir_all(dir.path().join("gone")).unwrap();
        let root_known_mtime = known[&dir.path().to_path_buf()];
        filetime::set_file_mtime(
            dir.path(),
            filetime::FileTime::from_unix_time(
                root_known_mtime / 1000,
                (root_known_mtime % 1000) as u32 * 1_000_000,
            ),
        )
        .unwrap();

        let rescan = scan_roots_pruned(&roots, &jpg_extensions(), &[], &known);
        // ルートはスキップ→既知の子 gone へ再帰→存在しない→エラー扱い
        assert!(
            rescan.ok_roots.is_empty(),
            "前提が崩れたルートはok扱いにせず、削除判定を保留させる"
        );
    }

    #[test]
    fn a_pruned_scan_of_a_missing_root_stays_out_of_ok_roots() {
        let missing = PathBuf::from("Z:/no/such/root");
        let outcome = scan_roots_pruned(
            std::slice::from_ref(&missing),
            &jpg_extensions(),
            &[],
            &HashMap::new(),
        );
        assert!(outcome.files.is_empty());
        assert!(outcome.ok_roots.is_empty());
    }
}
