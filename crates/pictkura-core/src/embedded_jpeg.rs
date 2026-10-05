//! RAW だけのカットを Google フォトへ送るための JPEG（`dev/plan.google-photos-at-import.md` §2・§4b）。
//!
//! 中身は**カメラが埋め込んだ表示用の JPEG**——ビューアが描いているもの
//! （[`crate::thumbs::raw_display_jpeg`]）と同じ絵で、現像はしない。Google は RAW そのものを
//! 節約画質の JPEG に詰め直す（win の S9）ので、手元で埋め込みの絵を出すほうが素直
//! （2026-10-05 利用者決定）。
//!
//! **撮影日時を書き込む。** 埋め込みの JPEG は EXIF を持たないことが多く、そのままだと
//! Google フォトは取り出した日に並べる。家族のアルバムで撮った日に並ぶよう、RAW の
//! `DateTimeOriginal` を JPEG に入れる。

use std::path::Path;

/// [`for_google`] が絵を返せなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// **ファイルを最後まで見たうえで**絵が無い（ビューアでも枠だけの機種）。何度やっても同じ
    NoPreview,
    /// 読み切れなかった（共有ロック・入出力の誤り等）。あとで読めば出るかもしれない（ゲート1）
    Unreadable,
}

/// `raw` の埋め込み JPEG を、向きを直し撮影日時を入れて返す。
///
/// - 向きは [`crate::thumbs::raw_display_jpeg`] と同じに直す（向きが1でなければ画素を回して
///   詰め直す。そのとき元の EXIF は落ちる）
/// - **JPEG が自分の EXIF を持っていれば触らない**——カメラが書いたもの（RAF の埋め込み等）で、
///   撮影日時もカメラ名も実体と同じものが入っている。回した絵は EXIF を持たないので、
///   向きの印と画素が食い違うことはない
/// - EXIF が無ければ撮影日時だけを入れる。日時が読めなければ絵だけを返す
pub fn for_google(raw: &Path) -> Result<Vec<u8>, Missing> {
    let exif = crate::thumbs::read_exif(raw);
    let Some(preview) = exif.thumbnail else {
        // 読めなかっただけの空振りを「絵が無い」と決めつけない（[`crate::thumbs::ExifData::preview_exhausted`]）
        return Err(if exif.preview_exhausted {
            Missing::NoPreview
        } else {
            Missing::Unreadable
        });
    };
    // 切手ほどの絵（IFD1 の 160x120 等）を写真として送らない。大きいプレビューを探し損ねた
    // ときにも、これが残っていることがある（ゲート2）
    if preview_long_edge(&preview) < MIN_LONG_EDGE {
        return Err(Missing::NoPreview);
    }
    let jpeg = if exif.orientation == 1 {
        preview
    } else {
        // 絵として読めないプレビューは、何度読んでも同じ
        crate::thumbs::rotate_raw_preview(&preview, exif.orientation).ok_or(Missing::NoPreview)?
    };
    if has_exif(&jpeg) {
        return Ok(jpeg);
    }
    Ok(match exif.taken_at_ms.and_then(exif_datetime) {
        Some(dt) => with_capture_time(&jpeg, &dt).unwrap_or(jpeg),
        None => jpeg,
    })
}

/// 送る絵の長辺の下限。これより小さいプレビューしか持たない RAW（古い機種・中判の一部）は
/// 「絵が無い」として送らない——家族のアルバムに切手が並ぶだけになる
/// （`dev/google-embedded-jpeg-mac-20261006.tsv`: 60本中、640px 以下は古い機種と中判の14本）
pub const MIN_LONG_EDGE: u32 = 1000;

fn preview_long_edge(jpeg: &[u8]) -> u32 {
    image::ImageReader::new(std::io::Cursor::new(jpeg))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map_or(0, |(w, h)| w.max(h))
}

/// 撮影日時（[`crate::thumbs::ExifData::taken_at_ms`]）を EXIF の `YYYY:MM:DD HH:MM:SS` に戻す。
/// 読んだときと同じくローカルの壁時計として戻すので、元の文字列と同じになる。
fn exif_datetime(ms: i64) -> Option<String> {
    use chrono::TimeZone;
    let t = chrono::Local.timestamp_millis_opt(ms).single()?;
    Some(t.format("%Y:%m:%d %H:%M:%S").to_string())
}

