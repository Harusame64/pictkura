//! 外部アプリの表示名（右クリックの「<名前>で開く」に出す名前）。
//!
//! 以前は exe のファイル名から拡張子を外しただけで、`gimp-2.10`・`mspaint`・`Photoshop` が並び、
//! 利用者に「雑」と言われた（2026-11-15）。OS が人に見せている名前を使う:
//! - Windows: exe の版情報の `FileDescription`（エクスプローラーの「ファイルの説明」）
//! - macOS: Finder と同じ表示名（`NSFileManager displayNameAtPath:`。`.app` を外し、言語に合わせる）
//!
//! 取れなければ今までどおりファイル名（拡張子なし）。

use std::path::Path;

/// ファイル名から拡張子を外した名前（取れなければパスそのもの）
pub fn fallback_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// OS が人に見せている名前。取れなければ [`fallback_name`]
pub fn display_name(path: &Path) -> String {
    os_name(path)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name(path))
}

#[cfg(windows)]
fn os_name(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: wide は NUL 終端。大きさを訊いてから同じ大きさの領域へ読む
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut()) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    // 言語とコードページの組。先頭の組を使う（たいてい 1 つ）
    let query = |sub: &str| -> Option<(*const u16, u32)> {
        let q: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        // SAFETY: buf は GetFileVersionInfoW が埋めた版情報。返る ptr は buf の中を指す
        let ok = unsafe { VerQueryValueW(buf.as_ptr().cast(), q.as_ptr(), &mut ptr, &mut len) };
        (ok != 0 && !ptr.is_null() && len > 0).then_some((ptr as *const u16, len))
    };
    let (tr, tr_len) = query("\\VarFileInfo\\Translation")?;
    let pairs: Vec<(u16, u16)> = (0..(tr_len as usize / 4))
        .map(|i| unsafe { (*tr.add(i * 2), *tr.add(i * 2 + 1)) })
        .collect();
    // 版情報に載っている組を順に。無ければ英語（米）・Unicode を試す
    let candidates = pairs.into_iter().chain(Some((0x0409, 0x04b0)));
    for (lang, cp) in candidates {
        if let Some((p, len)) = query(&format!(
            "\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"
        )) {
            // SAFETY: len は NUL を含む u16 の数
            let s = unsafe { std::slice::from_raw_parts(p, len as usize) };
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            let name = String::from_utf16_lossy(&s[..end]);
            if !name.trim().is_empty() {
                return Some(name);
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn os_name(path: &Path) -> Option<String> {
    use objc2_foundation::{NSFileManager, NSString};
    let p = NSString::from_str(path.to_str()?);
    let name = NSFileManager::defaultManager().displayNameAtPath(&p);
    let name = name.to_string();
    // 存在しないパスでは最後の要素がそのまま返る（拡張子付き）。それなら代わりの名前に任せる
    let raw_last = path.file_name()?.to_string_lossy();
    (name != raw_last).then_some(name)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn os_name(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_the_file_stem() {
        assert_eq!(
            fallback_name(Path::new("/nowhere/gimp-2.10.exe")),
            "gimp-2.10"
        );
        assert_eq!(
            display_name(Path::new("/nowhere/does-not-exist.app")),
            "does-not-exist"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_uses_the_finder_name() {
        // Finder の表示名は .app を外す（言語で「プレビュー」にもなる）
        let n = display_name(Path::new("/System/Applications/Preview.app"));
        assert!(!n.ends_with(".app"), "{n}");
        assert!(!n.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn windows_reads_the_file_description() {
        // notepad.exe の FileDescription は空でない（言語で文言は変わる）。ファイル名そのものではない
        let n = display_name(Path::new(r"C:\Windows\System32\notepad.exe"));
        assert!(!n.is_empty());
        assert_ne!(n, "notepad.exe");
    }
}
