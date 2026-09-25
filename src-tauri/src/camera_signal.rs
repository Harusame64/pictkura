//! サムネイルの流れがカメラを埋めたとき、左ペインの「カメラとメディア」へ知らせる（dev #28）。
//!
//! **走査は行を `camera_id` 空のまま足す。** 埋めるのは後から回るサムネイルの流れ
//! （`update_metadata`）で、1件ずつ書く。その完了は `media-updated` で1件の DTO を
//! 送るだけで、**DTO にカメラの欄は無い**——だから UI は「カメラが付いた」を知らない。
//! フォルダを足しても、次に何かが数え直すまで新しいカメラが左ペインに出なかった
//! （実機で、フォルダを外して足し直し、12 秒待っても出なかった）。
//!
//! **全体を数え直す口を、完了のたびに叩かない。** #148 の2版目は `refreshSummary`
//! のたびに数え直し、サムネイルを作っている間は 2 秒ごと・絞り込みを変えるたびに
//! `GROUP BY camera_id` を全件で回した。ここでは:
//!
//! - **行の `camera_id` が動いたときだけ**受け取る（[`CameraWrite`]）。完了の通知は
//!   絵を作り直しただけのとき（可視要求の高品質版など）にも来るので、それは数えない。
//!   動いたかどうかは、サムネイルの流れが処理の前後で読み比べて添える
//! - **左ペインにまだ出ていないカメラ**が埋まったら、**すぐ**知らせる。
//!   「出ているカメラ」は、最後に `list_cameras` が返した id の集合である
//!   （外したフォルダのカメラは、数え直しで一覧から落ちた時点で集合からも落ちる。
//!   だから足し直せば、また「まだ出ていない」になる）
//! - **枚数**は、動きが [`QUIET`] 途切れたときにまとめて知らせる。
//!   途切れないまま動き続けても、[`MAX_WAIT`] に1回は知らせる
//!   （新しいカメラが「1枚」のまま、何分も止まって見えないように）。
//!   すぐ知らせた新しいカメラも、この後追いに乗せる——UI がその知らせを
//!   取りこぼしても（読み込み直しの最中など）、もう一度数え直す機会が残る
//! - **UI がまだ一度も数えていない間**（起動直後）は、どのカメラも「すぐ」にはしない。
//!   UI は開いたときに自分で数えるので、そこへ連打を重ねない
//!
//! 判断は [`CameraSignal`] に閉じ込め、時刻を引数で受ける（試験で時計を動かすため）。
//! 糸と通り道は [`spawn`] が持つ。

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pictkura_core::db::Db;
use pictkura_core::thumbs::CameraWrite;

use crate::lock_ok;

/// 動きがこれだけ途切れたら、カメラの枚数を数え直させる。
pub const QUIET: Duration = Duration::from_secs(2);
/// 動きが途切れなくても、これ以上は待たせない。
pub const MAX_WAIT: Duration = Duration::from_secs(10);

pub struct CameraSignal {
    /// 最後に `list_cameras` が返したカメラの id（＝左ペインに出ているもの）。
    /// `None` は、UI がまだ一度も数えていない
    listed: Option<HashSet<i64>>,
    /// 知らせていない動きのうち、最初と最後の時刻
    pending: Option<(Instant, Instant)>,
    quiet: Duration,
    max_wait: Duration,
}

impl Default for CameraSignal {
    fn default() -> Self {
        Self::with_timing(QUIET, MAX_WAIT)
    }
}

impl CameraSignal {
    pub fn with_timing(quiet: Duration, max_wait: Duration) -> Self {
        Self {
            listed: None,
            pending: None,
            quiet,
            max_wait,
        }
    }

    /// `list_cameras` が返した id で、出ているカメラを置き換える。
    pub fn set_listed(&mut self, ids: impl IntoIterator<Item = i64>) {
        self.listed = Some(ids.into_iter().collect());
    }

