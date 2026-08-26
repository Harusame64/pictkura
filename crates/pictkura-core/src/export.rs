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
use crate::scanner::is_managed_package_path;

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
    /// 上と同じだが**サイドカー**（`.xmp` 等）。ゴミ箱へは送るが、
    /// **DBの行は無いし、件数にも数えない**——利用者が見ているのは写真の枚数で、
    /// 影の数を足すと「3枚移したのに5件」になる
    pub sidecars_to_remove: Vec<PathBuf>,
}

/// 書き出し先が使えない。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("書き出し先のフォルダを作れません: {0}")]
    DestUnusable(PathBuf),
    /// アプリが中身を管理している入れ物（macOSの `写真ライブラリ.photoslibrary` 等）。
    /// **フォルダ選択のダイアログでは中に入って選べてしまう**が、そこへ直接書くと
    /// 写真アプリの管理外のファイルを内部へ置くことになる
    #[error("この場所へは書き出せません（アプリが管理している入れ物です）: {0}")]
    DestIsPackage(PathBuf),
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
    sidecar_extensions: &[String],
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ExportOutcome, ExportError> {
    // **取り込み先と同じ門を通す**（`set_import_destination` も同じ判定で断っている）。
    // ネイティブのダイアログは `.photoslibrary` の中まで選べてしまう
    if is_managed_package_path(dest_dir) {
        return Err(ExportError::DestIsPackage(dest_dir.to_path_buf()));
    }
    if std::fs::create_dir_all(dest_dir).is_err() || !dest_dir.is_dir() {
        return Err(ExportError::DestUnusable(dest_dir.to_path_buf()));
    }
    let total = files.len();
    let mut out = ExportOutcome::default();
    let mut carry = Carry {
        mode,
        extensions: sidecar_extensions,
        // 中身は**写真を運び終えてから**入れる（下を参照）
        companions: crate::sidecar::Companions::new(Vec::new()),
        carried: HashSet::new(),
        written: HashSet::new(),
        pending: Vec::new(),
    };
    for (i, path) in files.iter().enumerate() {
        export_one(path, dest_dir, &mut carry, &mut out);
        on_progress(i + 1, total, path);
    }
    // **影を運ぶのは、写真の成否が出そろってから**（ゲート2の指摘）。
    // 「選んだから居なくなる」ではなく「**実際に居なくなった**」で判断する
    // ——書き出し先に同じものが既にあってスキップされた写真、コピーに失敗した
    // 写真は元の場所に**残る**。それを「居なくなる」に数えると、残った写真から
    // 共有の `.xmp` を奪ってしまう（守ろうとした相方を、こちらの都合で裏切る）
    if mode == ExportMode::Move {
        let left: Vec<PathBuf> = out
            .moved
            .iter()
            .chain(out.to_remove.iter())
            .cloned()
            .collect();
        carry.companions = crate::sidecar::Companions::new(left);
    }
    for (photo, dest_name) in std::mem::take(&mut carry.pending) {
        carry_sidecars(&photo, dest_dir, &dest_name, &mut carry, &mut out);
    }
    Ok(out)
}

/// 1回の書き出しのあいだ持ち回る状態。
struct Carry<'a> {
    mode: ExportMode,
    extensions: &'a [String],
    /// 移動で「残る相方から `.xmp` を奪わない」ための門（コピーでは空）
    companions: crate::sidecar::Companions,
    /// **この操作でもう運んだサイドカー**（元のパス）。RAW+JPGを両方選ぶと
    /// 同じ `IMG_0001.xmp` に2回行き当たり、素朴に運ぶと2枚目が連番へ落ちて
    /// **どの写真とも結び付かない `IMG_0001-1.xmp`** が書き出し先に生える
    carried: HashSet<PathBuf>,
    /// **この操作で自分が書いたパス**を覚える。名前もサイズも同じで中身が違う写真
    /// （連番が一周したRAW等）を「もうある」と誤判定して落とさないため
    written: HashSet<PathBuf>,
    /// 影を待たせておく列（元の写真, 付いた先の名前）。写真が**全部片付いてから**
    /// まとめて運ぶ——誰が実際に居なくなったかは、最後まで走らないと決まらない
    pending: Vec<(PathBuf, String)>,
}

