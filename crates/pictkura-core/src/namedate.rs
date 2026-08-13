//! ファイル名から撮影日時を推測する（第9部 段階H-2）。
//!
//! EXIFもOSのプロパティも何も返さなかったときの**最後の手段**。
//! それでも無ければ mtime に落ちるが、mtime は同期やコピーの日であることが多く、
//! 撮影日としては当てにならない。実測（ライブラリ6,685件）:
//!
//! - **213件**が OneDrive の同期日（2025-07-07）で並んでいた。EXIFもOSの
//!   プロパティも持たない保存画像（LINE・スクリーンショットの類）で、
//!   **ファイル名だけが正しい日付を知っている**
//! - ファイル名に日付があるのは 5,613件（83%）。日付が別途分かっている
//!   5,276件と突き合わせると 96% が一致した＝名前は信用してよい
//!
//! **時間帯の扱いが肝**。同じ「20220121_020918」でも、OneDriveのカメラロール
//! （`_iOS` で終わる命名）は**UTC**で、Android やスクリーンショットの命名は
//! **現地時刻**で書く。混ぜると時差ぶんずれる。実測では `_iOS` 命名 5,146件のうち
//! **5,145件（99.98%）がUTC**だった（残り1件は撮影地の時間帯が違うと思われる）。
//!
//! **日付だけの名前は採らない**。`IMG-20240101-WA0001` のように時刻を持たない
//! 命名もあるが、時刻を0時に決め打つとその日の先頭へ並んでしまい、
//! 「日付は合っているが並びは嘘」になる。取り違えたときの害が大きいわりに
//! 得るものが少ないので、**日付と時刻が揃っている名前だけ**を扱う。

use std::path::Path;

/// 年として受け入れる下限。デジタルカメラより前の年は
/// 「たまたま数字が並んだだけ」とみなす
const MIN_YEAR: i32 = 1990;

/// 名前から拾った年月日時分秒。
type Stamp = (i32, u32, u32, u32, u32, u32);

/// 名前から撮影日時を推測する（エポックミリ秒）。
///
/// 拾えなければ `None`。**既に分かっている撮影日時を上書きする用途ではない**
/// ——呼び出し側は EXIF → OSのプロパティ → ここ → mtime の順で落とすこと。
pub fn guess_taken_at(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    let stamp = find_stamp(stem)?;
    // OneDriveのカメラロールは UTC で名前を付ける（実測 5,145/5,146）
    to_epoch_ms(stamp, is_utc_naming(stem))
}

/// この名前は UTC で書かれているか。
///
/// **取り込み時の衝突リネームを剥がしてから見る**。同名で中身の違うファイルを
/// 取り込むと `resolve_dest_path` が `..._iOS-1.jpg` のように連番を足すので、
/// 語尾をそのまま見ると現地時刻と判定され、**時差ぶんずれた日に並ぶ**。
fn is_utc_naming(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    let base = match lower.rsplit_once('-') {
        // 剥がすのは連番だけ。日付の区切りに使われた `-` を剥がさない
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => lower.as_str(),
    };
    base.ends_with("_ios")
}

/// エポックミリ秒へ直す。未来の日付は弾く。
fn to_epoch_ms(stamp: Stamp, utc: bool) -> Option<i64> {
    use chrono::TimeZone;
    let (y, mo, d, h, mi, s) = stamp;
    let ms = if utc {
        chrono::Utc.with_ymd_and_hms(y, mo, d, h, mi, s).single()?
    } else {
        // 夏時間の切り替えで二重になる時刻は早い方を採る。
        // 存在しない時刻（時計が飛ぶ側）は拾えないので None になる
        chrono::Local
            .with_ymd_and_hms(y, mo, d, h, mi, s)
            .earliest()?
            .into()
    }
    .timestamp_millis();
    // 明日より先になるなら名前の数字を読み違えている
    let limit = chrono::Utc::now().timestamp_millis() + 24 * 60 * 60 * 1000;
    (ms > 0 && ms < limit).then_some(ms)
}

/// 名前の中から「日付＋時刻」を1つ探す。
///
/// 区切り文字は何でもよい（`-` `_` `.` `:` 空白 `T`、無しでも可）ので、
/// `20240101_123456` も `2024-01-01 12.34.56` も `IMG_20240101_123456` も拾える。
fn find_stamp(stem: &str) -> Option<Stamp> {
    let bytes = stem.as_bytes();
    for start in 0..bytes.len() {
        // 直前が数字なら、長い数字列の途中を年として切り出そうとしている
        if start > 0 && bytes[start - 1].is_ascii_digit() {
            continue;
        }
        if let Some(stamp) = parse_at(bytes, start) {
            return Some(stamp);
        }
    }
    None
}

