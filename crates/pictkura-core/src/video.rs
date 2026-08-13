//! 動画（第9部）。
//!
//! **画素は自前でデコードしない**という点でRAWやHEIFと同じ方針を取る。
//! 一覧に出す絵はOSのサムネイル機構から借り、再生はWebViewに任せる。
//! HEVCのデコーダは**配らない**——OSが持っていれば使い、無ければ案内を出す
//! （HEICで既に決めた線と同じ。特許の当事者にならずに済む）。
//!
//! ここが持つのは「コンテナを読んで長さ・寸法・撮影日時を知る」ところまで。
//! MP4/MOV は HEIF と同じ ISO-BMFF なので、[`crate::heif`] の箱読みを流用する。

use std::path::Path;

/// 走査・取り込みの対象にする動画の拡張子。
///
/// 一覧に出すかどうかと、アプリ内で再生できるかは**別**
/// （[`plays_in_webview`] を参照）。ここに無い形式はそもそも見つけられないので、
/// 再生できないものも入れておく。
pub const VIDEO_EXTENSIONS: &[&str] = &[
    // WebViewがそのまま再生できるコンテナ
    "mp4", "m4v", "mov", "webm",
    // コンテナが合わずWebViewでは再生できない。一覧には出す
    "avi", "mts", "m2ts", "mkv", "3gp", "wmv", "mpg", "mpeg",
];

/// 拡張子が動画か。
pub fn is_video_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    VIDEO_EXTENSIONS.contains(&lower.as_str())
}

/// パスの拡張子が動画か。
pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_video_extension)
}

/// WebViewの `<video>` がこの**コンテナ**を扱えるか。
///
/// 中身のコーデックまでは保証しない（HEVCはOSのデコーダ次第）。
/// ここが偽なら再生はOSの既定のアプリへ渡すしかない
/// ——MPEG-TS（.mts/.m2ts）やAVIはChromiumがコンテナごと相手にしない。
pub fn plays_in_webview(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "m4v" | "mov" | "webm")
    )
}

/// MP4/MOV（ISO-BMFF）か。`moov` を読める形式。
pub fn is_bmff_video_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "m4v" | "mov" | "3gp")
    )
}

/// 動画の素性。読めなかった項目は既定値のまま。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VideoInfo {
    /// 表示上の幅（回転を適用した後）
    pub width: u32,
    /// 表示上の高さ（回転を適用した後）
    pub height: u32,
    /// 長さ（ミリ秒）
    pub duration_ms: Option<i64>,
    /// 撮影日時（エポックからのミリ秒）
    pub taken_at_ms: Option<i64>,
}

/// `moov` の読み取り上限。無制限にすると、巨大な索引（`stbl`）を持つ
/// 長尺動画でメモリを持っていかれる
const MOOV_LIMIT: u64 = 32 * 1024 * 1024;

/// 1904-01-01 から 1970-01-01 までの秒数。
/// BMFFの時刻はこのApple由来の起点を使う
const EPOCH_1904_TO_1970: i64 = 2_082_844_800;

/// コンテナから長さ・寸法・撮影日時を読む（**画素は読まない**）。
///
/// MP4/MOV以外（AVI・MPEG-TS・Matroska）は `None`。
/// その場合の撮影日時はファイルのmtimeに落ちる。
pub fn read_info(path: &Path) -> Option<VideoInfo> {
    if !is_bmff_video_path(path) {
        return None;
    }
    let moov = crate::heif::read_top_level_box(path, b"moov", MOOV_LIMIT)?;
    let end = moov.len();

    let mut info = VideoInfo::default();

    // mvhd: 全体の長さと作成日時
    if let Some((b, e)) = crate::heif::find_box(&moov, 0, end, b"mvhd") {
        if let Some((duration_ms, created)) = parse_mvhd(&moov[b..e]) {
            info.duration_ms = duration_ms;
            info.taken_at_ms = created;
        }
    }

    // 映像トラックの tkhd から表示上の寸法を取る
    if let Some((w, h)) = video_track_dimensions(&moov, end) {
        info.width = w;
        info.height = h;
    }

    // udta の日付があれば、そちらを撮影日時として優先する。
    // mvhd の時刻は「UTCのはず」だが、iPhone等は**現地時刻をそのまま**書く。
    // udta 側は時差付きのISO 8601なので、こちらの方が信用できる
    if let Some(ms) = creation_date_from_udta(&moov, end) {
        info.taken_at_ms = Some(ms);
    }

    Some(info)
}

