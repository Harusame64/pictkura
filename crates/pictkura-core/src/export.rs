//! 選んだファイルを、指定したフォルダへ**コピー／移動**する。
//!
//! 取り込み（[`crate::import`]）との違い:
//! - 取り込みは**日付でフォルダを振り分ける**。こちらは**指定フォルダへ平置き**する
//!   ——「この何枚かを人に渡す」「USBメモリへ入れる」が目的なので、
//!   受け取り手が階層をたどらずに済む形にする
//! - 取り込み元は外部メディア。こちらの元は**ライブラリの中のファイル**
//!
//! 衝突の避け方（同名・同サイズは「もうある」、別内容なら連番）は取り込みと
//! 同じ規則を使う。同じに見える操作が場所によって違う振る舞いをしないように、
//! 規則は1か所（[`crate::import`]）に置く。
//!
//! **移動でファイルを消すのはこのモジュールではない。** 同じドライブなら
//! `rename` で済むが、別のドライブへはコピーしてから元を消すことになる。
//! 元を消す部分は呼び出し側（アプリ）へ返し、**OSのゴミ箱経由**で消させる
//! ——「アプリがファイルを直接消すことはない」という利用者への約束を、
//! 移動でも崩さないため。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::import::{resolve_dest_path_avoiding, DestResolution};

/// コピーか移動か。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportMode {
    Copy,
    Move,
}

/// 何件どうなったか。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ExportStats {
    /// コピー／移動できた件数
    pub done: usize,
    /// 同名・同サイズが既にあったので何もしなかった件数
    pub skipped: usize,
    /// 読めない・書けない等で落ちた件数
    pub failed: usize,
}

/// 結果と、呼び出し側にやってもらう後始末。
#[derive(Debug, Default)]
pub struct ExportOutcome {
    pub stats: ExportStats,
    /// **もう元の場所に無い**ファイル（`rename` で移動できたもの）。
    /// 呼び出し側はDBからこの行を落とす
    pub moved: Vec<PathBuf>,
    /// **コピーは済んだが、元がまだ残っている**ファイル（別ドライブへの移動）。
    /// 呼び出し側がゴミ箱へ送り、送れたものはDBからも落とす
    pub to_remove: Vec<PathBuf>,
}

/// 書き出し先が使えない。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("書き出し先のフォルダを作れません: {0}")]
    DestUnusable(PathBuf),
}

/// `files` を `dest_dir` へコピー／移動する。
///
/// `on_progress(処理済み件数, 総件数, いま処理したファイル)` は1件ごとに呼ばれる。
/// **1件の失敗で全体を止めない**（取り込みと同じ作法）。読めなくなっていた1枚で
/// 500枚の書き出しが丸ごと消えるほうが困る。
pub fn export_files(
    files: &[PathBuf],
    dest_dir: &Path,
    mode: ExportMode,
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ExportOutcome, ExportError> {
    if std::fs::create_dir_all(dest_dir).is_err() || !dest_dir.is_dir() {
        return Err(ExportError::DestUnusable(dest_dir.to_path_buf()));
    }
    let total = files.len();
    let mut out = ExportOutcome::default();
    // **この操作で自分が書いたパス**を覚える。名前もサイズも同じで中身が違う写真
    // （連番が一周したRAW等）を「もうある」と誤判定して落とさないため
    let mut written: HashSet<PathBuf> = HashSet::new();
    for (i, path) in files.iter().enumerate() {
        export_one(path, dest_dir, mode, &mut written, &mut out);
        on_progress(i + 1, total, path);
    }
    Ok(out)
}

fn export_one(
    path: &Path,
    dest_dir: &Path,
    mode: ExportMode,
    written: &mut HashSet<PathBuf>,
    out: &mut ExportOutcome,
) {
    // 一覧に出したあとに消えている可能性があるので、ここで改めてstatする
    let Ok(meta) = std::fs::metadata(path) else {
        out.stats.failed += 1;
        return;
    };
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        out.stats.failed += 1;
        return;
    };
    let dest_path = match resolve_dest_path_avoiding(dest_dir, file_name, meta.len(), written) {
        DestResolution::CopyTo(p) => p,
        DestResolution::AlreadyImported => {
            // **同じものが既にある。移動でも元は消さない**——消してよいかは
            // 「同名・同サイズ」だけでは決められない（中身までは見ていない）
            out.stats.skipped += 1;
            return;
        }
        DestResolution::Exhausted => {
            out.stats.failed += 1;
            return;
        }
    };

    written.insert(dest_path.clone());

    // 同じドライブの移動は `rename` で終わる（メタデータの更新だけ）。
    // 別のドライブだと失敗するので、そのときはコピーへ落とす。
    //
    // **クラウドにしか実体が無いものは `rename` しない**。中身を持たない印だけが
    // 同期フォルダの外へ動き、同期側は「消えた」と見て**クラウドの実体まで消しに行く**
    // ——移した先には中身の無い印だけが残る。コピー（＝取り寄せ）してから
    // 元をゴミ箱へ送る経路に必ず落とす
    if mode == ExportMode::Move
        && !crate::cloud::is_cloud_only_path(path)
        && std::fs::rename(path, &dest_path).is_ok()
    {
        out.stats.done += 1;
        out.moved.push(path.to_path_buf());
        return;
    }

    if !copy_verified(path, &dest_path, meta.len()) {
        out.stats.failed += 1;
        return;
    }
    out.stats.done += 1;
    if mode == ExportMode::Move {
        // **元を消すのはここではない**（モジュールの説明を参照）
        out.to_remove.push(path.to_path_buf());
    }
}