    /// サムネイルの流れが1行を処理した。**いま知らせるなら真。**
    pub fn on_write(&mut self, camera: CameraWrite, now: Instant) -> bool {
        let listed = |id: Option<i64>| id.filter(|id| Db::is_listed_camera_id(*id));
        let to = match camera {
            CameraWrite::Unchanged => return false,
            // 数が動いたかどうか分からない。後追いには乗せる
            CameraWrite::Unknown => None,
            CameraWrite::Changed { from, to } => {
                // 未確認→「カメラなし」のような、左ペインの外だけの動き
                if listed(from).is_none() && listed(to).is_none() {
                    return false;
                }
                listed(to)
            }
        };
        let first = self.pending.map_or(now, |(first, _)| first);
        self.pending = Some((first, now));
        if let (Some(id), Some(shown)) = (to, self.listed.as_mut()) {
            if shown.insert(id) {
                return true;
            }
        }
        self.fire_if_due(now)
    }

    /// 次に [`Self::on_tick`] を呼ぶべき時刻。溜めていなければ `None`（待つだけ）。
    pub fn deadline(&self) -> Option<Instant> {
        self.pending
            .map(|(first, last)| (last + self.quiet).min(first + self.max_wait))
    }

    /// 時刻が来た。**いま知らせるなら真。**
    pub fn on_tick(&mut self, now: Instant) -> bool {
        self.fire_if_due(now)
    }

