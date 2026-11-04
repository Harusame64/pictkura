//! **向きの閉じ（FSI/LRI/RLI … PDI）を、古い埋め込み（LRE/RLE … PDF）に置き換える**。
//!
//! Windows の TaskDialog（`confirm_dialog.rs`）は閉じの印を解さない。点線の箱で描いたうえ、
//! アラビア語で終わる名前を周りの文へ混ぜた（#283 の win の実機）。埋め込みは古い描き手でも効く。
//! 辞書（`ui/src/i18n/ar.ts` の `iso()`）は画面のために閉じで書くので、**箱へ渡す直前にここで直す**。
//!
//! 字の向きは `unicode-bidi` の分類で引く（Unicode の双方向アルゴリズムと同じ表）:
//!
//! - FSI の向きは、中で最初に出る強い文字（L・R・AL）で決める。入れ子の中と、段落の切れ目（B）より
//!   先は見ない。何も無ければ左から右（UAX #9 の P2・P3）
//! - **対の無い PDI は捨てる**——アルゴリズムでも何もしない文字で、PDF に変えると開いている埋め込みを閉じる
//! - **閉じの境目を守る**: 閉じの中で開いたまま残った埋め込みは PDI の位置で閉じ、閉じの中の対の無い PDF は捨てる

use unicode_bidi::{bidi_class, BidiClass};

const LRE: char = '\u{202a}';
const RLE: char = '\u{202b}';
const PDF: char = '\u{202c}';

#[cfg_attr(not(windows), allow(dead_code))]
pub fn isolates_to_embeddings(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    // 開いている閉じごとに、その中で開いた埋め込み（LRE・RLE・LRO・RLO）の数
    let mut inner: Vec<usize> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        match bidi_class(c) {
            BidiClass::B => {
                inner.clear();
                out.push(c);
            }
            BidiClass::LRI => {
                inner.push(0);
                out.push(LRE);
            }
            BidiClass::RLI => {
                inner.push(0);
                out.push(RLE);
            }
            BidiClass::FSI => {
                inner.push(0);
                out.push(if first_strong_is_rtl(&chars[i + 1..]) {
                    RLE
                } else {
                    LRE
                });
            }
            BidiClass::PDI => {
                if let Some(open) = inner.pop() {
                    out.extend(std::iter::repeat_n(PDF, open + 1));
                }
            }
            BidiClass::LRE | BidiClass::RLE | BidiClass::LRO | BidiClass::RLO => {
                if let Some(open) = inner.last_mut() {
                    *open += 1;
                }
                out.push(c);
            }
            BidiClass::PDF => match inner.last_mut() {
                Some(0) => {}
                Some(open) => {
                    *open -= 1;
                    out.push(c);
                }
                None => out.push(c),
            },
            _ => out.push(c),
        }
    }
    out
}

/// FSI の後ろから、対になる PDI（か段落の終わり）までで最初の強い文字が右から左か
fn first_strong_is_rtl(rest: &[char]) -> bool {
    let mut depth = 0usize;
    for &c in rest {
        match bidi_class(c) {
            BidiClass::B => return false,
            BidiClass::LRI | BidiClass::RLI | BidiClass::FSI => depth += 1,
            BidiClass::PDI => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            BidiClass::L if depth == 0 => return false,
            BidiClass::R | BidiClass::AL if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::isolates_to_embeddings as conv;

    #[test]
    fn a_path_that_starts_latin_stays_left_to_right_even_if_it_ends_in_arabic() {
        // win の実機で割れた形
        assert_eq!(
            conv("نقل \u{2068}C:\\Temp\\pk283 اختبار\u{2069}؟"),
            "نقل \u{202a}C:\\Temp\\pk283 اختبار\u{202c}؟"
        );
    }

    #[test]
    fn fsi_takes_the_first_strong_character() {
        // 数字は強くない
        assert_eq!(conv("\u{2068}2024 صور\u{2069}"), "\u{202b}2024 صور\u{202c}");
        assert_eq!(conv("\u{2068}٢٠٢٤ a\u{2069}"), "\u{202a}٢٠٢٤ a\u{202c}");
        assert_eq!(conv("\u{2068}२०२४ صور\u{2069}"), "\u{202a}२०२४ صور\u{202c}");
        // 字でなくても強い（؟ は AL、RLM は R、LRM は L）。字でも強くない（ˈ は ON）
        assert_eq!(conv("\u{2068}؟abc\u{2069}"), "\u{202b}؟abc\u{202c}");
        assert_eq!(
            conv("\u{2068}\u{200f}abc\u{2069}"),
            "\u{202b}\u{200f}abc\u{202c}"
        );
        assert_eq!(
            conv("\u{2068}\u{200e}ب\u{2069}"),
            "\u{202a}\u{200e}ب\u{202c}"
        );
        assert_eq!(conv("\u{2068}ˈصور\u{2069}"), "\u{202b}ˈصور\u{202c}");
        // 画面の言語に無い字種・補助面の字も表で引く（Adlam・Phoenician は右から左、Brahmi は左から右）
        assert_eq!(
            conv("\u{2068}\u{1E900}\u{2069}"),
            "\u{202b}\u{1E900}\u{202c}"
        );
        assert_eq!(
            conv("\u{2068}\u{10900}\u{2069}"),
            "\u{202b}\u{10900}\u{202c}"
        );
        assert_eq!(
            conv("\u{2068}\u{11005}\u{2069}"),
            "\u{202a}\u{11005}\u{202c}"
        );
        // 強い文字が無ければ左から右
        assert_eq!(conv("\u{2068}123\u{2069}"), "\u{202a}123\u{202c}");
    }

    #[test]
    fn fsi_does_not_look_inside_nested_isolates_or_past_the_paragraph() {
        assert_eq!(
            conv("\u{2068}\u{2067}ب\u{2069}a\u{2069}"),
            "\u{202a}\u{202b}ب\u{202c}a\u{202c}"
        );
        assert_eq!(conv("\u{2068}123\nب"), "\u{202a}123\nب");
        // NEL も段落の切れ目
        assert_eq!(conv("\u{2068}123\u{85}ب"), "\u{202a}123\u{85}ب");
    }

    #[test]
    fn lri_and_rli_map_directly_and_plain_text_is_untouched() {
        assert_eq!(
            conv("\u{2066}a\u{2069}\u{2067}b\u{2069}"),
            "\u{202a}a\u{202c}\u{202b}b\u{202c}"
        );
        assert_eq!(conv("abc"), "abc");
        assert!(!conv("\u{2068}x\u{2069}")
            .chars()
            .any(|c| ('\u{2066}'..='\u{2069}').contains(&c)));
    }

    #[test]
    fn unmatched_controls_do_not_break_the_boundary() {
        // 対の無い PDI は捨てる（開いている埋め込みを閉じない）
        assert_eq!(conv("\u{2069}x"), "x");
        assert_eq!(conv("\u{202b}ب\u{2069}x\u{202c}"), "\u{202b}بx\u{202c}");
        // 閉じの中で開いたままの埋め込みは、PDI で一緒に閉じる
        assert_eq!(
            conv("\u{2068}a\u{202b}b\u{2069}c"),
            "\u{202a}a\u{202b}b\u{202c}\u{202c}c"
        );
        // 閉じの中の対の無い PDF は捨てる
        assert_eq!(conv("\u{2068}a\u{202c}b\u{2069}c"), "\u{202a}ab\u{202c}c");
        // 閉じの外の埋め込みには触らない
        assert_eq!(conv("\u{202b}a\u{202c}"), "\u{202b}a\u{202c}");
    }
}