/// JPEG の頭のほうの区切り（APPn）を並べる。`(印, 区切りの頭, 区切りの終わり)`。
/// 絵の本体（SOS）より前だけを見る。形が崩れていれば、そこまでを返す。
fn app_segments(jpeg: &[u8]) -> Vec<(u8, usize, usize)> {
    let mut out = Vec::new();
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return out;
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        if !(0xE0..=0xEF).contains(&marker) {
            break;
        }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        let end = at + 2 + len;
        if len < 2 || end > jpeg.len() {
            break;
        }
        out.push((marker, at, end));
        at = end;
    }
    out
}

/// EXIF の区切り（`APP1` で `Exif\0\0` から始まるもの）を持つか。
fn has_exif(jpeg: &[u8]) -> bool {
    app_segments(jpeg)
        .iter()
        .any(|&(m, start, end)| m == 0xE1 && jpeg[start + 4..end].starts_with(b"Exif\0\0"))
}

/// 撮影日時（`DateTimeOriginal`）だけを持つ EXIF を入れた JPEG を返す。
/// 入れる場所は SOI の直後——JFIF の `APP0` が先頭に在れば、その後ろ。`dt` は19文字の
/// `YYYY:MM:DD HH:MM:SS`。形が違う・JPEG でなければ `None`。
fn with_capture_time(jpeg: &[u8], dt: &str) -> Option<Vec<u8>> {
    if dt.len() != 19 || !jpeg.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // TIFF（リトルエンディアン）: 頭8バイト → IFD0（ExifIFD への指し1本、26バイト目まで）
    // → ExifIFD（DateTimeOriginal 1本、44バイト目まで）→ 日時の文字列20バイト
    let mut tiff: Vec<u8> = Vec::with_capacity(64);
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x8769u16.to_le_bytes()); // ExifIFDPointer
    tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&26u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x9003u16.to_le_bytes()); // DateTimeOriginal
    tiff.extend_from_slice(&2u16.to_le_bytes()); // ASCII
    tiff.extend_from_slice(&20u32.to_le_bytes());
    tiff.extend_from_slice(&44u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(dt.as_bytes());
    tiff.push(0);

    let mut app1: Vec<u8> = vec![0xFF, 0xE1];
    // 区切りの長さだけはビッグエンディアン（TIFF の中身とは別の決まり）
    app1.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);

    let at = match app_segments(jpeg).first() {
        Some(&(0xE0, _, end)) => end,
        _ => 2,
    };
    let mut out = Vec::with_capacity(jpeg.len() + app1.len());
    out.extend_from_slice(&jpeg[..at]);
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpeg[at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本物の絵（1x1 の JPEG）。`image` のエンコーダは JFIF の APP0 を付ける
    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(1, 1, image::Rgb([200, 100, 50]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut out),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        out
    }

    fn read_dto(jpeg: &[u8]) -> Option<String> {
        let exif = exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(jpeg))
            .ok()?;
        let f = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
        match &f.value {
            exif::Value::Ascii(v) => Some(String::from_utf8_lossy(v.first()?).into_owned()),
            _ => None,
        }
    }

    #[test]
    fn the_capture_time_reads_back_and_the_picture_still_decodes() {
        let stamped = with_capture_time(&tiny_jpeg(), "2026:10:05 09:30:15").unwrap();
        assert_eq!(read_dto(&stamped).as_deref(), Some("2026:10:05 09:30:15"));
        assert!(has_exif(&stamped));
        let img = image::load_from_memory(&stamped).expect("絵として読めない");
        assert_eq!((img.width(), img.height()), (1, 1));
    }

    #[test]
    fn the_exif_goes_after_a_leading_jfif_segment() {
        let plain = tiny_jpeg();
        assert_eq!(
            app_segments(&plain).first().map(|s| s.0),
            Some(0xE0),
            "治具に APP0 が無い"
        );
        let stamped = with_capture_time(&plain, "2026:10:05 09:30:15").unwrap();
        let segs = app_segments(&stamped);
        assert_eq!(segs[0].0, 0xE0);
        assert_eq!(segs[1].0, 0xE1);
    }

    #[test]
    fn a_jpeg_without_exif_has_none_and_a_bad_time_is_refused() {
        let plain = tiny_jpeg();
        assert!(!has_exif(&plain));
        assert!(with_capture_time(&plain, "2026-10-05").is_none());
        assert!(with_capture_time(b"not a jpeg", "2026:10:05 09:30:15").is_none());
    }

    #[test]
    fn a_time_read_as_local_ms_goes_back_to_the_same_string() {
        use chrono::TimeZone;
        let ms = chrono::Local
            .with_ymd_and_hms(2026, 10, 5, 9, 30, 15)
            .single()
            .unwrap()
            .timestamp_millis();
        assert_eq!(exif_datetime(ms).as_deref(), Some("2026:10:05 09:30:15"));
    }
}
