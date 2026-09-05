//! macOSのメニューバーを、**アプリの言語で組む**（Issue #14）。
//!
//! ## なぜ要るのか
//!
//! Tauriの既定のメニューは**項目名を英語で決め打ち**している。#13 で足した
//! `Info.plist` の `CFBundleLocalizations` は**OSが出す枠**（保存パネル・
//! フォルダ選択・削除の確認）には効くが、**Tauriが自分で組む項目名には届かない**。
//! 日本語のMacでも `File / Edit / View / Window / Help` のまま出ていた。
//!
//! ## 語は**macOS自身の訳から引く。こちらで訳さない**
//!
//! `ファイル` / `ほかを非表示` / `拡大/縮小` は**OSの語**であって、こちらの言葉ではない。
//! 6言語ぶん手で書けば、どこかで**他のMacアプリと違う語**になる——利用者から見れば
//! それは「訳が下手」ではなく「**このアプリだけ違う**」に見える。
//! 独語の `„%@“ ausblenden`（かぎ括弧つき）のような綴りは、手では出てこない。
//!
//! 下の表は **`tools/menu-strings.py`** が
//! `SwiftUI.framework` の `MainMenu.loctable` と `AppKit.framework` の
//! `MenuCommands.loctable` から引いて作る。**手で直さない。**
//!
//! ## どの言語で組むかを決めるのは**画面側**
//!
//! 言語は localStorage の指定が最優先で、その規則は
//! `ui/src/i18n/index.ts` の `pickLocale()` **にしかない**。**ここで作り直さない**
//! ——同じ規則を2か所に置くと、片方だけ直したときに黙ってずれる。
//! 画面が読み込まれた時点で [`crate::set_menu_locale`] が教えてくる。
//!
//! **起動の一瞬だけ、OSの言語から当てておく**（[`provisional`]）。
//! これは規則の写しではなく**画面が確定するまでの繋ぎ**で、
//! 食い違っても**ページが読み込まれた時点で直る**（数百ミリ秒）。
//! 当てずに英語のまま出すと、**日本語のMacで毎回英語のメニューが一瞬見える**。
//!
//! ## 言語を1つ足すときは、ここも足す
//!
//! `ui/src/i18n/` の辞書・`index.ts` の `DICTS` と `LOCALES`・`Info.plist` の
//! `CFBundleLocalizations`・**この表**の4か所。`tools/menu-strings.py` の `LOCALES` に
//! 1行足して回し直せば、この表は自動で出る。

use tauri::menu::{
    AboutMetadata, Menu, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Runtime};

/// メニュー1枚ぶんの語。**全部 macOS の訳**（上のとおり手で書かない）。
struct MenuText {
    about: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    quit: &'static str,
    file: &'static str,
    close_window: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    view: &'static str,
    fullscreen: &'static str,
    window: &'static str,
    minimize: &'static str,
    zoom: &'static str,
    help: &'static str,
}

// **ここから下は `tools/menu-strings.py` が作る。手で直さない。**
// 出どころは macOS 自身の訳（SwiftUI の MainMenu.loctable と
// AppKit の MenuCommands.loctable）。**OSの語なので、こちらで訳さない。**

static EN: MenuText = MenuText {
    about: "About %@",
    services: "Services",
    hide: "Hide %@",
    hide_others: "Hide Others",
    quit: "Quit %@",
    file: "File",
    close_window: "Close Window",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    view: "View",
    fullscreen: "Enter Full Screen",
    window: "Window",
    minimize: "Minimize",
    zoom: "Zoom",
    help: "Help",
};

static JA: MenuText = MenuText {
    about: "%@について",
    services: "サービス",
    hide: "%@を非表示",
    hide_others: "ほかを非表示",
    quit: "%@を終了",
    file: "ファイル",
    close_window: "ウインドウを閉じる",
    edit: "編集",
    undo: "取り消す",
    redo: "やり直す",
    cut: "カット",
    copy: "コピー",
    paste: "ペースト",
    select_all: "すべてを選択",
    view: "表示",
    fullscreen: "フルスクリーンにする",
    window: "ウインドウ",
    minimize: "しまう",
    zoom: "拡大/縮小",
    help: "ヘルプ",
};

