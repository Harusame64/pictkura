//! SVG（ベクタ画像）の扱い。
//!
//! ラスタライザは持ち込まない。**WebViewがSVGをそのまま描ける**ので、
//! 一覧のタイルもビューアも原本を配ればよく、サムネイルを作る必要がない
//! （resvg等を抱えるとフォント一式まで付いてきて、配布物も起動時間も膨らむ）。
//! 拡大しても劣化しないというベクタの利点もそのまま残る。
//!
//! アプリ側に要るのは**寸法だけ**。グリッドが描画前に枠を確保するために使う。
//! ルート要素の `width`/`height`、無ければ `viewBox` から読む。
//!
//! XMLパーサも持ち込まない。必要なのは開始タグ1つ分の属性なので、
//! 先頭を少し読んで `<svg ...>` の中を舐めれば足りる。

use std::io::Read;
use std::path::Path;

/// ルート要素を探して読む上限。装飾的なコメントやDOCTYPEが前に付くことがある。
const HEAD_LIMIT: usize = 64 * 1024;

/// 拡張子がSVGか。
pub fn is_svg_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("svg")
}

/// パスの拡張子がSVGか。
pub fn is_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_svg_extension)
}

/// 表示上の寸法を読む。読めなければ None。
///
/// 単位付き（`100px` `10mm` `50%`）の値は、グリッドが必要とするのは
/// **縦横比**なので数値部分だけを採る。`%` は基準が無いので使わない。
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    let head = read_head(path)?;
    let text = String::from_utf8_lossy(&head);
    let tag = svg_open_tag(&text)?;

    let width = attribute(tag, "width").and_then(parse_length);
    let height = attribute(tag, "height").and_then(parse_length);
    if let (Some(w), Some(h)) = (width, height) {
        if w > 0.0 && h > 0.0 {
            return Some((w.round() as u32, h.round() as u32));
        }
    }

    // width/height が無い（レスポンシブなSVG）なら viewBox の後ろ2つが寸法
    let view_box = attribute(tag, "viewBox")?;
    let parts: Vec<f64> = view_box
        .split([' ', ',', '\t', '\n', '\r'])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let (w, h) = (*parts.get(2)?, *parts.get(3)?);
    (w > 0.0 && h > 0.0).then(|| (w.round() as u32, h.round() as u32))
}

/// ファイルの先頭を読む。
fn read_head(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(HEAD_LIMIT as u64).read_to_end(&mut buf).ok()?;
    (!buf.is_empty()).then_some(buf)
}

/// `<svg ...>` の開始タグの中身（属性が並ぶ部分）を切り出す。
fn svg_open_tag(text: &str) -> Option<&str> {
    let mut from = 0usize;
    while let Some(offset) = text[from..].find("<svg") {
        let start = from + offset + 4;
        // `<svgfoo` のような別の要素を拾わない
        let next = text[start..].chars().next();
        if !matches!(next, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
            from = start;
            continue;
        }
        let end = text[start..].find('>')? + start;
        return Some(&text[start..end]);
    }
    None
}

/// 開始タグの中から属性値を取り出す（`name="値"` / `name='値'`）。
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0usize;
    while let Some(offset) = tag[from..].find(name) {
        let at = from + offset;
        from = at + name.len();
        // 直前が属性の切れ目であること（`stroke-width` の `width` を拾わない）
        let before_ok = at == 0
            || tag[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace());
        if !before_ok {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let value = &rest[quote.len_utf8()..];
        let end = value.find(quote)?;
        return Some(&value[..end]);
    }
    None
}

