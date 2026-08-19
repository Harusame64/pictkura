//! サイドカー（`.xmp` 等）と、同じ名前の組（RAW+JPG）。
//!
//! **写真の隣に置かれた小さなファイルは、写真と一緒に動かないと意味を失う**。
//! `.xmp` には現像ソフト（Lightroom・darktable 等）が書いた現像設定・評価・
//! キーワードが入っていて、写真だけ取り込んで置き去りにすると、
//! 利用者から見れば**編集がぜんぶ消えたのと同じ**になる。ファイルが小さいので
//! 「一緒に運ぶ」以外の判断は要らない。
//!
//! ここに置くのは**名前の対応づけだけ**で、コピーも削除もしない。呼ぶ側
//! （取り込み・ゴミ箱）が実際の入出力を持つ。

use std::path::{Path, PathBuf};

/// 既定で一緒に運ぶ拡張子（2026-08-19に調べた）。
///
/// **入っているのは「利用者の作業そのもの」だけ**——置いていくと取り返しが
/// つかないものに絞ってある:
///
/// | 拡張子 | 書くソフト |
/// |---|---|
/// | `xmp` | Adobe（Camera Raw / Lightroom / Bridge）・darktable・digiKam・Photo Mechanic |
/// | `aae` | Apple 写真（iPhone / iPad の編集） |
/// | `dop` | DxO PhotoLab |
/// | `pp3` | RawTherapee |
/// | `on1` | ON1 Photo RAW |
///
/// **入れなかったもの**と、その理由:
///
/// - `thm`（動画のサムネイル）・`lrv`（低解像度のプロキシ動画。GoPro・DJI）:
///   カメラが作り直せる派生物で、`lrv` は**大きい**
/// - `modd` / `moff`: **PlayMemories Home の管理ファイル**。このアプリは
///   そこから離れるための道具なので、運んでも使い道が無い
/// - `wav`（音声メモ。Olympus・Pentax）: 中身は利用者のものだが、
///   `song.jpg` と `song.wav` のような**同名の別物を巻き込む**。要る人は設定で足せる
/// - `cos`（Capture One）: 同階層ではなく `CaptureOne` フォルダの中に置かれるので、
///   同名の規則では拾えない
///
/// **画像の拡張子とは別の並び**にしてある。ここに足したものは一覧には出ず、
/// 写真の影として付いて回るだけ——`config.import.extensions` に足すと、
/// サイドカーが1枚の写真として一覧に並んでしまう。
pub const DEFAULT_SIDECAR_EXTENSIONS: &[&str] = &["xmp", "aae", "dop", "pp3", "on1"];

/// 同じ写真の組（RAW+JPG など）をまとめる鍵。
///
/// **フォルダと、拡張子を除いた名前**。`IMG_0001.CR3` と `IMG_0001.JPG` は
/// 同じ組になり、`dest_dir_for` の日付をそろえるのに使う——組が別々の日の
/// フォルダへ散ると、あとで RAW と JPG を突き合わせられなくなる。
///
/// 大文字小文字は**畳んで**比べる。カメラは `IMG_0001.CR3` と `img_0001.jpg` を
/// 混ぜて書くことがあり、Windowsではどのみち同じ名前として扱われる。
pub fn pair_key(path: &Path) -> (PathBuf, String) {
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    (dir, stem)
}

/// `path` に付いているサイドカーを、**実在するものだけ**返す。
///
/// 名前の流儀が2つあるので両方見る:
///
/// - `IMG_0001.xmp` … 拡張子を**置き換える**（Adobe・Apple の `.aae`）
/// - `IMG_0001.CR3.xmp` … 拡張子の後ろへ**足す**（darktable・digiKam）
///
/// 大文字小文字の候補も試す。macOS/Linux では `.XMP` を別物として持つ機械がある。
///
/// **同じ1つのファイルを2度返さない**のが要点。Windowsは大文字小文字を区別しないので、
/// `.xmp` と `.XMP` の**どちらの綴りでも同じファイルが実在**として返り、素朴に集めると
/// 同じサイドカーを2回コピーし（連番が付いて `IMG_0001-1.xmp` が生える）、
/// ゴミ箱へも2回送ることになる。実体で見分けるため [`std::fs::canonicalize`] を通す
/// ——OSが返す**本当の綴り**で畳めば、区別する機械でも正しく2つのままになる。
pub fn sidecars_of(path: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    // 写真そのものは候補から外す（`IMG_0001.xmp` を取り込むときの自分自身）
    let itself = std::fs::canonicalize(path).ok();
    for ext in extensions {
        let ext = ext.trim().trim_start_matches('.');
        if ext.is_empty() {
            continue;
        }
        for cased in [ext.to_lowercase(), ext.to_uppercase()] {
            // 置き換え型: IMG_0001.CR3 → IMG_0001.xmp
            let replaced = path.with_extension(&cased);
            // 足す型: IMG_0001.CR3 → IMG_0001.CR3.xmp
            let mut appended = path.as_os_str().to_os_string();
            appended.push(".");
            appended.push(&cased);
            let appended = PathBuf::from(appended);
            for candidate in [replaced, appended] {
                if !candidate.is_file() {
                    continue;
                }
                // 実体で畳む。`canonicalize` が通らない環境では**拾わない**
                // （二重に運ぶより、運ばないほうが害が小さい……とは限らないが、
                // ここで綴り違いを見分けられない以上、同じものを2回運ぶ側は選べない）
                let Ok(canon) = std::fs::canonicalize(&candidate) else {
                    continue;
                };
                if Some(&canon) == itself.as_ref() || seen.contains(&canon) {
                    continue;
                }
                // **ディスク上の本当の綴りで返す**。Windowsでは `.aae` と綴っても
                // `IMG_1234.AAE` が開けてしまうので、こちらの候補の綴りをそのまま
                // 使うと**コピー先の名前が勝手に小文字になる**。現像ソフトや
                // 写真アプリは名前で結び付けるので、綴りは触らずに運ぶ
                let real = match (candidate.parent(), canon.file_name()) {
                    (Some(dir), Some(name)) => dir.join(name),
                    _ => candidate,
                };
                seen.push(canon);
                out.push(real);
            }
        }
    }
    out
}