/// `mvhd` から `(長さms, 作成日時ms)` を読む。
fn parse_mvhd(body: &[u8]) -> Option<(Option<i64>, Option<i64>)> {
    let version = *body.first()?;
    // version(1) + flags(3) の後ろから
    let (created, timescale, duration) = if version == 1 {
        if body.len() < 32 {
            return None;
        }
        (
            i64::from_be_bytes(body[4..12].try_into().ok()?),
            u32::from_be_bytes(body[20..24].try_into().ok()?),
            i64::from_be_bytes(body[24..32].try_into().ok()?),
        )
    } else {
        if body.len() < 20 {
            return None;
        }
        (
            i64::from(u32::from_be_bytes(body[4..8].try_into().ok()?)),
            u32::from_be_bytes(body[12..16].try_into().ok()?),
            i64::from(u32::from_be_bytes(body[16..20].try_into().ok()?)),
        )
    };
    if timescale == 0 {
        return None;
    }
    // 「不明」を表す番兵を長さとして採らない。v0で 0xFFFFFFFF、
    // 断片化MP4（fMP4）では0が入る。前者をそのまま採ると
    // 49日の動画ができ、後者は0秒として表示される
    let unknown = duration <= 0 || duration == i64::from(u32::MAX) || duration == i64::MAX;
    let duration_ms = if unknown {
        None
    } else {
        Some(duration.checked_mul(1000)? / i64::from(timescale))
    };
    // 0 は「書いていない」ことが多い。1970年より前の値も信用しない
    let created_ms = (created > 0)
        .then(|| (created - EPOCH_1904_TO_1970).checked_mul(1000))
        .flatten()
        .filter(|ms| *ms > 0);
    Some((duration_ms, created_ms))
}

/// 映像トラックを探し、`tkhd` から表示上の寸法を返す。
fn video_track_dimensions(moov: &[u8], end: usize) -> Option<(u32, u32)> {
    for (kind, tb, te) in crate::heif::boxes(moov, 0, end) {
        if &kind != b"trak" || !is_video_track(moov, tb, te) {
            continue;
        }
        // 映像トラックは見つかったので、寸法が読めなくてもここで打ち切る
        let (hb, he) = crate::heif::find_box(moov, tb, te, b"tkhd")?;
        return parse_tkhd(&moov[hb..he]);
    }
    None
}

/// `trak` の中の `mdia/hdlr` を見て、映像トラックか判定する。
///
/// 音声トラックの `tkhd` は寸法が0なので紛れ込んでも実害は無いが、
/// 字幕やメタデータのトラックまで拾うと当てが外れる。
fn is_video_track(moov: &[u8], start: usize, end: usize) -> bool {
    let Some((mb, me)) = crate::heif::find_box(moov, start, end, b"mdia") else {
        return false;
    };
    let Some((hb, he)) = crate::heif::find_box(moov, mb, me, b"hdlr") else {
        return false;
    };
    // version(1) flags(3) pre_defined(4) handler_type(4)
    he >= hb + 12 && moov.get(hb + 8..hb + 12) == Some(&b"vide"[..])
}

