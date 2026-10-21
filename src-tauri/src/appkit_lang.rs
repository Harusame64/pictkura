//! **macOS の枠（AppKit）を、画面で選んだ言語に合わせる**（pictkura-dev #27）。
//!
//! ## なぜ要るのか
//!
//! 確認ダイアログの「キャンセル」、保存パネルやフォルダ選択の枠、印刷パネル——**AppKit が描く語は
//! OS の言語で出る**。画面（設定 → 言語）だけドイツ語にした日本語の Mac では、ドイツ語の文と OK の
//! 隣に日本語の「キャンセル」が並んでいた。しかも AppKit が Esc を割り当てるのは、題が AppKit の言語の
//! 「キャンセル」か英語の `Cancel` のボタンだけなので、画面の言語の語を渡すと Esc が効かない（#147）。
//!
//! ## 何をするか（2026-10-21 の実測、dev の spike27）
//!
//! 起動の頭で、**この起動の中だけで効く引数の領域（`NSArgumentDomain`）に `AppleLanguages` を置く**。
//! AppKit はそれを見て、選んだ言語を解決する（`NSBundle.preferredLocalizations`）。
//!
//! - **永続の領域には書かない。** `UserDefaults.set` で書くと次の起動でも残り、システム設定の
//!   「アプリごとの言語」と同じ物を上書きしてしまう（実測で残った）
//! - **上書きは OS の言語も隠す。** 置いたあとの `CFLocaleCopyPreferredLanguages`（`sys_locale` が読む物）は
//!   `["de"]` を返し、地域書式の `CFLocaleCopyCurrent` も `ja_JP` が `de_JP` になる。だから**OS の言語と
//!   地域書式は、置く前に読む**（`lib.rs` の `run()` が順番を持っている）
//! - **言語を切り替えた起動の中では変わらない。** AppKit は最初に1回だけ解決する。画面は
//!   「次に起動したときから」と添える（利用者の選択）
//!
//! 選んだ言語は画面の localStorage が正で、`pictkura.toml` の `[ui] locale` はその控え。

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArgumentDomain, NSArray, NSBundle, NSDictionary, NSString, NSUserDefaults,
};

/// 控えの言語を、この起動の AppKit に置く。**`run()` の頭で、OS の言語と地域書式を読んだあとに呼ぶ。**
/// 知らないコードは置かない（OS に合わせる）
pub fn apply(code: &str) {
    let Some(id) = appkit_id(code) else {
        return;
    };
    let languages: Retained<NSArray<NSString>> =
        NSArray::from_retained_slice(&[NSString::from_str(id)]);
    let key = NSString::from_str("AppleLanguages");
    let value: &AnyObject = &languages;
    let domain = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[value]);
    // SAFETY: 渡す辞書と名前は呼び出しのあいだ生きている。`NSArgumentDomain` は Foundation の定数
    unsafe {
        NSUserDefaults::standardUserDefaults().setVolatileDomain_forName(&domain, NSArgumentDomain);
    }
}

/// **AppKit が実際に解決した言語**を、画面の辞書のコードで返す。持っていない言語なら `None`。
///
/// 画面はこれで確認ダイアログの「キャンセル」の語を決める（AppKit が Esc を割り当てる語と揃える）。
/// 推し量らずに AppKit に訊く——推し量る規則は、広東語だけの Mac のような台で外れうる（#147）
pub fn resolved() -> Option<&'static str> {
    let first = NSBundle::mainBundle()
        .preferredLocalizations()
        .firstObject()?;
    dict_code(&first.to_string())
}

/// 画面の辞書のコード → AppKit の言語 ID（`Info.plist` の `CFBundleLocalizations` の綴り）
fn appkit_id(code: &str) -> Option<&'static str> {
    crate::os_dict::DICT_CODES
        .iter()
        .find(|c| **c == code)
        .map(|c| match *c {
            "zh" => "zh-Hans",
            "zh-hant" => "zh-Hant",
            other => other,
        })
}

/// AppKit の言語 ID → 画面の辞書のコード（[`appkit_id`] の逆）
fn dict_code(id: &str) -> Option<&'static str> {
    crate::os_dict::DICT_CODES
        .iter()
        .copied()
        .find(|c| appkit_id(c).is_some_and(|a| a.eq_ignore_ascii_case(id)))
}

#[cfg(test)]
mod tests {
    use super::{appkit_id, dict_code};
    use crate::os_dict::DICT_CODES;

    /// **辞書のどの言語も、AppKit の ID と行き来できる**（`Info.plist` の綴りと同じ）
    #[test]
    fn every_language_round_trips_through_its_appkit_id() {
        let plist = include_str!("../Info.plist");
        for code in DICT_CODES {
            let Some(id) = appkit_id(code) else {
                panic!("{code} に AppKit の ID が無い")
            };
            assert!(
                plist.contains(&format!("<string>{id}</string>")),
                "{id} が Info.plist に無い"
            );
            assert_eq!(dict_code(id), Some(*code));
        }
    }

    /// 知らない言語は置かないし、返さない（OS に合わせる側へ倒す）
    #[test]
    fn an_unknown_language_is_left_alone() {
        assert_eq!(appkit_id("ko"), None);
        assert_eq!(dict_code("ko"), None);
        assert_eq!(dict_code("zh-HANS"), Some("zh"), "大文字小文字は問わない");
    }
}
