//! 連写を見分ける材料（dev #32、`dev/adr.grid-stacks.md` の PR 2）。
//!
//! 一覧で連写を1枚に重ねるには、**同じ機体で続けて撮った**ことが分かる必要がある:
//!
//! - **秒未満の撮影時刻**——EXIF の `DateTimeOriginal` は秒までしか持たない。
//!   `SubSecTimeOriginal` を足さないと、1秒に2〜4コマの連写は同じ時刻に潰れ、
//!   間隔で切れない（2026-09-25 の実測: Canon の最大間隔 0.85 秒）
//! - **機体**——機種名（`camera_id`）と、あれば**本体シリアル**（`BodySerialNumber`）。
//!   同じ機種を2台使っていても混ぜない。iPhone はシリアルを持たないので機種まで
//!
//! ここは値の読み方と鍵の作り方だけを持つ。DB に書くのは `Db::set_shot_meta`、
//! 束ねるのは UI（`ui/src/stacks.ts`）。

/// EXIF の `SubSecTime*`（10進の数字の並び）をミリ秒へ直す。
///
/// **小数点以下の桁として読む**——`"48"` は 0.48 秒＝480ms、`"664"` は 664ms、`"5"` は 500ms。
/// 4桁以上は切り捨てる。数字以外（空白・NUL）は前後を落とし、途中に混ざっていたら読まない
/// （壊れた値で時刻をずらすより、秒までに留める）。
pub fn subsec_ms(raw: &str) -> Option<i64> {
    let s = raw.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits: String = s.chars().take(3).collect();
    while digits.len() < 3 {
        digits.push('0');
    }
    digits.parse().ok()
}

/// 機体の鍵（一覧へ送る数値。JS の数値で正確な 53 bit に畳む）。
///
/// **機種が分からない行は 0**（束ねない）。シリアルが無い機体は機種だけで鍵にする
/// ——同じ機種の2台が同じ瞬間に撮ると混ざりうる（iPhone はシリアルを持たない）。
/// `camera_id` 0（「確認済み・カメラなし」）も 0
pub fn body_key(camera_id: Option<i64>, serial: Option<&str>) -> i64 {
    use std::hash::{Hash, Hasher};
    let Some(camera) = camera_id.filter(|c| *c > 0) else {
        return 0;
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    camera.hash(&mut h);
    serial.map(str::trim).filter(|s| !s.is_empty()).hash(&mut h);
    // 0 は「分からない」に取ってあるので、畳んだ値が 0 になったら 1 にする
    ((h.finish() & ((1u64 << 53) - 1)) as i64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_second_digits_are_read_as_a_fraction() {
        assert_eq!(subsec_ms("48"), Some(480));
        assert_eq!(subsec_ms("664"), Some(664));
        assert_eq!(subsec_ms("5"), Some(500));
        assert_eq!(subsec_ms("03"), Some(30));
        assert_eq!(subsec_ms("1234"), Some(123), "4桁目以降は切り捨て");
        assert_eq!(subsec_ms("48\0"), Some(480));
        assert_eq!(subsec_ms(" 17 "), Some(170));
        assert_eq!(subsec_ms(""), None);
        assert_eq!(subsec_ms("4a"), None, "数字以外が混ざったら読まない");
        assert_eq!(subsec_ms("-1"), None);
    }

    #[test]
    fn the_body_key_separates_bodies_and_refuses_unknown_cameras() {
        let a = body_key(Some(7), Some("051022000405"));
        assert_eq!(
            a,
            body_key(Some(7), Some("051022000405")),
            "同じ機体は同じ鍵"
        );
        assert_ne!(
            a,
            body_key(Some(7), Some("041041000834")),
            "同じ機種の別の本体"
        );
        assert_ne!(a, body_key(Some(8), Some("051022000405")), "別の機種");
        assert_eq!(
            body_key(Some(7), None),
            body_key(Some(7), Some("  ")),
            "空のシリアルは無いと同じ"
        );
        assert_eq!(body_key(None, Some("x")), 0, "機種が分からない");
        assert_eq!(body_key(Some(0), None), 0, "確認済み・カメラなし");
        for k in [
            a,
            body_key(Some(7), None),
            body_key(Some(123_456), Some("z")),
        ] {
            assert!((1..(1i64 << 53)).contains(&k), "{k}");
        }
    }
}
