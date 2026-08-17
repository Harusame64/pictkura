//! 原寸表示用JPEG（[`crate::thumbs::display_jpeg`]）のバイト列キャッシュ。
//!
//! WebViewが描けない形式（HEIC・RAW・TIFF）の原寸表示は、要求のたびに
//! 展開してJPEGへ詰め直している。**実測でHEICは1枚1095ms**
//! （OSのWICデコード492ms＋24.5MPの再エンコード約600ms）、TIFFは約300ms。
//! ビューアの送りで前後へ行き来すると、同じファイルを何度もこの値段で作り直す。
//!
//! ここで持つのは**画素ではなくJPEGのバイト列**。実測でHEIC1枚の出力は
//! 3.29MBで、同じ絵をデコード済み画素で持つ93MiBより**30倍安い**。
//! 128MBの上限でも約38枚分入る。画素の側（＝先読みした隠し `<img>`）は
//! WebViewが総量660MiBの崖で全部まとめて捨てるので、
//! **落ちてきたときの受け皿**としてこの段が要る。
//!
//! 鍵に `mtime_ms` を含めるのは、ファイルが差し替わったときに古い絵を
//! 出し続けないため（配信URLの `?v=` と同じ考え方）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 上限の既定値（128MB）。実測のHEIC1枚3.29MBで約38枚分。
pub const DEFAULT_CAPACITY_BYTES: usize = 128 * 1024 * 1024;

/// 件数の上限。追い出す相手を線形探索で選ぶので、走査の長さを縛っておく。
/// 小さい絵ばかりで件数が伸びても、1回の追い出しはこの回数で頭打ちになる。
const MAX_ENTRIES: usize = 512;

/// キャッシュの鍵。同じidでもファイルが差し替われば別物として扱う。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub id: i64,
    pub mtime_ms: i64,
}

struct Entry {
    bytes: Arc<Vec<u8>>,
    /// 最後に使った順番（大きいほど新しい）
    used: u64,
}

struct Inner {
    entries: HashMap<Key, Entry>,
    /// 単調増加のカウンタ。実時刻を使わないのは、解像度の粗い時計で
    /// 同着が並ぶと追い出す相手が決まらなくなるため
    tick: u64,
    bytes: usize,
    capacity: usize,
}

/// (id, mtime) → 表示用JPEG のLRU。上限は**バイト数**で切る。
pub struct DisplayCache {
    inner: Mutex<Inner>,
}

impl DisplayCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: HashMap::new(),
                tick: 0,
                bytes: 0,
                capacity,
            }),
        }
    }

    /// あれば返し、同時に「いま使った」と記録する。
    pub fn get(&self, key: Key) -> Option<Arc<Vec<u8>>> {
        let mut inner = self.lock();
        inner.tick += 1;
        let tick = inner.tick;
        let entry = inner.entries.get_mut(&key)?;
        entry.used = tick;
        Some(Arc::clone(&entry.bytes))
    }

    /// 入れる。上限を超えたぶんは古いものから捨てる。
    ///
    /// **1件で上限を超えるものは入れない**——入れた瞬間に他を全部追い出して
    /// 自分も残らない、という無駄な往復になるため。
    pub fn insert(&self, key: Key, bytes: Arc<Vec<u8>>) {
        let len = bytes.len();
        let mut inner = self.lock();
        if len > inner.capacity {
            return;
        }
        inner.tick += 1;
        let tick = inner.tick;
        if let Some(old) = inner.entries.insert(key, Entry { bytes, used: tick }) {
            inner.bytes -= old.bytes.len();
        }
        inner.bytes += len;
        while inner.bytes > inner.capacity || inner.entries.len() > MAX_ENTRIES {
            let Some(victim) = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(k, _)| *k)
            else {
                break;
            };
            // いま入れたものを追い出しても意味が無い（上で上限内と確かめてある）
            if victim == key {
                break;
            }
            if let Some(e) = inner.entries.remove(&victim) {
                inner.bytes -= e.bytes.len();
            }
        }
    }

    /// 保持している件数と合計バイト数（テストと将来の計測用）。
    pub fn stats(&self) -> (usize, usize) {
        let inner = self.lock();
        (inner.entries.len(), inner.bytes)
    }

    /// 毒された錠でも中身は壊れていない（HashMapの整合性は保たれている）ので、
    /// 表示のためのキャッシュを理由にアプリを落とさない。
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for DisplayCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(id: i64) -> Key {
        Key { id, mtime_ms: 1 }
    }
    fn blob(n: usize) -> Arc<Vec<u8>> {
        Arc::new(vec![0u8; n])
    }

    #[test]
    fn 入れたものが取り出せる() {
        let c = DisplayCache::new(1000);
        c.insert(key(1), blob(10));
        assert_eq!(c.get(key(1)).map(|b| b.len()), Some(10));
        assert_eq!(c.stats(), (1, 10));
    }

    #[test]
    fn 更新日時が違えば別物として扱う() {
        let c = DisplayCache::new(1000);
        c.insert(
            Key {
                id: 1,
                mtime_ms: 100,
            },
            blob(10),
        );
        // 差し替わったファイルに古い絵を返してはいけない
        assert!(c
            .get(Key {
                id: 1,
                mtime_ms: 200
            })
            .is_none());
    }

    #[test]
    fn 上限を超えたら古いものから捨てる() {
        let c = DisplayCache::new(100);
        c.insert(key(1), blob(40));
        c.insert(key(2), blob(40));
        c.insert(key(3), blob(40)); // 合計120 > 100 なので1件落ちる
        assert!(c.get(key(1)).is_none());
        assert!(c.get(key(2)).is_some());
        assert!(c.get(key(3)).is_some());
        assert_eq!(c.stats(), (2, 80));
    }

    #[test]
    fn 直近に使ったものは残る() {
        let c = DisplayCache::new(100);
        c.insert(key(1), blob(40));
        c.insert(key(2), blob(40));
        // 1を触って新しくしてから、3を入れて追い出しを起こす
        assert!(c.get(key(1)).is_some());
        c.insert(key(3), blob(40));
        assert!(c.get(key(1)).is_some());
        assert!(c.get(key(2)).is_none());
    }

    #[test]
    fn 上限より大きいものは入れない() {
        let c = DisplayCache::new(100);
        c.insert(key(1), blob(40));
        c.insert(key(2), blob(200));
        // 巨大な1件のために既存を全部捨てる、が起きないこと
        assert!(c.get(key(2)).is_none());
        assert!(c.get(key(1)).is_some());
        assert_eq!(c.stats(), (1, 40));
    }

    #[test]
    fn 同じ鍵の入れ直しで合計が二重に増えない() {
        let c = DisplayCache::new(1000);
        c.insert(key(1), blob(10));
        c.insert(key(1), blob(30));
        assert_eq!(c.stats(), (1, 30));
    }
}
