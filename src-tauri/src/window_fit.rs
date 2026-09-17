//! 窓を、実際に載っている画面に合わせる（完成度週間の残件6・7）。
//!
//! **`tauri.conf.json` の `width`/`height`/`minWidth`/`minHeight` は、
//! クライアント領域の論理 px** である（`tauri-utils` の `config.rs` が
//! "The window width in logical pixels."、`tauri-runtime-wry` はそれを
//! `inner_size` に渡す）。**作業領域も枠も物理 px** なので、比べるには倍率で割る。
//! **この2つを混ぜると、拡大率のある台でだけ間違える**——そしてこちらの台は
//! 100% なので、混ぜたまま緑になる。
//!
//! **なぜ要るか**（2026-09-09 に win が実測。原本は `dev/plan.completeness-week.md`。
//! 枠は `AdjustWindowRectExForDpi` に DPI を渡して OS に訊いたもので、
//! **100% については実機の窓と一致することを確かめてある**）:
//!
//! - **`1366×768` の 100%**: 既定の 1280×840 は**外形 1296×879**。
//!   **作業領域 720 に対して 159 px 高い。** タスクバーを消しても、画面の 768 に
//!   対して 111 px 超える——**この判定はタスクバーの高さを仮定していない**
//! - **`1366×768` の 150%**: **床の 960×520 でさえ外形 1462×836** で、
//!   **幅 96 px・高さ 68 px 足りない**。しかも床があるので**利用者は縮められない**
//! - **`1920×1080` の 150%**: 床は入るが**既定は入らない**（外形 1942×1316）
//!
//! **だから、寸法だけでなく床も一緒に下げる。** 画面が床を映せないとき、床は
//! 「これ以上は縮めさせない」ではなく「**窓を画面に入れさせない**」として働く。
//!
//! **床を下げると、床が守っていたものを1つ手放す。** 09-08 の win の実測に
//! 「**下限 960 が入った以上、Windows では窓を細くして歯車を押し出すことがそもそも
//! 出来ない**」と書いてある——**その保護が、この画面では外れる**。
//! **ただし同じ実測が「Windows では独語でも 960 で歯車は見えていて、切れ始めるのは
//! 620 付近」**とも言っているので、**896 では出ている見込みが高い**
//! （**mac 側の測定は「独語は 1024 でも 960 でも消える」で食い違う。差は字体らしい、
//! というところまでで測っていない**）。**896 そのものはどちらの台でも測っていない。**
//! **設定への2本目の扉（`Ctrl`/`Cmd` + `,`・#136）は、どちらに転んでも在る。**
//!
//! **この台（macOS・3024×1964 の 2x）で実際に通したこと**:
//!
//! - **設定を 4000×3000 に置き換えて起動**すると、作業領域 `3024×1768` から
//!   **1512×884 に縮み、記録に1行出た**。**08:25 の周では、画面上の窓も
//!   `1512×884`＝`NSScreen.visibleFrame` ぴったり**（`(0, 33)` から）。
//!   **のちの周では内側に収まるが、ぴったりではない**（下の 98% の件）
//! - **枠は `0×0` と返る**——**測ると `outer_size()` と `inner_size()` が同じ値**で、
//!   起動から 6 秒後まで 4 回読んでも `2560×1680` のまま動かない。
//!   → **こちらの引き算は、Windows でしか働かない**（あちらは 16×39）
//!
//!   **画面上の実寸は、この台では当てにならなかった。** 08:43 の周は
//!   `CGWindowListCopyWindowInfo` が **`1280×840`＝設定どおり**を返したのに、
//!   08:46 以降の周は**どの設定でも要求の 98%**（`1280×840`→`1254×824`、
//!   `1200×700`→`1176×686`）になった。**この工事を外した対照でも同じ 98%** なので、
//!   **こちらの差分ではない**。**何が変わったかは測っていない。**
//!   **言えるのは「縮めた窓は毎回 `NSScreen.visibleFrame` の内側に収まった」まで。**
//!
//!   **tao のコードを読むと、そうならないはずである**——macOS の `inner_size()` は
//!   `NSView::frame`、`outer_size()` は `NSWindow::frame` なので、
//!   **題名帯のぶん（およそ 28 pt）だけ違ってよい**（2ゲート目の指摘）。
//!   **測ったほうを採る。** 読みが正しければ、`fit` は macOS で高さを題名帯のぶん
//!   多く見積もっていることになり、**縮めた窓は作業領域から 28 pt はみ出すはず**
//!   ——**08:25 の周は `1512×884` ちょうどで、はみ出していない**。
//!   **この反証が効くのはその周だけ**である（98% の周は、どのみち内側に入るので
//!   何も言えない）。**なぜ食い違うかは測っていない。**
//!
//!   **だから、この見積もりは1回しか使わない。** 位置のほうは
//!   **`landing` が決める**（2026-09-17 に「縮めたら原点」から変えた——1軸が数 px
//!   はみ出しただけで隅へ飛んでいた。判定はあの関数の doc に1か所だけ置く）——**外形を組み立て直すと、
//!   同じ誤差を寸法と位置で2回使う**（2ゲート目の指摘）。
//!   **残る危険は寸法の側だけ**で、**`visibleFrame` が既定の 840 pt ＋ 題名帯より
//!   低い Mac では、題名帯のぶん高い窓を頼むことになる**。
//!   **AppKit の `constrainFrameRect:toScreen:` がそれを詰めている可能性があるが、
//!   それは測っていない。**
//! - **896×442（150% の `1366×768` で出るはずの寸法）で起動して、画面を見た**。
//!   一覧・側の帯・歯車まで出る。**ただしツールバーの見出しは2行に折り返す**
//!   （既知の「ツールバーが縮まない」件がそのまま出る形）
//!
//! **測っていないこと**（`fit` の単体試験は数の上の話であって、実機ではない）:
//! **150% の台も `1366×768` の台も、こちらにも win の台にも無い。**
//! `1920×1080` を 150% にした台なら**既定が入らない側は再現できる**——
//! 床が入らない側は、その配置の実機が要る。
//! **896 px でツールバーがどうなるかは、どちらの台でも測っていない。**
//!
//! **これは起動時に1回だけ効く。** `ScaleFactorChanged` も `Moved` も見ていないので、
//! **大きい画面で起動してから 150% の小さい画面へ持って行った窓**は、
//! **この工事の前と同じ状態になる**（2ゲート目の指摘）。**塞いでいない穴として書く。**
//!
//! **Tauri 自身の `preventOverflow` は、この用には足りない。**
//! `tauri-runtime-wry` は窓を建てる前に作業領域へ収めてくれる（ちらつかない）が、
//! **`inner_size_constraints.clamp` を先に通す**ので、**床より下へは決して行かない**
//! ——**床が入らない画面**という、この工事がまさに扱う場合を直せない。
//! **主画面を見る**（窓が載る画面ではない）のも別の勘定である。
//! **2つの機構が同じことを別の根拠で決める形にしない**ために、こちらに寄せてある。
//!
//! **床が効いていることは、掴んで縮めなくても読める**——tao が
//! `WM_GETMINMAXINFO` の `ptMinTrackSize` に入れるので、**外のプロセスから
//! `SendMessage` で読める**（09-08 に `976×559`＝`960+16 × 520+39` を実測）。
//! **`SetWindowPos` で測ってはいけない**——**あれは `ptMinTrackSize` を通らないので、
//! 床が在っても 300×250 に縮む**（09-09 に win が、床の無い 0.2.7 と並べて対照を取った）。

