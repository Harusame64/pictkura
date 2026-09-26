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
//! | `QUERYREMOVE` | そのルートだけ監視から外し、ハンドルを閉じる。**届け出は残す**。取り外し中の印を立てる |
//! | `QUERYREMOVEFAILED` | 他が断った。古い届け出を外し、開き直して届け出し直し、監視に戻す（開けなければ下の確かめへ回す） |
//! | `REMOVEPENDING` / `REMOVECOMPLETE` | 外れた。届け出を外す（抜かれたときはハンドルも閉じ、監視も外す） |
//! | 差し込み（ボリュームの `ARRIVAL`） | 0.5 秒ごとに確かめ、戻ったルートを監視に入れて届け出る |
//!
//! **外すのも戻すのもルート1つずつ**（`Watching::unwatch_root` / `watch_root`）。まるごと作り直すと、
//! ほかのルートの束ねる前のイベントまで捨てる（#161 のゲート2）。**取り外し中のルートは確かめで
//! 戻さない**——ボリュームは PnP が他のアプリに訊き終えるまで見えたままなので、戻すと取り外しを断る。
//! **ネットワークのルートは確かめない**——オフラインの NAS は `is_dir` が数秒〜数十秒止まり、
//! この窓が取り外しの問いに答えられなくなる（ボリュームの差し込みで戻るものでもない）
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

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDriveTypeW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::GUID_DEVINTERFACE_VOLUME;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterDeviceNotificationW, SetTimer,
    TranslateMessage, UnregisterDeviceNotification, DBT_DEVICEARRIVAL, DBT_DEVICEQUERYREMOVE,
    DBT_DEVICEQUERYREMOVEFAILED, DBT_DEVICEREMOVECOMPLETE, DBT_DEVICEREMOVEPENDING,
    DBT_DEVTYP_DEVICEINTERFACE, DBT_DEVTYP_HANDLE, DEVICE_NOTIFY_WINDOW_HANDLE,
    DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HANDLE, DEV_BROADCAST_HDR, HDEVNOTIFY,
    HWND_MESSAGE, MSG, WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE, WM_TIMER, WNDCLASSW,
};

use notify_debouncer_mini::notify::windows::{MetaEvent, ReadDirectoryChangesWatcher};
use notify_debouncer_mini::notify::{Config, EventHandler, RecursiveMode, Watcher, WatcherKind};

use super::Watching;

/// `WM_DEVICECHANGE` で TRUE（取り外してよい）
const BROADCAST_QUERY_ALLOW: LRESULT = 1;
const TIMER_REARM: usize = 1;
/// 差し込まれてからボリュームが見えるまで待つ回数（0.5 秒ごと・最長 10 秒）。
/// **止まらない見張りを作らない**——戻らないルートがあっても、ここで諦める
const REARM_TRIES: u32 = 20;

/// 閉じ終えるまで待つ監視器（#161 の codex の P1）。
///
/// notify 7.0.0 の `ReadDirectoryChangesWatcher` は、落とされると停止を送って起こすだけで戻り、
/// ハンドルを閉じる（`stop_watch`）のは監視の糸——**取り外しを許した時点でまだ握っている**ことがある。
/// 閉じ終えるたびに `MetaEvent::SingleWatchComplete` が送られる（素の `new` はその受け口を捨てている）ので、
/// 受け口を自分で持って作り、**落とすときは張った数ぶん届くまで待つ**（最長2秒。届かなければ諦めて戻る
/// ——取り外しが1回断られるだけで、固まらない）
pub struct AckedWatcher {
    inner: Option<ReadDirectoryChangesWatcher>,
    closed: mpsc::Receiver<MetaEvent>,
    /// 張っていて、まだ閉じていない数
    live: usize,
}

impl AckedWatcher {
    fn wait_closed(&self, mut n: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while n > 0 {
            let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
                return;
            };
            match self.closed.recv_timeout(left) {
                Ok(MetaEvent::SingleWatchComplete) => n -= 1,
                Ok(_) => {}
                Err(_) => return,
            }
        }
    }
}

impl Watcher for AckedWatcher {
    fn new<F: EventHandler>(
        event_handler: F,
        _config: Config,
    ) -> notify_debouncer_mini::notify::Result<Self> {
        let (tx, closed) = mpsc::channel();
        let inner = ReadDirectoryChangesWatcher::create(Arc::new(Mutex::new(event_handler)), tx)?;
        Ok(Self {
            inner: Some(inner),
            closed,
            live: 0,
        })
    }

    fn watch(
        &mut self,
        path: &Path,
        mode: RecursiveMode,
    ) -> notify_debouncer_mini::notify::Result<()> {
        let inner = self.inner.as_mut().expect("落とすまで在る");
        inner.watch(path, mode)?;
        self.live += 1;
        Ok(())
    }

    fn unwatch(&mut self, path: &Path) -> notify_debouncer_mini::notify::Result<()> {
        let inner = self.inner.as_mut().expect("落とすまで在る");
        inner.unwatch(path)?;
        self.live = self.live.saturating_sub(1);
        self.wait_closed(1);
        Ok(())
    }

    fn configure(&mut self, config: Config) -> notify_debouncer_mini::notify::Result<bool> {
        self.inner
            .as_mut()
            .expect("落とすまで在る")
            .configure(config)
    }

    fn kind() -> WatcherKind {
        WatcherKind::ReadDirectoryChangesWatcher
    }
}