/// コピー先での、そのサイドカーの名前を決める。
///
/// **写真の付いた先の名前に合わせる**のが要点。取り込みは同名衝突を
/// `IMG_0001-1.CR3` のように連番で避けるので、サイドカーだけ元の名前で置くと
/// **別の写真の設定として読まれる**（現像ソフトは名前で結びつける）。
///
/// - 置き換え型（`IMG_0001.xmp`）→ 付いた先の名前の拡張子を `.xmp` にしたもの
/// - 足す型（`IMG_0001.CR3.xmp`）→ 付いた先の名前に `.xmp` を足したもの
pub fn sidecar_dest_name(source_photo: &Path, sidecar: &Path, dest_photo_name: &str) -> String {
    let ext = sidecar
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let appended = sidecar
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source_name = source_photo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if appended.eq_ignore_ascii_case(&format!("{source_name}.{ext}")) {
        // 足す型
        format!("{dest_photo_name}.{ext}")
    } else {
        // 置き換え型
        let stem = Path::new(dest_photo_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| dest_photo_name.to_string());
        format!("{stem}.{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストは既定の並びで引く（設定から来るのと同じ形）
    fn exts() -> Vec<String> {
        DEFAULT_SIDECAR_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn 組の鍵は拡張子を落として大文字小文字を畳む() {
        let a = pair_key(Path::new("D:/photo/IMG_0001.CR3"));
        let b = pair_key(Path::new("D:/photo/img_0001.jpg"));
        assert_eq!(a, b, "RAWとJPGは同じ組");

        let other_dir = pair_key(Path::new("D:/別/IMG_0001.JPG"));
        assert_ne!(a, other_dir, "フォルダが違えば別の組");

        let other_name = pair_key(Path::new("D:/photo/IMG_0002.JPG"));
        assert_ne!(a, other_name);
    }

    #[test]
    fn 名前の流儀2つを実在するものだけ拾う() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_0001.CR3");
        std::fs::write(&photo, b"raw").unwrap();
        // まだサイドカーが無い
        assert!(sidecars_of(&photo, &exts()).is_empty());

        // 置き換え型
        let replaced = dir.path().join("IMG_0001.xmp");
        std::fs::write(&replaced, b"<x/>").unwrap();
        assert_eq!(sidecars_of(&photo, &exts()), vec![replaced.clone()]);

        // 足す型も同時に置く
        let appended = dir.path().join("IMG_0001.CR3.xmp");
        std::fs::write(&appended, b"<x/>").unwrap();
        let found = sidecars_of(&photo, &exts());
        assert_eq!(found.len(), 2, "両方の流儀を拾う: {found:?}");
        assert!(found.contains(&replaced) && found.contains(&appended));
    }

    #[test]
    fn iphoneのaaeも拾う() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_1234.HEIC");
        std::fs::write(&photo, b"heic").unwrap();
        let aae = dir.path().join("IMG_1234.AAE");
        std::fs::write(&aae, b"plist").unwrap();
        assert_eq!(sidecars_of(&photo, &exts()), vec![aae]);
    }

    /// `.xmp` 自身を取り込むときに、自分を自分のサイドカーとして拾わないこと。
    #[test]
    fn 自分自身は拾わない() {
        let dir = tempfile::tempdir().unwrap();
        let x = dir.path().join("IMG_0001.xmp");
        std::fs::write(&x, b"<x/>").unwrap();
        assert!(sidecars_of(&x, &exts()).is_empty());
    }

    #[test]
    fn コピー先の名前は写真の付いた先に合わせる() {
        let photo = Path::new("E:/DCIM/IMG_0001.CR3");
        // 置き換え型: 連番が付いたら、サイドカーも同じ連番になる
        let replaced = Path::new("E:/DCIM/IMG_0001.xmp");
        assert_eq!(
            sidecar_dest_name(photo, replaced, "IMG_0001-1.CR3"),
            "IMG_0001-1.xmp"
        );
        // 足す型: 付いた先の名前まるごとの後ろに足す
        let appended = Path::new("E:/DCIM/IMG_0001.CR3.xmp");
        assert_eq!(
            sidecar_dest_name(photo, appended, "IMG_0001-1.CR3"),
            "IMG_0001-1.CR3.xmp"
        );
        // 衝突が無ければ名前は変わらない
        assert_eq!(
            sidecar_dest_name(photo, replaced, "IMG_0001.CR3"),
            "IMG_0001.xmp"
        );
    }
}