use pictkura_core::applog;

/// 作業領域に収めたあとの、クライアント領域の論理 px。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    /// 開くときの寸法。
    pub size: (f64, f64),
    /// 縮められる下限。**画面が設定の床を映せないときは、こちらも下がる。**
    pub min: (f64, f64),
    /// 設定値のどちらかを削ったか。**記録に1行残すのはこのときだけ**——
    /// 何も削っていない台で毎回書くと、書いてあること自体が意味を失う。
    pub shrunk: bool,
}

/// 返ってきた矩形を信用しなくなる線（**物理 px**・枠を引いたあと）。
///
/// **これより狭い作業領域を返す台は、配置ではなく値のほうを疑う**——`work_area` が
/// 0 を返す道（仮想デスクトップの取り違え、画面が繋がっていない瞬間）が実際に在り、
/// そこで 1 px の窓を作ると**利用者は掴む場所を失う**。
///
/// **論理 px で引いてはいけない。** 論理の寸法は倍率で割ったあとの数なので、
/// **倍率が高いほど小さくなる**——`1366×768` を 250% で使う台は作業領域が
/// 論理 288 しか無く、**線に掛かって工事ごと止まる**。**物理で 320 px より狭い
/// デスクトップは実在しない**が、論理で 320 を下回るデスクトップは実在する。
const DISTRUST_BELOW_PHYSICAL: u32 = 320;