static DE: MenuText = MenuText {
    about: "Über „%@“",
    services: "Dienste",
    hide: "„%@“ ausblenden",
    hide_others: "Andere ausblenden",
    quit: "„%@“ beenden",
    file: "Ablage",
    close_window: "Fenster schließen",
    edit: "Bearbeiten",
    undo: "Widerrufen",
    redo: "Wiederholen",
    cut: "Ausschneiden",
    copy: "Kopieren",
    paste: "Einsetzen",
    select_all: "Alles auswählen",
    view: "Darstellung",
    fullscreen: "Vollbildmodus",
    window: "Fenster",
    minimize: "Im Dock ablegen",
    zoom: "Zoomen",
    help: "Hilfe",
};

static ES: MenuText = MenuText {
    about: "Acerca de %@",
    services: "Servicios",
    hide: "Ocultar %@",
    hide_others: "Ocultar otras apps",
    quit: "Salir de %@",
    file: "Archivo",
    close_window: "Cerrar ventana",
    edit: "Edición",
    undo: "Deshacer",
    redo: "Rehacer",
    cut: "Cortar",
    copy: "Copiar",
    paste: "Pegar",
    select_all: "Seleccionar todo",
    view: "Visualización",
    fullscreen: "Usar pantalla completa",
    window: "Ventana",
    minimize: "Minimizar",
    zoom: "Zoom",
    help: "Ayuda",
};

static ES_419: MenuText = MenuText {
    about: "Acerca de %@",
    services: "Servicios",
    hide: "Ocultar %@",
    hide_others: "Ocultar otros",
    quit: "Salir de %@",
    file: "Archivo",
    close_window: "Cerrar ventana",
    edit: "Edición",
    undo: "Deshacer",
    redo: "Rehacer",
    cut: "Cortar",
    copy: "Copiar",
    paste: "Pegar",
    select_all: "Seleccionar todo",
    view: "Visualización",
    fullscreen: "Ver en pantalla completa",
    window: "Ventana",
    minimize: "Minimizar",
    zoom: "Maximizar",
    help: "Ayuda",
};

static ZH: MenuText = MenuText {
    about: "关于%@",
    services: "服务",
    hide: "隐藏%@",
    hide_others: "隐藏其他",
    quit: "退出%@",
    file: "文件",
    close_window: "关闭窗口",
    edit: "编辑",
    undo: "撤销",
    redo: "重做",
    cut: "剪切",
    copy: "拷贝",
    paste: "粘贴",
    select_all: "全选",
    view: "显示",
    fullscreen: "进入全屏幕",
    window: "窗口",
    minimize: "最小化",
    zoom: "缩放",
    help: "帮助",
};

static ZH_HANT: MenuText = MenuText {
    about: "關於%@",
    services: "服務",
    hide: "隱藏%@",
    hide_others: "隱藏其他",
    quit: "結束%@",
    file: "檔案",
    close_window: "關閉視窗",
    edit: "編輯",
    undo: "還原",
    redo: "重做",
    cut: "剪下",
    copy: "拷貝",
    paste: "貼上",
    select_all: "全選",
    view: "顯示方式",
    fullscreen: "進入全螢幕",
    window: "視窗",
    minimize: "縮到最小",
    zoom: "縮放",
    help: "輔助說明",
};

/// 辞書のコードと、その語。**`ui/src/i18n/index.ts` の `DICTS` と同じ顔ぶれ**
static TEXTS: &[(&str, &MenuText)] = &[
    ("en", &EN),
    ("ja", &JA),
    ("de", &DE),
    ("es", &ES),
    ("es-419", &ES_419),
    ("zh", &ZH),
    ("zh-hant", &ZH_HANT),
];

/// 辞書のコードに対応する語を返す。**知らないコードは英語へ倒す**
/// （画面側が知らない言語を送ってくることは無いが、倒す先を決めておく）。
fn text_for(code: &str) -> &'static MenuText {
    TEXTS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, t)| *t)
        .unwrap_or(&EN)
}

/// **OSの言語から、繋ぎの1つを当てる。**
///
/// **これは `pickLocale()` の写しではない。** 画面が `set_menu_locale` を呼ぶまでの
/// 数百ミリ秒を埋めるためだけの当て推量で、**食い違っても直る**。
/// だから込み入ったことはしない——**中国語の書き言葉だけ見分けて、あとは言語の副タグ**。
fn provisional(os_locales: &[String]) -> &'static str {
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
        if let Some((code, _)) = TEXTS.iter().find(|(c, _)| {
            !c.starts_with("zh") && (tag == **c || tag.starts_with(&format!("{c}-")))
        }) {
            return code;
        }
    }
    "en"
}

