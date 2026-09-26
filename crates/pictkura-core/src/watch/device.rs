//! Windows: 取り外せるドライブの上のルートを、「安全な取り外し」の求めで手放す（dev #35）。
//!
//! 監視（ReadDirectoryChangesW）はルートごとにディレクトリのハンドルを握るので、放っておくと
//! **アプリを開いている間は取り外しが必ず断られる**（win の実測: Kernel-PnP 225 が pictkura を名指し）。
//! OS は、ハンドルを `RegisterDeviceNotificationW`（`DBT_DEVTYP_HANDLE`）で届け出た窓にだけ
//! 「取り外してよいか」（`DBT_DEVICEQUERYREMOVE`）を訊く。訊かれたら手放して TRUE を返す。
//!
//! 形は Microsoft の手順どおり（win の spike の S2b。dev `spikes/usb-eject/results.md`）:
//!
//! | 知らせ | すること |
//! |---|---|
//! | `QUERYREMOVE` | 監視を外し、ディレクトリのハンドルを閉じる。**届け出は残す** |
//! | `QUERYREMOVEFAILED` | 他が断った。古い届け出を外し、開き直して届け出し直し、監視に戻す |
//! | `REMOVEPENDING` / `REMOVECOMPLETE` | 外れた。届け出を外す（抜かれたときはハンドルも閉じる） |
//! | 差し込み（ボリュームの `ARRIVAL`） | 0.5 秒ごとに確かめ、戻ったルートを監視に入れて届け出る |
//!
//! **`QUERYREMOVE` で届け出まで外すと、他が断ったときの `QUERYREMOVEFAILED` が届かない**
//! ——その知らせは届け出に宛てて来るので、監視が外れたまま黙って止まる（spike の S2 で実測）。
//!
//! 窓はメッセージ専用（`HWND_MESSAGE`）で、この糸が自前のメッセージループを回す。
//! Tauri の窓とは関係が無い（spike で、メッセージ専用の窓に3種類とも届くと確かめた）

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterDeviceNotificationW, SetTimer,
    TranslateMessage, UnregisterDeviceNotification, DEVICE_NOTIFY_WINDOW_HANDLE, HDEVNOTIFY,
    HWND_MESSAGE, MSG, WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE, WM_TIMER, WNDCLASSW,
};

use super::Watching;

const DBT_DEVICEARRIVAL: usize = 0x8000;
const DBT_DEVICEQUERYREMOVE: usize = 0x8001;
const DBT_DEVICEQUERYREMOVEFAILED: usize = 0x8002;
const DBT_DEVICEREMOVEPENDING: usize = 0x8003;
const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;
const DBT_DEVTYP_DEVICEINTERFACE: u32 = 5;
const DBT_DEVTYP_HANDLE: u32 = 6;
/// `WM_DEVICECHANGE` で TRUE（取り外してよい）
const BROADCAST_QUERY_ALLOW: LRESULT = 1;
const TIMER_REARM: usize = 1;
/// 差し込まれてからボリュームが見えるまで待つ回数（0.5 秒ごと・最長 10 秒）。
/// **止まらない見張りを作らない**——戻らないルートがあっても、ここで諦める
const REARM_TRIES: u32 = 20;

// {53F5630D-B6BF-11D0-94F2-00A0C91EFB8B}
const GUID_DEVINTERFACE_VOLUME: GUID = GUID::from_u128(0x53f5630d_b6bf_11d0_94f2_00a0c91efb8b);

#[repr(C)]
struct DevBroadcastHdr {
    size: u32,
    devicetype: u32,
    reserved: u32,
}

#[repr(C)]
struct DevBroadcastHandle {
    size: u32,
    devicetype: u32,
    reserved: u32,
    handle: HANDLE,
    hdevnotify: HDEVNOTIFY,
    eventguid: GUID,
    nameoffset: i32,
    data: [u8; 1],
}

#[repr(C)]
struct DevBroadcastDeviceInterfaceW {
    size: u32,
    devicetype: u32,
    reserved: u32,
    classguid: GUID,
    name: [u16; 1],
}

/// 知らせを受ける糸。drop すると窓を閉じ、糸を待ち合わせる（届け出とハンドルは糸が片付ける）
pub(crate) struct DeviceGuard {
    /// 窓（`HWND` は糸をまたげないので数で持つ）
    hwnd: usize,
    thread: Option<JoinHandle<()>>,
}

