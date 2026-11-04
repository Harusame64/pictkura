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
//!
//! **閉じと埋め込みは同じではない**。閉じは外から見ると中立の1文字だが、埋め込みの前後の中立の文字は
//! 埋め込みの向きに引かれる。名前の一覧（`pk1, pk2`。名前ごとに閉じてあり、区切りは外。
//! `ui/src/direction.ts` の `isolateEach`）では、左から右の名前に挟まれた `, ` が左から右に解かれ、
//! 一覧ごと左から右に並んで、右から読む人に2つ目の名前が先に来た（#284 のゲート2）。名前の直後の
//! 数字も名前の側に寄る。だから**外側の閉じを置き換えるたびに、前後に箱の向きの強い印を置く**
//! （`mark`。右から左の箱なら RLM）——閉じが中立だったのと同じく、前後の文字が文の向きで解かれる
//!
//! 閉じの境目を壊す文字は、辞書と画面の `isolate()`（`ui/src/direction.ts`）が先に外している。
//! ここでも守るのは、そこを通らない文が箱に来たときのため

use unicode_bidi::{bidi_class, BidiClass};

const LRE: char = '\u{202a}';
const RLE: char = '\u{202b}';
const PDF: char = '\u{202c}';
const RLM: char = '\u{200f}';

/// 右から左の箱に渡す文（`confirm_dialog.rs`）。外側の閉じの前後に RLM を置く
#[cfg_attr(not(windows), allow(dead_code))]
pub fn for_right_to_left_box(s: &str) -> String {
    isolates_to_embeddings(s, Some(RLM))
}

/// `mark` は外側の閉じを置き換えた前後に置く強い印（箱の段落の向き）。`None` なら置かない
fn isolates_to_embeddings(s: &str, mark: Option<char>) -> String {
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
                open_outer(&mut out, &inner, mark);
                inner.push(0);
                out.push(LRE);
            }
            BidiClass::RLI => {
                open_outer(&mut out, &inner, mark);
                inner.push(0);
                out.push(RLE);
            }
            BidiClass::FSI => {
                open_outer(&mut out, &inner, mark);
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
                    // 外側の閉じを閉じたら、後ろにも印
                    if inner.is_empty() {
                        out.extend(mark);
                    }
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

/// 外側の閉じを開く前に印を置く
fn open_outer(out: &mut String, inner: &[usize], mark: Option<char>) {
    if inner.is_empty() {
        out.extend(mark);
    }
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
    use super::{for_right_to_left_box, isolates_to_embeddings};

    fn conv(s: &str) -> String {
        isolates_to_embeddings(s, None)
    }

    #[test]
    fn the_right_to_left_box_keeps_a_list_of_names_in_sentence_order() {
        // 名前ごとに閉じた一覧（#284 のゲート2）。区切りの `, ` が RLM に挟まれて右から左に解かれる
        assert_eq!(
            for_right_to_left_box("מקומות: \u{2068}C:\\a\u{2069}, \u{2068}D:\\b\u{2069}."),
            "מקומות: \u{200f}\u{202a}C:\\a\u{202c}\u{200f}, \u{200f}\u{202a}D:\\b\u{202c}\u{200f}."
        );
        // 入れ子の内側には置かない
        assert_eq!(
            for_right_to_left_box("\u{2068}a\u{2067}ب\u{2069}\u{2069}"),
            "\u{200f}\u{202a}a\u{202b}ب\u{202c}\u{202c}\u{200f}"
        );
        // 閉じの無い文は変えない
        assert_eq!(for_right_to_left_box("abc"), "abc");
    }

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
