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
/// 知らないコードは置かない（OS に合わせる）。
///
/// **引数の領域の今の中身に足す**（ゲート2）。`setVolatileDomain:forName:` は領域を丸ごと差し替えるので、
/// そのまま置くと起動の引数（`--args -NSDocumentRevisionsDebugMode YES` など）を捨てる。
/// **引数で `-AppleLanguages` が渡されていれば、そちらを優先して何もしない**——別の言語で説明書の
/// スクリーンショットを撮るときの、いつもの手である
pub fn apply(code: &str) {
    let Some(id) = appkit_id(code) else {
        return;
    };
    let defaults = NSUserDefaults::standardUserDefaults();
    // SAFETY: `NSArgumentDomain` は Foundation の定数
    let name = unsafe { NSArgumentDomain };
    let key = NSString::from_str("AppleLanguages");
    let (mut keys, mut values) = defaults.volatileDomainForName(name).to_vecs();
    if keys.iter().any(|k| **k == *key) {
        return;
    }
    let languages: Retained<NSArray<NSString>> =
        NSArray::from_retained_slice(&[NSString::from_str(id)]);
    keys.push(key);
    values.push(Retained::into_super(Retained::into_super(languages)));
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let domain = NSDictionary::<NSString, AnyObject>::from_retained_objects(&key_refs, &values);
    // SAFETY: 渡す辞書と名前は呼び出しのあいだ生きている
    unsafe { defaults.setVolatileDomain_forName(&domain, name) };
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

/// 画面の辞書のコード → AppKit の言語 ID（`Info.plist` の `CFBundleLocalizations` の綴り）。
/// **言語を足す人はここを見る**——辞書のコードと綴りが違う言語（中国語・葡語）はここにしか書かない
/// （`menu.rs` の升もこれを呼ぶ）
pub(crate) fn appkit_id(code: &str) -> Option<&'static str> {
    crate::os_dict::DICT_CODES
        .iter()
        .find(|c| **c == code)
        .map(|c| match *c {
            "zh" => "zh-Hans",
            "zh-hant" => "zh-Hant",
            // 葡語の辞書はブラジルの言葉。AppKit の枠も pt_BR.lproj を引かせる（ポルトガルの Mac でも）
            "pt" => "pt-BR",
            other => other,
        })
}

/// `Info.plist` に**枠のためだけに**載せている AppKit の言語 ID と、その辞書のコード。
///
/// **葡語の本国（`pt-PT`）。** 辞書はブラジルの言葉1つだが、`pt-BR` しか載せないと、言語の並びに
/// 英語も入っているポルトガルの Mac（`pt-PT, en`）で、AppKit は国をまたいで `pt-BR` へ寄らずに
/// **英語を選ぶ**（2026-11-04、`Bundle.preferredLocalizations` で確かめた）——画面は葡語なのに
/// 保存・フォルダ選択の枠だけ英語になる。本国の ID も載せて、枠は本国の訳、画面は同じ辞書にする。
///
/// **代わりにメニューバーは方言が混ざる**——アプリの項目（`Arquivo`・`Ajustes…`）は辞書のコードで
/// 引くので pt_BR、AppKit が自分で足す項目（サービスの中身・音声入力など）は pt_PT になる。
/// どちらも葡語で読めるので、枠が英語になるよりこちらを選んだ（2026-11-04、ゲート2）
const ALIASES: &[(&str, &str)] = &[("pt-PT", "pt")];

/// AppKit の言語 ID → 画面の辞書のコード（[`appkit_id`] の逆。[`ALIASES`] も引く）
fn dict_code(id: &str) -> Option<&'static str> {
    crate::os_dict::DICT_CODES
        .iter()
        .copied()
        .find(|c| appkit_id(c).is_some_and(|a| a.eq_ignore_ascii_case(id)))
        .or_else(|| {
            ALIASES
                .iter()
                .find(|(a, _)| a.eq_ignore_ascii_case(id))
                .map(|(_, c)| *c)
        })
}

#[cfg(test)]
mod tests {
    use super::{appkit_id, dict_code, ALIASES};
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
        // 枠のためだけの ID も Info.plist に在り、辞書の言語へ戻ること
        for (id, code) in ALIASES {
            assert!(
                plist.contains(&format!("<string>{id}</string>")),
                "{id} が Info.plist に無い"
            );
            assert!(DICT_CODES.contains(code), "{code} は辞書に無い");
            assert_eq!(dict_code(id), Some(*code));
        }
        assert_eq!(dict_code("pt-pt"), Some("pt"), "大文字小文字は問わない");

        // 裏返し: Info.plist の **どの** ID も辞書のコードへ戻ること。足した ID を ALIASES に
        // 書き忘れると `resolved()` が黙って None を返し、キャンセルの語が AppKit と食い違いうる
        let block = plist
            .split("<key>CFBundleLocalizations</key>")
            .nth(1)
            .and_then(|rest| rest.split("</array>").next())
            .expect("CFBundleLocalizations を読めない");
        let ids: Vec<&str> = block
            .split("<string>")
            .skip(1)
            .filter_map(|s| s.split("</string>").next())
            .collect();
        assert!(ids.len() >= DICT_CODES.len(), "読めた ID が少ない: {ids:?}");
        for id in ids {
            assert!(
                dict_code(id).is_some(),
                "{id} が辞書のコードへ戻らない（ALIASES に足すこと）"
            );
        }
    }

    /// **綴りが辞書のコードと違う言語は、値で固定する**（`menu.rs` の升は `appkit_id` を呼ぶので、
    /// ここを書き換えても向こうは落ちない）。葡語は pt-BR——設定で葡語を選んだ人の枠をブラジルの訳にする
    #[test]
    fn the_ids_that_differ_from_the_dictionary_code_are_pinned() {
        assert_eq!(appkit_id("zh"), Some("zh-Hans"));
        assert_eq!(appkit_id("zh-hant"), Some("zh-Hant"));
        assert_eq!(appkit_id("pt"), Some("pt-BR"));
        assert_eq!(appkit_id("ja"), Some("ja"));
    }

    /// 知らない言語は置かないし、返さない（OS に合わせる側へ倒す）
    #[test]
    fn an_unknown_language_is_left_alone() {
        assert_eq!(appkit_id("th"), None);
        assert_eq!(dict_code("th"), None);
        assert_eq!(dict_code("zh-HANS"), Some("zh"), "大文字小文字は問わない");
    }
}