/// 作業領域と枠から、クライアント領域の論理 px を決める。
///
/// `work` と `frame` は**物理 px**、`want` と `floor` は**論理 px**。
/// `frame` は外形とクライアントの差（＝装飾の実費）で、**倍率ごとに違う**。
pub(crate) fn fit(
    work: (u32, u32),
    frame: (u32, u32),
    scale: f64,
    want: (f64, f64),
    floor: (f64, f64),
) -> Option<Fit> {
    // 倍率が 0 や NaN で返る道を見たわけではない。**割る前に確かめるのが安いだけ**である。
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    // **信用しないかどうかは、物理 px で決める。** 論理 px で線を引くと、
    // **倍率の高い小さな画面ほど線に掛かる**——`1366×768` の作業領域 720 を 250% で
    // 使う台は論理で 288 しか無く、**いちばんこの工事が要る配置で工事ごと止まる**
    // （PR 側の codex）。**画面の実在を疑うなら、画面の単位で疑う。**
    let usable = (
        work.0.saturating_sub(frame.0),
        work.1.saturating_sub(frame.1),
    );
    if usable.0 < DISTRUST_BELOW_PHYSICAL || usable.1 < DISTRUST_BELOW_PHYSICAL {
        return None;
    }
    // **1 論理 px を下回らせない。** 倍率が極端でも、寸法 0 の窓は頼まない。
    let max = (
        (f64::from(usable.0) / scale).floor().max(1.0),
        (f64::from(usable.1) / scale).floor().max(1.0),
    );
    let size = (want.0.min(max.0), want.1.min(max.1));
    let min = (floor.0.min(max.0), floor.1.min(max.1));
    Some(Fit {
        shrunk: size != want || min != floor,
        size,
        min,
    })
}

/// 作業領域からはみ出している外形を、押し戻す先。**動かす必要が無ければ `None`。**
///
/// **寸法を直しただけでは足りない**——左上が画面の下寄りに置かれていれば、
/// 作業領域と同じ高さの窓でも下端が隠れる。
pub(crate) fn clamp_position(
    pos: (i32, i32),
    outer: (u32, u32),
    work_pos: (i32, i32),
    work_size: (u32, u32),
) -> Option<(i32, i32)> {
    let fitted = (
        clamp_axis(pos.0, outer.0, work_pos.0, work_size.0),
        clamp_axis(pos.1, outer.1, work_pos.1, work_size.1),
    );
    (fitted != pos).then_some(fitted)
}

