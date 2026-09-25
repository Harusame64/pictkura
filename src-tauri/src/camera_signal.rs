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
//! - **行の `camera_id` が動いたときだけ**数える（[`CameraWrite`]）。完了の通知は
//!   絵を作り直しただけのとき（可視要求の高品質版など）にも来るので、それは数えない。
//!   動いたかどうかは、サムネイルの流れが処理の前後で読み比べて添える
//! - 動きが [`QUIET`] 途切れたら、**1回だけ**数え直させる。途切れないまま動き続けても、
//!   [`MAX_WAIT`] に1回は数え直させる（新しいカメラが「1枚」のまま、何分も止まって
//!   見えないように）
//!
//! **「まだ出ていないカメラはすぐ」は持たない。** 一度は持ったが（#152 の初版）、
//! 左ペインに出ているカメラを覚える集合が要り、その集合は UI の数え直しと競り、
//! 起動直後には空で、共有のロックも要った。新しいカメラが出るまで、動きが途切れれば
//! [`QUIET`]、取り込みのように途切れなければ最長 [`MAX_WAIT`] 待つ——
//! サムネイルが並ぶのと同じ速さで、その差のために持つ仕掛けではない。
//!
//! **走査が空にした行を埋め直している間は、枚数がいったん減って見える。**
//! 中身の変わったファイルは走査が `camera_id` を空にし、ここは埋め直した行を1枚ずつ
//! 数える。[`MAX_WAIT`] ごとの数え直しは途中の値を出し、埋め終われば元に戻る
//! （main では、ファイルの監視の道で減ったまま戻らない形だった。コードから読んだ）。
//!
//! 判断は [`CameraSignal`] に閉じ込め、時刻を引数で受ける（試験で時計を動かすため）。
//! 状態は糸だけが持つ（ロックは無い）。糸と通り道は [`spawn`] が持つ。

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pictkura_core::thumbs::CameraWrite;

/// 動きがこれだけ途切れたら、カメラの枚数を数え直させる。
pub const QUIET: Duration = Duration::from_secs(2);
/// 動きが途切れなくても、これ以上は待たせない。
pub const MAX_WAIT: Duration = Duration::from_secs(10);

