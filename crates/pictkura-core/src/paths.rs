//! DBへ書くパスの綴りを揃える。
//!
//! 同じファイルでも、どの経路で見つけたかで文字列が違うことがある:
//!
//! - 通常のスキャンは**設定のライブラリルート**から辿る。ルートが
//!   `C:/Users/me/Pictures` と書かれていれば、配下は
//!   `C:/Users/me/Pictures\2020\a.jpg` と混ざった綴りになる
//! - USNジャーナルの差分起動は `GetFinalPathNameByHandleW` が返す
//!   `C:\Users\me\Pictures\2020\a.jpg`（全部バックスラッシュ）で辿る
//!
//! DBはパスを**文字列として**比較する（`ON CONFLICT(path)`）ので、
//! この2つは別のファイルになり**同じ写真が2件並ぶ**。実際に、クラウドの
//! ファイルを取り寄せた（＝USNに変更が載った）だけで32件の重複ができた。
//!
//! そこで、DBへ書くパスは必ずここを通して綴りを揃える。
//! 比較のためだけの正規化なので、**実体の解決（シンボリックリンクの追跡や
//! 短縮名の展開）はしない**——ファイルを開かずに済ませる方針を崩さないため。

use std::path::{Path, PathBuf};

/// パスの綴りを揃える。
///
/// Windowsでは区切り文字をバックスラッシュへ統一し、ドライブ文字を大文字にする。
/// それ以外のOSでは何もしない（区切りは `/` だけで、大文字小文字も区別されるため）。
pub fn normalize(path: &Path) -> PathBuf {
    PathBuf::from(normalize_str(&path.to_string_lossy()))
}

/// 文字列としてのパスを揃える（[`normalize`] の中身）。
pub fn normalize_str(path: &str) -> String {
    #[cfg(not(windows))]
    {
        path.to_string()
    }
    #[cfg(windows)]
    {
        let mut out = path.replace('/', "\\");
        // "c:\..." と "C:\..." を同じ物として扱う。
        // `\\?\C:\...` や UNC (`\\server\share`) は2文字目が ':' にならないので素通り
        let bytes = out.as_bytes();
        if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_lowercase() {
            // u8 のまま to_string() すると数値（"67"）になるので char へ直してから
            let upper = (bytes[0] as char).to_ascii_uppercase();
            out.replace_range(0..1, upper.encode_utf8(&mut [0u8; 4]));
        }
        out
    }
}

/// ディレクトリのパスを揃え、末尾の区切り文字を落とす。
///
/// `dirs` テーブルの値と `media.parent_dir`（SQL側で切り出す）を
/// 同じ形にするために使う。
pub fn normalize_dir_str(path: &Path) -> String {
    let normalized = normalize_str(&path.to_string_lossy());
    let trimmed = normalized.trim_end_matches(['\\', '/']);
    // `C:\` のようなルート自身は削り切らない（空文字にしない）
    if trimmed.is_empty() {
        normalized
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn 区切り文字とドライブ文字を揃える() {
        assert_eq!(
            normalize_str("C:/Users/me/Pictures\\2020\\a.jpg"),
            "C:\\Users\\me\\Pictures\\2020\\a.jpg"
        );
        assert_eq!(normalize_str("c:/photos/a.jpg"), "C:\\photos\\a.jpg");
        // 既に揃っていれば変わらない
        assert_eq!(normalize_str("D:\\photos\\a.jpg"), "D:\\photos\\a.jpg");
    }

    #[cfg(windows)]
    #[test]
    fn uncとベリベイティムパスは壊さない() {
        assert_eq!(
            normalize_str("\\\\server\\share\\a.jpg"),
            "\\\\server\\share\\a.jpg"
        );
        assert_eq!(
            normalize_str("\\\\?\\c:\\photos\\a.jpg"),
            "\\\\?\\c:\\photos\\a.jpg"
        );
    }

    #[test]
    fn 経路が違っても同じ綴りになる() {
        // スキャン経由（ルートの綴りを引きずる）と USN 経由（全部バックスラッシュ）
        let from_scan = Path::new("C:/Users/me/Pictures").join("2020").join("a.jpg");
        let from_usn = Path::new("C:\\Users\\me\\Pictures\\2020\\a.jpg");
        assert_eq!(normalize(&from_scan), normalize(from_usn));
    }

    #[test]
    fn ディレクトリは末尾の区切りを落とす() {
        assert_eq!(
            normalize_dir_str(Path::new("C:/photos/2020/")),
            normalize_str("C:/photos/2020")
        );
        // ルート自身は空にしない
        assert!(!normalize_dir_str(Path::new("C:/")).is_empty());
    }
}