/// `tkhd` から表示上の幅・高さを読む（回転を適用した後の値）。
fn parse_tkhd(body: &[u8]) -> Option<(u32, u32)> {
    let version = *body.first()?;
    // version(1) flags(3) の後ろ。v1 は時刻と長さが64ビットになる
    //   v0: created(4) modified(4) track_id(4) reserved(4) duration(4)
    //   v1: created(8) modified(8) track_id(4) reserved(4) duration(8)
    let after_times = if version == 1 { 4 + 32 } else { 4 + 20 };
    // reserved(8) layer(2) alternate_group(2) volume(2) reserved(2)
    let matrix_at = after_times + 16;
    let width_at = matrix_at + 36;
    if body.len() < width_at + 8 {
        return None;
    }
    // 16.16の固定小数。小数部は端数なので整数部だけ見る
    let w = u32::from_be_bytes(body[width_at..width_at + 4].try_into().ok()?) >> 16;
    let h = u32::from_be_bytes(body[width_at + 4..width_at + 8].try_into().ok()?) >> 16;
    if w == 0 || h == 0 {
        return None;
    }
    // 回転行列の a と b（先頭2要素）だけで90度系かどうかは分かる。
    // 縦持ちのiPhone動画は 1920x1080 で格納し、行列で90度回して見せる
    let a = i32::from_be_bytes(body[matrix_at..matrix_at + 4].try_into().ok()?);
    let b = i32::from_be_bytes(body[matrix_at + 4..matrix_at + 8].try_into().ok()?);
    let quarter_turn = a == 0 && b != 0;
    Some(if quarter_turn { (h, w) } else { (w, h) })
}

/// `udta` の日付（時差付きの撮影日時）を読む。
///
/// QuickTimeの箱名は先頭が著作権記号（0xA9）の `(c)day`。
fn creation_date_from_udta(moov: &[u8], end: usize) -> Option<i64> {
    let (ub, ue) = crate::heif::find_box(moov, 0, end, b"udta")?;
    let (db, de) = crate::heif::find_box(moov, ub, ue, &[0xA9, b'd', b'a', b'y'])?;
    let body = moov.get(db..de)?;
    // QuickTimeの文字列は 長さ(2) 言語(2) の後ろに本体が続く
    if body.len() <= 4 {
        return None;
    }
    let text = std::str::from_utf8(&body[4..]).ok()?;
    parse_iso8601(text.trim().trim_end_matches('\0'))
}

