//! Windows のビューアの全画面（#295）。**枠を外して、窓を画面ぴったりに広げるだけ**——Tauri（tao）の全画面は使わない。
//!
//! 一部の AMD の機械で、全画面のビューアで押せる物の上にカーソルが乗る・文字を選び始めると、その場でカーソルが止まった
//! （1.0.2 でも 1.1.0 でも毎回）。win の実機で、同じ起動の中で比べると（2026-11-26、利用者の立ち会い）:
//!
//! - 頁の全画面（WebView2）・tao の全画面: 毎回止まる
//! - tao の全画面のまま、シェルへの「全画面の窓」の印（`ITaskbarList2::MarkFullscreenWindow`）だけを外す: 0/5、
//!   付け直すとまた止まる（印の有無で行き来する）。ただし印を外すとタスクバーが全画面の上に出る
//! - 枠（`WS_CAPTION`・`WS_THICKFRAME`）を外して画面ぴったりに広げただけの窓（印を付けない）: 0/5
//!
//! ここは最後の形を自分で作る。抜けるときは、入る前の窓の型と置き場所（最大化かどうかを含む）へ戻す。
//! 画面の大きさは窓が居る画面（`MonitorFromWindow`）。タスクバーは、画面をぴったり覆う窓としてシェルが隠す
//! （印は付けない）。

use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowPlacement, IsZoomed, SetWindowLongPtrW, SetWindowPlacement,
    SetWindowPos, ShowWindow, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_RESTORE, WINDOWPLACEMENT, WS_CAPTION, WS_MAXIMIZE, WS_THICKFRAME,
};

/// 入る前の窓の型と置き場所。`Some` の間が全画面
struct Saved {
    style: isize,
    placement: WINDOWPLACEMENT,
}

// `WINDOWPLACEMENT` は数と矩形だけの構造体で、スレッドをまたいで持ってよい
unsafe impl Send for Saved {}

static SAVED: Mutex<Option<Saved>> = Mutex::new(None);

fn saved() -> std::sync::MutexGuard<'static, Option<Saved>> {
    SAVED.lock().unwrap_or_else(|e| e.into_inner())
}

/// 全画面か（このモジュールで入れた全画面だけ）
pub fn is_on() -> bool {
    saved().is_some()
}

/// 全画面に入る。もう入っていれば何もしない
pub fn enter(hwnd: HWND) -> windows::core::Result<()> {
    let mut held = saved();
    if held.is_some() {
        return Ok(());
    }
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        GetWindowPlacement(hwnd, &mut placement)?;
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err(windows::core::Error::from_win32());
        }
        // 最大化のままだと、広げた後も最大化の印が残り、戻すときの置き場所が狂う。win の実測（③b・③c）と同じく
        // 先に元の大きさへ戻してから枠を外す
        if IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let frame = (WS_CAPTION.0 | WS_THICKFRAME.0) as isize;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !frame);
        let r = info.rcMonitor;
        SetWindowPos(
            hwnd,
            None,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
        *held = Some(Saved { style, placement });
    }
    Ok(())
}

/// 全画面から抜ける（入る前の型と置き場所へ）。入っていなければ何もしない
pub fn exit(hwnd: HWND) -> windows::core::Result<()> {
    let Some(s) = saved().take() else {
        return Ok(());
    };
    unsafe {
        // 最大化の印は置き場所（`SetWindowPlacement`）が付け直す。型に残したまま戻すと、元の大きさの窓が最大化を名乗る
        SetWindowLongPtrW(hwnd, GWL_STYLE, s.style & !(WS_MAXIMIZE.0 as isize));
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
        SetWindowPlacement(hwnd, &s.placement)?;
    }
    Ok(())
}