/// コピーして、**サイズが合っていることを確かめる**。
/// 合わなければ中途半端なファイルを残さずに消して失敗を返す。
///
/// **見るのはサイズだけ**（取り込みの `verify_after_copy` と同じ）。中身の化けは
/// すり抜けるが、ハッシュを取ると1枚ごとに全体を読み直すことになる。
/// 移動でも元はゴミ箱に残るので、取り返しはつく。
fn copy_verified(src: &Path, dest: &Path, expected: u64) -> bool {
    if std::fs::copy(src, dest).is_err() {
        // 途中まで書けている可能性がある
        let _ = std::fs::remove_file(dest);
        return false;
    }
    // Unixの `fs::copy` はmtimeを保持しない。受け取り手の並び順が変わるので引き継ぐ
    if let Ok(mtime) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(mtime));
    }
    // 「今の」元のサイズと比べる（書き出し中に元が変わっていたら正しくない）
    let src_now = std::fs::metadata(src).map(|m| m.len()).unwrap_or(expected);
    if std::fs::metadata(dest).map(|m| m.len()).ok() != Some(src_now) {
        let _ = std::fs::remove_file(dest);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn コピーは元を残して平置きする() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib/2026-08");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(src.join("b.jpg"), b"bbbb").unwrap();

        let out = export_files(
            &[src.join("a.jpg"), src.join("b.jpg")],
            &dest,
            ExportMode::Copy,
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 2);
        assert!(out.moved.is_empty() && out.to_remove.is_empty());
        // **日付のフォルダは作らない**（受け取り手がたどらずに済む形）
        assert_eq!(files_in(&dest), ["a.jpg", "b.jpg"]);
        assert!(src.join("a.jpg").exists(), "元は残る");
    }

    #[test]
    fn 同名は同サイズなら飛ばし別内容なら連番になる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(dest.join("a.jpg"), b"aaa").unwrap();

        let out =
            export_files(&[src.join("a.jpg")], &dest, ExportMode::Copy, |_, _, _| {}).unwrap();
        assert_eq!(out.stats.skipped, 1, "同名・同サイズは何もしない");
        assert_eq!(files_in(&dest), ["a.jpg"]);

        // 別内容（サイズ違い）なら連番で避ける
        fs::write(dest.join("a.jpg"), b"zzzzzzzz").unwrap();
        let out =
            export_files(&[src.join("a.jpg")], &dest, ExportMode::Copy, |_, _, _| {}).unwrap();
        assert_eq!(out.stats.done, 1);
        assert_eq!(files_in(&dest), ["a-1.jpg", "a.jpg"]);
    }

    #[test]
    fn 別の日の同名同サイズでも1枚も落とさない() {
        // カメラの連番が一周すると、別の日の `DSC00001` が同じフォルダへ落ちる。
        // 非圧縮RAWは中身が違ってもサイズが同じなので、「同名・同サイズ＝同じもの」で
        // 飛ばすと**選んだ写真が黙って1枚欠ける**
        let dir = tempfile::tempdir().unwrap();
        let day1 = dir.path().join("lib/2024-05");
        let day2 = dir.path().join("lib/2025-09");
        let dest = dir.path().join("out");
        fs::create_dir_all(&day1).unwrap();
        fs::create_dir_all(&day2).unwrap();
        fs::write(day1.join("DSC00001.ARW"), b"1111").unwrap();
        fs::write(day2.join("DSC00001.ARW"), b"2222").unwrap(); // 同じサイズ・別内容

        let out = export_files(
            &[day1.join("DSC00001.ARW"), day2.join("DSC00001.ARW")],
            &dest,
            ExportMode::Copy,
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 2, "2枚とも書き出される");
        assert_eq!(out.stats.skipped, 0);
        assert_eq!(files_in(&dest), ["DSC00001-1.ARW", "DSC00001.ARW"]);
        // 中身が入れ替わっていないこと
        let mut bodies = vec![
            fs::read(dest.join("DSC00001.ARW")).unwrap(),
            fs::read(dest.join("DSC00001-1.ARW")).unwrap(),
        ];
        bodies.sort();
        assert_eq!(bodies, vec![b"1111".to_vec(), b"2222".to_vec()]);
    }

    #[test]
    fn 同じドライブの移動は元が消えて後始末が要らない() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        let out =
            export_files(&[src.join("a.jpg")], &dest, ExportMode::Move, |_, _, _| {}).unwrap();

        assert_eq!(out.stats.done, 1);
        assert!(!src.join("a.jpg").exists(), "元は消える");
        assert_eq!(out.moved, vec![src.join("a.jpg")]);
        // ゴミ箱へ送るものは無い（renameで済んだので、消す作業自体が無い）
        assert!(out.to_remove.is_empty());
    }

    #[test]
    fn 移動でも同名同サイズがあれば元を消さない() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(dest.join("a.jpg"), b"aaa").unwrap();

        let out =
            export_files(&[src.join("a.jpg")], &dest, ExportMode::Move, |_, _, _| {}).unwrap();

        assert_eq!(out.stats.skipped, 1);
        assert!(
            src.join("a.jpg").exists(),
            "**元は残す**（中身までは見ていない）"
        );
        assert!(out.moved.is_empty() && out.to_remove.is_empty());
    }

    #[test]
    fn 読めないものは1件だけ落として続ける() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        // 進捗は `Fn` で受けるので、数えるには内側可変性が要る
        let seen = std::cell::Cell::new(0);
        let out = export_files(
            &[src.join("no-such.jpg"), src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            |_, _, _| seen.set(seen.get() + 1),
        )
        .unwrap();

        assert_eq!(out.stats.failed, 1);
        assert_eq!(out.stats.done, 1);
        assert_eq!(seen.get(), 2, "進捗は落ちた分も含めて1件ずつ来る");
        assert_eq!(files_in(&dest), ["a.jpg"]);
    }
}
