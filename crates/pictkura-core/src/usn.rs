//! NTFS USNジャーナルによる差分検知（plan.md 第3部 段階B-1、Windows専用）。
//!
//! 起動時フルスキャンの代わりに「前回終了以降に変更のあったディレクトリ」だけを
//! ジャーナルから特定する。設計の要点:
//!
//! - USNレコードのパスを1件ずつ復元するのではなく、**親ディレクトリのFRN
//!   （ファイル参照番号）を集めて現在のパスへ解決**し、「ダーティディレクトリ集合」
//!   として返す。呼び出し側はそのディレクトリだけを枝刈りウォーカー（段階B-2）で
//!   再走査すればよい。リネームや削除で古いパスが復元できない問題を回避でき、
//!   反映ロジックも通常のスキャンと共通化できる
//! - 失敗はすべて [`UsnOutcome::FullScanNeeded`] へ倒す（初回・ジャーナルの
//!   ラップ/削除・非NTFS・権限不足・変更が多すぎる場合）。呼び出し側は
//!   枝刈りフルスキャンへフォールバックするだけでよく、誤検知しても壊れない
//! - 読み取りは `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`（Windows 10以降）を使い、
//!   管理者権限なしで動かす

use std::path::{Path, PathBuf};

/// ジャーナル内の読み取り位置。ボリュームごとにDBのmetaへ永続化する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsnPosition {
    /// ジャーナルの識別子。再作成されると変わる（＝差分の連続性が切れる）
    pub journal_id: u64,
    /// 次に読むべきUSN
    pub next_usn: i64,
}

impl UsnPosition {
    /// metaテーブル保存用の文字列（"journal_id:next_usn"）。
    pub fn to_meta(&self) -> String {
        format!("{}:{}", self.journal_id, self.next_usn)
    }

    pub fn from_meta(s: &str) -> Option<Self> {
        let (id, usn) = s.split_once(':')?;
        Some(Self {
            journal_id: id.parse().ok()?,
            next_usn: usn.parse().ok()?,
        })
    }
}

/// 前回位置から読み取れた差分。
#[derive(Debug)]
pub struct UsnDelta {
    /// 直下に変更（追加・削除・改名・上書き）のあったディレクトリの**現在の**パス。
    /// 重複除去済み。解決できなかった親（＝そのディレクトリ自体が消えた）は
    /// 含まれないが、消えたディレクトリの削除イベントはさらに上の親に記録される
    /// ため、生きている祖先が必ずダーティになる
    pub dirty_dirs: Vec<PathBuf>,
    /// 反映後に保存すべき新しい読み取り位置
    pub position: UsnPosition,
    /// 処理したレコード数（爆速メーター用）
    pub record_count: usize,
}

/// ジャーナル読み取りの結果。
#[derive(Debug)]
pub enum UsnOutcome {
    /// 差分が取れた。dirty_dirsだけを再走査すればよい
    Delta(UsnDelta),
    /// 差分は取れない（初回・ラップ・非NTFS・権限不足・変更多数など）。
    /// フルスキャンが必要。現在位置が取れていれば、フルスキャン完了後に
    /// 保存することで次回起動から差分にできる
    FullScanNeeded(Option<UsnPosition>),
}

/// パスの属するボリューム（"C:" 形式）。UNCパス等は非対応でNone。
pub fn volume_of(path: &Path) -> Option<String> {
    match path.components().next()? {
        std::path::Component::Prefix(p) => match p.kind() {
            std::path::Prefix::Disk(d) | std::path::Prefix::VerbatimDisk(d) => {
                Some(format!("{}:", d.to_ascii_uppercase() as char))
            }
            _ => None,
        },
        _ => None,
    }
}

/// 1回の起動で扱うダーティディレクトリ数の上限。
/// これを超える変更はFRN解決のコストが線形に効くため、枝刈りフルスキャンの方が速い。
/// （読むのはジャーナルの実装だけなので、他のOSでは未使用の警告になる）
#[cfg(windows)]
const MAX_DIRTY_DIRS: usize = 512;
/// ジャーナルレコード数の上限（大量変更時の暴走防止）。
#[cfg(windows)]
const MAX_RECORDS: usize = 200_000;

/// ボリュームのUSNジャーナルを `stored` の位置から読み、差分を返す。
#[cfg(windows)]
pub fn read_changes_since(volume: &str, stored: Option<&UsnPosition>) -> UsnOutcome {
    windows_impl::read_changes_since(volume, stored)
}