    fn fire_if_due(&mut self, now: Instant) -> bool {
        match self.deadline() {
            Some(due) if now >= due => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }
}

/// 受け口の糸を立てる。`rx` には完了ごとの [`CameraWrite`] が来る。
/// 送る側が全部落ちたら（アプリの終了）、糸も終わる。
pub fn spawn(
    signal: Arc<Mutex<CameraSignal>>,
    rx: Receiver<CameraWrite>,
    announce: impl Fn() + Send + 'static,
) {
    let _ = std::thread::Builder::new()
        .name("camera-signal".into())
        .spawn(move || loop {
            let due = lock_ok(&signal).deadline();
            let got = match due {
                Some(due) => rx.recv_timeout(due.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            let now = Instant::now();
            let mut s = lock_ok(&signal);
            let fire = match got {
                Ok(camera) => s.on_write(camera, now),
                Err(RecvTimeoutError::Timeout) => s.on_tick(now),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            drop(s);
            if fire {
                announce();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    fn to(id: i64) -> CameraWrite {
        CameraWrite::Changed {
            from: None,
            to: Some(id),
        }
    }

    fn listed(ids: &[i64]) -> CameraSignal {
        let mut s = CameraSignal::default();
        s.set_listed(ids.iter().copied());
        s
    }

    #[test]
    fn the_timing_is_two_seconds_quiet_and_ten_at_most() {
        assert_eq!(QUIET, Duration::from_secs(2));
        assert_eq!(MAX_WAIT, Duration::from_secs(10));
    }

    #[test]
    fn a_camera_not_yet_listed_is_announced_at_once_and_only_once() {
        let t0 = Instant::now();
        let mut s = listed(&[1]);
        assert!(s.on_write(to(2), t0), "左ペインに無いカメラはすぐ知らせる");
        assert!(
            !s.on_write(to(2), at(t0, 50)),
            "同じカメラの次の1枚では知らせない"
        );
    }

    #[test]
    fn an_announced_camera_is_recounted_once_more_after_the_writes_go_quiet() {
        // すぐの知らせを UI が取りこぼしても、後追いがもう一度数え直させる
        let t0 = Instant::now();
        let mut s = listed(&[]);
        assert!(s.on_write(to(2), t0));
        assert_eq!(s.deadline(), Some(t0 + QUIET));
        assert!(s.on_tick(t0 + QUIET));
    }

    #[test]
    fn a_listed_camera_is_recounted_once_the_writes_go_quiet() {
        let t0 = Instant::now();
        let mut s = listed(&[1]);
        assert!(!s.on_write(to(1), t0));
        assert!(!s.on_write(to(1), at(t0, 500)));
        assert_eq!(s.deadline(), Some(at(t0, 500) + QUIET));
        assert!(!s.on_tick(at(t0, 500) + QUIET - Duration::from_millis(1)));
        assert!(s.on_tick(at(t0, 500) + QUIET), "途切れたら1回知らせる");
        assert_eq!(s.deadline(), None);
        assert!(!s.on_tick(at(t0, 60_000)), "知らせた後は黙る");
    }

    #[test]
    fn writes_that_never_go_quiet_are_still_recounted_every_max_wait() {
        let t0 = Instant::now();
        let mut s = listed(&[1]);
        let step = QUIET.as_millis() as u64 / 2;
        let mut fired_at = Vec::new();
        let mut ms = 0;
        while ms <= 25_000 {
            if s.on_write(to(1), at(t0, ms)) {
                fired_at.push(ms);
            }
            ms += step;
        }
        assert_eq!(fired_at, vec![10_000, 21_000], "書き続けても10秒に1回");
    }

    #[test]
    fn a_camera_dropped_from_the_list_is_new_again() {
        // 実機の再現: フォルダを外す→数え直しで一覧から落ちる→足し直す
        let t0 = Instant::now();
        let mut s = listed(&[1, 2]);
        assert!(!s.on_write(to(2), t0));
        s.set_listed([1]);
        assert!(s.on_write(to(2), at(t0, 10)));
    }

    #[test]
    fn a_write_that_did_not_move_the_camera_is_not_counted() {
        // 眺めているだけ（可視要求の高品質版）で数え直さない
        let mut s = listed(&[]);
        assert!(!s.on_write(CameraWrite::Unchanged, Instant::now()));
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn moves_outside_the_sidebar_are_not_counted() {
        // 未確認→「カメラなし」は左ペインの数を変えない
        let mut s = listed(&[]);
        let none = CameraWrite::Changed {
            from: None,
            to: Some(0),
        };
        assert!(!s.on_write(none, Instant::now()));
        assert_eq!(s.deadline(), None);
        assert!(!s.on_write(to(0), Instant::now()), "0 はカメラではない");
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn a_camera_that_goes_away_is_recounted_but_not_announced_at_once() {
        let t0 = Instant::now();
        let mut s = listed(&[1]);
        let gone = CameraWrite::Changed {
            from: Some(1),
            to: Some(0),
        };
        assert!(!s.on_write(gone, t0));
        assert_eq!(s.deadline(), Some(t0 + QUIET), "減ったぶんも後で数え直す");
    }

    #[test]
    fn an_unreadable_write_is_recounted_later() {
        let t0 = Instant::now();
        let mut s = listed(&[1]);
        assert!(!s.on_write(CameraWrite::Unknown, t0));
        assert_eq!(s.deadline(), Some(t0 + QUIET));
    }

    #[test]
    fn nothing_is_announced_at_once_before_the_ui_first_counts() {
        // 起動直後: UI は開いたときに自分で数える。そこへ連打を重ねない
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        assert!(!s.on_write(to(1), t0));
        assert!(!s.on_write(to(2), at(t0, 10)));
        assert_eq!(s.deadline(), Some(at(t0, 10) + QUIET), "後追いには乗る");
    }

    #[test]
    fn nothing_is_pending_before_any_write() {
        let mut s = CameraSignal::default();
        assert_eq!(s.deadline(), None);
        assert!(!s.on_tick(Instant::now()));
    }

    /// 時間を縮めた糸で、すぐの知らせ・後追い・終わりを通しで見る
    #[test]
    fn the_thread_announces_now_then_once_quiet_and_ends_with_its_senders() {
        let quiet = Duration::from_millis(100);
        let mut s = CameraSignal::with_timing(quiet, Duration::from_secs(60));
        s.set_listed([1]);
        let signal = Arc::new(Mutex::new(s));
        let (tx, rx) = std::sync::mpsc::channel();
        let (fired_tx, fired_rx) = std::sync::mpsc::channel();
        spawn(signal, rx, move || {
            let _ = fired_tx.send(Instant::now());
        });

        // 出ているカメラ: すぐには知らせず、途切れてから1回
        let sent = Instant::now();
        tx.send(to(1)).unwrap();
        let fired = fired_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("途切れたら知らせる");
        assert!(fired >= sent + quiet, "途切れる前に知らせた");
        assert!(fired_rx.recv_timeout(quiet * 3).is_err(), "1回だけ知らせる");

        // まだ出ていないカメラ: すぐ
        let sent = Instant::now();
        tx.send(to(7)).unwrap();
        let fired = fired_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("まだ出ていないカメラは知らせる");
        assert!(fired < sent + quiet, "すぐ知らせていない");

        drop(tx);
        // 送る側が落ちたら糸も終わり、知らせる口も落ちる（後追いの1回は残りうる）
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match fired_rx.recv_timeout(Duration::from_secs(5)) {
                Err(RecvTimeoutError::Disconnected) => break,
                Ok(_) => assert!(Instant::now() < deadline),
                Err(RecvTimeoutError::Timeout) => panic!("糸が終わらない"),
            }
        }
    }
}