impl Drop for AckedWatcher {
    fn drop(&mut self) {
        drop(self.inner.take());
        self.wait_closed(self.live);
    }
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
    /// 取り外しを訊かれて手放した（外れるか断られるまで、確かめで戻さない）
    ejecting: bool,
    /// ネットワーク上のルート（差し込みの確かめで触らない）
    remote: bool,
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
    let filter = DEV_BROADCAST_HANDLE {
        dbch_size: std::mem::size_of::<DEV_BROADCAST_HANDLE>() as u32,
        dbch_devicetype: DBT_DEVTYP_HANDLE,
        dbch_handle: h,
        ..Default::default()
    };
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

/// そのルートを監視に入れ、入ったら開いて届け出る（開き直す前に古い届け出とハンドルは外す）
fn arm(st: &mut State, i: usize) {
    unregister(&mut st.entries[i]);
    close_dir(&mut st.entries[i]);
    let ok = match st.watching.lock() {
        Ok(mut w) => w.watch_root(&st.entries[i].path),
        Err(_) => false,
    };
    st.entries[i].watched = ok;
    if ok {
        let hwnd = st.hwnd;
        open_and_register(hwnd, &mut st.entries[i]);
    }
}

/// そのルートを監視から外す（ほかのルートには触らない）
fn disarm_watch(st: &mut State, i: usize) {
    if st.entries[i].watched {
        if let Ok(mut w) = st.watching.lock() {
            w.unwatch_root(&st.entries[i].path);
        }
        st.entries[i].watched = false;
    }
}

/// 差し込みの確かめを始める（もう回っていれば回数を戻す）
fn start_rearm(st: &mut State) {
    st.tries_left = REARM_TRIES;
    unsafe { SetTimer(st.hwnd, TIMER_REARM, 500, None) };
}

/// 確かめで戻してよいルートか
fn rearmable(e: &Entry) -> bool {
    !e.watched && !e.ejecting && !e.remote
}

/// ネットワーク上のルートか（UNC か、割り当てたネットワークドライブ）
fn is_remote(path: &Path) -> bool {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\") && !s.starts_with(r"\\?\") {
        return true;
    }
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(d), Some(':')) if d.is_ascii_alphabetic() => {
            let root: Vec<u16> = format!("{d}:\\\0").encode_utf16().collect();
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
        }
        _ => false,
    }
}

/// 知らせの届け出に当たるルート（同じドライブに複数のルートがあれば、届け出ごとに別々に来る）
unsafe fn entry_of(st: &mut State, lparam: LPARAM) -> Option<usize> {
    if lparam == 0 {
        return None;
    }
    let hdr = &*(lparam as *const DEV_BROADCAST_HDR);
    if hdr.dbch_devicetype != DBT_DEVTYP_HANDLE {
        return None;
    }
    let h = &*(lparam as *const DEV_BROADCAST_HANDLE);
    st.entries
        .iter()
        .position(|e| !e.notify.is_null() && e.notify == h.dbch_hdevnotify)
}

fn on_device_change(st: &mut State, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Ok(event) = u32::try_from(wparam) else {
        return BROADCAST_QUERY_ALLOW;
    };
    match event {
        DBT_DEVICEQUERYREMOVE => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // そのルートだけ監視から外し（閉じ終えるまで待つ）、ハンドルを閉じる。
                // **届け出は残す**（FAILED を受けるため）
                st.entries[i].ejecting = true;
                disarm_watch(st, i);
                close_dir(&mut st.entries[i]);
            }
        }
        DBT_DEVICEQUERYREMOVEFAILED => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // 他が断った。ドライブは付いたまま——古い届け出とハンドルを外し、開き直して監視へ戻す。
                // **ハンドルも閉じてから開き直す**: QUERYREMOVE を経ずに FAILED だけが来ることがあり、
                // そのとき握ったまま上書きすると、届け出の無いハンドルが残って次の取り外しを断る
                // （#161 の codex の P2）。**その場で開けなければ確かめに回す**——戻る道を失わない
                st.entries[i].ejecting = false;
                arm(st, i);
                if !st.entries[i].watched && !st.entries[i].remote {
                    start_rearm(st);
                }
            }
        }
        DBT_DEVICEREMOVEPENDING | DBT_DEVICEREMOVECOMPLETE => {
            if let Some(i) = unsafe { entry_of(st, lparam) } {
                // 外れた。**いきなり抜かれた**ときは QUERYREMOVE が来ないので、ここで監視も手放す
                st.entries[i].ejecting = false;
                disarm_watch(st, i);
                unregister(&mut st.entries[i]);
                close_dir(&mut st.entries[i]);
            }
        }
        // ボリュームがまだ見えていないことがあるので、少しずつ確かめる
        DBT_DEVICEARRIVAL if st.entries.iter().any(rearmable) => start_rearm(st),
        _ => {}
    }
    BROADCAST_QUERY_ALLOW
}

fn on_rearm_timer(st: &mut State) {
    st.tries_left = st.tries_left.saturating_sub(1);
    for i in 0..st.entries.len() {
        if rearmable(&st.entries[i]) && st.entries[i].path.is_dir() {
            arm(st, i);
        }
    }
    if st.tries_left == 0 || !st.entries.iter().any(rearmable) {
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
        let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
            dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
            dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
            dbcc_classguid: GUID_DEVINTERFACE_VOLUME,
            ..Default::default()
        };
        let volume = RegisterDeviceNotificationW(
            hwnd,
            &filter as *const _ as *const _,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );

        let mut entries: Vec<Entry> = roots
            .into_iter()
            .map(|path| Entry {
                watched: watched.contains(&path),
                ejecting: false,
                remote: is_remote(&path),
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
