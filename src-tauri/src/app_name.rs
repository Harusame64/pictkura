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
    // u16 を読むので、2 バイト境界に揃った領域を取る（Vec<u8> は揃うと約束しない。/code-review）
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    if unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr().cast()) } == 0 {
        return None;
    }
    let query = |sub: &str| -> Option<(*const u16, u32)> {
        let q: Vec<u16> = sub.encode_utf16().chain(Some(0)).collect();
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len: u32 = 0;
        // SAFETY: buf は GetFileVersionInfoW が埋めた版情報。返る ptr は buf の中を指す
        let ok = unsafe { VerQueryValueW(buf.as_ptr().cast(), q.as_ptr(), &mut ptr, &mut len) };
        (ok != 0 && !ptr.is_null() && len > 0).then_some((ptr as *const u16, len))
    };
    // 言語とコードページの組（版情報に載っている順）。載っていない exe もあるので、その後に
    // 英語（米）の Unicode と cp1252 を必ず試す——組の表が無いだけで諦めない（/code-review）
    let mut candidates: Vec<(u16, u16)> = Vec::new();
    if let Some((tr, tr_len)) = query("\\VarFileInfo\\Translation") {
        for i in 0..(tr_len as usize / 4) {
            // SAFETY: 組の表は u16 の対が tr_len バイト。read_unaligned で境界を仮定しない
            let pair = unsafe {
                (
                    tr.add(i * 2).read_unaligned(),
                    tr.add(i * 2 + 1).read_unaligned(),
                )
            };
            candidates.push(pair);
        }
    }
    candidates.extend([(0x0409, 0x04b0), (0x0409, 0x04e4)]);
    let read = |lang: u16, cp: u16, key: &str| -> Option<String> {
        let (p, len) = query(&format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\{key}"))?;
        // SAFETY: len は NUL を含む u16 の数。1 文字ずつ read_unaligned で読む
        let units: Vec<u16> = (0..len as usize)
            .map(|k| unsafe { p.add(k).read_unaligned() })
            .take_while(|&c| c != 0)
            .collect();
        let s = String::from_utf16_lossy(&units);
        (!s.trim().is_empty()).then_some(s)
    };
    // 「ファイルの説明」を先に。ただ、説明がファイル名そのもの（Store のペイントは「mspaint.exe」）なら
    // 製品名（「Paint」）へ落とす（win2 の実測）
    let file_name = path.file_name().map(|f| f.to_string_lossy().to_lowercase());
    let file_ish = |s: &str| {
        let l = s.trim().to_lowercase();
        Some(&l) == file_name.as_ref()
            || l == fallback_name(path).to_lowercase()
            || l.ends_with(".exe")
    };
    for key in ["FileDescription", "ProductName"] {
        for &(lang, cp) in &candidates {
            if let Some(name) = read(lang, cp, key) {
                if !file_ish(&name) {
                    return Some(name);
                }
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
    fn mac_reads_a_name_from_the_os() {
        // 代わりの名前（Preview）でも通ってしまわないよう、OS から引けたこと自体を見る
        let n = os_name(Path::new("/System/Applications/Preview.app"));
        assert!(
            n.as_deref()
                .is_some_and(|n| !n.is_empty() && !n.ends_with(".app")),
            "{n:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_reads_the_file_description() {
        // notepad.exe には FileDescription がある（言語で文言は変わる）。代わりの名前（notepad）では通らない
        let n = os_name(Path::new(r"C:\Windows\System32\notepad.exe"));
        assert!(
            n.as_deref()
                .is_some_and(|n| !n.is_empty() && n != "notepad"),
            "{n:?}"
        );
    }
}