impl DeviceGuard {
    /// 糸を立てる。窓が作れなければ `None`（監視そのものは動く——取り外しを断る元の姿に戻るだけ）
    pub(crate) fn start(
        watching: Arc<Mutex<Watching>>,
        roots: Vec<PathBuf>,
        watched: &[PathBuf],
    ) -> Option<Self> {
        let watched = watched.to_vec();
        let (tx, rx) = mpsc::channel::<usize>();
        let thread = std::thread::Builder::new()
            .name("pictkura-device-watch".into())
            .spawn(move || run(watching, roots, watched, tx))
            .ok()?;
        match rx.recv() {
            Ok(hwnd) if hwnd != 0 => Some(Self {
                hwnd,
                thread: Some(thread),
            }),
            _ => {
                let _ = thread.join();
                None
            }
        }
    }
}

impl Drop for DeviceGuard {
    fn drop(&mut self) {
        unsafe { PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// ルート1つぶんの状態
struct Entry {
    path: PathBuf,
    /// 届け出のために開いたディレクトリのハンドル（閉じていれば null）
    dir: HANDLE,
    /// 届け出（外していれば null）
    notify: HDEVNOTIFY,
    /// いま監視に入っているか
    watched: bool,
}

struct State {
    hwnd: HWND,
    watching: Arc<Mutex<Watching>>,
    entries: Vec<Entry>,
    volume: HDEVNOTIFY,
    tries_left: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// ディレクトリを開いて届け出る。どちらかが駄目なら何も持たない（その場合は取り外しを断る元の姿）
fn open_and_register(hwnd: HWND, e: &mut Entry) {
    let path = wide(&e.path);
    let h = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return;
    }
    let mut filter: DevBroadcastHandle = unsafe { std::mem::zeroed() };
    filter.size = std::mem::size_of::<DevBroadcastHandle>() as u32;
    filter.devicetype = DBT_DEVTYP_HANDLE;
    filter.handle = h;
    let notify = unsafe {
        RegisterDeviceNotificationW(
            hwnd,
            &filter as *const _ as *const _,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    };
    if notify.is_null() {
        unsafe { CloseHandle(h) };
        return;
    }
    e.dir = h;
    e.notify = notify;
}

fn close_dir(e: &mut Entry) {
    if !e.dir.is_null() {
        unsafe { CloseHandle(e.dir) };
        e.dir = std::ptr::null_mut();
    }
}

fn unregister(e: &mut Entry) {
    if !e.notify.is_null() {
        unsafe { UnregisterDeviceNotification(e.notify) };
        e.notify = std::ptr::null_mut();
    }
}

/// `watched` の立っているルートで監視を作り直し、実際に監視できたかを書き戻す
fn rewatch(st: &mut State) {
    let want: Vec<PathBuf> = st
        .entries
        .iter()
        .filter(|e| e.watched)
        .map(|e| e.path.clone())
        .collect();
    let got = match st.watching.lock() {
        Ok(mut w) => w.rebuild(&want).unwrap_or_default(),
        Err(_) => return,
    };
    for e in &mut st.entries {
        e.watched = got.contains(&e.path);
    }
}

/// 知らせの届け出に当たるルート（同じドライブに複数のルートがあれば、届け出ごとに別々に来る）
unsafe fn entry_of(st: &mut State, lparam: LPARAM) -> Option<usize> {
    if lparam == 0 {
        return None;
    }
    let hdr = &*(lparam as *const DevBroadcastHdr);
    if hdr.devicetype != DBT_DEVTYP_HANDLE {
        return None;
    }
    let h = &*(lparam as *const DevBroadcastHandle);
    st.entries
        .iter()
        .position(|e| !e.notify.is_null() && e.notify == h.hdevnotify)
}

fn on_device_change(st: &mut State, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match wparam {
        DBT_DEVICEQUERYREMOVE => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // 監視を外して（作り直して）、ハンドルを閉じる。**届け出は残す**（FAILED を受けるため）
                st.entries[i].watched = false;
                rewatch(st);
                close_dir(&mut st.entries[i]);
            }
            BROADCAST_QUERY_ALLOW
        }
        DBT_DEVICEQUERYREMOVEFAILED => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // 他が断った。ドライブは付いたまま——古い届け出を外し、開き直して監視へ戻す
                unregister(&mut st.entries[i]);
                if st.entries[i].path.is_dir() {
                    st.entries[i].watched = true;
                    rewatch(st);
                    let hwnd = st.hwnd;
                    open_and_register(hwnd, &mut st.entries[i]);
                }
            }
            BROADCAST_QUERY_ALLOW
        }
        DBT_DEVICEREMOVEPENDING | DBT_DEVICEREMOVECOMPLETE => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // 外れた。**いきなり抜かれた**ときは QUERYREMOVE が来ないので、ここで監視も手放す
                unregister(&mut st.entries[i]);
                close_dir(&mut st.entries[i]);
                if st.entries[i].watched {
                    st.entries[i].watched = false;
                    rewatch(st);
                }
            }
            BROADCAST_QUERY_ALLOW
        }
        DBT_DEVICEARRIVAL => {
            // ボリュームがまだ見えていないことがあるので、少しずつ確かめる
            if st.entries.iter().any(|e| !e.watched) {
                st.tries_left = REARM_TRIES;
                unsafe { SetTimer(st.hwnd, TIMER_REARM, 500, None) };
            }
            BROADCAST_QUERY_ALLOW
        }
        _ => BROADCAST_QUERY_ALLOW,
    }
}

