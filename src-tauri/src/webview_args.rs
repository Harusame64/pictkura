//! WebView2 へ渡す起動の指定（Windows だけ）。設定の `[performance] hardware_acceleration` を窓より前に効かせる。
//!
//! **渡す口は環境変数 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`。** アプリの側から渡す口
//! （`additional_browser_args`）は wry の既定の指定（`--disable-features=msWebOOUI,…` と
//! `--autoplay-policy=no-user-gesture-required`）を**丸ごと置き換える**（wry 0.55.1 の
//! `webview2/mod.rs` の `unwrap_or_else`）。環境変数は**足し合わされる**（2026-11-15 win2 の実測。
//! msedgewebview2.exe のコマンドラインに既定と足した指定の両方が載った）。既定を写さずに済み、
//! wry を上げて既定が変わっても置いていかれない。
//!
//! 環境変数は子のプロセスへ引き継がれる——pictkura から開いた別の WebView2 アプリ（「他のアプリで開く」等）まで
//! 合成を切らないよう、**窓が建ったら元の値へ戻す**（`restore`。config の窓は `setup` より前に建つ）。

/// 合成を切ったときに足す指定。win2 の A/B（2026-11-15）で、カーソルの絵が止まらなくなった指定のうち、
/// `--disable-gpu` と重さは同じで影響が狭いほう
#[cfg_attr(not(windows), allow(dead_code))]
const DISABLE_DCOMP: &str = "--disable-direct-composition";

#[cfg_attr(not(windows), allow(dead_code))]
const ENV: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";

/// 環境変数に置く値。**利用者が自分で置いていた値は残し、後ろへ足す**。足すものが無ければ `None`（触らない）
#[cfg_attr(not(windows), allow(dead_code))]
fn combined(existing: Option<&str>, hardware_acceleration: bool) -> Option<String> {
    if hardware_acceleration {
        return None;
    }
    match existing.map(str::trim).filter(|s| !s.is_empty()) {
        Some(prev) if prev.split_whitespace().any(|a| a == DISABLE_DCOMP) => None,
        Some(prev) => Some(format!("{prev} {DISABLE_DCOMP}")),
        None => Some(DISABLE_DCOMP.to_string()),
    }
}

/// [`apply`] が触る前の環境変数の値（触らなかったときは `None`）。`setup` の閉包に持ち込まずに済むよう、ここに置く
#[cfg(windows)]
static SAVED: std::sync::Mutex<Option<Option<std::ffi::OsString>>> = std::sync::Mutex::new(None);

/// オフなのに足せなかった（利用者の値が UTF-8 で読めなかった）。記録の置き場は `setup` で決まるので、
/// [`restore`] で書く——[`apply`] の時点で書くと、置き場が無くて消える
#[cfg(windows)]
static SKIPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 窓を作る前に呼ぶ
#[cfg(windows)]
pub fn apply(hardware_acceleration: bool) {
    let before = std::env::var_os(ENV);
    // 利用者の値が UTF-8 で読めないときは触らない（上書きして消すより、足さないほうがよい）。
    // オフの設定が効かない理由は、記録に残す（[`restore`]）
    let existing = match &before {
        Some(v) => match v.to_str() {
            Some(s) => Some(s),
            None => {
                SKIPPED.store(!hardware_acceleration, std::sync::atomic::Ordering::Relaxed);
                return;
            }
        },
        None => None,
    };
    let Some(value) = combined(existing, hardware_acceleration) else {
        return;
    };
    std::env::set_var(ENV, value);
    *SAVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(before);
}

/// 窓が建ったあとに呼ぶ。[`apply`] が触った環境変数を、触る前の値へ戻す
#[cfg(windows)]
pub fn restore() {
    if SKIPPED.swap(false, std::sync::atomic::Ordering::Relaxed) {
        pictkura_core::applog::note(&format!(
            "graphics acceleration stays on: {ENV} is set but is not UTF-8, so {DISABLE_DCOMP} was not added"
        ));
    }
    match SAVED.lock().unwrap_or_else(|e| e.into_inner()).take() {
        None => {}
        Some(Some(before)) => std::env::set_var(ENV, before),
        Some(None) => std::env::remove_var(ENV),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_added_while_acceleration_is_on() {
        assert_eq!(combined(None, true), None);
        assert_eq!(combined(Some("--foo"), true), None);
    }

    #[test]
    fn turning_it_off_adds_the_one_switch() {
        assert_eq!(
            combined(None, false).as_deref(),
            Some("--disable-direct-composition")
        );
        assert_eq!(
            combined(Some("  "), false).as_deref(),
            Some("--disable-direct-composition")
        );
    }

    #[test]
    fn a_value_the_user_set_is_kept_in_front() {
        assert_eq!(
            combined(Some("--force-device-scale-factor=1"), false).as_deref(),
            Some("--force-device-scale-factor=1 --disable-direct-composition")
        );
    }

    /// `apply` と `restore` の往復（Windows だけ。環境変数を触るので、この1本の中で順に見る——並ぶ試験と取り合わない）
    #[cfg(windows)]
    #[test]
    fn apply_adds_the_switch_and_restore_puts_back_what_was_there() {
        // 無かったときは、戻したあとも無い
        std::env::remove_var(ENV);
        apply(false);
        assert_eq!(
            std::env::var(ENV).as_deref(),
            Ok("--disable-direct-composition")
        );
        restore();
        assert!(std::env::var_os(ENV).is_none());

        // 在ったときは、後ろへ足し、戻すと元の値
        std::env::set_var(ENV, "--foo");
        apply(false);
        assert_eq!(
            std::env::var(ENV).as_deref(),
            Ok("--foo --disable-direct-composition")
        );
        restore();
        assert_eq!(std::env::var(ENV).as_deref(), Ok("--foo"));

        // 空文字は空文字のまま戻す（未設定にしない）
        std::env::set_var(ENV, "");
        apply(false);
        restore();
        assert_eq!(std::env::var(ENV).as_deref(), Ok(""));

        // オンなら触らない。restore も何もしない
        apply(true);
        assert_eq!(std::env::var(ENV).as_deref(), Ok(""));
        restore();
        assert_eq!(std::env::var(ENV).as_deref(), Ok(""));
        std::env::remove_var(ENV);
    }

    #[test]
    fn the_switch_is_not_added_twice() {
        assert_eq!(
            combined(Some("--foo --disable-direct-composition"), false),
            None
        );
    }
}
