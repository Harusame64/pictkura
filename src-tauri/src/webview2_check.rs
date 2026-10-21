//! **WebView2 ランタイムが無いときは、自前で知らせて終わる**（pictkura-dev #72）。
//!
//! ## なぜ要るのか
//!
//! Store 版はランタイムを入れない（MSIX から bootstrapper を走らせられない。MSI・NSIS は入れる）。
//! 無い台で起動すると、**tauri が英語のダイアログを出したうえで、窓の無いプロセスを残す**
//! （2026-10-21 win の実測、dev `round.store-no-webview2.md`）:
//!
//! - 窓づくりはイベントループへの伝言なので、`setup` の `build()` は送った時点で `Ok` を返す。
//!   ループの中で「無い」と分かっても `log::error!` に書かれるだけで、窓は登録されない
//!   （tauri-runtime-wry 2.11.4 の `create_webview` と `Message::CreateWindow` の受け口）
//! - 窓が1枚も無いので「最後の窓を閉じたら終わる」も来ない。**2回目からの起動は単一起動で
//!   その抜け殻へ渡され、何も起きない**。ダイアログのリンクからランタイムを入れて押し直しても
//!   直らない（抜け殻を止めるか、サインアウト・再起動するまで）
//! - ログも 1000 も WER も残らない
//!
//! だから **tauri を組む前に** 自分で確かめ、無ければ OS の言語で知らせて**必ず終わる**。
//! 単一起動のプラグインより前なので、抜け殻が残る道そのものを通らない。
//!
//! ## 確かめ方は tauri と同じ
//!
//! tauri は起動時に `wry::webview_version().is_ok()` を1回だけ見て「在る／無い」を決める。
//! **同じ関数を見る**——ここが通って tauri が「無い」と言う食い違いを作らない。方針の鍵
//! （`BrowserExecutableFolder` が空の場所を指す）で塞がれた台もこれで「無い」になる
//! （win の実測で、tauri は手で隠した台と同じダイアログを出した）。

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONWARNING, MB_SETFOREGROUND, MB_YESNO, SW_SHOWNORMAL,
};

use pictkura_core::applog;

/// ランタイムが在れば何もしない。無ければ知らせて、**この関数から戻らない**
pub fn ensure_runtime_or_exit() {
    let Err(e) = wry::webview_version() else {
        return;
    };
    // **書けるときだけ書く。** 初めての起動ではデータのフォルダがまだ無く、`applog` は
    // フォルダを作らない（アンインストールの途中で戻さないため）——その台では残らない
    crate::set_log_path_without_app();
    applog::note(&format!("the WebView2 runtime was not found: {e}"));
    applog::flush_pending();

    let locales: Vec<String> = sys_locale::get_locales().collect();
    let notice = notice_for(crate::os_dict::guess(&locales));
    let text = HSTRING::from(notice.text);
    // SAFETY: 文字列は呼び出しのあいだ生きている `HSTRING` と、静的な `w!`。親の窓は無い（null）
    let answer = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("pictkura"),
            MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND,
        )
    };
    if answer == IDYES {
        let url = HSTRING::from(notice.url);
        // SAFETY: 文字列は呼び出しのあいだ生きている。開けなくても終わるだけ（戻り値は見ない）
        unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(url.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    std::process::exit(1);
}

/// 案内の文と、開く頁。**頁は手引き（`docs/*/install.html`）と同じ Microsoft の WebView2 の頁**
/// を、その言語の地域で
struct Notice {
    text: &'static str,
    url: &'static str,
}

fn notice_for(code: &str) -> Notice {
    match code {
        "ja" => Notice {
            text: "pictkura を動かすには「Microsoft Edge WebView2 ランタイム」が必要ですが、この PC には見つかりませんでした。\n\n\
                   入手ページを開きますか？インストールしたあとで、もう一度 pictkura を起動してください。",
            url: "https://developer.microsoft.com/ja-jp/microsoft-edge/webview2/",
        },
        "de" => Notice {
            text: "pictkura braucht die Microsoft Edge WebView2-Runtime, sie wurde auf diesem PC aber nicht gefunden.\n\n\
                   Downloadseite öffnen? Starte pictkura nach der Installation erneut.",
            url: "https://developer.microsoft.com/de-de/microsoft-edge/webview2/",
        },
        "es" => Notice {
            text: "pictkura necesita Microsoft Edge WebView2 Runtime, pero no se ha encontrado en este ordenador.\n\n\
                   ¿Abrir la página de descarga? Cuando lo hayas instalado, vuelve a abrir pictkura.",
            url: "https://developer.microsoft.com/es-es/microsoft-edge/webview2/",
        },
        "es-419" => Notice {
            text: "pictkura necesita Microsoft Edge WebView2 Runtime, pero no se encontró en esta computadora.\n\n\
                   ¿Abrir la página de descarga? Cuando lo hayas instalado, vuelve a abrir pictkura.",
            url: "https://developer.microsoft.com/es-mx/microsoft-edge/webview2/",
        },
        "zh" => Notice {
            text: "pictkura 需要 Microsoft Edge WebView2 运行时，但在这台电脑上没有找到。\n\n\
                   要打开下载页面吗？安装后请重新启动 pictkura。",
            url: "https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/",
        },
        "zh-hant" => Notice {
            text: "pictkura 需要 Microsoft Edge WebView2 執行階段，但在這台電腦上找不到。\n\n\
                   要開啟下載頁面嗎？安裝後請重新啟動 pictkura。",
            url: "https://developer.microsoft.com/zh-tw/microsoft-edge/webview2/",
        },
        _ => Notice {
            text: "pictkura needs the Microsoft Edge WebView2 Runtime, but it was not found on this PC.\n\n\
                   Open the download page? After installing it, start pictkura again.",
            url: "https://developer.microsoft.com/en-us/microsoft-edge/webview2/",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::notice_for;
    use crate::os_dict::DICT_CODES;

    /// **画面が出せる言語ぶん、案内も持っていること**（英語へ落ちるのは知らない言語だけ）
    #[test]
    fn every_shipped_language_has_its_own_notice() {
        let en = notice_for("en");
        for code in DICT_CODES.iter().filter(|c| **c != "en") {
            let n = notice_for(code);
            assert_ne!(n.text, en.text, "{code} の案内が英語のまま");
            assert_ne!(n.url, en.url, "{code} の頁が英語のまま");
        }
    }

    /// 文は**頁を開くかを訊いて**終わり、名前を名乗る（[はい]／[いいえ] で答えられる形）
    #[test]
    fn every_notice_names_the_runtime_and_asks() {
        for code in DICT_CODES {
            let n = notice_for(code);
            assert!(n.text.contains("WebView2"), "{code}");
            assert!(
                n.text.contains('?') || n.text.contains('？'),
                "{code} は訊いていない"
            );
            assert!(
                n.url.starts_with("https://developer.microsoft.com/"),
                "{code}"
            );
        }
    }
}
