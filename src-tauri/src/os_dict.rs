//! **OSの言語から、画面の辞書のコードを当てる**（台に依らない）。
//!
//! 画面が立つ前に語を出す場所が2つある——macOS のメニューバーの繋ぎ（[`crate::menu`]）と、
//! **WebView2 が無いときの案内**（[`crate::webview2_check`]、pictkura-dev #72）。後者は画面が
//! そもそも立たないので、利用者の言語の選択（localStorage）は読めない。OS の言語で出す。
//!
//! **規則を2か所に置かない**ためにここへ出した（メニューの中にあったものをそのまま移した）。
//! 画面の本当の規則は `ui/src/i18n/index.ts` の `pickLocale()` で、これはその当て推量である
//! ——中国語の書き言葉とスペイン語の地域だけ見分けて、あとは言語の副タグ。

/// 画面の辞書のコード。**`ui/src/i18n/index.ts` の `DICTS` と同じ顔ぶれ**（下の升が見張る）
pub const DICT_CODES: &[&str] = &["en", "ja", "de", "es", "es-419", "zh", "zh-hant"];

/// OS の言語の並びから、辞書のコードを1つ当てる。知らない言語ばかりなら英語
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub fn guess(os_locales: &[String]) -> &'static str {
    for tag in os_locales {
        let tag = tag.to_lowercase();
        // **粵語もここ。** `pickLocale()` が `yue` を書き言葉として繁体字へ寄せている
        // （香港・マカオの書き言葉は繁体字の中国語）ので、当て推量も同じ側へ寄せる
        // ——ここだけ英語へ落とすと、**繁体字のMacで毎回英語のメニューが一瞬見える**、
        // というこの節が消したい形がそのまま残る（ゲート2）
        if tag.starts_with("zh") || tag.starts_with("yue") {
            // **`Hans` と書いてあれば地域より優先**。書いていなければ地域で見て、
            // 粵語は既定で繁体字（CLDRの既定が `yue-Hant-HK`）
            let hans = tag.contains("hans");
            let hant = tag.contains("hant")
                || tag.starts_with("yue")
                || tag.contains("-tw")
                || tag.contains("-hk")
                || tag.contains("-mo");
            return if hant && !hans { "zh-hant" } else { "zh" };
        }
        // **スペイン語も地域で割れる**（dev #17）。`pickLocale()` は
        // **`ES` 以外の地域つき**を中南米（`es-419`）へ寄せるので、繋ぎも同じ側へ寄せる
        // ——ここだけ本国に倒すと、メキシコのMacで**一瞬だけ本国のスペイン語**が出る
        if tag == "es" || tag.starts_with("es-") {
            // **1文字の副タグで止める**（ゲート1）。BCP-47 では1文字は拡張の始まりで、
            // `es-u-nu-latn` の `nu` は**2文字だが地域ではない**
            let region = tag
                .split('-')
                .skip(1)
                .take_while(|p| p.len() != 1)
                .find(|p| p.len() == 2 || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit())));
            return match region {
                None | Some("es") => "es",
                _ => "es-419",
            };
        }
        // **前方一致では足りない**（ゲート2）。副タグの切れ目で切らないと
        // `jam-JM`（ジャマイカ・クレオール）が `ja` に、`esu`（ユピック）が `es` に当たる
        if let Some(code) = DICT_CODES
            .iter()
            .find(|c| !c.starts_with("zh") && (tag == **c || tag.starts_with(&format!("{c}-"))))
        {
            return code;
        }
    }
    "en"
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{guess, DICT_CODES};

    /// `ui/src/i18n/index.ts` の `DICTS` に並んでいる言語コード。
    ///
    /// **1行だと決め打ちしない**（ゲート2）。7つ目を足せば行は折り返されるし、
    /// **最後の要素には末尾のカンマが無い**——**言語を足したときにだけ壊れる歯止め**は、
    /// 守りたい場面でちょうど役に立たない。だから中括弧の中を素直に読む。
    pub(crate) fn dict_codes() -> Vec<String> {
        let src = include_str!("../../ui/src/i18n/index.ts");
        let Some(at) = src.find("const DICTS: Record<string, Dict>") else {
            panic!("index.ts に DICTS が無い（綴りが変わった？）")
        };
        let rest = src.get(at..).unwrap_or_default();
        let (Some(open), Some(close)) = (rest.find('{'), rest.find('}')) else {
            panic!("DICTS の中括弧を読めない")
        };
        let inner = rest.get(open + 1..close).unwrap_or_default();
        inner
            .split(',')
            .filter_map(|part| {
                let name = part
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"');
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect()
    }

    /// **顔ぶれは `DICTS` と揃っていること**——片方だけ足すと、その言語の人に英語が出る
    #[test]
    fn the_codes_are_the_ones_the_app_ships() {
        let mut codes = dict_codes();
        let mut ours: Vec<String> = DICT_CODES.iter().map(|c| c.to_string()).collect();
        assert!(codes.len() >= 2, "DICTS を読めていない: {codes:?}");
        codes.sort();
        ours.sort();
        assert_eq!(codes, ours);
    }

    /// **繋ぎの当て推量**。込み入ったことはしないが、書き言葉つきの中国語だけは分ける
    #[test]
    fn the_provisional_guess_splits_the_two_chinese_scripts() {
        assert_eq!(guess(&["ja-JP".into()]), "ja");
        assert_eq!(guess(&["de-DE".into()]), "de");
        assert_eq!(guess(&["en-GB".into()]), "en");
        // 簡体字と繁体字
        assert_eq!(guess(&["zh-Hans-CN".into()]), "zh");
        assert_eq!(guess(&["zh-CN".into()]), "zh");
        assert_eq!(guess(&["zh-TW".into()]), "zh-hant");
        assert_eq!(guess(&["zh-Hant-HK".into()]), "zh-hant");
        assert_eq!(guess(&["zh-MO".into()]), "zh-hant");
        // **`Hans` と書いてあれば地域より優先**（`pickLocale()` と同じ）
        assert_eq!(guess(&["zh-Hans-TW".into()]), "zh");
        // **粵語も繁体字へ。** ここを英語へ落とすと、この節が消したい
        // 「英語のメニューが一瞬見える」がそのまま起きる（ゲート2）
        assert_eq!(guess(&["yue-Hant-HK".into()]), "zh-hant");
        assert_eq!(guess(&["yue".into()]), "zh-hant");
        // **副タグの切れ目で切る。** 前方一致だけだと `ja` / `es` に当たってしまう
        assert_eq!(guess(&["jam-JM".into()]), "en");
        assert_eq!(guess(&["esu".into()]), "en");
        // **スペイン語は地域で本国と中南米に割れる**（dev #17。`pickLocale()` と同じ規則）
        assert_eq!(guess(&["es-ES".into()]), "es");
        assert_eq!(guess(&["es".into()]), "es", "地域が分からなければ本国側");
        assert_eq!(guess(&["es-MX".into()]), "es-419");
        assert_eq!(guess(&["es-419".into()]), "es-419");
        assert_eq!(guess(&["es-Latn-MX".into()]), "es-419");
        // **拡張は地域ではない**（`-u-` の先の `nu` / `hc` は2文字だが国ではない）
        assert_eq!(guess(&["es-u-nu-latn".into()]), "es");
        assert_eq!(guess(&["es-u-hc-h23".into()]), "es");
        // 知らない言語は飛ばして次を見る。全部知らなければ英語
        assert_eq!(guess(&["th-TH".into(), "ja-JP".into()]), "ja");
        assert_eq!(guess(&["th-TH".into()]), "en");
        assert_eq!(guess(&[]), "en");
    }
}
