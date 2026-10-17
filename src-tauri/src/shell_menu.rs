//! 右クリックの「その他のオプションを表示」——エクスプローラーと同じメニュー（ADR B-4。Windows だけ）。
//!
//! シェルの `IContextMenu` をそのまま出す。送る・プログラムから開く・プロパティ・7-Zip・OneDrive も、
//! 入っている拡張がそのまま並ぶ。**選ばれた項目はシェルが走らせる**——移動のように中身や場所が変わるものは、
//! 見張りが拾ってタイルを直す（win の実測で約0.9秒。`dev/spikes/b4-results/`）。
//!
//! 実測で決めたこと（2026-10-17、win）:
//!
//! - **UI の糸で回す。** そこは既に STA（`CoInitializeEx` は S_FALSE）で、メニューの持ち主の窓もそこに居る
//! - **メニューのメッセージを `IContextMenu3` へ渡す。** 渡さないと「プログラムから開く」が仮の1行のまま
//! - **名前の変更は出さない**（`CMF_CANRENAME` を渡さない）。pictkura の外での改名は ★・⚑ を外すので、
//!   入口を自分から増やさない（ADR C で見送り）
//! - **削除と回転はシェルの項目を外す**（[`OWN_VERBS`]）。pictkura の右クリックに同じ項目が在り、そちらは
//!   付属の `.xmp` も一緒に捨て、回した記録（取り込みの「済」の控え）も残す。シェルのものはどちらもしない
//! - `QueryContextMenu` は、間が空くと毎回 1 秒ほどかかる（拡張を読み直す）。待ちの合図は画面の側で出す。
//!   その間と `InvokeCommand` の間は UI の糸が止まる。**別の糸へは移さない**——メニューの持ち主の窓と
//!   `IContextMenu` はこの糸のもので、走らせる側の実測は 0〜245 ms（プロパティ・削除）。重い処理（圧縮等）は
//!   シェルが自分の糸へ回す
#![cfg(windows)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::ptr::null_mut;

use windows::core::{Interface, HSTRING, PCSTR, PCWSTR, PSTR};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    DefSubclassProc, IContextMenu, IContextMenu2, IContextMenu3, ILFindLastID, ILFree,
    IShellFolder, RemoveWindowSubclass, SHBindToParent, SHParseDisplayName, SetWindowSubclass,
    CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX, GCS_VERBW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DeleteMenu, DestroyMenu, GetMenuItemCount, GetMenuItemID, SetForegroundWindow,
    TrackPopupMenuEx, HMENU, MF_BYCOMMAND, SW_SHOWNORMAL, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR,
};

/// シェルに渡す番号の幅。0 は「何も選ばなかった」なので 1 から
const FIRST: u32 = 1;
const LAST: u32 = 0x7FFF;
/// `CMIC_MASK_UNICODE`（windows クレートでは別のモジュールに居る）
const CMIC_MASK_UNICODE: u32 = 0x4000;
const SUBCLASS_ID: usize = 0x504B_4234;

/// シェルのメニューから外す動詞。pictkura の右クリックに同じ項目が在り、シェルのものは pictkura の後始末を
/// しない: 削除は `.xmp` を残し、回転は回した記録を書かない（取り込みで同じ写真が二重に入る。#211）。
/// `rotate90` は win の実測（`dev/spikes/b4-results/`）、`rotate270` はその対
const OWN_VERBS: [&str; 3] = ["delete", "rotate90", "rotate270"];

