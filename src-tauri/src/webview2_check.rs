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
//! **同じ関数を見る**（`tauri::webview_version` はその再エクスポート）——ここが通って tauri が「無い」と言う
//! 食い違いを作らない。方針の鍵
//! （`BrowserExecutableFolder` が空の場所を指す）で塞がれた台もこれで「無い」になる
//! （win の実測で、tauri は手で隠した台と同じダイアログを出した）。

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Com::{
    CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONWARNING, MB_SETFOREGROUND, MB_SYSTEMMODAL, MB_YESNO, SW_SHOWNORMAL,
};

use pictkura_core::applog;

/// ランタイムが在れば何もしない。無ければ知らせて、**この関数から戻らない**
pub fn ensure_runtime_or_exit() {
    let Err(e) = tauri::webview_version() else {
        return;
    };
    // **この道ではフォルダを作ってから書く。** `applog` は自分ではフォルダを作らない
    // （アンインストールの途中で戻さないため）ので、作らないと初めての台——この案内が
    // いちばん出る台——で何も残らない（ゲート2）。起動できていれば `setup` が同じものを作る
    if let Some(dir) = crate::app_dir_without_app() {
        let _ = std::fs::create_dir_all(dir);
    }
    crate::set_log_path_without_app();
    applog::note(&format!("the WebView2 runtime was not found: {e}"));
    applog::flush_pending();

    // **案内の前に DPI の扱いを宣言する**（ゲート2）。宣言は窓を作る tao がするので、その前の
    // ここでは未宣言のまま——倍率の高い画面で、案内が引き伸ばされてぼやける。この道は
    // 終わるだけなので、tao の宣言とはぶつからない。失敗しても見た目が変わるだけ
    // SAFETY: 引数は定数。戻り値（既に宣言済みなど）は見ない
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };

    let locales: Vec<String> = sys_locale::get_locales().collect();
    let notice = notice_for(crate::os_dict::guess(&locales));
    let text = HSTRING::from(notice.text);
    // SAFETY: 文字列は呼び出しのあいだ生きている `HSTRING` と、静的な `w!`。親の窓は無い（null）
    let answer = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("pictkura"),
            // **最前面には `MB_SYSTEMMODAL` で載せる**（pictkura-dev #72、win の実測 dev mbspike-win-20261021）。
            // 前に出る権利を持たないプロセス（スケジュールタスクや AutoPlay からの起動）では、`MB_TOPMOST` は
            // 効かず（箱に WS_EX_TOPMOST が付かない）、エクスプローラーの後ろに出て点滅するだけだった。
            // `MB_SYSTEMMODAL` なら付いて、上に見える。**それでも前景にはならない**——キーはエクスプローラーに
            // 残り、押すにはクリックが要る。親を最前面の見えない窓にする手も上に出たが、タスクバーから
            // 辿れなくなるので取らない
            MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND | MB_SYSTEMMODAL,
        )
    };
    if answer == IDYES {
        if let Err(e) = open_page(notice.url) {
            applog::note(&format!("could not open the WebView2 download page: {e}"));
            applog::flush_pending();
        }
    }
    std::process::exit(1);
}

/// 頁を既定のブラウザで開く。**開き終わるまで待つ**——このあとすぐ終わるので、
/// `SEE_MASK_NOASYNC` が無いと、COM で渡す既定のブラウザ（DelegateExecute）では
/// 渡す前にプロセスが消えて何も開かない（ShellExecuteEx の決まり。ゲート2）。
/// 同じ理由で COM を先に立てる。`opener` プラグインはこの旗を付けないので使わない
fn open_page(url: &str) -> windows::core::Result<()> {
    // SAFETY: このスレッドで初めての初期化。戻り値（既に初期化済みなど）は見ない
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    let url = HSTRING::from(url);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: w!("open"),
        lpFile: PCWSTR(url.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` と、その指す文字列は呼び出しのあいだ生きている
    unsafe { ShellExecuteExW(&mut info) }
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
            text: "pictkura を動かすには「Microsoft Edge WebView2 ランタイム」が必要ですが、見つかりませんでした。\n\n\
                   入手ページを開きますか？インストールしたあとで、もう一度 pictkura を起動してください。",
            url: "https://developer.microsoft.com/ja-jp/microsoft-edge/webview2/",
        },
        "de" => Notice {
            text: "pictkura braucht die Microsoft Edge WebView2-Runtime, sie wurde aber nicht gefunden.\n\n\
                   Downloadseite öffnen? Starte pictkura nach der Installation erneut.",
            url: "https://developer.microsoft.com/de-de/microsoft-edge/webview2/",
        },
        "es" => Notice {
            text: "pictkura necesita Microsoft Edge WebView2 Runtime, pero no se ha encontrado.\n\n\
                   ¿Abrir la página de descarga? Cuando lo hayas instalado, vuelve a abrir pictkura.",
            url: "https://developer.microsoft.com/es-es/microsoft-edge/webview2/",
        },
        "es-419" => Notice {
            text: "pictkura necesita Microsoft Edge WebView2 Runtime, pero no se encontró.\n\n\
                   ¿Abrir la página de descarga? Cuando lo hayas instalado, vuelve a abrir pictkura.",
            url: "https://developer.microsoft.com/es-mx/microsoft-edge/webview2/",
        },
        "zh" => Notice {
            text: "pictkura 需要 Microsoft Edge WebView2 运行时，但没有找到。\n\n\
                   要打开下载页面吗？安装后请重新启动 pictkura。",
            url: "https://developer.microsoft.com/zh-cn/microsoft-edge/webview2/",
        },
        "zh-hant" => Notice {
            text: "pictkura 需要 Microsoft Edge WebView2 執行階段，但找不到。\n\n\
                   要開啟下載頁面嗎？安裝後請重新啟動 pictkura。",
            url: "https://developer.microsoft.com/zh-tw/microsoft-edge/webview2/",
        },
        _ => Notice {
            text: "pictkura needs the Microsoft Edge WebView2 Runtime, but it could not be found.\n\n\
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
