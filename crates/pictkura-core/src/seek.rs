//! そのドライブが**シークに時間のかかる媒体**（回転する円盤）か（pictkura-dev #73）。
//!
//! 取り込み済みの判定は、カードのファイルを4本並べて読む（[`crate::import::PROBE_READS`]）。
//! フラッシュの媒体では1件ごとの往復の待ちが重なって速くなる——win の実測で、
//! USB 2.0 のメモリでも UHS-II の SD でも4本が1本より速かった
//! （dev `measure73-probe-threads-win.md`）。**円盤の媒体は測っていない**: 4か所を
//! 交互に読むとヘッドが行き来して、遅くなるかもしれない。だから、OS が
//! 「シークに時間がかかる」と答えたドライブだけ、1.0.0 と同じ1本に戻す。
//!
//! 分けるのは通信の規格（USB 2/3・UAS・SD の速度クラス）ではない。規格は OS から
//! 確かには取れず、取れても速さとの対応はまちまちである。
//!
//! 答えられないとき（Windows 以外・ドライブ文字の無いパス・問い合わせの失敗）は `None`。

use std::path::Path;

/// `path` の在るドライブが、シークに時間のかかる媒体か。
pub fn incurs_seek_penalty(path: &Path) -> Option<bool> {
    #[cfg(windows)]
    {
        windows_impl::incurs_seek_penalty(&crate::usn::volume_of(path)?)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        PropertyStandardQuery, StorageDeviceSeekPenaltyProperty, DEVICE_SEEK_PENALTY_DESCRIPTOR,
        IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_PROPERTY_QUERY,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// `volume` は `G:` の形（[`crate::usn::volume_of`]）。
    ///
    /// 開くのは**ボリュームのデバイス**（`\\.\G:`）で、アクセス権は 0。
    /// この問い合わせは読み書きの権利を要らないので、管理者でなくても通る
    pub(super) fn incurs_seek_penalty(volume: &str) -> Option<bool> {
        let path: Vec<u16> = format!(r"\\.\{volume}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceSeekPenaltyProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut answer: DEVICE_SEEK_PENALTY_DESCRIPTOR = unsafe { std::mem::zeroed() };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &query as *const _ as *const c_void,
                std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                &mut answer as *mut _ as *mut c_void,
                std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(handle) };
        // 足りない長さで返ってきた答えは読まない
        (ok != 0 && returned as usize >= std::mem::size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>())
            .then_some(answer.IncursSeekPenalty)
    }
}