/// `menu` から [`OWN_VERBS`] の項目を外す。動詞を名乗らない項目は残す
unsafe fn drop_own_verbs(cm: &IContextMenu, menu: HMENU) {
    let count = unsafe { GetMenuItemCount(Some(menu)) };
    for pos in (0..count).rev() {
        let id = unsafe { GetMenuItemID(menu, pos) };
        // 区切り（0）とサブメニュー（u32::MAX）は動詞を持たない
        if !(FIRST..=LAST).contains(&id) {
            continue;
        }
        let mut buf = [0u16; 64];
        let named = unsafe {
            cm.GetCommandString(
                (id - FIRST) as usize,
                GCS_VERBW,
                None,
                PSTR(buf.as_mut_ptr().cast()),
                buf.len() as u32,
            )
        };
        if named.is_err() {
            continue;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let verb = String::from_utf16_lossy(&buf[..n]);
        if OWN_VERBS.iter().any(|v| v.eq_ignore_ascii_case(&verb)) {
            let _ = unsafe { DeleteMenu(menu, id, MF_BYCOMMAND) };
        }
    }
}

thread_local! {
    /// 開いている間のメニュー。持ち主の窓に来るメッセージをこれへ渡す
    static ACTIVE: RefCell<Option<IContextMenu>> = const { RefCell::new(None) };
}

/// サブメニュー（送る・プログラムから開く）は、開くときに中身を組む。その合図は持ち主の窓へ来るので、
/// メニューが開いている間だけ窓を横取りして渡す
unsafe extern "system" fn forward(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if matches!(
        msg,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        if let Some(cm) = ACTIVE.with_borrow(|c| c.clone()) {
            if let Ok(cm3) = cm.cast::<IContextMenu3>() {
                let mut r = LRESULT(0);
                if unsafe { cm3.HandleMenuMsg2(msg, wp, lp, Some(&mut r)) }.is_ok() {
                    return r;
                }
            }
            // 3 を名乗っても 2 の口でしか答えない拡張がある（ゲート2）
            if let Ok(cm2) = cm.cast::<IContextMenu2>() {
                if unsafe { cm2.HandleMenuMsg(msg, wp, lp) }.is_ok() {
                    return LRESULT(0);
                }
            }
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// `paths` のメニューを画面の `at`（物理ピクセル）に出し、選ばれた項目を走らせる。
/// **UI の糸で呼ぶこと。** 閉じるまで戻らない。`paths` は同じフォルダに在ること（シェルの制約）。
///
/// `shown` はメニューを組み終えて出す直前に呼ぶ（組むのに1秒ほどかかることがあり、その間の合図を
/// 画面の側で消すため）。出せなかったときは呼ばない
pub fn show(
    hwnd: HWND,
    paths: &[PathBuf],
    at: (i32, i32),
    shown: impl FnOnce(),
) -> Result<(), String> {
    let mut abs: Vec<*mut ITEMIDLIST> = Vec::with_capacity(paths.len());
    let r = unsafe { show_inner(hwnd, paths, at, shown, &mut abs) };
    for p in abs {
        unsafe { ILFree(Some(p)) };
    }
    r
}

unsafe fn show_inner(
    hwnd: HWND,
    paths: &[PathBuf],
    at: (i32, i32),
    shown: impl FnOnce(),
    abs: &mut Vec<*mut ITEMIDLIST>,
) -> Result<(), String> {
    for p in paths {
        let mut pidl = null_mut();
        unsafe { SHParseDisplayName(&HSTRING::from(p.as_os_str()), None, &mut pidl, 0, None) }
            .map_err(|e| e.to_string())?;
        abs.push(pidl);
    }
    let first = *abs.first().ok_or("no files")?;
    let folder: IShellFolder = unsafe { SHBindToParent(first, None) }.map_err(|e| e.to_string())?;
    let children: Vec<*const ITEMIDLIST> = abs
        .iter()
        .map(|&p| unsafe { ILFindLastID(p) } as *const _)
        .collect();
    let cm: IContextMenu =
        unsafe { folder.GetUIObjectOf(hwnd, &children, None) }.map_err(|e| e.to_string())?;
    let menu = unsafe { CreatePopupMenu() }.map_err(|e| e.to_string())?;
    let r = (move || {
        unsafe { cm.QueryContextMenu(menu, 0, FIRST, LAST, CMF_NORMAL) }
            .ok()
            .map_err(|e| e.to_string())?;
        unsafe { drop_own_verbs(&cm, menu) };
        ACTIVE.set(Some(cm.clone()));
        let _ = unsafe { SetWindowSubclass(hwnd, Some(forward), SUBCLASS_ID, 0) };
        // 前面に居ないと、外をクリックしてもメニューが閉じない（TrackPopupMenu の決まり）
        let _ = unsafe { SetForegroundWindow(hwnd) };
        shown();
        let cmd = unsafe {
            TrackPopupMenuEx(
                menu,
                (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                at.0,
                at.1,
                hwnd,
                None,
            )
        }
        .0 as u32;
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(forward), SUBCLASS_ID) };
        ACTIVE.set(None);
        if cmd < FIRST {
            return Ok(());
        }
        let offset = (cmd - FIRST) as usize;
        let info = CMINVOKECOMMANDINFOEX {
            cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
            hwnd,
            // 動詞の名前ではなく、番号を渡す（MAKEINTRESOURCE の形）
            lpVerb: PCSTR(offset as *const u8),
            lpVerbW: PCWSTR(offset as *const u16),
            nShow: SW_SHOWNORMAL.0,
            ptInvoke: POINT { x: at.0, y: at.1 },
            ..Default::default()
        };
        match unsafe {
            cm.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)
        } {
            // 走った項目の中で利用者が取りやめた（確認の画面で「いいえ」等）
            Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => Ok(()),
            r => r.map_err(|e| e.to_string()),
        }
    })();
    let _ = unsafe { DestroyMenu(menu) };
    r
}