/// 時差付きのISO 8601をエポックミリ秒へ。
///
/// Appleは `2024-10-05T12:44:56+0900` のように**コロン無しの時差**を書く。
/// RFC 3339（`+09:00`）しか受けない解析では落ちるので両方試す。
fn parse_iso8601(text: &str) -> Option<i64> {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .or_else(|| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%z").ok())
        .map(|dt| dt.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_video_extensions() {
        assert!(is_video_path(Path::new("a.MP4")));
        assert!(is_video_path(Path::new("a.m2ts")));
        assert!(!is_video_path(Path::new("a.jpg")));
        assert!(!is_video_path(Path::new("a")));
    }

    /// 一覧に出す形式と、アプリ内で再生できる形式は別
    #[test]
    fn separates_listing_from_playback() {
        assert!(plays_in_webview(Path::new("a.mp4")));
        assert!(plays_in_webview(Path::new("a.MOV")));
        // コンテナがMPEG-TS/AVIのものはChromiumが扱えない
        assert!(is_video_path(Path::new("a.m2ts")) && !plays_in_webview(Path::new("a.m2ts")));
        assert!(is_video_path(Path::new("a.avi")) && !plays_in_webview(Path::new("a.avi")));
    }

    #[test]
    fn parses_mvhd_v0() {
        // version0 flags0 / created / modified / timescale=600 / duration=1200
        let mut body = vec![0u8; 20];
        let created = EPOCH_1904_TO_1970 + 86_400; // 1970-01-02
        body[4..8].copy_from_slice(&(created as u32).to_be_bytes());
        body[12..16].copy_from_slice(&600u32.to_be_bytes());
        body[16..20].copy_from_slice(&1200u32.to_be_bytes());
        let (duration, created_ms) = parse_mvhd(&body).unwrap();
        assert_eq!(duration, Some(2000), "600分の1200＝2秒のはず");
        assert_eq!(created_ms, Some(86_400_000));
    }

    /// 1970年以前になる時刻は「書いていない」とみなす。
    /// 0 のままの動画は珍しくなく、そのまま採ると1904年の山ができる
    #[test]
    fn treats_implausible_creation_time_as_missing() {
        let mut body = vec![0u8; 20];
        body[12..16].copy_from_slice(&600u32.to_be_bytes());
        // created = 0（未設定）
        assert_eq!(parse_mvhd(&body).unwrap().1, None);
        // created = ちょうど1970-01-01（実在の撮影日時ではありえない）
        body[4..8].copy_from_slice(&(EPOCH_1904_TO_1970 as u32).to_be_bytes());
        assert_eq!(parse_mvhd(&body).unwrap().1, None);
    }

    #[test]
    fn parses_mvhd_v1() {
        let mut body = vec![0u8; 32];
        body[0] = 1;
        body[4..12].copy_from_slice(&(EPOCH_1904_TO_1970 + 86400).to_be_bytes());
        body[20..24].copy_from_slice(&1000u32.to_be_bytes());
        body[24..32].copy_from_slice(&5500i64.to_be_bytes());
        let (duration, created) = parse_mvhd(&body).unwrap();
        assert_eq!(duration, Some(5500));
        assert_eq!(created, Some(86_400_000));
    }

    /// 「不明」を表す長さは採らない
    #[test]
    fn rejects_sentinel_durations() {
        let build = |duration: u32| {
            let mut body = vec![0u8; 20];
            body[12..16].copy_from_slice(&1000u32.to_be_bytes());
            body[16..20].copy_from_slice(&duration.to_be_bytes());
            parse_mvhd(&body).unwrap().0
        };
        assert_eq!(build(u32::MAX), None, "0xFFFFFFFFは不明の印");
        assert_eq!(build(0), None, "0は断片化MP4などで入る");
        assert_eq!(build(2500), Some(2500));
    }

    /// timescale が0のファイルでゼロ除算しない
    #[test]
    fn survives_zero_timescale() {
        let body = vec![0u8; 20];
        assert!(parse_mvhd(&body).is_none());
    }

    /// 短すぎる mvhd で範囲外を読まない
    #[test]
    fn survives_truncated_mvhd() {
        assert!(parse_mvhd(&[0u8; 3]).is_none());
        assert!(parse_mvhd(&[1u8, 0, 0, 0]).is_none());
    }

    /// 縦持ちの動画は、格納寸法と表示寸法が入れ替わる
    #[test]
    fn tkhd_applies_quarter_turn() {
        let build = |a: i32, b: i32, w: u32, h: u32| {
            let matrix_at = 4 + 20 + 16;
            let width_at = matrix_at + 36;
            let mut body = vec![0u8; width_at + 8];
            body[matrix_at..matrix_at + 4].copy_from_slice(&a.to_be_bytes());
            body[matrix_at + 4..matrix_at + 8].copy_from_slice(&b.to_be_bytes());
            body[width_at..width_at + 4].copy_from_slice(&(w << 16).to_be_bytes());
            body[width_at + 4..width_at + 8].copy_from_slice(&(h << 16).to_be_bytes());
            body
        };
        // 回転なし（a=1.0, b=0）
        assert_eq!(
            parse_tkhd(&build(0x0001_0000, 0, 1920, 1080)),
            Some((1920, 1080))
        );
        // 90度（a=0, b=1.0）→ 入れ替わる
        assert_eq!(
            parse_tkhd(&build(0, 0x0001_0000, 1920, 1080)),
            Some((1080, 1920))
        );
    }

    #[test]
    fn parses_apple_style_timestamp() {
        // コロン無しの時差（Appleの書き方）
        let ms = parse_iso8601("2024-10-05T12:44:56+0900").unwrap();
        // 同じ瞬間をRFC 3339で書いたものと一致する
        assert_eq!(ms, parse_iso8601("2024-10-05T12:44:56+09:00").unwrap());
        assert!(parse_iso8601("なんでもない文字列").is_none());
    }

    /// 動画でないファイルを渡しても落ちない
    #[test]
    fn returns_none_for_non_video() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.mp4");
        std::fs::write(&p, b"not a real mp4").unwrap();
        assert!(read_info(&p).is_none());
    }
}
