//! Windows の AutoPlay（自動再生）の、**アプリ側に残った仕事**——片付けだけ。
//!
//! **登録はインストーラーと Store のマニフェストが書く**（2026-10-19 利用者の決定。pictkura-dev #53、
//! dev/plan.msix.md）。アプリは実行中にレジストリへ登録を書かない:
//!
//! - MSI: `windows/autoplay.wxs` が HKLM に書き、アンインストールで MSI が消す
//! - NSIS: `windows/installer-hooks.nsh` が HKCU に書き、アンインストールで消す
//! - Store 版: `windows/msix/AppxManifest.xml`
//! - 持ち歩き版: 候補に出さない
//!
//! ここに残るのは2つ:
//!
//! - [`remove_legacy`] — **前の版がアプリから書いた登録**（名前が `pictkuraImportFromDrive`）を消す。インストーラーが
//!   入れた直後と、アプリの起動時に走る。新しい登録（`pictkuraAutoPlay`）とその「常にこの操作」には触らない
//! - [`uninstall_cleanup`] — 上に加えて、**新しい候補を指す「常にこの操作」**も消す（アンインストールの直前）。
//!   残すとエクスプローラーはもう居ないハンドラを呼び続け、通常の選択画面にも戻らない

#[cfg(windows)]
pub use imp::{legacy_present, remove_legacy, uninstall_cleanup};