fn on_rearm_timer(st: &mut State) {
    st.tries_left = st.tries_left.saturating_sub(1);
    let back: Vec<usize> = st
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.watched && e.path.is_dir())
        .map(|(i, _)| i)
        .collect();
    if !back.is_empty() {
        for &i in &back {
            st.entries[i].watched = true;
        }
        rewatch(st);
        let hwnd = st.hwnd;
        for &i in &back {
            // 開き直す前に古い届け出が残っていれば外す（抜かれて REMOVECOMPLETE を取り逃した等）
            unregister(&mut st.entries[i]);
            close_dir(&mut st.entries[i]);
            if st.entries[i].watched {
                open_and_register(hwnd, &mut st.entries[i]);
            }
        }
    }
    if st.tries_left == 0 || st.entries.iter().all(|e| e.watched) {
        unsafe { KillTimer(st.hwnd, TIMER_REARM) };
    }
}

fn cleanup(st: &mut State) {
    unsafe { KillTimer(st.hwnd, TIMER_REARM) };
    for e in &mut st.entries {
        unregister(e);
        close_dir(e);
    }
    if !st.volume.is_null() {
        unsafe { UnregisterDeviceNotification(st.volume) };
        st.volume = std::ptr::null_mut();
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DEVICECHANGE => STATE.with(|s| match s.borrow_mut().as_mut() {
            Some(st) => on_device_change(st, wparam, lparam),
            None => BROADCAST_QUERY_ALLOW,
        }),
        WM_TIMER if wparam == TIMER_REARM => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    on_rearm_timer(st);
                }
            });
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    cleanup(st);
                }
            });
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn run(
    watching: Arc<Mutex<Watching>>,
    roots: Vec<PathBuf>,
    watched: Vec<PathBuf>,
    ready: mpsc::Sender<usize>,
) {
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class: Vec<u16> = "PictkuraDeviceWatch\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        // 2度目（監視の張り直し）は「もう在る」で 0 が返る。窓が作れるかで判断する
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            let _ = ready.send(0);
            return;
        }

        // 差し込みを知るための届け出（ボリュームの種類に宛てる。ハンドルは握らない）
        let mut filter: DevBroadcastDeviceInterfaceW = std::mem::zeroed();
        filter.size = std::mem::size_of::<DevBroadcastDeviceInterfaceW>() as u32;
        filter.devicetype = DBT_DEVTYP_DEVICEINTERFACE;
        filter.classguid = GUID_DEVINTERFACE_VOLUME;
        let volume = RegisterDeviceNotificationW(
            hwnd,
            &filter as *const _ as *const _,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );

        let mut entries: Vec<Entry> = roots
            .into_iter()
            .map(|path| Entry {
                watched: watched.contains(&path),
                path,
                dir: std::ptr::null_mut(),
                notify: std::ptr::null_mut(),
            })
            .collect();
        for e in &mut entries {
            if e.watched {
                open_and_register(hwnd, e);
            }
        }
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                hwnd,
                watching,
                entries,
                volume,
                tries_left: 0,
            })
        });
        let _ = ready.send(hwnd as usize);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        STATE.with(|s| *s.borrow_mut() = None);
    }
}