/// Windows以外: USNジャーナルは無いので常にフルスキャン。
/// （macOS: FSEvents履歴 / Linux: fanotify は将来対応）
#[cfg(not(windows))]
pub fn read_changes_since(_volume: &str, _stored: Option<&UsnPosition>) -> UsnOutcome {
    UsnOutcome::FullScanNeeded(None)
}

#[cfg(windows)]
mod windows_impl {
    use super::{UsnDelta, UsnOutcome, UsnPosition, MAX_DIRTY_DIRS, MAX_RECORDS};
    use std::collections::HashSet;
    use std::ffi::c_void;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdType, GetFinalPathNameByHandleW, OpenFileById,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, VOLUME_NAME_DOS,
    };
    use windows_sys::Win32::System::Ioctl::{
        FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_UNPRIVILEGED_USN_JOURNAL, READ_USN_JOURNAL_DATA_V1,
        USN_JOURNAL_DATA_V0,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// dropでCloseHandleするラッパ。
    struct OwnedHandle(HANDLE);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// ボリュームのハンドルを開く。ボリュームデバイス（`\\.\C:`）ではなく
    /// **ルートディレクトリ**（`C:\` ＋ FILE_FLAG_BACKUP_SEMANTICS）を開くのが要点:
    /// デバイスハンドルへのUSN系FSCTLは管理者権限が要る（非管理者では
    /// GENERIC_READで開けず、アクセス権0だとERROR_INVALID_FUNCTION）が、
    /// ルートディレクトリハンドルなら非管理者・アクセス権0でも
    /// FSCTL_QUERY_USN_JOURNAL / FSCTL_READ_UNPRIVILEGED_USN_JOURNAL が通る
    /// （実測: Windows 11。fsutil usn queryjournal が非管理者で動くのと同じ経路）。
    /// OpenFileByIdのボリュームヒントとしてもこのハンドルで足りる。
    fn open_volume(volume: &str) -> Option<OwnedHandle> {
        let path: Vec<u16> = format!(r"{volume}\")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        (handle != INVALID_HANDLE_VALUE).then_some(OwnedHandle(handle))
    }

    fn query_journal(volume: &OwnedHandle) -> Option<USN_JOURNAL_DATA_V0> {
        let mut data: USN_JOURNAL_DATA_V0 = unsafe { std::mem::zeroed() };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                volume.0,
                FSCTL_QUERY_USN_JOURNAL,
                std::ptr::null(),
                0,
                &mut data as *mut _ as *mut c_void,
                std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(data)
    }

    /// USN_RECORD_V2の生バイト列から必要なフィールドだけを読むビュー。
    /// （バッファ内の可変長レコードを直接ポインタキャストせず、オフセット読みで安全に扱う）
    struct RecordView<'a>(&'a [u8]);
    impl RecordView<'_> {
        /// **足りなければ0**を返す。呼ぶ側はどれも「0なら見送る」判定を持って
        /// いるので、短いバッファが来ても打ち切りに落ちるだけで済む
        /// ——OSが返す長さを信じて添字を書くと、そこがそのまま落ち口になる
        fn le_u32(&self, at: usize) -> u32 {
            self.0
                .get(at..at + 4)
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map(u32::from_le_bytes)
                .unwrap_or(0)
        }
        fn record_length(&self) -> u32 {
            self.le_u32(0)
        }
        fn major_version(&self) -> u16 {
            self.0
                .get(4..6)
                .and_then(|b| <[u8; 2]>::try_from(b).ok())
                .map(u16::from_le_bytes)
                .unwrap_or(0)
        }
        fn parent_frn(&self) -> u64 {
            self.0
                .get(16..24)
                .and_then(|b| <[u8; 8]>::try_from(b).ok())
                .map(u64::from_le_bytes)
                .unwrap_or(0)
        }
    }

    /// FRN（ファイル参照番号）を現在のパスへ解決する。
    /// 消えたファイル・アクセスできないものはNone。
    fn resolve_frn(volume: &OwnedHandle, frn: u64) -> Option<PathBuf> {
        let desc = FILE_ID_DESCRIPTOR {
            dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: FileIdType,
            Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: frn as i64 },
        };
        let handle = unsafe {
            OpenFileById(
                volume.0,
                &desc,
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                FILE_FLAG_BACKUP_SEMANTICS,
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return None;
        }
        let handle = OwnedHandle(handle);
        let mut buf = vec![0u16; 32768];
        let len = unsafe {
            GetFinalPathNameByHandleW(
                handle.0,
                buf.as_mut_ptr(),
                buf.len() as u32,
                VOLUME_NAME_DOS,
            )
        };
        if len == 0 || len as usize >= buf.len() {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..len as usize]);
        // `\\?\C:\...` の拡張プレフィックスを外して通常形式に揃える
        Some(PathBuf::from(
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string(),
        ))
    }

    pub fn read_changes_since(volume_name: &str, stored: Option<&UsnPosition>) -> UsnOutcome {
        let Some(volume) = open_volume(volume_name) else {
            return UsnOutcome::FullScanNeeded(None);
        };
        let Some(journal) = query_journal(&volume) else {
            // ジャーナル無効・非NTFS・権限不足
            return UsnOutcome::FullScanNeeded(None);
        };
        let current = UsnPosition {
            journal_id: journal.UsnJournalID,
            next_usn: journal.NextUsn,
        };

        // 差分の連続性チェック: 初回・ジャーナル再作成・古い記録の削除（ラップ）
        let Some(stored) = stored else {
            return UsnOutcome::FullScanNeeded(Some(current));
        };
        if stored.journal_id != journal.UsnJournalID
            || stored.next_usn < journal.FirstUsn
            || stored.next_usn > journal.NextUsn
        {
            return UsnOutcome::FullScanNeeded(Some(current));
        }

        // レコードを読み進め、親ディレクトリのFRNを集める
        let mut input = READ_USN_JOURNAL_DATA_V1 {
            StartUsn: stored.next_usn,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: journal.UsnJournalID,
            MinMajorVersion: 2,
            MaxMajorVersion: 2,
        };
        let mut buf = vec![0u8; 1 << 16];
        let mut parent_frns: HashSet<u64> = HashSet::new();
        let mut record_count = 0usize;
        let final_position = loop {
            let mut returned = 0u32;
            let ok = unsafe {
                DeviceIoControl(
                    volume.0,
                    FSCTL_READ_UNPRIVILEGED_USN_JOURNAL,
                    &input as *const _ as *const c_void,
                    std::mem::size_of::<READ_USN_JOURNAL_DATA_V1>() as u32,
                    buf.as_mut_ptr() as *mut c_void,
                    buf.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                // 旧Windows（FSCTL未対応）や読み取り中のジャーナル削除など
                return UsnOutcome::FullScanNeeded(Some(current));
            }
            let returned = returned as usize;
            if returned < 8 {
                break UsnPosition {
                    journal_id: journal.UsnJournalID,
                    next_usn: input.StartUsn,
                };
            }
            let Some(next_usn) = buf
                .get(0..8)
                .and_then(|b| <[u8; 8]>::try_from(b).ok())
                .map(i64::from_le_bytes)
            else {
                // 8バイトあることは上で確かめているので通らない。
                // 通ったときは差分をあきらめて全件走査へ落とす（黙って止めない）
                return UsnOutcome::FullScanNeeded(Some(current));
            };
            let mut offset = 8usize;
            while offset + 60 <= returned {
                let view = RecordView(&buf[offset..]);
                let len = view.record_length() as usize;
                if len < 60 || offset + len > returned {
                    break;
                }
                if view.major_version() == 2 {
                    record_count += 1;
                    if record_count > MAX_RECORDS {
                        return UsnOutcome::FullScanNeeded(Some(current));
                    }
                    parent_frns.insert(view.parent_frn());
                }
                offset += len;
            }
            if returned <= 8 || next_usn <= input.StartUsn {
                break UsnPosition {
                    journal_id: journal.UsnJournalID,
                    next_usn,
                };
            }
            input.StartUsn = next_usn;
        };

        // 親FRN → 現在のディレクトリパスへ解決
        if parent_frns.len() > MAX_DIRTY_DIRS {
            return UsnOutcome::FullScanNeeded(Some(current));
        }
        let mut dirty_dirs = Vec::new();
        let mut seen_paths = HashSet::new();
        for frn in parent_frns {
            // 解決できない親 = そのディレクトリ自体が消えた。削除イベントは
            // 生きている祖先ディレクトリにも記録されているため無視してよい
            if let Some(path) = resolve_frn(&volume, frn) {
                if seen_paths.insert(path.clone()) {
                    dirty_dirs.push(path);
                }
            }
        }

        UsnOutcome::Delta(UsnDelta {
            dirty_dirs,
            position: final_position,
            record_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positionはmeta文字列と往復できる() {
        let pos = UsnPosition {
            journal_id: 0x0123_4567_89AB_CDEF,
            next_usn: 42_000_000,
        };
        assert_eq!(UsnPosition::from_meta(&pos.to_meta()), Some(pos));
        assert_eq!(UsnPosition::from_meta("garbage"), None);
        assert_eq!(UsnPosition::from_meta("1:2:3"), None);
        assert_eq!(UsnPosition::from_meta(""), None);
    }

    /// Windows専用。ドライブ文字は `Component::Prefix` として解析されるが、
    /// この要素はWindowsの `Path` にしか存在しない（他のOSでは `D:\photos\a`
    /// 全体が1つのファイル名になり、常に `None` になる）
    #[cfg(windows)]
    #[test]
    fn volume_ofはドライブレターを返す() {
        assert_eq!(volume_of(Path::new(r"D:\photos\a")), Some("D:".into()));
        assert_eq!(volume_of(Path::new(r"c:\x")), Some("C:".into()));
        assert_eq!(volume_of(Path::new(r"\\server\share\x")), None);
        assert_eq!(volume_of(Path::new("relative/path")), None);
    }

    /// 実ボリュームでの読み取り（Windows・NTFS前提の結合テスト）。
    /// ジャーナルへアクセスできない環境では黙ってスキップする。
    #[cfg(windows)]
    #[test]
    fn 実ジャーナルからファイル作成の差分が取れる() {
        let dir = tempfile::tempdir().unwrap();
        let Some(volume) = volume_of(dir.path()) else {
            return;
        };
        // 初回: 位置なし → フルスキャン要求＋現在位置
        let pos = match read_changes_since(&volume, None) {
            UsnOutcome::FullScanNeeded(Some(pos)) => pos,
            UsnOutcome::FullScanNeeded(None) => return, // ジャーナル無効環境
            UsnOutcome::Delta(_) => panic!("位置なしでDeltaは返らない"),
        };

        // ファイルを作ると、その親ディレクトリがダーティになる
        std::fs::write(dir.path().join("usn_test.jpg"), b"x").unwrap();
        match read_changes_since(&volume, Some(&pos)) {
            UsnOutcome::Delta(delta) => {
                assert!(delta.record_count > 0);
                // **比較は非対称にする**。期待値（TEMP）は短縮名のことがあるので解決するが、
                // 製品の出力は小文字化しかしない。両側を解決すると、`resolve_frn` が
                // 短縮名を返す退行をテストが洗浄してしまい、緑のまま通ってしまう
                // （そうなると `rebase_to_root_spelling` の前方一致が外れ、
                // ダーティディレクトリが黙って捨てられて差分同期が取りこぼす）
                let canon_temp = canon_lower(dir.path());
                assert!(
                    delta
                        .dirty_dirs
                        .iter()
                        .any(|d| plain_lower(d) == canon_temp),
                    "一時ディレクトリ {canon_temp} がダーティ集合に含まれる: {:?}",
                    delta.dirty_dirs
                );
                assert!(delta.position.next_usn >= pos.next_usn);
            }
            // 直後に大量の他プロセス変更があった等。環境依存なので失敗にしない
            UsnOutcome::FullScanNeeded(_) => {}
        }
    }

    /// **期待値側**の正規化。8.3短縮名を長い綴りへ解決してから小文字化する。
    ///
    /// `TEMP` は短縮名のことがある（GitHub Actions のランナーは
    /// `C:\Users\RUNNER~1\...`）。ジャーナル側は `GetFinalPathNameByHandleW` の
    /// 長い綴りなので、解決しないと `runneradmin` と `runner~1` が一致せず、
    /// 正しい実装のまま失敗する。`canonicalize` が付ける拡張長プレフィックスは剥がす。
    #[cfg(windows)]
    fn canon_lower(p: &Path) -> String {
        let resolved = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let s = resolved.to_string_lossy();
        s.strip_prefix(r"\\?\").unwrap_or(&s).to_lowercase()
    }

    /// **製品出力側**の正規化。大小の揺れだけを吸収し、それ以上は何もしない。
    ///
    /// ここで解決までしてしまうと、このテストが張っている契約
    /// （「`resolve_frn` は長い綴りを返す」）が消える。
    #[cfg(windows)]
    fn plain_lower(p: &Path) -> String {
        p.to_string_lossy().to_lowercase()
    }
}
