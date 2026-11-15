//! 「他のアプリで開く」の候補を OS に訊き、選ばれたものを OS に開かせる（2026-11-15 利用者、spike は
//! `dev/plan.md` の同日の節）。
//!
//! - **探しに行かない。** 出すのは OS がその種類のファイルに挙げるアプリだけ（Windows の「プログラムから開く」、
//!   macOS の「このアプリケーションで開く」と同じ顔ぶれ）。並べ替え（名簿で上へ・雑音を下へ）は画面の側
//! - **パスで覚えない。** Windows の Store アプリ（フォト等）はパスが無いか、版番号入りで更新のたびに変わる。
//!   開くときは毎回 OS の一覧から `id` で引き直し、OS に起動させる（`IAssocHandler::Invoke`）

use serde::Serialize;
use std::path::Path;

/// OS が挙げたアプリ 1 つ
#[derive(Debug, Clone, Serialize)]
pub struct OsApp {
    /// 開くときに渡す印（Windows は `IAssocHandler::GetName`、macOS はアプリのパス）
    pub id: String,
    /// 人に見せる名前（OS の表示名）
    pub name: String,
    /// OS が勧めているか（Windows の「推奨」。macOS には無いので、仮想機械のアプリ以外は true）
    pub recommended: bool,
    /// 名簿と照らす鍵（小文字）。Windows は exe のファイル名（パスが無い Store アプリは名前）、macOS は束 ID
    pub key: String,
    /// その写真の形式ではなく、別の形式（拡張子、点なし）に OS が挙げたアプリ。RAW のときだけ [`raw_extras`] が付ける
    pub via: Option<String>,
}

/// `file` を開けるアプリの一覧（OS の順）。取れなければ空
pub fn list(file: &Path) -> Vec<OsApp> {
    imp::list(file)
}

/// RAW のときに足す候補を探す形式。現像ソフトの多くは RAW の拡張子に関連付けを登録しない（Lightroom 等）が、
/// DNG・TIFF・JPEG には登録していることが多い（2026-11-15 利用者「RAW 用もあるの？ 漏れがないように」）。
/// **探しに行くのではなく、OS がほかの形式に挙げているアプリを読むだけ**。名簿で RAW を扱えるとしたものだけを
/// 画面の側で残す
pub const RAW_EXTRA_TYPES: [&str; 3] = ["dng", "tif", "jpg"];

/// [`RAW_EXTRA_TYPES`] に OS が挙げるアプリ（`via` 付き。`own` に既に在る鍵と、写真自身の形式は除く）。
///
/// 3 つの形式への問い合わせは遅い（拡張子の登録を読む）ので、**5 分のあいだ覚えて使い回す**——RAW を右クリック
/// するたびに 3 回重ねていた（/code-review）。入れたばかりのアプリは 5 分のうちに出てくる
pub fn raw_extras(file: &Path, own: &[OsApp]) -> Vec<OsApp> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, Vec<OsApp>)>> = Mutex::new(None);
    const KEEP: Duration = Duration::from_secs(300);

    let all = {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match cache.as_ref() {
            Some((at, apps)) if at.elapsed() < KEEP => apps.clone(),
            _ => {
                let mut apps = Vec::new();
                for ext in RAW_EXTRA_TYPES {
                    for mut a in imp::list_for_type(ext) {
                        a.via = Some(ext.to_string());
                        apps.push(a);
                    }
                }
                *cache = Some((Instant::now(), apps.clone()));
                apps
            }
        }
    };
    let own_ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut out: Vec<OsApp> = Vec::new();
    for a in all {
        if a.via.as_deref() == Some(own_ext.as_str())
            || own.iter().chain(out.iter()).any(|o| o.key == a.key)
        {
            continue;
        }
        out.push(a);
    }
    out
}

/// `via` として受け取ってよい形式か（[`RAW_EXTRA_TYPES`] のどれかで、写真が RAW のときだけ）。それ以外で
/// 別の形式のハンドラに写真を渡させない（/code-review: 任意の拡張子のハンドラを呼べた）
pub fn via_allowed(file: &Path, via: Option<&str>) -> bool {
    match via {
        None => true,
        Some(ext) => RAW_EXTRA_TYPES.contains(&ext) && pictkura_core::raw::is_raw_path(file),
    }
}

/// `list`（または `raw_extras`）で返した `id` のアプリで `file` を開く。`via` は `raw_extras` の印をそのまま
pub fn open(file: &Path, id: &str, via: Option<&str>) -> Result<(), String> {
    let_launched_app_take_focus();
    imp::open(file, id, via)
}