// AutoPlay は Windows だけの機構。他の OS では何もしない（呼び出し側を分岐させない）
#[cfg(not(windows))]
pub fn legacy_present() -> bool {
    false
}
#[cfg(not(windows))]
pub fn remove_legacy() -> std::io::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn uninstall_cleanup() -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
mod imp {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{
        ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
    };
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteKeyValueW, RegDeleteTreeW, RegEnumKeyExW, RegGetValueW,
        RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ,
    };

    /// 前の版がアプリから書いたハンドラの名前と ProgID（もう書かない。片付けるだけ）
    const LEGACY_HANDLER: &str = "pictkuraImportFromDrive";
    const LEGACY_PROGID: &str = "pictkura.ImportFromDrive";
    /// インストーラーが書くハンドラの名前（`windows/autoplay.wxs`・`windows/installer-hooks.nsh` と同じ綴り）。
    /// アプリは書かない——アンインストールで、これを指す「常にこの操作」を消すためにだけ名前を持つ
    const HANDLER: &str = "pictkuraAutoPlay";
    /// 前の版が登録していた到着イベント
    const EVENTS: &[&str] = &[
        "ShowPicturesOnArrival",
        "MixedContentOnArrival",
        "StorageOnArrival",
    ];
    /// OS に無いイベント名だった（正しくは `MixedContentOnArrival`）。古い版が HKCU に孤立した鍵を作っている。
    /// 他アプリが相乗りしようがないので**丸ごと**消す
    const LEGACY_EVENTS: &[&str] = &["ShowMixedContentOnArrival"];

    const AUTOPLAY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\AutoplayHandlers";

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// `HKCU\<subkey>` の文字列値を読む。`name=None` は既定値。
    ///
    /// **「無い」と「読めなかった」を区別する**のが要点。どちらも `None` に畳むと、
    /// 権限やポリシーで読めなかったときに「こちらのハンドラではない」と誤判定して
    /// 消し漏らし、しかも成功として返してしまう。
    ///
    /// - `Ok(Some(s))` 値がある
    /// - `Ok(None)`    鍵か値が無い（目的は満たしている）
    /// - `Err(rc)`     読めなかった
    fn get_string(subkey: &str, name: Option<&str>) -> Result<Option<String>, u32> {
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
        if rc == ERROR_FILE_NOT_FOUND || rc == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        if rc != ERROR_SUCCESS {
            return Err(rc);
        }
        // 空文字（NULだけ）は「値はあるが中身なし」。照合には使えないので無い扱い。
        if len < 2 {
            return Ok(None);
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
            return Err(rc);
        }
        let chars = (len2 as usize / 2).min(buf.len());
        let s = &buf[..chars];
        let s = match s.iter().position(|&c| c == 0) {
            Some(i) => &s[..i],
            None => s,
        };
        Ok(Some(String::from_utf16_lossy(s)))
    }

    /// `HKCU\<subkey>` の直下にあるサブキーの名前を並べる。
    ///
    /// 開けなかったときは `Err(rc)`。**空と区別する**——開けないまま空を返すと、
    /// その下にある記録を丸ごと見落としたまま成功として返してしまう。
    fn subkeys(subkey: &str) -> Result<Vec<String>, u32> {
        let subkey_w = wide(subkey);
        let mut hkey: HKEY = std::ptr::null_mut();
        let rc =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) };
        if rc != ERROR_SUCCESS {
            return Err(rc);
        }
        let mut names = Vec::new();
        // レジストリの鍵名は255文字まで（終端込みで256）。
        let mut buf = [0u16; 256];
        let mut index = 0u32;
        loop {
            let mut len = buf.len() as u32;
            let rc = unsafe {
                RegEnumKeyExW(
                    hkey,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if rc != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buf[..len as usize]));
            index += 1;
        }
        unsafe {
            RegCloseKey(hkey);
        }
        Ok(names)
    }

    fn legacy_handler_key() -> String {
        format!(r"{AUTOPLAY}\Handlers\{LEGACY_HANDLER}")
    }

    /// 前の版の登録が、この利用者の HKCU に残っているか（起動のたびに消しに行かないための安い問い）。
    /// **読めなかったら「在る」に倒す**——片付けを試みるだけで害が無い
    pub fn legacy_present() -> bool {
        !matches!(
            get_string(&legacy_handler_key(), Some("InvokeProgID")),
            Ok(None)
        )
    }

    /// 削除の結果を集める。**「無かった」と「消せなかった」を分ける**——既に無い鍵や値は目的を満たしている
    /// ので成功、権限やポリシーで拒まれたものは最初の1つを返す（途中で失敗しても残りは続ける）
    #[derive(Default)]
    struct Outcome(Option<i32>);
    impl Outcome {
        fn check(&mut self, rc: u32) {
            if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND && self.0.is_none() {
                self.0 = Some(rc as i32);
            }
        }
        fn into_result(self) -> io::Result<()> {
            match self.0 {
                Some(rc) => Err(io::Error::from_raw_os_error(rc)),
                None => Ok(()),
            }
        }
    }

    /// 「常にこの操作」で `handler` が選ばれていた記録を消す。
    ///
    /// **記録は入れ子になっている**のが罠。リムーバブルドライブで選ぶと `UserChosenExecuteHandlers\
    /// StorageOnArrival` に直接書かれるが、**メモリカードで選ぶと `...\CameraAlternate\ShowPicturesOnArrival`**
    /// に書かれる（実測）。イベント名を決め打ちで辿らず、**中身がこのハンドラである鍵を探して消す**（他アプリの
    /// 選択は中身が違うので巻き添えにしない）。深さは2まで——壊れたレジストリで回り続けないため
    fn forget_choices(handler: &str, out: &mut Outcome) {
        let root = format!(r"{AUTOPLAY}\UserChosenExecuteHandlers");
        let mut stack: Vec<(String, u32)> = vec![(root, 0)];
        while let Some((key, depth)) = stack.pop() {
            match get_string(&key, None) {
                Ok(Some(v)) if v == handler => {
                    let w = wide(&key);
                    out.check(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, w.as_ptr()) });
                    continue;
                }
                Ok(_) => {}
                // 読めなければ、こちらのものか判断が付かない。**黙って飛ばさない**
                Err(rc) => out.check(rc),
            }
            if depth < 2 {
                match subkeys(&key) {
                    Ok(children) => {
                        for child in children {
                            stack.push((format!(r"{key}\{child}"), depth + 1));
                        }
                    }
                    Err(rc) => out.check(rc),
                }
            }
        }
    }

    fn remove_legacy_into(out: &mut Outcome) {
        // 正規のイベントの鍵は**他アプリも値を並べている**ので、こちらの値だけ消す
        for event in EVENTS {
            let key = wide(&format!(r"{AUTOPLAY}\EventHandlers\{event}"));
            let name = wide(LEGACY_HANDLER);
            out.check(unsafe {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr())
            });
        }
        for event in LEGACY_EVENTS {
            let key = wide(&format!(r"{AUTOPLAY}\EventHandlers\{event}"));
            out.check(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) });
        }
        forget_choices(LEGACY_HANDLER, out);
        for tree in [
            legacy_handler_key(),
            format!(r"Software\Classes\{LEGACY_PROGID}"),
        ] {
            let key = wide(&tree);
            out.check(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) });
        }
    }

    /// 前の版がアプリから書いた登録を消す（この利用者の HKCU）
    pub fn remove_legacy() -> io::Result<()> {
        let mut out = Outcome::default();
        remove_legacy_into(&mut out);
        out.into_result()
    }

    /// アンインストールの直前に: 前の版の登録と、新しい候補を指す「常にこの操作」を消す。
    /// 新しい登録そのものはインストーラーが消す（MSI はコンポーネントごと、NSIS はフックで）
    pub fn uninstall_cleanup() -> io::Result<()> {
        let mut out = Outcome::default();
        remove_legacy_into(&mut out);
        forget_choices(HANDLER, &mut out);
        out.into_result()
    }
}