/// `100` `100px` `10mm` のような長さを **CSSピクセル**へ直す。
///
/// 単位を落として数値だけ採ると、`width="10cm" height="100px"` のように
/// **幅と高さで単位が違う**ファイルで縦横比が壊れる（10cm は約378px なのに
/// 1:10 になってしまう）。CSSの絶対単位は px との比が決まっているので換算する。
///
/// `%` と `em`/`ex`（フォント基準）は基準が無いので None を返し、
/// 呼び出し側で `viewBox` に落とす。
fn parse_length(value: &str) -> Option<f64> {
    /// CSSの絶対単位と、1単位あたりのピクセル数（1in = 96px）
    const UNITS: &[(&str, f64)] = &[
        ("px", 1.0),
        ("pt", 96.0 / 72.0),
        ("pc", 16.0),
        ("in", 96.0),
        ("cm", 96.0 / 2.54),
        ("mm", 96.0 / 25.4),
        ("q", 96.0 / 25.4 / 4.0),
    ];

    let value = value.trim();
    let digits: String = value
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    let number: f64 = digits.parse().ok()?;
    let unit = value[digits.len()..].trim().to_ascii_lowercase();
    if unit.is_empty() {
        // 単位なしはユーザー単位＝CSSピクセル
        return Some(number);
    }
    let scale = UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, scale)| *scale)?;
    Some(number * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.svg");
        std::fs::write(&path, body).unwrap();
        (dir, path)
    }

    #[test]
    fn recognises_the_svg_extensions() {
        assert!(is_svg_extension("svg"));
        assert!(is_svg_extension("SVG"));
        assert!(!is_svg_extension("png"));
    }

    #[test]
    fn reads_the_size_from_width_and_height() {
        let (_d, p) = write_temp(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="600"></svg>"#,
        );
        assert_eq!(dimensions(&p), Some((800, 600)));
    }

    #[test]
    fn the_number_is_taken_even_with_a_unit_attached() {
        let (_d, p) = write_temp(r#"<svg width="100px" height="50.5px"></svg>"#);
        assert_eq!(dimensions(&p), Some((100, 51)));
    }

    #[test]
    fn without_a_width_the_viewbox_is_used() {
        let (_d, p) = write_temp(r#"<svg viewBox="0 0 1024 768"></svg>"#);
        assert_eq!(dimensions(&p), Some((1024, 768)));
    }

    #[test]
    fn different_units_on_width_and_height_do_not_break_the_ratio() {
        // 単位を落として数値だけ採ると 1:10 になってしまう。
        // 10cm ≒ 378px なので、正しくは横長
        let (_d, p) = write_temp(r#"<svg width="10cm" height="100px"></svg>"#);
        let (w, h) = dimensions(&p).unwrap();
        assert_eq!((w, h), (378, 100));
    }

    #[test]
    fn the_common_css_units_are_converted() {
        for (value, expected) in [
            ("96px", 96u32),
            ("1in", 96),
            ("72pt", 96),
            ("6pc", 96),
            ("25.4mm", 96),
            ("100", 100),
        ] {
            let body = format!(r#"<svg width="{value}" height="{value}"></svg>"#);
            let (_d, p) = write_temp(&body);
            assert_eq!(dimensions(&p), Some((expected, expected)), "{value}");
        }
    }

    #[test]
    fn font_relative_units_fall_back_to_the_viewbox() {
        // em/ex は font-size 次第なので寸法にできない
        let (_d, p) = write_temp(r#"<svg width="10em" height="5em" viewBox="0 0 20 10"></svg>"#);
        assert_eq!(dimensions(&p), Some((20, 10)));
    }

    #[test]
    fn a_percentage_falls_back_to_the_viewbox() {
        // レスポンシブなSVGの典型。%は基準が無いので寸法にできない
        let (_d, p) = write_temp(r#"<svg width="100%" height="100%" viewBox="0 0 16 9"></svg>"#);
        assert_eq!(dimensions(&p), Some((16, 9)));
    }

    #[test]
    fn a_declaration_or_comment_in_front_still_reads() {
        let (_d, p) = write_temp(
            "<?xml version=\"1.0\"?>\n<!-- 作った人 -->\n<svg\n  width='40'\n  height='20'>\n</svg>",
        );
        assert_eq!(dimensions(&p), Some((40, 20)));
    }

    #[test]
    fn a_similarly_named_attribute_is_not_picked_up() {
        // stroke-width の width を拾うと寸法を間違える
        let (_d, p) = write_temp(r#"<svg stroke-width="3" viewBox="0 0 10 20"></svg>"#);
        assert_eq!(dimensions(&p), Some((10, 20)));
    }

    #[test]
    fn a_file_that_is_not_svg_yields_none() {
        let (_d, p) = write_temp("<html><body>これはSVGではない</body></html>");
        assert_eq!(dimensions(&p), None);
    }

    #[test]
    fn no_size_yields_none() {
        let (_d, p) = write_temp(r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#);
        assert_eq!(dimensions(&p), None);
    }
}
