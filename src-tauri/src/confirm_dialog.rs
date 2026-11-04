//! **右から左へ書く言語の確認画面**（ゴミ箱へ・移動・フォルダを外す）。Windows だけ。
//!
//! 確認はふだんダイアログのプラグインが出す（`ui/src/confirm.ts`）。Windows でのプラグインの
//! 中身は rfd の `TaskDialogIndirect` で、**旗が `TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT`
//! に決め打ち**——右から左の旗（`TDF_RTL_LAYOUT`）を渡す口が無い。付けないと、アラビア語の文が
//! 左寄せで組まれ、ボタンも左右が逆のまま並ぶ。
//!
//! だから**右から左の言語のときだけ**、同じ呼び方の箱をここで出す。旗が1つ多いほかは
//! rfd と同じにしてある（アイコン・既定のボタン・Esc で閉じる・親の窓・別のスレッドで出す）。
//! 左から右の言語はこれまでどおりプラグインを通る。

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOG_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_RTL_LAYOUT, TDF_SIZE_TO_CONTENT, TD_INFORMATION_ICON,
    TD_WARNING_ICON,
};

/// rfd と同じ番号（`win_cid/message_dialog.rs`）。Esc や閉じるボタンは `IDCANCEL`（2）を返す
const ID_OK: i32 = 1000;
const ID_CANCEL: i32 = 1001;

/// 確認画面の中身。**文字列はすべて画面の言語のもの**（呼び側が辞書から渡す）
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub ok_label: String,
    pub cancel_label: String,
    /// `false` なら情報のアイコン（取り返しのつく確認）
    pub warning: bool,
}

/// 箱を出して、**OK が押されたときだけ `true`**。閉じるボタン・Esc・取り消しは `false`。
/// **呼んだスレッドで、閉じるまで戻らない**——呼び側は UI のスレッドを避けること
pub fn show(parent: HWND, c: &Confirm) -> windows::core::Result<bool> {
    // 閉じの印（FSI…PDI）は箱に描かれ、向きも効かない（#283 の win の実機）。**箱へ渡す文字はすべて**
    // 埋め込みに直す——閉じが無ければ何も変わらない
    let text = |s: &str| HSTRING::from(crate::legacy_bidi::for_right_to_left_box(s));
    let title = text(&c.title);
    let message = text(&c.message);
    let ok = text(&c.ok_label);
    let cancel = text(&c.cancel_label);
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: ID_OK,
            pszButtonText: PCWSTR(ok.as_ptr()),
        },
        TASKDIALOG_BUTTON {
            nButtonID: ID_CANCEL,
            pszButtonText: PCWSTR(cancel.as_ptr()),
        },
    ];
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: parent,
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT | TDF_RTL_LAYOUT,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszContent: PCWSTR(message.as_ptr()),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: if c.warning {
                TD_WARNING_ICON
            } else {
                TD_INFORMATION_ICON
            },
        },
        pButtons: buttons.as_ptr(),
        cButtons: buttons.len() as u32,
        ..Default::default()
    };
    let mut pressed = 0i32;
    // SAFETY: 文字列（`HSTRING`）とボタンの配列は、この呼び出しが戻るまで生きている。
    // 受け取り口は1つだけ渡し、ほかは null（要らない）
    unsafe { TaskDialogIndirect(&config, Some(&mut pressed), None, None)? };
    Ok(pressed == ID_OK)
}