/// `at` から「年月日 時分秒」を読む。
fn parse_at(bytes: &[u8], at: usize) -> Option<Stamp> {
    let (year, at) = digits(bytes, at, 4)?;
    let year = i32::try_from(year).ok()?;
    if year < MIN_YEAR {
        return None;
    }
    let at = separators(bytes, at);
    let (month, at) = digits(bytes, at, 2)?;
    let at = separators(bytes, at);
    let (day, at) = digits(bytes, at, 2)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // 日付と時刻の間に区切りが入る形も、続けて書く形もある
    let at = separators(bytes, at);
    let (hour, at) = digits(bytes, at, 2)?;
    let at = separators(bytes, at);
    let (minute, at) = digits(bytes, at, 2)?;
    let at = separators(bytes, at);
    let (second, at) = digits(bytes, at, 2)?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // 秒の後ろに続く数字はミリ秒まで（iOSは3桁）。それ以上続くなら、
    // 日付ではない長い数字列を切り刻んでいる
    let extra = bytes[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if extra > 3 {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

/// `at` からちょうど `n` 桁の数字を読む。
fn digits(bytes: &[u8], at: usize, n: usize) -> Option<(u32, usize)> {
    let end = at.checked_add(n)?;
    let slice = bytes.get(at..end)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0u32;
    for b in slice {
        value = value * 10 + u32::from(b - b'0');
    }
    Some((value, end))
}

/// 区切り文字を読み飛ばす。
fn separators(bytes: &[u8], at: usize) -> usize {
    let mut at = at;
    while matches!(
        bytes.get(at),
        Some(b'-' | b'_' | b'.' | b':' | b' ' | b'T' | b't')
    ) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn guess(name: &str) -> Option<i64> {
        guess_taken_at(&PathBuf::from(name))
    }

    fn local_ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
        chrono::Local
            .with_ymd_and_hms(y, mo, d, h, mi, s)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    fn utc_ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
        chrono::Utc
            .with_ymd_and_hms(y, mo, d, h, mi, s)
            .single()
            .unwrap()
            .timestamp_millis()
    }

    /// OneDriveのカメラロール命名は**UTC**（実測 5,145/5,146）
    #[test]
    fn reads_onedrive_camera_roll_as_utc() {
        assert_eq!(
            guess("20220121_020918000_iOS.jpg"),
            Some(utc_ms(2022, 1, 21, 2, 9, 18))
        );
        // 拡張子の大小が違っても同じ扱い
        assert_eq!(
            guess("20191218_025219000_iOS.MOV"),
            Some(utc_ms(2019, 12, 18, 2, 52, 19))
        );
    }

    /// 取り込みの衝突リネーム（`_iOS-1`）でもUTCのまま
    #[test]
    fn keeps_utc_naming_through_collision_renames() {
        let plain = guess("20220121_020918000_iOS.jpg").unwrap();
        assert_eq!(guess("20220121_020918000_iOS-1.jpg"), Some(plain));
        assert_eq!(guess("20220121_020918000_iOS-12.jpg"), Some(plain));
        // 連番でない語尾は剥がさない（別の命名を巻き込まない）
        assert_ne!(guess("20220121_020918000_iOS-copy.jpg"), Some(plain));
    }

    /// `_iOS` が付かない命名は現地時刻
    #[test]
    fn reads_other_namings_as_local_time() {
        for name in [
            "IMG_20240101_123456.jpg",
            "VID_20240101_123456.mp4",
            "PXL_20240101_123456789.jpg",
            "Screenshot_2024-01-01-12-34-56.png",
            "2024-01-01 12.34.56.jpg",
            "スクリーンショット 2024-01-01 12.34.56.png",
        ] {
            assert_eq!(
                guess(name),
                Some(local_ms(2024, 1, 1, 12, 34, 56)),
                "{name} を読めていない"
            );
        }
    }

    /// 同じ数字でも `_iOS` の有無で時差ぶん変わる
    #[test]
    fn separates_utc_naming_from_local_naming() {
        let utc = guess("20240101_123456000_iOS.jpg").unwrap();
        let local = guess("20240101_123456.jpg").unwrap();
        // 同じ壁時計の数字でも、UTCとして読むぶんだけ後ろの瞬間になる
        assert_eq!(utc - local, local_offset_ms(2024, 1, 1));
    }

    fn local_offset_ms(y: i32, mo: u32, d: u32) -> i64 {
        use chrono::Offset;
        let at = chrono::Local
            .with_ymd_and_hms(y, mo, d, 12, 0, 0)
            .earliest()
            .unwrap();
        i64::from(at.offset().fix().local_minus_utc()) * 1000
    }

    /// 日付だけの名前は採らない（時刻を0時に決め打つと並びが嘘になる）
    #[test]
    fn ignores_names_without_a_time() {
        assert_eq!(guess("IMG-20240101-WA0001.jpg"), None);
        assert_eq!(guess("2024-01-01.jpg"), None);
        assert_eq!(guess("20240101.png"), None);
    }

    /// 日付でない数字の並びを日付として読まない
    #[test]
    fn ignores_numbers_that_are_not_dates() {
        // 連番・IDの類
        assert_eq!(guess("DSC01234.JPG"), None);
        assert_eq!(guess("IMG_5678.JPG"), None);
        // 桁が続きすぎるものは長い数字列の切り刻み
        assert_eq!(guess("20240101123456789012.jpg"), None);
        // 月・日・時・分・秒として成立しない値
        assert_eq!(guess("20241301_123456.jpg"), None);
        assert_eq!(guess("20240132_123456.jpg"), None);
        assert_eq!(guess("20240101_243456.jpg"), None);
        assert_eq!(guess("20240101_126056.jpg"), None);
        assert_eq!(guess("20240101_123460.jpg"), None);
    }

    /// デジタルカメラより前の年は拾わない
    #[test]
    fn ignores_years_before_digital_cameras() {
        assert_eq!(guess("19240101_123456.jpg"), None);
    }

    /// 未来の日付は数字の読み違い
    #[test]
    fn ignores_dates_in_the_future() {
        let year: i32 = chrono::Local::now()
            .format("%Y")
            .to_string()
            .parse()
            .unwrap();
        assert_eq!(guess(&format!("{}0101_123456.jpg", year + 1)), None);
    }

    /// 長い数字列の途中から年を切り出さない
    #[test]
    fn does_not_start_inside_a_longer_number() {
        // 先頭の1を飛ばして 2024… から読むと日付になってしまう
        assert_eq!(guess("1202401011234561.jpg"), None);
    }

    /// 名前が無い・読めないパスでも落ちない
    #[test]
    fn survives_odd_paths() {
        assert_eq!(guess(""), None);
        assert_eq!(guess(".jpg"), None);
        assert_eq!(guess("写真.jpg"), None);
    }
}