/// 起こしたアプリが前に出られるようにする（Windows）。前面を持っている pictkura が許さないと、OS の前面ロックで
/// 起こしたアプリの窓が裏に開き、「何も起きない」に見える（2026-11-15 利用者「つねに裏で起動だと混乱する」）。
/// 許すのは、押した直後のこの 1 回の前面の移しだけ（次の入力で OS が自然に取り消す）
pub fn let_launched_app_take_focus() {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        // SAFETY: 引数は定数。失敗（前面を持っていない等）しても起動はそのまま進める
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    }
}

#[cfg(windows)]
mod imp {
    use super::OsApp;
    use std::path::Path;
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, IDataObject, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        BHID_DataObject, IAssocHandler, IShellItem, SHAssocEnumHandlers,
        SHCreateItemFromParsingName, ASSOC_FILTER_NONE,
    };

    /// COM を自分の糸で STA として使う（呼ぶ側の糸の状態に寄らない）。終わったら畳む。糸が落ちたら None
    fn with_com<T>(f: impl FnOnce() -> T + Send) -> Option<T>
    where
        T: Send,
    {
        std::thread::scope(|s| {
            s.spawn(|| {
                // SAFETY: この糸で初めての初期化。成功（S_OK / S_FALSE）なら同じ数だけ畳む
                let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let out = f();
                if hr.is_ok() {
                    unsafe { CoUninitialize() };
                }
                out
            })
            .join()
            .ok()
        })
    }

    fn take(p: PWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        // SAFETY: OS が CoTaskMemAlloc で渡した NUL 終端の文字列。読んだら返す
        let s = unsafe { p.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
        s
    }

    fn handlers(file: &Path) -> Vec<IAssocHandler> {
        match file.extension() {
            Some(ext) => handlers_for(&ext.to_string_lossy()),
            None => Vec::new(),
        }
    }

    /// 拡張子（点なし）に OS が挙げるハンドラ
    fn handlers_for(ext: &str) -> Vec<IAssocHandler> {
        let ext = HSTRING::from(format!(".{ext}"));
        // SAFETY: 拡張子の文字列を渡して列挙子を受け取る
        let Ok(e) = (unsafe { SHAssocEnumHandlers(&ext, ASSOC_FILTER_NONE) }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        loop {
            let mut one = [None];
            let mut got = 0u32;
            // S_FALSE（もう無い）も Ok で返るので、受け取った数で止める
            if unsafe { e.Next(&mut one, Some(&mut got)) }.is_err() || got == 0 {
                break;
            }
            if let Some(h) = one[0].take() {
                out.push(h);
            }
        }
        out
    }

    fn describe(h: &IAssocHandler) -> Option<OsApp> {
        // SAFETY: どれも OS が確保した文字列を返す（take が返す）
        let id = take(unsafe { h.GetName() }.ok()?);
        let name = take(unsafe { h.GetUIName() }.ok()?);
        if id.is_empty() || name.trim().is_empty() {
            return None;
        }
        let recommended = unsafe { h.IsRecommended() }.0 == 0;
        // exe のパスならファイル名、パスの無い Store アプリは名前（spike の (c)）
        let key = Path::new(&id)
            .file_name()
            .filter(|_| id.contains('\\'))
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| name.to_lowercase());
        Some(OsApp {
            id,
            name,
            recommended,
            key,
            via: None,
        })
    }

    pub fn list(file: &Path) -> Vec<OsApp> {
        let file = file.to_path_buf();
        with_com(move || handlers(&file).iter().filter_map(describe).collect()).unwrap_or_default()
    }

    pub fn list_for_type(ext: &str) -> Vec<OsApp> {
        let ext = ext.to_string();
        with_com(move || handlers_for(&ext).iter().filter_map(describe).collect())
            .unwrap_or_default()
    }

    /// **呼ぶ側の糸で動かす（UI の糸から呼ぶこと）。** そこは STA でメッセージも回っている。使い捨ての糸で
    /// `Invoke` すると、非同期で起動するハンドラ（DelegateExecute・一部の Store アプリ）が働く前に COM を畳んで
    /// しまう（/code-review。`ShellExecuteEx` に `SEE_MASK_NOASYNC` が要るのと同じ理由）
    pub fn open(file: &Path, id: &str, via: Option<&str>) -> Result<(), String> {
        // `via` があれば、そのアプリは別の形式の一覧に居る。そこから引き直して、この写真を渡す
        let all = match via {
            Some(ext) => handlers_for(ext),
            None => handlers(file),
        };
        let h = all
            .into_iter()
            .find(|h| {
                unsafe { h.GetName() }
                    .ok()
                    .map(take)
                    .is_some_and(|n| n == id)
            })
            .ok_or_else(|| crate::errs::code("errAppGone"))?;
        let path = HSTRING::from(file.as_os_str());
        // SAFETY: ファイルのシェル項目からデータオブジェクトを作り、ハンドラに渡す
        let item: IShellItem = unsafe { SHCreateItemFromParsingName(&path, None) }
            .map_err(|e| crate::errs::coded("errOpenWith", e))?;
        let data: IDataObject = unsafe { item.BindToHandler(None, &BHID_DataObject) }
            .map_err(|e| crate::errs::coded("errOpenWith", e))?;
        unsafe { h.Invoke(&data) }.map_err(|e| crate::errs::coded("errOpenWith", e))
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::OsApp;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSBundle, NSString, NSURL};
    use std::path::Path;

    /// 拡張子（点なし）の空のファイルを一時フォルダに置いて訊く（LaunchServices は拡張子で引く）
    pub fn list_for_type(ext: &str) -> Vec<OsApp> {
        let probe = std::env::temp_dir().join(format!("pictkura-openwith-probe.{ext}"));
        if !probe.exists() && std::fs::write(&probe, b"").is_err() {
            return Vec::new();
        }
        list(&probe)
    }

    pub fn list(file: &Path) -> Vec<OsApp> {
        // `URLsForApplicationsToOpenURL:` は macOS 12 から。11 では呼ぶと例外で落ちるので、空を返して
        // 選ぶ画面へ落とす（/code-review。最低の版は tauri.macos.conf.json の 11.0）
        if !objc2::available!(macos = 12.0) {
            return Vec::new();
        }
        let Some(p) = file.to_str() else {
            return Vec::new();
        };
        let url = NSURL::fileURLWithPath(&NSString::from_str(p));
        let ws = NSWorkspace::sharedWorkspace();
        let urls = ws.URLsForApplicationsToOpenURL(&url);
        let mut out: Vec<OsApp> = Vec::new();
        for app in urls.iter() {
            let Some(path) = app.path().map(|s| s.to_string()) else {
                continue;
            };
            let key = NSBundle::bundleWithURL(&app)
                .and_then(|b| b.bundleIdentifier())
                .map(|s| s.to_string().to_lowercase())
                .unwrap_or_else(|| path.to_lowercase());
            // 同じアプリが何か所にも居る（Edge の更新の古い版の置き場など。spike）。束 ID で 1 つにし、
            // アプリの置き場（/Applications・/System）に居るほうを残す
            let in_apps = |p: &str| p.starts_with("/Applications/") || p.starts_with("/System/");
            if let Some(prev) = out.iter_mut().find(|a| a.key == key) {
                if !in_apps(&prev.id) && in_apps(&path) {
                    // 名前も残すほうのものに（古い版の置き場の名前を出さない）
                    prev.name = crate::app_name::display_name(Path::new(&path));
                    prev.id = path;
                }
                continue;
            }
            let name = crate::app_name::display_name(Path::new(&path));
            // 仮想機械（Parallels）が出す Windows のアプリは、この Mac では写真の道具ではない
            let recommended = !path.contains("Applications (Parallels)");
            out.push(OsApp {
                id: path,
                name,
                recommended,
                key,
                via: None,
            });
        }
        out
    }

    pub fn open(file: &Path, id: &str, _via: Option<&str>) -> Result<(), String> {
        // `open -a` はアプリのパスで開くので、どの形式の一覧から来たかは要らない
        tauri_plugin_opener::open_path(file, Some(id))
            .map_err(|e| crate::errs::coded("errOpenWith", e))
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::OsApp;
    use std::path::Path;

    pub fn list(_file: &Path) -> Vec<OsApp> {
        Vec::new()
    }

    pub fn list_for_type(_ext: &str) -> Vec<OsApp> {
        Vec::new()
    }

    pub fn open(file: &Path, id: &str, _via: Option<&str>) -> Result<(), String> {
        tauri_plugin_opener::open_path(file, Some(id)).map_err(|e| e.to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn mac_lists_preview_for_a_jpeg_once() {
        let dir = std::env::temp_dir().join("pictkura-os-apps-test");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("probe.jpg");
        std::fs::write(&f, b"\xff\xd8\xff\xd9").unwrap();
        let apps = list(&f);
        let preview: Vec<_> = apps
            .iter()
            .filter(|a| a.key == "com.apple.preview")
            .collect();
        assert_eq!(preview.len(), 1, "{apps:?}");
        // 束 ID ごとに 1 つ
        let mut keys: Vec<_> = apps.iter().map(|a| &a.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), apps.len());
    }
}