/// 1軸ぶん。**窓のほうが作業領域より大きいときは、左（上）端に寄せる**——
/// どちらの端も選べないので、**利用者が掴める側**を残す。
fn clamp_axis(pos: i32, outer: u32, work_pos: i32, work_len: u32) -> i32 {
    if outer >= work_len {
        return work_pos;
    }
    let max = i64::from(work_pos) + i64::from(work_len) - i64::from(outer);
    let clamped = i64::from(pos).clamp(i64::from(work_pos), max);
    // `max` は `work_pos + work_len` を超えないが、**足し算は i64 で持つ**
    // （`work_len` は u32 なので、i32 のままだと理屈の上では溢れる）。
    clamped.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// 寸法を直したあと、窓をどこへ置くか。**`None` なら動かさない。**
///
/// **外形は「読んだ値」しか使わない。** 縮めた窓の外形をこちらで組み立てると、
/// **枠の見積もりを寸法と位置で2回使う**ことになる（2026-09-16 の2ゲート目の指摘。
/// macOS の枠は測ると `0×0`）。
///
/// **読めた外形が作業領域からはみ出しているぶんだけ押し戻す**——収まっていれば `None` で、
/// **OS が選んだ置き場所をそのまま残す**。**これが無いと、1軸が数 px はみ出しただけの窓が、
/// もう1軸に余白があっても作業領域の隅へ飛ぶ**（2026-09-17、この台で実測: 作業領域 1512×884 に
/// 対して高さだけ 6 pt 超える設定で、幅に 232 pt 余っているのに `(0, 33)` へ寄った）。
///
/// **読んだ値は「縮めたあとの外形」とは限らない。** macOS の `set_inner_size` は
/// `setContentSize:` を **`DispatchQueue::main().exec_async`** に渡す——**主スレッドから呼んでも
/// 待たない**（`tao-0.35.3/src/platform_impl/macos/util/async.rs:87`。隣の
/// `set_style_mask` は `is_main_thread()` で分岐して `exec_sync` を使うので、**これは
/// 書き分けである**）。だから直後の読みは**縮める前**を映しうる。
/// **それでもこの規則で正しい**のは、**縮めた軸は作業領域ぴったりまで縮める**からで、
/// 古い（大きい）値は `clamp_axis` の「作業領域より大きい軸は近い端へ」に落ち、
/// **縮めたあとの値と同じ答えになる**——`fit_shrinks_the_overflowing_axis_to_exactly_the_usable_extent`
/// がその前提を固定している。**縮んでいない軸だけが動かずに残る。それがこの直しの全部である。**
///
/// **読めなかったときは、縮めたかどうかで割れる。** 縮めたなら原点へ——
/// **外形を知らなくても「これ以上ましな置き場は無い」と言える置き方**で、2026-09-16 の形である。
/// **縮めていないなら動かさない**: はみ出している証拠が何も無いのに、
/// **正しく置かれている窓を隅へ運ぶ**ことになる（2026-09-17 の2ゲート目の指摘。
/// 直しの前の道はここで `and_then` によって「何もしない」に落ちていた）。
///
/// **「読めたが作業領域より大きい」に別の枝は要らない。** `clamp_axis` が既に
/// **その軸だけ近い端へ寄せる**と決めていて、**もう1軸は余白ぶん普通に押し戻せる**——
/// 原点へ落とすと、大きいほうの軸のために小さいほうまで動かすことになる。
/// （最初に書いた版はその枝を持っていた。**変異で殺せなかった**——`clamp_axis` と
/// 答えが同じだったからで、**殺せない枝は「別の規則が在る」と読む人に嘘をつく**。）
fn landing(
    shrunk: bool,
    read: Option<((i32, i32), (u32, u32))>,
    work_pos: (i32, i32),
    work_size: (u32, u32),
) -> Option<(i32, i32)> {
    match read {
        Some((pos, outer)) => clamp_position(pos, outer, work_pos, work_size),
        None if shrunk => Some(work_pos),
        None => None,
    }
}

/// 主窓を、いま載っている画面の作業領域に合わせる。**入っているなら何もしない。**
///
/// **数は `tauri.conf.json` から読む**——ここに写しを置くと、
/// **設定を直した人が、直したはずの窓が変わらないのを見る**ことになる。
///
/// **失敗しても起動は止めない。** 画面の寸法が取れないことと、
/// アプリが立ち上がらないことでは、後者のほうがずっと重い。
pub(crate) fn fit_main_window(app: &tauri::AppHandle) {
    use tauri::{LogicalSize, Manager, PhysicalPosition};

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let config = app.config();
    let Some(cfg) = config
        .app
        .windows
        .iter()
        .find(|w| w.label == window.label())
    else {
        return;
    };

    // **窓が載っている画面**。取れなければ何もしない——**主画面で代用しない**。
    // `set_size` は**窓の**倍率で物理 px に直され、`clamp_position` は渡した矩形へ
    // 押し込むので、**別の画面の倍率と作業領域を混ぜると、入っていた窓を
    // 「合わせた」あげく別の画面へ運ぶ**（2ゲート目の指摘）。
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let work = *monitor.work_area();
    let scale = monitor.scale_factor();

    // **枠の実費は、訊けるので訊く**（`AdjustWindowRectEx` 相当を自前で持たない）。
    // 倍率でも、装飾の設定でも変わる。
    let (Ok(outer), Ok(inner)) = (window.outer_size(), window.inner_size()) else {
        return;
    };
    let frame = (
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    );

    let want = (cfg.width, cfg.height);
    let floor = (
        cfg.min_width.unwrap_or_default(),
        cfg.min_height.unwrap_or_default(),
    );
    // **信用できない矩形なら、寸法も位置も触らない。** 寸法だけ守って位置を
    // 動かすと、**0×0 の作業領域を信じて窓をその原点へ運ぶ**（2ゲート目の指摘）。
    let Some(f) = fit(
        (work.size.width, work.size.height),
        frame,
        scale,
        want,
        floor,
    ) else {
        return;
    };

    if f.shrunk {
        // **床を先に下げる。** 上限より高い床が残っていると、続く `set_size` が
        // そこで止まる台がある。
        let floor_set = if cfg.min_width.is_some() || cfg.min_height.is_some() {
            window.set_min_size(Some(LogicalSize::new(f.min.0, f.min.1)))
        } else {
            Ok(())
        };
        let size_set = window.set_size(LogicalSize::new(f.size.0, f.size.1));
        // **記録は、頼んだあとに書く。** 先に書くと、**断られた台の記録に
        // 「この寸法で開いた」と残る**——**書いた本人の言い分がいちばん監査されない**
        // （2ゲート目の指摘）。
        applog::note(&format!(
            "window does not fit this display: work area {}x{} at {scale}x, frame {}x{} \
             -> asked for {}x{} (configured {}x{}), floor {}x{} (configured {}x{}){}{}",
            work.size.width,
            work.size.height,
            frame.0,
            frame.1,
            f.size.0,
            f.size.1,
            want.0,
            want.1,
            f.min.0,
            f.min.1,
            floor.0,
            floor.1,
            match &floor_set {
                Ok(()) => String::new(),
                Err(e) => format!("; the floor was refused: {e}"),
            },
            match &size_set {
                Ok(()) => String::new(),
                Err(e) => format!("; the size was refused: {e}"),
            },
        ));
    }

    // **寸法が入っていても、置き場所が外なら見えない。**
    //
    // **置き場所は、寸法を直した「あと」に読む。** 縮めたかどうかで読む時点を
    // 変えないので、`set_size` を呼んだ道と呼んでいない道が同じ判定を通る。
    let read = match (window.outer_position(), window.outer_size()) {
        (Ok(pos), Ok(size)) => Some(((pos.x, pos.y), (size.width, size.height))),
        _ => None,
    };
    let moved_to = landing(
        f.shrunk,
        read,
        (work.position.x, work.position.y),
        (work.size.width, work.size.height),
    );
    if let Some((x, y)) = moved_to {
        // **動かしたことは書く。** 利用者から見える症状は「窓が起動時に跳んだ」で、
        // **成功した移動こそ記録が無いと追えない**（2026-09-17 の指摘）。
        match window.set_position(PhysicalPosition::new(x, y)) {
            Ok(()) => applog::note(&format!(
                "moved the window onto the work area {}: -> ({x}, {y})",
                match read {
                    Some(((px, py), (ow, oh))) => format!("from ({px}, {py}), outer {ow}x{oh}"),
                    None => "without being able to read its outer rect".to_string(),
                },
            )),
            Err(e) => applog::note(&format!(
                "could not move the window onto the work area: {e}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // win の実測（2026-09-09）。**枠は倍率ごとに OS が返した値**。
    const FRAME_100: (u32, u32) = (16, 39);
    const FRAME_150: (u32, u32) = (22, 56);
    const WANT: (f64, f64) = (1280.0, 840.0);
    const FLOOR: (f64, f64) = (960.0, 520.0);

    /// **信用できる矩形のときだけ値が返る。** `None` は「触るな」である。
    fn fitted(work: (u32, u32), frame: (u32, u32), scale: f64) -> Fit {
        fit(work, frame, scale, WANT, FLOOR).expect("この作業領域は信用できるはず")
    }

    #[test]
    fn wide_screen_is_left_alone() {
        // 1920×1080・100%・タスクバー 48 px（win の台そのもの）。
        let f = fitted((1920, 1032), FRAME_100, 1.0);
        assert_eq!(f.size, WANT);
        assert_eq!(f.min, FLOOR);
        assert!(!f.shrunk);
    }

    #[test]
    fn short_screen_loses_height_only() {
        // 1366×768・100%・タスクバー 48 px。**幅は 70 px 余る**ので触らない。
        let f = fitted((1366, 720), FRAME_100, 1.0);
        assert_eq!(f.size, (1280.0, 681.0));
        assert_eq!(f.min, FLOOR);
        assert!(f.shrunk);
    }

    #[test]
    fn the_floor_comes_down_when_the_screen_cannot_show_it() {
        // 1366×768・150%。**床の 960×520 が外形 1462×836 になって入らない配置。**
        let f = fitted((1366, 720), FRAME_150, 1.5);
        assert_eq!(f.size, (896.0, 442.0));
        assert_eq!(f.min, (896.0, 442.0));
        assert!(f.shrunk);
        // #136 の本文が導いた「幅の上限は 896」と同じ数。
        assert_eq!(f.size.0, ((1366.0 - 22.0) / 1.5f64).floor());
    }

    #[test]
    fn floor_fits_but_the_default_does_not() {
        // 1920×1080・150%・タスクバー 72 px（外挿値・win の記録どおり）。
        let f = fitted((1920, 1008), FRAME_150, 1.5);
        assert_eq!(f.size, (1265.0, 634.0));
        // **床はそのまま**——この配置では 960×520 が映る。
        assert_eq!(f.min, FLOOR);
        assert!(f.shrunk);
    }

    #[test]
    fn a_small_screen_with_heavy_scaling_is_still_fitted() {
        // `1366×768` の作業領域 720 を 250% で使う台。**論理では 265 しか無い**
        // ——ここで止まると、**いちばんこの工事が要る配置で何もしない**（PR 側の codex）。
        let f = fitted((1366, 720), FRAME_150, 2.5);
        assert_eq!(f.size, (537.0, 265.0));
        assert_eq!(f.min, (537.0, 265.0));
        assert!(f.shrunk);
        // 疑うのは物理のほう。**320 物理 px より狭い矩形は、値のほうを疑う。**
        assert_eq!(fit((1366, 300), FRAME_150, 2.5, WANT, FLOOR), None);
    }

    #[test]
    fn a_work_area_that_makes_no_sense_is_not_obeyed() {
        // 0 を返してきた台。**1 px の窓を作らないし、位置も動かさない**
        // ——`None` は呼び手に「この矩形は使うな」と言う。
        assert_eq!(fit((0, 0), FRAME_100, 1.0, WANT, FLOOR), None);
        // 枠のほうが作業領域より大きい、も同じ扱い。
        assert_eq!(fit((10, 10), FRAME_100, 1.0, WANT, FLOOR), None);
    }

    #[test]
    fn a_scale_that_makes_no_sense_is_not_obeyed() {
        for scale in [0.0, -1.5, f64::NAN, f64::INFINITY] {
            assert_eq!(
                fit((1366, 720), FRAME_100, scale, WANT, FLOOR),
                None,
                "scale = {scale}"
            );
        }
    }

    #[test]
    fn a_window_inside_the_work_area_is_not_moved() {
        assert_eq!(
            clamp_position((100, 100), (800, 600), (0, 0), (1920, 1032)),
            None
        );
    }

    #[test]
    fn a_window_hanging_off_the_bottom_comes_back() {
        // 下端が 1032 を 68 px 超える置き方。
        assert_eq!(
            clamp_position((100, 500), (800, 600), (0, 0), (1920, 1032)),
            Some((100, 432))
        );
    }

    #[test]
    fn a_window_larger_than_the_work_area_goes_to_the_corner() {
        // **入らない窓は左上へ。** 右下に寄せると、掴む場所ごと画面の外へ出る。
        assert_eq!(
            clamp_position((40, 40), (2000, 1200), (0, 0), (1920, 1032)),
            Some((0, 0))
        );
    }

    #[test]
    fn the_work_area_may_not_start_at_the_origin() {
        // 2枚目の画面（左に置かれた台）と、上にタスクバーがある台。
        assert_eq!(
            clamp_position((-2000, 10), (800, 600), (-1920, 40), (1920, 992)),
            Some((-1920, 40))
        );
        assert_eq!(
            clamp_position((-1900, 100), (800, 600), (-1920, 40), (1920, 992)),
            None
        );
    }

    #[test]
    fn one_axis_can_shrink_while_the_other_has_slack() {
        // **1366×768 の 100%、ただし高さだけ足りない台**。幅は 70 px 余っている。
        // **この形の試験が無かったので、「縮んだら隅へ飛ぶ」が見えていなかった**
        // （2026-09-17 のレビュー）。
        let f = fit((1366, 700), (16, 39), 1.0, (1280.0, 840.0), (960.0, 520.0)).unwrap();
        assert!(f.shrunk);
        assert_eq!(f.size, (1280.0, 661.0)); // 幅は要求どおり、高さだけ縮む
    }

    #[test]
    fn a_window_that_fits_is_left_where_the_os_put_it() {
        // **両軸とも作業領域の内側**——この升が当てたい形である。
        // （最初の版は高さが作業領域ちょうどで、`clamp_axis` の「大きい軸は近い端へ」を
        // 通って `None` になっていた。**名前が言う枝を通っていなかった**＝2ゲート目の指摘。）
        assert_eq!(
            landing(true, Some(((232, 100), (2560, 1700))), (0, 66), (3024, 1768)),
            None
        );
    }

    #[test]
    fn a_window_that_hangs_off_is_pushed_back_only_as_far_as_needed() {
        // 右へ 100 px はみ出している。**左端まで戻さない。**
        assert_eq!(
            landing(true, Some(((1000, 40), (1000, 600))), (0, 40), (1920, 992)),
            Some((920, 40))
        );
    }

    #[test]
    fn an_unreadable_rect_falls_back_to_the_origin_only_when_the_window_was_shrunk() {
        // **縮めた**なら原点へ——外形を知らなくても「これ以上ましな置き場は無い」と言える。
        assert_eq!(landing(true, None, (0, 40), (1920, 992)), Some((0, 40)));
        // **縮めていない**なら動かさない。はみ出している証拠が何も無いのに
        // **正しく置かれている窓を隅へ運ぶ**のは、この PR が消した症状そのものである。
        assert_eq!(landing(false, None, (0, 40), (1920, 992)), None);
    }

    #[test]
    fn an_oversized_rect_needs_no_branch_of_its_own() {
        // 枠がこちらの見えない量だけ大きい台、そして **macOS の「縮める前の外形」**。
        // `clamp_axis` が既にその軸を近い端へ寄せるので、専用の枝は要らない。
        assert_eq!(
            landing(true, Some(((10, 50), (2000, 1200))), (0, 40), (1920, 992)),
            Some((0, 40))
        );
    }

    #[test]
    fn fit_shrinks_the_overflowing_axis_to_exactly_the_usable_extent() {
        // **`landing` の macOS での正しさは、この前提に乗っている。**
        // あちらの `set_inner_size` は `setContentSize:` を非同期に投げる（主スレッドでも待たない、
        // `tao-0.35.3/.../util/async.rs:87`）ので、直後の読みは**縮める前**を映しうる。
        // **縮めた軸をぴったりまで縮めている限り**、古い（大きい）値も `clamp_axis` の
        // 「大きい軸は近い端へ」に落ちて**同じ答え**になる。
        // **ここに余白を残す変更をしたら、この升が赤くなって `landing` の doc へ連れて行く。**
        let f = fit((3024, 1768), (0, 0), 2.0, (1280.0, 890.0), (960.0, 520.0)).unwrap();
        assert!(f.shrunk);
        assert_eq!(f.size.1 * 2.0, 1768.0); // 高さは作業領域ぴったり
        assert_eq!(f.size.0, 1280.0); // 幅は要求どおり（余白は縮めない側に残る）
    }
}
