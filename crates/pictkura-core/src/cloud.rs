//! クラウドにしか実体が無いファイル（プレースホルダ）の判別。
//!
//! OneDrive・iCloud Drive・Dropbox の「ファイル オンデマンド」では、
//! ファイルはフォルダに見えていても**中身がローカルに無い**ことがある。
//! この状態で開くと、その場で同期クライアントがダウンロードを始める
//! （通信量とディスクを消費し、数百ms〜数十秒ブロックする）。
//!
//! Windowsの「ピクチャ」フォルダは既定でOneDriveへリダイレクトされている環境が
//! 普通にあるので、ライブラリのルートがまるごとクラウドという利用者は珍しくない。
//! **実測した環境ではHEICの94%がクラウドのみだった**。ここを素通しにすると、
//! アプリを入れただけで数GBのダウンロードが静かに始まる。
//!
//! Microsoft自身も「インデクサやウイルス対策は `RECALL_ON_DATA_ACCESS` の
//! ファイルを読むな」と案内していて、Windowsのサムネイルキャッシュも
//! `OFFLINE` 属性のファイルは意図的に飛ばしている。pictkuraもそれに倣う。
//!
//! 取り込みウィザード（[`crate::browse`]）は最初からこの判定を使っている。
//! ライブラリ側のサムネイル生成にも同じ配慮を効かせるためにここへ切り出した。

use std::path::Path;

/// メタデータから「実体がクラウドにしか無い」かを判定する。
///
/// - `OFFLINE`: 実体がオフライン記憶域にある
/// - `RECALL_ON_OPEN`: 開いた時点で取りに行く
/// - `RECALL_ON_DATA_ACCESS`: 中身を読んだ時点で取りに行く（OneDriveの既定）
#[cfg(windows)]
pub fn is_cloud_only(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    meta.file_attributes()
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS)
        != 0
}

/// macOSは「データレスファイル」（`SF_DATALESS`）で同じ状態を表す。
///
/// Sonoma以降のiCloud Driveと、FileProviderを使う同期クライアント
/// （OneDrive for Mac 等）がこの形をとる。
#[cfg(target_os = "macos")]
pub fn is_cloud_only(meta: &std::fs::Metadata) -> bool {
    // `st_flags` は macOS 固有。`std::os::unix::fs::MetadataExt` には無い
    use std::os::macos::fs::MetadataExt;
    /// `SF_DATALESS`（`sys/stat.h`）。実体が取り寄せ前であることを表す
    const SF_DATALESS: u32 = 0x4000_0000;
    meta.st_flags() & SF_DATALESS != 0
}

/// Linuxには共通のプレースホルダ機構が無い。
///
/// 主要なOneDriveクライアントは完全同期なのでプレースホルダ自体が生じず、
/// この判定が無くても困らない。
#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_cloud_only(_meta: &std::fs::Metadata) -> bool {
    false
}

/// パスから判定する。メタデータが読めなければ「クラウドではない」とみなす
/// （読めないファイルはどのみち後段で失敗するので、ここで握り潰さない）。
///
/// 見るのはディレクトリ項目の属性だけなので、**ファイルは開かない**
/// ＝この判定自体がダウンロードを誘発することはない。
pub fn is_cloud_only_path(path: &Path) -> bool {
    // symlink_metadata: 実体を辿らない（辿ると取り寄せが走る系のFSがある）
    std::fs::symlink_metadata(path).is_ok_and(|m| is_cloud_only(&m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 普通のローカルファイルはクラウド扱いしない() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, b"x").unwrap();
        assert!(!is_cloud_only_path(&path));
    }

    #[test]
    fn 存在しないパスはクラウド扱いしない() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_cloud_only_path(&dir.path().join("nope.jpg")));
    }

    /// 属性の立ったファイルを合成して判定できること（Windowsのみ）。
    ///
    /// 本物のプレースホルダは同期クライアントが作るので用意できない。
    /// `OFFLINE` 属性は普通のファイルにも立てられるので、そこで代用する。
    #[cfg(windows)]
    #[test]
    fn オフライン属性が立っていればクラウド扱いにする() {
        use std::os::windows::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cloud.jpg");
        std::fs::write(&path, b"x").unwrap();

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(
                wide.as_ptr(),
                FILE_ATTRIBUTE_OFFLINE,
            )
        };
        assert_ne!(ok, 0, "属性を立てられること");

        assert!(is_cloud_only_path(&path));
    }
}
