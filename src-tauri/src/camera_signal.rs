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
//! `GROUP BY camera_id` を全件で回した。ここでは2つに分ける:
//!
//! - **左ペインにまだ出ていないカメラ**が埋まったら、**すぐ**知らせる。
//!   「出ているカメラ」は、最後に `list_cameras` が返した id の集合である
//!   （外したフォルダのカメラは、数え直しで一覧から落ちた時点で集合からも落ちる。
//!   だから足し直せば、また「まだ出ていない」になる）
//! - **出ているカメラ**の枚数は、書き込みが [`QUIET`] 途切れたときにまとめて知らせる。
//!   途切れないまま書き続けても、[`MAX_WAIT`] に1回は知らせる
//!   （新しいカメラが「1枚」のまま、何分も止まって見えないように）
//!
//! 判断は [`CameraSignal`] に閉じ込め、時刻を引数で受ける（試験で時計を動かすため）。
//! 糸と通り道は [`spawn`] が持つ。

use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lock_ok;

/// 書き込みがこれだけ途切れたら、出ているカメラの枚数を数え直させる。
pub const QUIET: Duration = Duration::from_secs(2);
/// 書き込みが途切れなくても、これ以上は待たせない。
pub const MAX_WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct CameraSignal {
    /// 最後に `list_cameras` が返したカメラの id（＝左ペインに出ているもの）
    listed: HashSet<i64>,
    /// 知らせていない書き込みのうち、最初と最後の時刻
    pending: Option<(Instant, Instant)>,
}

impl CameraSignal {
    /// `list_cameras` が返した id で、出ているカメラを置き換える。
    pub fn set_listed(&mut self, ids: impl IntoIterator<Item = i64>) {
        self.listed = ids.into_iter().collect();
    }

    /// 左ペインに出るカメラが1行に書かれた。**いま知らせるなら真。**
    pub fn on_write(&mut self, camera_id: i64, now: Instant) -> bool {
        if self.listed.insert(camera_id) {
            // 数え直しは他の書き込みのぶんも拾うので、溜めていたものも済んだことになる
            self.pending = None;
            return true;
        }
        let first = self.pending.map_or(now, |(first, _)| first);
        self.pending = Some((first, now));
        self.fire_if_due(now)
    }

    /// 次に [`Self::on_tick`] を呼ぶべき時刻。溜めていなければ `None`（待つだけ）。
    pub fn deadline(&self) -> Option<Instant> {
        self.pending
            .map(|(first, last)| (last + QUIET).min(first + MAX_WAIT))
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

/// 受け口の糸を立てる。`rx` に来るのは**左ペインに出る** `camera_id` だけ
/// （未確認の NULL と「カメラなし」は送る側で落とす）。
/// 送る側が全部落ちたら（アプリの終了）、糸も終わる。
pub fn spawn(
    signal: Arc<Mutex<CameraSignal>>,
    rx: Receiver<i64>,
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
                Ok(camera_id) => s.on_write(camera_id, now),
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

    #[test]
    fn a_camera_not_yet_listed_is_announced_at_once_and_only_once() {
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        s.set_listed([1]);
        assert!(s.on_write(2, t0), "左ペインに無いカメラはすぐ知らせる");
        assert_eq!(s.deadline(), None, "知らせたぶんは溜めない");
        assert!(
            !s.on_write(2, at(t0, 50)),
            "同じカメラの次の1枚では知らせない"
        );
        assert!(s.deadline().is_some(), "ただし枚数のために溜める");
    }

    #[test]
    fn a_listed_camera_is_recounted_once_the_writes_go_quiet() {
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        s.set_listed([1]);
        assert!(!s.on_write(1, t0));
        assert!(!s.on_write(1, at(t0, 500)));
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
        s.set_listed([1]);
        let step = QUIET.as_millis() as u64 / 2;
        let mut fired_at = Vec::new();
        let mut ms = 0;
        while ms <= 25_000 {
            if s.on_write(1, at(t0, ms)) {
                fired_at.push(ms);
            }
            ms += step;
        }
        let max = MAX_WAIT.as_millis() as u64;
        assert_eq!(fired_at, vec![max, 2 * max + step], "書き続けても10秒に1回");
    }

    #[test]
    fn a_camera_dropped_from_the_list_is_new_again() {
        // 実機の再現: フォルダを外す→数え直しで一覧から落ちる→足し直す
        let t0 = Instant::now();
        let mut s = CameraSignal::default();
        s.set_listed([1, 2]);
        assert!(!s.on_write(2, t0));
        s.set_listed([1]);
        assert!(s.on_write(2, at(t0, 10)));
    }

    #[test]
    fn nothing_is_pending_before_any_write() {
        let mut s = CameraSignal::default();
        assert_eq!(s.deadline(), None);
        assert!(!s.on_tick(Instant::now()));
    }

    #[test]
    fn the_thread_announces_and_ends_with_its_senders() {
        let signal = Arc::new(Mutex::new(CameraSignal::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        let (fired_tx, fired_rx) = std::sync::mpsc::channel();
        spawn(signal.clone(), rx, move || {
            let _ = fired_tx.send(());
        });
        tx.send(7).unwrap();
        fired_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("まだ出ていないカメラは知らせる");
        drop(tx);
        // 送る側が落ちたら糸も終わり、知らせる口も落ちる
        assert!(matches!(
            fired_rx.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }
}
