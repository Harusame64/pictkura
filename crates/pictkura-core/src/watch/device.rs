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
//! | `CUSTOMEVENT` の `VOLUME_LOCK` / `VOLUME_DISMOUNT` | **リムーバブルのドライブだけ**: `QUERYREMOVE` と同じく手放す（届け出は残す）。ロック中の印を立てる |
//! | `CUSTOMEVENT` の `VOLUME_LOCK_FAILED` / `VOLUME_DISMOUNT_FAILED` | ロック中なら、`QUERYREMOVEFAILED` と同じく戻す |
//! | `CUSTOMEVENT` の `VOLUME_UNLOCK` | ロック中の印を下ろし、**短い確かめに回す**（その場では戻さない。2回・1秒まで） |
//! | `CUSTOMEVENT` の `VOLUME_MOUNT` | ロック中の印を下ろし、**監視中でも張り直す**（カードを差し直したとき、`ARRIVAL` は来ない） |
//!
//! **ロック中の印は、取り外し中の印（`QUERYREMOVE`）と別に持つ**。`UNLOCK` や `MOUNT` が USB の取り外しの
//! 途中に来ても、取り外し中のルートを確かめに戻さない（#164 のゲート2）。固定ドライブのロック（バックアップや
//! 点検のツール）では手放さない——戻るまでの変化を取りこぼすだけで、取り出しは起きないから
//!
//! **カードリーダーの SD は `QUERYREMOVE` を通らない**（dev #38）。取り出しは「メディアの取り出し」で、
//! デバイスは残る——ボリュームのロック（`FSCTL_LOCK_VOLUME`）が開いたハンドルで失敗し、「使用中」の
//! 画面になっていた。ロックの知らせは、ディレクトリのハンドルの届け出に `DBT_CUSTOMEVENT` で届く
//! （win の spike: `dev/spikes/usb-eject/results-38.md`）。**`UNLOCK` は取り出しが通ったあとにも**
//! 70 ms ほど遅れて来るので、そこで張り直すと手放したハンドルをまた握る。確かめ（0.5 秒後に
//! ルートが見えるか）に回せば、取り出したあとはボリュームが消えていて張り直さず、別の理由の
//! ロック（点検など）なら見えたままなので戻る
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

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDriveTypeW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Diagnostics::Debug::{SetThreadErrorMode, SEM_FAILCRITICALERRORS};
use windows_sys::Win32::System::Ioctl::GUID_DEVINTERFACE_VOLUME;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_REMOTE, DRIVE_REMOVABLE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, KillTimer,
    PostMessageW, PostQuitMessage, RegisterClassW, RegisterDeviceNotificationW, SetTimer,
    TranslateMessage, UnregisterDeviceNotification, DBT_CUSTOMEVENT, DBT_DEVICEARRIVAL,
    DBT_DEVICEQUERYREMOVE, DBT_DEVICEQUERYREMOVEFAILED, DBT_DEVICEREMOVECOMPLETE,
    DBT_DEVICEREMOVEPENDING, DBT_DEVTYP_DEVICEINTERFACE, DBT_DEVTYP_HANDLE,
    DEVICE_NOTIFY_WINDOW_HANDLE, DEV_BROADCAST_DEVICEINTERFACE_W, DEV_BROADCAST_HANDLE,
    DEV_BROADCAST_HDR, GUID_IO_VOLUME_DISMOUNT, GUID_IO_VOLUME_DISMOUNT_FAILED,
    GUID_IO_VOLUME_LOCK, GUID_IO_VOLUME_LOCK_FAILED, GUID_IO_VOLUME_MOUNT, GUID_IO_VOLUME_UNLOCK,
    HDEVNOTIFY, HWND_MESSAGE, MSG, WM_APP, WM_CLOSE, WM_DESTROY, WM_DEVICECHANGE, WM_TIMER,
    WNDCLASSW,
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
    /// 落とすまでは必ず在る。無ければ（落とした後に呼ばれた）失敗として返す
    fn inner(&mut self) -> notify_debouncer_mini::notify::Result<&mut ReadDirectoryChangesWatcher> {
        self.inner
            .as_mut()
            .ok_or_else(|| notify_debouncer_mini::notify::Error::generic("watcher already dropped"))
    }

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
        self.inner()?.watch(path, mode)?;
        self.live += 1;
        Ok(())
    }

    fn unwatch(&mut self, path: &Path) -> notify_debouncer_mini::notify::Result<()> {
        self.inner()?.unwatch(path)?;
        self.live = self.live.saturating_sub(1);
        self.wait_closed(1);
        Ok(())
    }

    fn configure(&mut self, config: Config) -> notify_debouncer_mini::notify::Result<bool> {
        self.inner()?.configure(config)
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

/// 「確かめを始めよ」の合図（[`DeviceGuard::rearm`]）。窓の糸だけが状態に触れるので、外からは合図を送る
const WM_APP_REARM: u32 = WM_APP + 1;

impl DeviceGuard {
    /// あとから差し込まれたドライブの上のルートを、確かめに回させる（`LibraryWatcher::watch_returned`）。
    /// 確かめは監視に入れるのと一緒に、取り外しの知らせの届け出もする
    pub(crate) fn rearm(&self) {
        unsafe { PostMessageW(self.hwnd as HWND, WM_APP_REARM, 0, 0) };
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
    /// リムーバブルのドライブの上か（カードリーダーの SD・USB メモリ）。ボリュームのロックで手放すのはこちらだけ
    removable: bool,
    /// ボリュームのロックで手放した（`UNLOCK` / `LOCK_FAILED` / `MOUNT` まで、確かめで戻さない）
    locked: bool,
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

/// そのルートを監視に入れ、入ったら開いて届け出る（開き直す前に古い届け出とハンドルは外す）。
///
/// **監視中なら先に外す**: notify 7.0.0 の `add_watch` は同じパスの監視を止めずに表を上書きし、
/// 古いディレクトリのハンドルは誰も閉じられなくなる——取り外しがアプリを閉じるまで断られる
/// （QUERYREMOVE を経ずに FAILED が来たとき。#161 の codex、2周目の P1）
fn arm(st: &mut State, i: usize) {
    disarm_watch(st, i);
    unregister(&mut st.entries[i]);
    close_dir(&mut st.entries[i]);
    let ok = match st.watching.lock() {
        Ok(mut w) => w.watch_root(&st.entries[i].path),
        Err(_) => false,
    };
    st.entries[i].watched = ok;
    if ok {
        // **張れた時点でリムーバブルかを訊き直す**——起動時にドライブが無かったルートは
        // `GetDriveTypeW` が「ルートが無い」と答えて偽のまま残り、ロックの知らせを無視していた
        // （#164 の codex、2周目）
        st.entries[i].removable = is_removable(&st.entries[i].path);
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

/// 差し込みの確かめを始める（もう回っていれば、多いほうの回数にそろえる）
fn start_rearm(st: &mut State, tries: u32) {
    st.tries_left = st.tries_left.max(tries);
    unsafe { SetTimer(st.hwnd, TIMER_REARM, 500, None) };
}

/// 確かめで戻してよいルートか
fn rearmable(e: &Entry) -> bool {
    !e.watched && !e.ejecting && !e.locked && !e.remote
}

/// 取り外し（`QUERYREMOVE`）かボリュームのロックの求めで、そのルートを手放す。**届け出は残す**
/// （断られたときの知らせを受けるため）
fn release_for_removal(st: &mut State, i: usize) {
    disarm_watch(st, i);
    close_dir(&mut st.entries[i]);
}

/// 他が断った（取り外しは起きなかった）。開き直して監視へ戻す。その場で開けなければ確かめに回す
fn restore_after_refusal(st: &mut State, i: usize) {
    arm(st, i);
    if !st.entries[i].watched && !st.entries[i].remote {
        start_rearm(st, REARM_TRIES);
    }
}

/// ネットワーク上のルートか（UNC か、割り当てたネットワークドライブ）。形の読み分けは
/// `super::root_location`（長いパスの書き方も読む）
fn is_remote(path: &Path) -> bool {
    match super::root_location(&path.as_os_str().to_string_lossy()) {
        super::RootLocation::Unc => true,
        super::RootLocation::Drive(d) => {
            let root: Vec<u16> = format!("{d}:\\\0").encode_utf16().collect();
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
        }
        super::RootLocation::Other => false,
    }
}

/// リムーバブルのドライブの上のルートか（ドライブ文字で OS に訊く。長いパスの書き方も読む）
fn is_removable(path: &Path) -> bool {
    match super::root_location(&path.as_os_str().to_string_lossy()) {
        super::RootLocation::Drive(d) => {
            let root: Vec<u16> = format!("{d}:\\\0").encode_utf16().collect();
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOVABLE }
        }
        _ => false,
    }
}

/// 知らせの中身がハンドルの届け出なら、それを返す（型が違えば `None`）
unsafe fn broadcast_handle<'a>(lparam: LPARAM) -> Option<&'a DEV_BROADCAST_HANDLE> {
    if lparam == 0 {
        return None;
    }
    let hdr = &*(lparam as *const DEV_BROADCAST_HDR);
    if hdr.dbch_devicetype != DBT_DEVTYP_HANDLE {
        return None;
    }
    Some(&*(lparam as *const DEV_BROADCAST_HANDLE))
}

/// 知らせの届け出に当たるルート（同じドライブに複数のルートがあれば、届け出ごとに別々に来る）
fn entry_of(st: &State, h: &DEV_BROADCAST_HANDLE) -> Option<usize> {
    st.entries
        .iter()
        .position(|e| !e.notify.is_null() && e.notify == h.dbch_hdevnotify)
}

/// GUID が同じか（windows-sys の `GUID` は `PartialEq` を持たない）
fn same_guid(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

/// ボリュームのロックの知らせ（カードリーダーの SD の取り出し、dev #38）
fn on_volume_event(st: &mut State, i: usize, guid: &GUID) {
    let is = |g: &GUID| same_guid(guid, g);
    if is(&GUID_IO_VOLUME_LOCK) {
        // 固定ドライブのロックでは手放さない（取り出しは起きない。戻るまでの変化を取りこぼすだけ）
        if st.entries[i].removable {
            st.entries[i].locked = true;
            release_for_removal(st, i);
        }
    } else if is(&GUID_IO_VOLUME_DISMOUNT) {
        // 手放すが、**ロック中の印は立てない**——ロック無しで来る取り外し（`fsutil volume dismount` 等）
        // では、印を下ろす UNLOCK が来ず、印が立ったまま戻らなくなる（#164 のゲート2）。取り出しの
        // 途中なら、先に来た LOCK が印を立てている
        if st.entries[i].removable {
            release_for_removal(st, i);
        }
    } else if is(&GUID_IO_VOLUME_LOCK_FAILED) || is(&GUID_IO_VOLUME_DISMOUNT_FAILED) {
        // ロックで手放したもの**と、ロック無しの DISMOUNT で手放したもの**（印は立てていない）を戻す
        // （#164 の codex、最終 head）。**取り外し（QUERYREMOVE）の答えを待っている間は戻さない**
        // ——その印は別に持っている（#164 の codex、3周目）
        let released = st.entries[i].locked || !st.entries[i].watched;
        st.entries[i].locked = false;
        if released && !st.entries[i].ejecting {
            restore_after_refusal(st, i);
        }
    } else if is(&GUID_IO_VOLUME_UNLOCK) {
        // その場では戻さない（取り出しが通ったあとにも 70 ms ほどで来る）。短い確かめに回す——取り出した
        // あとは空のカードリーダーを長く叩かない（2回・1秒まで）
        if st.entries[i].locked {
            st.entries[i].locked = false;
            if rearmable(&st.entries[i]) {
                start_rearm(st, 2);
            }
        }
    } else if is(&GUID_IO_VOLUME_MOUNT) {
        // カードが差し直された。**監視中でも張り直す**——取り出さずに抜いたカードは LOCK が来ないので
        // 監視中のまま古いボリュームを指している（#164 のゲート2）。`arm` は先に外してから張る
        st.entries[i].locked = false;
        if st.entries[i].watched {
            arm(st, i);
        }
        // その場で張れなければ確かめに回す（差し直しでは ARRIVAL が来ないので、ここで諦めると戻らない）
        if rearmable(&st.entries[i]) {
            start_rearm(st, REARM_TRIES);
        }
    }
}

fn on_device_change(st: &mut State, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Ok(event) = u32::try_from(wparam) else {
        return BROADCAST_QUERY_ALLOW;
    };
    let handle = unsafe { broadcast_handle(lparam) };
    let entry = handle.and_then(|h| entry_of(st, h).map(|i| (i, h.dbch_eventguid)));
    match (event, entry) {
        (DBT_DEVICEQUERYREMOVE, Some((i, _))) => {
            // そのルートだけ監視から外し（閉じ終えるまで待つ）、ハンドルを閉じる。
            // **届け出は残す**（FAILED を受けるため）
            st.entries[i].ejecting = true;
            release_for_removal(st, i);
        }
        (DBT_DEVICEQUERYREMOVEFAILED, Some((i, _))) => {
            // 他が断った。ドライブは付いたまま——古い届け出とハンドルを外し、開き直して監視へ戻す。
            // **ハンドルも閉じてから開き直す**: QUERYREMOVE を経ずに FAILED だけが来ることがあり、
            // そのとき握ったまま上書きすると、届け出の無いハンドルが残って次の取り外しを断る
            // （#161 の codex の P2）。**その場で開けなければ確かめに回す**——戻る道を失わない
            st.entries[i].ejecting = false;
            // ボリュームのロックの答えを待っている間は戻さない（逆向きも同じ）
            if !st.entries[i].locked {
                restore_after_refusal(st, i);
            }
        }
        (DBT_DEVICEREMOVEPENDING | DBT_DEVICEREMOVECOMPLETE, Some((i, _))) => {
            // 外れた。**いきなり抜かれた**ときは QUERYREMOVE が来ないので、ここで監視も手放す
            st.entries[i].ejecting = false;
            st.entries[i].locked = false;
            disarm_watch(st, i);
            unregister(&mut st.entries[i]);
            close_dir(&mut st.entries[i]);
        }
        (DBT_CUSTOMEVENT, Some((i, guid))) => on_volume_event(st, i, &guid),
        // ボリュームがまだ見えていないことがあるので、少しずつ確かめる
        (DBT_DEVICEARRIVAL, _) if st.entries.iter().any(rearmable) => start_rearm(st, REARM_TRIES),
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
        // **残りの回数も捨てる**——残すと、次の UNLOCK の短い確かめ（2回）が `max` で前の回数を継ぐ
        st.tries_left = 0;
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
        WM_APP_REARM => {
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    // ドライブが現れた。**リムーバブルの上で監視中のルートも張り直す**——取り出さずに
                    // 差し替えたカードは、監視中のまま古いボリュームを指していることがある（#164 の
                    // ゲート2）。取り外し中・ロック中のものには触らない
                    for i in 0..st.entries.len() {
                        let e = &st.entries[i];
                        if e.watched && e.removable && !e.ejecting && !e.locked && e.path.is_dir() {
                            arm(st, i);
                        }
                    }
                    if st.entries.iter().any(rearmable) {
                        start_rearm(st, REARM_TRIES);
                    }
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
        // この糸はカードリーダーの空のドライブを確かめる（`is_dir`）ことがある。古いドライバで
        // 「ドライブにディスクがありません」の画面を出させない（#164 のゲート2）
        SetThreadErrorMode(SEM_FAILCRITICALERRORS, std::ptr::null_mut());
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
                removable: is_removable(&path),
                locked: false,
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