/// そのコードでメニューを1枚組む。
fn build<R: Runtime>(app: &AppHandle<R>, code: &str) -> tauri::Result<Menu<R>> {
    let t = text_for(code);
    let pkg = app.package_info();
    let name = pkg.name.clone();
    let config = app.config();
    let about_metadata = AboutMetadata {
        name: Some(name.clone()),
        version: Some(pkg.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    // **`%@` はアプリ名が入る場所**（Appleの綴りのまま持ってきている）。
    // 独語は `„%@“ ausblenden` のようにかぎ括弧ごと入るので、**前後を足さない**
    let with_name = |s: &str| s.replace("%@", &name);

    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                &name,
                true,
                &[
                    &PredefinedMenuItem::about(
                        app,
                        Some(&with_name(t.about)),
                        Some(about_metadata),
                    )?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, Some(t.services))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, Some(&with_name(t.hide)))?,
                    &PredefinedMenuItem::hide_others(app, Some(t.hide_others))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, Some(&with_name(t.quit)))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t.file,
                true,
                &[&PredefinedMenuItem::close_window(
                    app,
                    Some(t.close_window),
                )?],
            )?,
            &Submenu::with_items(
                app,
                t.edit,
                true,
                &[
                    &PredefinedMenuItem::undo(app, Some(t.undo))?,
                    &PredefinedMenuItem::redo(app, Some(t.redo))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, Some(t.cut))?,
                    &PredefinedMenuItem::copy(app, Some(t.copy))?,
                    &PredefinedMenuItem::paste(app, Some(t.paste))?,
                    &PredefinedMenuItem::select_all(app, Some(t.select_all))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                t.view,
                true,
                &[&PredefinedMenuItem::fullscreen(app, Some(t.fullscreen))?],
            )?,
            // **`WINDOW_SUBMENU_ID` でなければならない**（ゲート1の指摘）。
            // Tauri はこのIDの submenu だけを `set_as_windows_menu_for_nsapp` へ渡す。
            // 名前を訳すついでにIDを落とすと、**開いている窓の一覧も
            // 「すべてを手前に移動」もOSが足さなくなる**——訳したせいで
            // 標準の振る舞いが減る、といういちばん avoid したい形になる
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                t.window,
                true,
                &[
                    &PredefinedMenuItem::minimize(app, Some(t.minimize))?,
                    // **macOSでは `maximize` が「拡大/縮小」（zoom:）になる。**
                    // 既定は `Maximize` と出るが、**それはmacOSの語ではない**
                    &PredefinedMenuItem::maximize(app, Some(t.zoom))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::close_window(app, Some(t.close_window))?,
                ],
            )?,
            // **中身は空のまま**（Tauriの既定と同じ）。macOSはヘルプメニューに
            // **検索欄を自前で足す**——ただし `HELP_SUBMENU_ID` のときだけである。
            // 空の枠に見えて、IDを落とすと本当にただの空の枠になる
            &Submenu::with_id_and_items(app, HELP_SUBMENU_ID, t.help, true, &[])?,
        ],
    )
}

/// 起動時に1枚組んで掛ける。**OSの言語からの当て推量**（画面が後で直す）。
pub fn install<R: Runtime>(app: &AppHandle<R>, os_locales: &[String]) -> tauri::Result<()> {
    apply(app, provisional(os_locales))
}