pub struct CameraSignal {
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
            pending: None,
            quiet,
            max_wait,
        }
    }

    /// サムネイルの流れが1行を処理した。**いま知らせるなら真。**
    pub fn on_write(&mut self, camera: CameraWrite, now: Instant) -> bool {
        // **期日は、この書き込みで延ばす前に見る。** 糸は期日を過ぎても溜まった知らせを
        // 先に受け取る（`recv_timeout` は溜まっていれば時間切れを返さない）ので、
        // ここで見ないと、眺めているだけの完了や遅れて届いた書き込みの間、
        // 数え直しが後ろへずれる
        let overdue = self.fire_if_due(now);
        // `Unknown` は動いたかどうか分からないので、数え直しに回す
        if camera != CameraWrite::Unchanged {
            let first = self.pending.map_or(now, |(first, _)| first);
            self.pending = Some((first, now));
        }
        overdue || self.fire_if_due(now)
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
    mut signal: CameraSignal,
    rx: Receiver<CameraWrite>,
    announce: impl Fn() + Send + 'static,
) {
    let _ = std::thread::Builder::new()
        .name("camera-signal".into())
        .spawn(move || loop {
            let got = match signal.deadline() {
                Some(due) => rx.recv_timeout(due.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            let now = Instant::now();
            let fire = match got {
                Ok(camera) => signal.on_write(camera, now),
                Err(RecvTimeoutError::Timeout) => signal.on_tick(now),
                Err(RecvTimeoutError::Disconnected) => break,
            };
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

    #[test]
    fn the_timing_is_two_seconds_quiet_and_ten_at_most() {
        assert_eq!(QUIET, Duration::from_secs(2));
        assert_eq!(MAX_WAIT, Duration::from_secs(10));
    }

    #[test]
    fn a_move_is_recounted_once_the_writes_go_quiet() {
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        assert!(!s.on_write(to(1), t0));
        assert!(!s.on_write(to(2), at(t0, 500)));
        assert_eq!(s.deadline(), Some(at(t0, 500) + QUIET));
        assert!(!s.on_tick(at(t0, 500) + QUIET - Duration::from_millis(1)));
        assert!(s.on_tick(at(t0, 500) + QUIET), "途切れたら1回知らせる");
        assert_eq!(s.deadline(), None);
        assert!(!s.on_tick(at(t0, 60_000)), "知らせた後は黙る");
    }

    #[test]
    fn writes_that_never_go_quiet_are_still_recounted_every_max_wait() {
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        let mut fired_at = Vec::new();
        let mut ms = 0;
        while ms <= 25_000 {
            if s.on_write(to(1), at(t0, ms)) {
                fired_at.push(ms);
            }
            ms += 1_000;
        }
        assert_eq!(fired_at, vec![10_000, 20_000], "書き続けても10秒に1回");
    }

    #[test]
    fn a_write_that_did_not_move_the_camera_is_not_counted() {
        // 眺めているだけ（可視要求の高品質版）で数え直さない
        let mut s = CameraSignal::default();
        assert!(!s.on_write(CameraWrite::Unchanged, Instant::now()));
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn a_due_recount_is_not_held_back_by_writes_that_did_not_move() {
        // 期日を過ぎても溜まった完了が先に届く。その間も数え直しは出る
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        assert!(!s.on_write(to(1), t0));
        assert!(s.on_write(CameraWrite::Unchanged, t0 + QUIET));
        assert_eq!(s.deadline(), None);
    }

    #[test]
    fn an_overdue_recount_fires_before_a_late_write_extends_it() {
        // 期日を過ぎてから動きが届いた: まず溜まっていたぶんを出し、新しい動きは次へ
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        assert!(!s.on_write(to(1), t0));
        let late = t0 + QUIET + Duration::from_millis(1);
        assert!(s.on_write(to(2), late));
        assert_eq!(s.deadline(), Some(late + QUIET), "新しい動きは次の回へ");
    }

    #[test]
    fn every_kind_of_move_is_counted() {
        // 新しいカメラ・カメラなしへ・別のカメラへ・減る・読めない
        for camera in [
            to(1),
            to(0),
            CameraWrite::Changed {
                from: Some(1),
                to: Some(2),
            },
            CameraWrite::Changed {
                from: Some(1),
                to: Some(0),
            },
            CameraWrite::Unknown,
        ] {
            let t0 = Instant::now();
            let mut s = CameraSignal::default();
            assert!(!s.on_write(camera, t0));
            assert_eq!(s.deadline(), Some(t0 + QUIET), "{camera:?}");
        }
    }

    #[test]
    fn nothing_is_pending_before_any_write() {
        let mut s = CameraSignal::default();
        assert_eq!(s.deadline(), None);
        assert!(!s.on_tick(Instant::now()));
    }

    /// 時間を縮めた糸で、途切れてからの1回と終わりを通しで見る。
    /// **上限の時間では判定しない**（混んだ CI で糸の起き上がりが遅れても赤くしない）
    #[test]
    fn the_thread_announces_once_after_quiet_and_ends_with_its_senders() {
        let quiet = Duration::from_millis(100);
        let signal = CameraSignal::with_timing(quiet, Duration::from_secs(60));
        let (tx, rx) = std::sync::mpsc::channel();
        let (fired_tx, fired_rx) = std::sync::mpsc::channel();
        spawn(signal, rx, move || {
            let _ = fired_tx.send(Instant::now());
        });

        let sent = Instant::now();
        tx.send(to(1)).unwrap();
        tx.send(CameraWrite::Unchanged).unwrap();
        let fired = fired_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("途切れたら知らせる");
        assert!(fired >= sent + quiet, "途切れる前に知らせた");
        assert!(fired_rx.recv_timeout(quiet * 3).is_err(), "1回だけ知らせる");

        drop(tx);
        // 送る側が落ちたら糸も終わり、知らせる口も落ちる
        assert!(matches!(
            fired_rx.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }
}