fn export_one(path: &Path, dest_dir: &Path, carry: &mut Carry, out: &mut ExportOutcome) {
    let mode = carry.mode;
    // 一覧に出したあとに消えている可能性があるので、ここで改めてstatする
    let Ok(meta) = std::fs::metadata(path) else {
        out.stats.failed += 1;
        return;
    };
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        out.stats.failed += 1;
        return;
    };
    let mtime_ms = filetime::FileTime::from_last_modification_time(&meta).unix_seconds() * 1000;
    let dest_path =
        match resolve_dest_path_avoiding(dest_dir, file_name, meta.len(), mtime_ms, &carry.written)
        {
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
        let dest_name = dest_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        carry.written.insert(dest_path);
        out.moved.push(path.to_path_buf());
        carry.pending.push((path.to_path_buf(), dest_name));
        return;
    }

    if !copy_verified(path, &dest_path, meta.len()) {
        // **失敗した名前は使用済みにしない**。次の1枚が理由もなく連番になる
        out.stats.failed += 1;
        return;
    }
    out.stats.done += 1;
    let dest_name = dest_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    carry.written.insert(dest_path);
    if mode == ExportMode::Move {
        // **元を消すのはここではない**（モジュールの説明を参照）
        out.to_remove.push(path.to_path_buf());
    }
    carry.pending.push((path.to_path_buf(), dest_name));
}