/// 画面が決めた言語で組み直す。
pub fn apply<R: Runtime>(app: &AppHandle<R>, code: &str) -> tauri::Result<()> {
    let menu = build(app, code)?;
    app.set_menu(menu)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{provisional, text_for, TEXTS};

    /// `ui/src/i18n/index.ts` の `DICTS` に並んでいる言語コード。
    ///
    /// **1行だと決め打ちしない**（ゲート2）。7つ目を足せば行は折り返されるし、
    /// **最後の要素には末尾のカンマが無い**——**言語を足したときにだけ壊れる歯止め**は、
    /// 守りたい場面でちょうど役に立たない。だから中括弧の中を素直に読む。
    fn dict_codes() -> Vec<String> {
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

    /// **表の顔ぶれは `DICTS` と揃っていること。**
    /// 片方だけ足すと、その言語だけメニューが英語で出る
    #[test]
    fn the_menu_knows_every_language_the_app_ships() {
        let codes = dict_codes();
        assert!(codes.len() >= 2, "DICTS を読めていない: {codes:?}");
        for code in &codes {
            assert!(
                TEXTS.iter().any(|(c, _)| c == code),
                "{code} の語がこの表に無い。tools/menu-strings.py を回すこと"
            );
        }
        for (code, _) in TEXTS {
            assert!(
                codes.iter().any(|c| c == code),
                "{code} が DICTS に無い（辞書を消したなら、この表からも消すこと）"
            );
        }
    }

    /// **`Info.plist` の `CFBundleLocalizations` も同じ顔ぶれであること。**
    ///
    /// **一度忘れている**——独語と西語の辞書（#94 / #95）を足したとき、あちらは
    /// そのままだった。書き忘れると**保存パネルとフォルダ選択だけ英語**という
    /// 混ざり方になり、画面を見ているだけでは気づきにくい。
    /// **綴りはAppKitの言語IDなので、中国語だけ辞書のコードと違う。**
    #[test]
    fn the_bundle_declares_every_language_the_app_ships() {
        let plist = include_str!("../Info.plist");
        for code in dict_codes() {
            let appkit = match code.as_str() {
                "zh" => "zh-Hans",
                "zh-hant" => "zh-Hant",
                other => other,
            };
            assert!(
                plist.contains(&format!("<string>{appkit}</string>")),
                "{appkit} が Info.plist の CFBundleLocalizations に無い"
            );
        }
    }

    /// **繋ぎの当て推量**。込み入ったことはしないが、書き言葉つきの中国語だけは分ける
    #[test]
    fn the_provisional_guess_splits_the_two_chinese_scripts() {
        assert_eq!(provisional(&["ja-JP".into()]), "ja");
        assert_eq!(provisional(&["de-DE".into()]), "de");
        assert_eq!(provisional(&["en-GB".into()]), "en");
        // 簡体字と繁体字
        assert_eq!(provisional(&["zh-Hans-CN".into()]), "zh");
        assert_eq!(provisional(&["zh-CN".into()]), "zh");
        assert_eq!(provisional(&["zh-TW".into()]), "zh-hant");
        assert_eq!(provisional(&["zh-Hant-HK".into()]), "zh-hant");
        assert_eq!(provisional(&["zh-MO".into()]), "zh-hant");
        // **`Hans` と書いてあれば地域より優先**（`pickLocale()` と同じ）
        assert_eq!(provisional(&["zh-Hans-TW".into()]), "zh");
        // **粵語も繁体字へ。** ここを英語へ落とすと、この節が消したい
        // 「英語のメニューが一瞬見える」がそのまま起きる（ゲート2）
        assert_eq!(provisional(&["yue-Hant-HK".into()]), "zh-hant");
        assert_eq!(provisional(&["yue".into()]), "zh-hant");
        // **副タグの切れ目で切る。** 前方一致だけだと `ja` / `es` に当たってしまう
        assert_eq!(provisional(&["jam-JM".into()]), "en");
        assert_eq!(provisional(&["esu".into()]), "en");
        // **スペイン語は地域で本国と中南米に割れる**（dev #17。`pickLocale()` と同じ規則）
        assert_eq!(provisional(&["es-ES".into()]), "es");
        assert_eq!(
            provisional(&["es".into()]),
            "es",
            "地域が分からなければ本国側"
        );
        assert_eq!(provisional(&["es-MX".into()]), "es-419");
        assert_eq!(provisional(&["es-419".into()]), "es-419");
        assert_eq!(provisional(&["es-Latn-MX".into()]), "es-419");
        // **拡張は地域ではない**（`-u-` の先の `nu` / `hc` は2文字だが国ではない）
        assert_eq!(provisional(&["es-u-nu-latn".into()]), "es");
        assert_eq!(provisional(&["es-u-hc-h23".into()]), "es");
        // 知らない言語は飛ばして次を見る。全部知らなければ英語
        assert_eq!(provisional(&["th-TH".into(), "ja-JP".into()]), "ja");
        assert_eq!(provisional(&["th-TH".into()]), "en");
        assert_eq!(provisional(&[]), "en");
    }

    /// 知らないコードは英語へ倒す（画面から来ることは無いが、倒す先を決めておく）
    #[test]
    fn an_unknown_code_falls_back_to_english() {
        assert_eq!(text_for("ko").file, "File");
        assert_eq!(text_for("ja").file, "ファイル");
    }
}
