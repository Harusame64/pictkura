//! 新しい版が出ていないかの判定（0.2）。
//!
//! ここに置くのは**版の比べ方だけ**で、通信はアプリ側（`src-tauri`）が持つ。
//! 分けているのは、比較が「配っている版より新しいかどうか」というただ1つの
//! 判断に効くのに対し、通信は差し替えの効く外側だから——ここならテストが回る。
//!
//! semverクレートは使わない。こちらが打つタグは `v0.2.0` のような素直な3つ組
//! だけで、プレリリースやビルドメタの前後関係は要らない。**読めなかったものは
//! 「新しくない」に倒す**（読めない文字列で「更新があります」と言わない）。

/// `v0.2.0` / `0.2` / `0.2.0-rc1` のような文字列を `(major, minor, patch)` にする。
///
/// - 先頭の `v` / `V` は落とす
/// - `-` や `+` から後ろ（プレリリース・ビルドメタ）は**見ない**
/// - 足りない桁は0で埋める（`0.2` は `0.2.0`）
/// - 数字以外が混じっていたら `None`
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split(['-', '+']).next().unwrap_or(s);
    if s.is_empty() {
        return None;
    }
    let mut parts = s.split('.');
    let mut out = [0u32; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        match parts.next() {
            Some(p) => *slot = p.parse().ok()?,
            // 桁が足りないのは許す（`0.2` → `0.2.0`）が、先頭が無いのは駄目
            None if i > 0 => break,
            None => return None,
        }
    }
    // 4桁目以降が付いていたら、こちらの想定していない書式なので触らない
    if parts.next().is_some() {
        return None;
    }
    Some((out[0], out[1], out[2]))
}

/// `candidate` が `current` より新しいか。
///
/// **どちらか一方でも読めなければ false**。「新しい版があります」は
/// 押させる合図なので、確かなときだけ出す。
pub fn is_newer(current: &str, candidate: &str) -> bool {
    match (parse_version(current), parse_version(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_v_and_omitted_digits_are_read() {
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("V1.0"), Some((1, 0, 0)));
        assert_eq!(parse_version(" 0.1.1 "), Some((0, 1, 1)));
        assert_eq!(parse_version("2"), Some((2, 0, 0)));
    }

    #[test]
    fn prerelease_and_build_metadata_are_ignored() {
        assert_eq!(parse_version("v0.2.0-rc1"), Some((0, 2, 0)));
        assert_eq!(parse_version("0.2.0+win"), Some((0, 2, 0)));
    }

    #[test]
    fn an_unparsable_version_yields_none() {
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("v"), None);
        assert_eq!(parse_version("latest"), None);
        assert_eq!(parse_version("0.2.x"), None);
        assert_eq!(parse_version("0.2.0.1"), None);
    }

    #[test]
    fn compared_by_number_not_by_string_order() {
        assert!(is_newer("0.9.9", "v0.10.0"));
        assert!(!is_newer("0.10.0", "v0.9.9"));
        assert!(is_newer("0.1.1", "v0.2.0"));
        assert!(is_newer("0.1.1", "v0.1.2"));
        assert!(is_newer("0.1.1", "v1.0.0"));
    }

    #[test]
    fn the_same_or_older_is_not_newer() {
        assert!(!is_newer("0.2.0", "v0.2.0"));
        assert!(!is_newer("0.2.0", "0.2"));
        assert!(!is_newer("0.2.0", "v0.1.9"));
    }

    #[test]
    fn an_unreadable_side_is_not_newer() {
        assert!(!is_newer("0.2.0", "latest"));
        assert!(!is_newer("", "v0.3.0"));
        assert!(!is_newer("nightly", "v9.9.9"));
    }
}
