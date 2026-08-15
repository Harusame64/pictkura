//! WindowsのAutoPlay（自動再生）ハンドラ登録。
//!
//! USB/SDカードを挿したときの「このデバイスに対して行う操作」の一覧に
//! pictkura を候補として足す（**既定は乗っ取らない**——選ぶのは利用者）。
//! 選ばれると `pictkura.exe --import <ドライブ>` が起動し、取り込みウィザードが
//! そのドライブで開く（引数の処理は [`crate`] 本体側）。
//!
//! 登録先は **HKCU**（現在のユーザー）なので**管理者権限は要らない**。
//! 起動のたびに冪等に書き直すので、ポータブル版を別の場所へ移しても
//! 次回起動で実行ファイルのパスが更新される。設定 `[import] register_autoplay`
//! を false にすると [`unregister`] で候補ごと消える。

#[cfg(windows)]
pub use imp::{register, unregister};

// AutoPlayはWindowsだけの機構。他のOSでは何もしない（呼び出し側を分岐させない）。
#[cfg(not(windows))]
pub fn register(_exe: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn unregister() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW,
        RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RRF_RT_REG_SZ,
    };

    /// AutoPlayハンドラの内部識別子（EventHandlersに並ぶ名前）。
    const HANDLER: &str = "pictkuraImportFromDrive";
    /// 起動に使うProgID（下に開くverbのコマンドを持たせる）。
    const PROGID: &str = "pictkura.ImportFromDrive";
    /// ProgIDのverb名。
    const VERB: &str = "import";
    /// 自動再生のメニューに出る文言。
    const ACTION: &str = "pictkura で写真を取り込む";
    const PROVIDER: &str = "pictkura";
    /// この候補を出すデバイス到着イベント。SDカード（写真）とUSBメモリ（汎用ストレージ）の
    /// 両方で拾えるようにする。挿しても既定にはならず、あくまで選択肢に並ぶだけ。
    const EVENTS: &[&str] = &[
        "ShowPicturesOnArrival",
        "ShowMixedContentOnArrival",
        "StorageOnArrival",
    ];

    const AUTOPLAY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers";

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// `HKCU\<subkey>` を作り（無ければ）、文字列値を書く。`name=None` は既定値。
    fn set_string(subkey: &str, name: Option<&str>, value: &str) -> io::Result<()> {
        let subkey_w = wide(subkey);
        let mut hkey: HKEY = std::ptr::null_mut();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey_w.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        let value_w = wide(value);
        let name_w = name.map(wide);
        let name_ptr = name_w.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        // 長さはNUL込みのバイト数（UTF-16なので要素数×2）。
        let rc = unsafe {
            RegSetValueExW(
                hkey,
                name_ptr,
                0,
                REG_SZ,
                value_w.as_ptr() as *const u8,
                (value_w.len() * 2) as u32,
            )
        };
        unsafe {
            RegCloseKey(hkey);
        }
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        Ok(())
    }

    /// `HKCU\<subkey>` の文字列値を読む（無ければ `None`）。`name=None` は既定値。
    fn get_string(subkey: &str, name: Option<&str>) -> Option<String> {
        let subkey_w = wide(subkey);
        let name_w = name.map(wide);
        let name_ptr = name_w.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        // まず必要なバイト数を聞く（NUL込みで返る）。
        let mut len: u32 = 0;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey_w.as_ptr(),
                name_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if rc != ERROR_SUCCESS || len < 2 {
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2)];
        let mut len2 = len;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                subkey_w.as_ptr(),
                name_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut std::ffi::c_void,
                &mut len2,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let chars = (len2 as usize / 2).min(buf.len());
        let s = &buf[..chars];
        let s = match s.iter().position(|&c| c == 0) {
            Some(i) => &s[..i],
            None => s,
        };
        Some(String::from_utf16_lossy(s))
    }

    pub fn register(exe: &Path) -> io::Result<()> {
        let exe = exe.display().to_string();
        // verbのコマンド。`%L` が対象のパスに置き換わる。ドライブ直下なら `E:\` で
        // スペースは無いが、`ShowPicturesOnArrival` は MTP/ポータブル機器や
        // フォルダにマウントされたボリュームでも上がり、そこでは `C:\My Photos\` の
        // ようにスペースを含みうる。裸で渡すと `C:\My` で切れて存在しない
        // フォルダを開いてしまうので**囲む**。ただし末尾のバックスラッシュが
        // 閉じ引用符をエスケープして `E:"` の形で届くため、受け側
        // （`import_path_from_args`）で元に戻している。
        let command = format!("\"{exe}\" --import \"%L\"");
        set_string(
            &format!(r"Software\Classes\{PROGID}\shell\{VERB}\command"),
            None,
            &command,
        )?;
        set_string(&format!(r"Software\Classes\{PROGID}"), None, ACTION)?;

        // ハンドラの定義。
        let handler_key = format!(r"{AUTOPLAY}\Handlers\{HANDLER}");
        set_string(&handler_key, Some("Action"), ACTION)?;
        set_string(&handler_key, Some("Provider"), PROVIDER)?;
        set_string(&handler_key, Some("InvokeProgID"), PROGID)?;
        set_string(&handler_key, Some("InvokeVerb"), VERB)?;
        set_string(&handler_key, Some("DefaultIcon"), &format!("{exe},0"))?;

        // 到着イベントに紐付ける（値名がハンドラ名、中身は空でよい）。
        for event in EVENTS {
            set_string(
                &format!(r"{AUTOPLAY}\EventHandlers\{event}"),
                Some(HANDLER),
                "",
            )?;
        }
        Ok(())
    }

    /// 登録を消す（`register_autoplay = false` のとき）。失敗しても致命的ではないので
    /// 個々の削除エラーは無視する（既に無い鍵の削除は失敗するため）。
    pub fn unregister() -> io::Result<()> {
        for event in EVENTS {
            let key = wide(&format!(r"{AUTOPLAY}\EventHandlers\{event}"));
            let name = wide(HANDLER);
            unsafe {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
            }
            // 「常にこの操作」で pictkura を選ばれていた場合、その記録も消す。
            // 残すとエクスプローラーは**もう居ないハンドラを呼び続け**、
            // 通常の選択画面にも戻らない。他アプリの選択を巻き添えにしないよう、
            // 中身がこちらのハンドラのときだけ消す。
            let chosen = format!(r"{AUTOPLAY}\UserChosenExecuteHandlers\{event}");
            if get_string(&chosen, None).as_deref() == Some(HANDLER) {
                let key = wide(&chosen);
                unsafe {
                    RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr());
                }
            }
        }
        for tree in [
            format!(r"{AUTOPLAY}\Handlers\{HANDLER}"),
            format!(r"Software\Classes\{PROGID}"),
        ] {
            let key = wide(&tree);
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr());
            }
        }
        Ok(())
    }
}
