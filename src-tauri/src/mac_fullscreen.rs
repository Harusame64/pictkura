//! macOS のビューアの全画面を、WKWebView の側で効くようにする。
//!
//! **頁の全画面を入れる。** WKWebView は頁の全画面（Fullscreen API）を既定で切っていて、
//! `document.documentElement.requestFullscreen` が無い。ビューアの ⛶ は v0.1 から押しても何も起きず、
//! 動画の再生バーにも全画面ボタンが出なかった。公開の `WKPreferences.elementFullscreenEnabled`
//! （macOS 12.3+）で入れると、Windows（WebView2）と同じ頁の道に乗る。**アプリは macOS 11 から動く**ので、
//! その設定を持たない版では入れずに素通りする（無い selector を呼ぶと落ちる）。
//!
//! **窓の全画面を抜けたら、キーの受け手を WebView に戻す。** 動画の再生バーのボタンはウィンドウの全画面
//! （Tauri）に替えている（dev #75）。それを抜けると、キーの受け手が WebView から窓へ移り、Esc も ←/→ も
//! 頁に届かなくなった（mac の実測 2026-11-07）。**取り上げているのは tao** で、抜けたときの
//! `restore_state_from_fullscreen` が `set_style_mask_async` を主キューに積み、そこで自分の
//! `ns_view` を first responder にする（WKWebView はその子）。だから `didExitFullScreen` で、
//! **同じ主キューの後ろに**戻す処理を積む（その場で戻すと、後から来る tao の分に上書きされた）。
//! 緑のボタン・メニューの「フルスクリーンにする」から抜けたときも同じ道を通る。
//! 入った・抜けたは画面にも知らせる（`window-fullscreen-changed`）。窓の記録をアニメーションの途中で読むと
//! 外れるので、終わったところで読み直させる。

use std::ptr::NonNull;

use block2::RcBlock;
use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSResponder, NSWindow, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};
use objc2_web_kit::WKWebView;
use tauri::{AppHandle, Emitter, Manager};

/// 画面へ送る知らせの名前（`App.tsx` が聞く）
pub const FULLSCREEN_CHANGED: &str = "window-fullscreen-changed";

/// 主窓の WebView に、頁の全画面と、窓の全画面を抜けたときの受け手の戻しを入れる。失敗しても起動は続ける
pub fn install(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.clone();
    let _ = w.with_webview(move |wv| {
        // SAFETY: macOS の `inner()` はこの窓の WKWebView、`ns_window()` はその NSWindow を指す
        // （tauri の PlatformWebview）。この閉包は主スレッドで呼ばれる
        let (Some(view), Some(window)) = (unsafe {
            (
                Retained::retain(wv.inner().cast::<WKWebView>()),
                Retained::retain(wv.ns_window().cast::<NSWindow>()),
            )
        }) else {
            return;
        };

        // SAFETY: 主スレッドで、生きている WKWebView の設定を読む
        let prefs = unsafe { view.configuration().preferences() };
        if prefs.respondsToSelector(sel!(setElementFullscreenEnabled:)) {
            // SAFETY: 上で selector があることを確かめた（macOS 12.3+）
            unsafe { prefs.setElementFullscreenEnabled(true) };
        }

        let center = NSNotificationCenter::defaultCenter();
        let obj: &AnyObject = &window;

        let h = handle.clone();
        let entered = RcBlock::new(move |_n: NonNull<NSNotification>| {
            let _ = h.emit(FULLSCREEN_CHANGED, ());
        });
        let (v, win, h) = (view.clone(), window.clone(), handle.clone());
        let exited = RcBlock::new(move |_n: NonNull<NSNotification>| {
            let _ = h.emit(FULLSCREEN_CHANGED, ());
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let pair = MainThreadBound::new((win.clone(), v.clone()), mtm);
            DispatchQueue::main().exec_async(move || {
                let Some(mtm) = MainThreadMarker::new() else {
                    return;
                };
                let (win, v) = pair.get(mtm);
                let r: &NSResponder = v;
                win.makeFirstResponder(Some(r));
            });
        });
        // SAFETY: 名前は AppKit の定数、相手はこの窓。queue を渡さないので、知らせを出した主スレッドで走る
        unsafe {
            for (name, block) in [
                (NSWindowDidEnterFullScreenNotification, &entered),
                (NSWindowDidExitFullScreenNotification, &exited),
            ] {
                let token = center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(obj),
                    None,
                    block,
                );
                // 窓はアプリと同じだけ生きる。外す時は無いので、受け口は持ったままにする
                std::mem::forget(token);
            }
        }
    });
}