/// 写真に付いているサイドカーを、写真と同じ場所へ連れていく（0.2）。
///
/// **移動でこれをしないと、`.xmp` だけがライブラリに取り残される**——一覧に
/// 出ないファイルなので、誰にも見えないまま現像設定だけが残り続ける。
/// コピーでも連れていくのは、渡した先で同じ絵を再現できるようにするため。
///
/// **失敗しても写真の成否は変えない**。影のために本体の結果を書き換えない。
fn carry_sidecars(
    source_photo: &Path,
    dest_dir: &Path,
    dest_photo_name: &str,
    carry: &mut Carry,
    out: &mut ExportOutcome,
) {
    if carry.extensions.is_empty() {
        return;
    }
    let mode = carry.mode;
    let found = match mode {
        // 移動は元が消えるので、**残る相方のもの**は連れていかない
        ExportMode::Move => carry.companions.sidecars_of(source_photo, carry.extensions),
        ExportMode::Copy => crate::sidecar::sidecars_of(source_photo, carry.extensions),
    };
    for sidecar in found {
        // **同じサイドカーは1回だけ**。RAW+JPGを両方選ぶと同じ `.xmp` に2回
        // 行き当たる——2回目は名前が衝突して連番へ落ち、どの写真とも結び付かない
        // `IMG_0001-1.xmp` が書き出し先に生える（移動では、消す元も二重に積まれる）
        if !carry.carried.insert(sidecar.clone()) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&sidecar) else {
            continue;
        };
        let name = crate::sidecar::sidecar_dest_name(source_photo, &sidecar, dest_photo_name);
        let mtime_ms = filetime::FileTime::from_last_modification_time(&meta).unix_seconds() * 1000;
        let target =
            match resolve_dest_path_avoiding(dest_dir, &name, meta.len(), mtime_ms, &carry.written)
            {
                DestResolution::CopyTo(p) => p,
                DestResolution::AlreadyImported | DestResolution::Exhausted => continue,
            };
        if mode == ExportMode::Move
            && !crate::cloud::is_cloud_only_path(&sidecar)
            && std::fs::rename(&sidecar, &target).is_ok()
        {
            carry.written.insert(target);
            continue;
        }
        if std::fs::copy(&sidecar, &target).is_ok() {
            // **写真と同じくmtimeを引き継ぐ**（`copy_verified` と同じ理由）。
            // Unixの `fs::copy` は保持しないので、同じUSBメモリへ2回書き出すと
            // 写真は「もうある」で飛ばされるのに、サイドカーだけ別物と見なされて
            // `IMG_0001-1.xmp` `-2` … と増え続ける
            if let Ok(mtime) = meta.modified() {
                let _ =
                    filetime::set_file_mtime(&target, filetime::FileTime::from_system_time(mtime));
            }
            carry.written.insert(target);
            if mode == ExportMode::Move {
                out.sidecars_to_remove.push(sidecar);
            }
        } else {
            let _ = std::fs::remove_file(&target);
        }
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

    /// テストの既定（設定から来るのと同じ形）
    fn exts() -> Vec<String> {
        crate::sidecar::DEFAULT_SIDECAR_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// **移動で `.xmp` を置き去りにしない**（0.2・`dev/loadmap.md` 1.1）。
    /// 一覧に出ないファイルなので、取り残すと誰にも見えない迷子になる。
    #[test]
    fn a_move_takes_the_sidecar_along() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        let out = export_files(
            &[src.join("IMG_0001.jpg")],
            &dest,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 1, "枚数は写真のぶんだけ");
        assert_eq!(files_in(&dest), ["IMG_0001.jpg", "IMG_0001.xmp"]);
        // 同じドライブなので rename で動く＝元は残っていない
        assert!(
            !src.join("IMG_0001.xmp").exists(),
            "サイドカーが取り残された"
        );
        assert!(
            out.sidecars_to_remove.is_empty(),
            "renameで動いたぶんは後始末が要らない"
        );
    }

    #[test]
    fn a_copy_takes_the_sidecar_too_and_leaves_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        export_files(
            &[src.join("IMG_0001.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(files_in(&dest), ["IMG_0001.jpg", "IMG_0001.xmp"]);
        assert!(src.join("IMG_0001.xmp").exists(), "コピーでは元を消さない");
    }

    /// **P1（ゲート1）**: RAW+JPGのうち片方だけ移すと、共有の `IMG_0001.xmp` が
    /// 道連れになる。写真（RAW）はライブラリに残るので**利用者は気付かない**。
    #[test]
    fn a_move_does_not_rob_the_partner_left_behind_of_its_develop_settings() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("IMG_0001.CR3"), b"raw").unwrap();
        std::fs::write(src.join("IMG_0001.JPG"), b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        // JPGだけを別フォルダへ移す
        export_files(
            &[src.join("IMG_0001.JPG")],
            &dest,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(files_in(&dest), ["IMG_0001.JPG"], "`.xmp` はRAWのもの");
        assert!(
            src.join("IMG_0001.xmp").exists(),
            "残るRAWから現像設定を奪ってはいけない"
        );

        // 組ごと移すなら連れていく（取り残しにしない）
        let dest2 = dir.path().join("out2");
        export_files(
            &[src.join("IMG_0001.CR3")],
            &dest2,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(files_in(&dest2), ["IMG_0001.CR3", "IMG_0001.xmp"]);
    }

    /// **P2（ゲート2）**: 「選んだから居なくなる」ではなく「**実際に居なくなった**」で
    /// 判断すること。書き出し先に同じものが既にある写真はスキップされて元の場所に
    /// **残る**のに、それを居なくなる側に数えると共有の `.xmp` を奪ってしまう。
    #[test]
    fn nor_does_it_rob_a_partner_that_the_move_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let raw = src.join("IMG_0001.CR3");
        let jpg = src.join("IMG_0001.JPG");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(&jpg, b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();
        // 書き出し先に**同じRAWが既にある**（同名・同サイズ・同じ更新時刻）
        std::fs::write(dest.join("IMG_0001.CR3"), b"raw").unwrap();
        let stamp = filetime::FileTime::from_unix_time(1_500_000_000, 0);
        filetime::set_file_mtime(&raw, stamp).unwrap();
        filetime::set_file_mtime(dest.join("IMG_0001.CR3"), stamp).unwrap();

        let out = export_files(
            &[raw.clone(), jpg.clone()],
            &dest,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.skipped, 1, "既にあるRAWはスキップされる");
        assert!(raw.exists(), "スキップされたRAWは元の場所に残る");
        assert!(
            src.join("IMG_0001.xmp").exists(),
            "残ったRAWから現像設定を奪ってはいけない"
        );
        assert!(!jpg.exists(), "JPGのほうは移動できている");
    }

    /// **P2（ゲート1）**: 組を両方選ぶと同じ `.xmp` に2回行き当たる。
    /// 素朴に運ぶと2枚目が連番へ落ち、どの写真とも結び付かない孤児が生える。
    #[test]
    fn a_shared_sidecar_is_not_carried_twice() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("IMG_0001.CR3"), b"raw").unwrap();
        std::fs::write(src.join("IMG_0001.JPG"), b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        let out = export_files(
            &[src.join("IMG_0001.CR3"), src.join("IMG_0001.JPG")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 2);
        assert_eq!(
            files_in(&dest),
            ["IMG_0001.CR3", "IMG_0001.JPG", "IMG_0001.xmp"],
            "`IMG_0001-1.xmp` はどの写真の設定でもない"
        );
    }

    /// **P2（ゲート1）**: サイドカーだけmtimeを引き継がないと、同じUSBメモリへ
    /// 2回書き出すたびに `-1` `-2` … と増える（Unixの `fs::copy` は保持しない）。
    #[test]
    fn a_copied_sidecar_keeps_the_original_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        let sidecar = src.join("IMG_0001.xmp");
        std::fs::write(&sidecar, b"<x/>").unwrap();
        let old = filetime::FileTime::from_unix_time(1_500_000_000, 0);
        filetime::set_file_mtime(src.join("IMG_0001.jpg"), old).unwrap();
        filetime::set_file_mtime(&sidecar, old).unwrap();

        let files = [src.join("IMG_0001.jpg")];
        export_files(&files, &dest, ExportMode::Copy, &exts(), |_, _, _| {}).unwrap();
        let copied = filetime::FileTime::from_last_modification_time(
            &std::fs::metadata(dest.join("IMG_0001.xmp")).unwrap(),
        );
        assert_eq!(copied.unix_seconds(), old.unix_seconds());

        // 2回目は写真もサイドカーも「もうある」で増えない
        export_files(&files, &dest, ExportMode::Copy, &exts(), |_, _, _| {}).unwrap();
        assert_eq!(files_in(&dest), ["IMG_0001.jpg", "IMG_0001.xmp"]);
    }

    /// 写真が連番になったら、サイドカーも同じ連番で付いていくこと。
    #[test]
    fn a_numbered_copy_keeps_its_sidecar_on_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        // 書き出し先に、同名で中身の違うものが既にある
        std::fs::write(dest.join("IMG_0001.jpg"), b"someone-else").unwrap();
        std::fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        std::fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        export_files(
            &[src.join("IMG_0001.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(
            files_in(&dest),
            ["IMG_0001-1.jpg", "IMG_0001-1.xmp", "IMG_0001.jpg"],
            "サイドカーだけ元の名前で残ると、別の写真の設定として読まれる"
        );
    }

    #[test]
    fn a_copy_lays_the_files_flat_and_leaves_the_originals() {
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
            &exts(),
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
    fn the_same_name_is_skipped_at_the_same_size_and_numbered_when_it_differs() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(dest.join("a.jpg"), b"aaa").unwrap();

        let out = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out.stats.skipped, 1, "同名・同サイズは何もしない");
        assert_eq!(files_in(&dest), ["a.jpg"]);

        // 別内容（サイズ違い）なら連番で避ける
        fs::write(dest.join("a.jpg"), b"zzzzzzzz").unwrap();
        let out = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out.stats.done, 1);
        assert_eq!(files_in(&dest), ["a-1.jpg", "a.jpg"]);
    }

    #[test]
    fn same_name_and_size_on_another_day_still_loses_nothing() {
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
            &exts(),
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
    fn same_name_and_size_as_an_earlier_export_is_kept_when_the_content_differs() {
        // 「同名・同サイズ＝同じもの」は平置きでは成り立たない。**別の操作で**書いた
        // ものとの衝突（USBメモリへ何度も足していく使い方）でも落とさないこと
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("DSC00001.ARW"), b"2222").unwrap();
        // 先に別物（同じサイズ・別の日）が書き出されている
        fs::write(dest.join("DSC00001.ARW"), b"1111").unwrap();
        filetime::set_file_mtime(
            dest.join("DSC00001.ARW"),
            filetime::FileTime::from_unix_time(1_000_000_000, 0),
        )
        .unwrap();
        filetime::set_file_mtime(
            src.join("DSC00001.ARW"),
            filetime::FileTime::from_unix_time(1_700_000_000, 0),
        )
        .unwrap();

        let out = export_files(
            &[src.join("DSC00001.ARW")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 1, "撮影時刻が違うので別物として書き出す");
        assert_eq!(out.stats.skipped, 0);
        assert_eq!(files_in(&dest), ["DSC00001-1.ARW", "DSC00001.ARW"]);
        assert_eq!(fs::read(dest.join("DSC00001.ARW")).unwrap(), b"1111");
        assert_eq!(fs::read(dest.join("DSC00001-1.ARW")).unwrap(), b"2222");
    }

    #[test]
    fn exporting_the_same_thing_again_adds_nothing() {
        // USBメモリへ足していく使い方。2回目は何も増えない
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        let first = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(first.stats.done, 1);
        let second = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(second.stats.skipped, 1, "同じものは飛ばす");
        assert_eq!(files_in(&dest), ["a.jpg"]);
    }

    #[test]
    fn an_hour_of_daylight_saving_drift_still_counts_as_the_same_thing() {
        // FAT32 は更新時刻をローカル時刻で持つので、夏時間の切り替えをまたぐと
        // 同じファイルが**ちょうど1時間**ずれて見える。ここを見落とすと
        // USBメモリ1本ぶんが丸ごと二重に書き出される
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(dest.join("a.jpg"), b"aaa").unwrap();
        filetime::set_file_mtime(
            src.join("a.jpg"),
            filetime::FileTime::from_unix_time(1_700_000_000, 0),
        )
        .unwrap();
        filetime::set_file_mtime(
            dest.join("a.jpg"),
            filetime::FileTime::from_unix_time(1_700_000_000 + 3_600, 0),
        )
        .unwrap();

        let out = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.skipped, 1, "1時間ちょうどのずれは同じもの");
        assert_eq!(files_in(&dest), ["a.jpg"]);
    }

    #[test]
    fn a_name_whose_export_failed_is_not_marked_as_taken() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        // 1枚目は読めない（失敗する）。2枚目は同じ名前で書ける
        let out = export_files(
            &[dir.path().join("gone/a.jpg"), src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.failed, 1);
        assert_eq!(out.stats.done, 1);
        // 落ちた1枚のせいで `a-1.jpg` にならないこと
        assert_eq!(files_in(&dest), ["a.jpg"]);
    }

    #[test]
    fn nothing_is_exported_into_a_folder_the_app_manages() {
        // ネイティブのフォルダ選択は `.photoslibrary` の中まで選べてしまう
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        let dest = dir.path().join("写真ライブラリ.photoslibrary/originals");

        let err = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Copy,
            &exts(),
            |_, _, _| {},
        )
        .unwrap_err();

        assert!(matches!(err, ExportError::DestIsPackage(_)), "{err:?}");
        assert!(!dest.exists(), "フォルダも作らない");
    }

    #[test]
    fn a_move_on_the_same_drive_leaves_no_original_to_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        let out = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.done, 1);
        assert!(!src.join("a.jpg").exists(), "元は消える");
        assert_eq!(out.moved, vec![src.join("a.jpg")]);
        // ゴミ箱へ送るものは無い（renameで済んだので、消す作業自体が無い）
        assert!(out.to_remove.is_empty());
    }

    #[test]
    fn a_move_keeps_the_original_when_the_same_name_and_size_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("lib");
        let dest = dir.path().join("out");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(dest.join("a.jpg"), b"aaa").unwrap();

        let out = export_files(
            &[src.join("a.jpg")],
            &dest,
            ExportMode::Move,
            &exts(),
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(out.stats.skipped, 1);
        assert!(
            src.join("a.jpg").exists(),
            "**元は残す**（中身までは見ていない）"
        );
        assert!(out.moved.is_empty() && out.to_remove.is_empty());
    }

    #[test]
    fn an_unreadable_one_is_dropped_alone_and_the_rest_carry_on() {
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
            &exts(),
            |_, _, _| seen.set(seen.get() + 1),
        )
        .unwrap();

        assert_eq!(out.stats.failed, 1);
        assert_eq!(out.stats.done, 1);
        assert_eq!(seen.get(), 2, "進捗は落ちた分も含めて1件ずつ来る");
        assert_eq!(files_in(&dest), ["a.jpg"]);
    }
}
