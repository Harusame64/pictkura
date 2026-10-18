//! Microsoft Store 版（MSIX）として動いているか（pictkura-dev #53。計画は dev/plan.msix.md）。
//!
//! exe は MSI・NSIS・持ち歩き版と同じバイト。パッケージとして動くときだけ、振る舞いを変える:
//!
//! - **AutoPlay** はマニフェストが登録する。実行時に HKCU へ書いても、パッケージ専用の仮想レジストリに入って
//!   エクスプローラーからは見えない——書かない・消さない
//! - **更新**は Store が配る。GitHub の最新版を訊きに行かない

use std::sync::OnceLock;

/// パッケージの ID を持って動いているか（＝MSIX から起動された）。1回だけ訊いて覚える
pub fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(detect)
}

#[cfg(windows)]
fn detect() -> bool {
    use windows_sys::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    // 長さだけを訊く。パッケージの中なら「足りない」（ERROR_INSUFFICIENT_BUFFER）、外なら NO_PACKAGE が返る
    let mut len: u32 = 0;
    // SAFETY: 長さの出力先は有効な u32。名前の出力先は null（長さだけを訊く呼び方）
    let r = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    r != APPMODEL_ERROR_NO_PACKAGE
}

#[cfg(not(windows))]
fn detect() -> bool {
    false
}
