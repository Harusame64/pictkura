//! 印刷に渡す絵（ADR B-3）。
//!
//! **表示用の絵（`full/`）を印刷に使ってはいけない。** JPEG は原本のバイト列をそのまま配っているので、
//! Windows の印刷プレビューで「PDF として保存」を選ぶと、**元の JPEG が EXIF ごと PDF に埋め込まれる**
//! ——撮った場所（GPS）・機種・番号が、印刷のつもりで作った PDF に乗って出ていく（win の実測、
//! 2026-10-17。埋め込まれた JPEG の SHA256 が原本と一致した）。
//!
//! だからここで**撮影情報を落とした絵**を作る。残すのは見た目に要るものだけ:
//!
//! - **色**（ICC プロファイル・Adobe の APP14）。落とすと Display P3 や Adobe RGB の写真がくすむ
//! - **向き**は画素に当てて渡す（下）
//!
//! 向きを持たない JPEG は**画素を作り直さない**（符号化されたデータをそのまま写す）ので、画質は落ちず、
//! 45MP でも展開の費用を払わない。**向きを持つ JPEG は画素へ起こして立ててから詰め直す**——向きの EXIF を
//! 付けたまま渡すと、WebView2 が自分で立てた画素を **PNG（Flate）で PDF に埋め込み**、6000×4000 で
//! 25MB になった（win の実測、2026-10-17。向きの無いものは JPEG のまま 6MB）。描けない形式（RAW・HEIC・TIFF）は表示と同じ道で作った JPEG を、同じ規則で
//! 洗う。そのほか（PNG・WebP・GIF・BMP・AVIF）は画素へ起こして JPEG に詰め直す——PNG の `eXIf`・
//! WebP の `EXIF` を1つずつ見るより、作り直すほうが漏れない。

use std::path::Path;

/// 詰め直すときの品質。**表示の 82 より上げる**——紙は画面より近くで、大きく見る
pub const PRINT_QUALITY: u8 = 92;

/// 詰め直すときの長辺の上限。A3 を 300dpi で刷っても 4961 画素なので、それを十分に超える値。1億画素の
/// スキャンを原寸で起こすと、RGBA と RGB の2枚で 700MB を抱える（#213 のゲート2）
pub const PRINT_MAX_EDGE: u32 = 8192;

/// 印刷に渡す JPEG を作る。作れないもの（SVG・動画・読めないファイル）は `None`。
///
/// 描けない形式（RAW・HEIC・TIFF）の色と品質は**画面に出ている絵と同じ**（表示の道で作るため）——刷るのは
/// ビューアで見ていたもの
pub fn print_jpeg(path: &Path) -> Option<Vec<u8>> {
    if crate::video::is_video_path(path) || crate::svg::is_svg_path(path) {
        return None;
    }
    if crate::jpeg::is_jpeg_path(path) {
        // 向きを持つものは立てて詰め直す（上の説明。PDF が4倍に膨らむ）
        if (2..=8).contains(&crate::thumbs::read_exif_meta(path).orientation) {
            return redraw(path);
        }
        let bytes = std::fs::read(path).ok()?;
        // 区画の並びが崩れている（区画の間にゴミがある・中身が別の形式）ものも、ビューアは描けている。
        // 洗えなければ画素から作り直す（#213 のゲート2）
        return strip_jpeg(&bytes).or_else(|| redraw(path));
    }
    if crate::thumbs::needs_display_transcode(path) {
        // 作った側が向きを当て済み。丸めるのは**印刷の上限**——表示の上限（4096）は配信の都合で紙には小さいが、
        // 丸めないと1億画素のスキャン TIFF を原寸で抱える（#213 の PR codex。効くのは TIFF だけ）
        return from_display_jpeg(&crate::thumbs::display_jpeg_within(
            path,
            Some(PRINT_MAX_EDGE),
        )?);
    }
    redraw(path)
}

/// 表示の道で作った JPEG（向きは当て済み）を印刷用に洗う。呼ぶ側が表示のキャッシュに同じバイト列を持って
/// いれば、それを渡せば作り直さずに済む（HEIC は1枚 0.6〜1秒）
pub fn from_display_jpeg(jpeg: &[u8]) -> Option<Vec<u8>> {
    strip_jpeg(jpeg)
}

/// 画素へ起こして JPEG に詰め直す（向きを当て、白の上に置き、長辺を [`PRINT_MAX_EDGE`] に抑え、ICC を付け直す）
fn redraw(path: &Path) -> Option<Vec<u8>> {
    // JPEG は libjpeg-turbo で起こす（サムネイルと同じ道）。`image` の解読は 6000×4000 の向き付きの写真で
    // 印刷の画面が出るまで10秒かかった（win、debug ビルド）。読めないもの（区画の間のゴミ等）は `image` へ
    let decoded = if crate::jpeg::is_jpeg_path(path) {
        crate::jpeg::decode_scaled(path, PRINT_MAX_EDGE).map(|img| {
            crate::thumbs::apply_orientation(img, crate::thumbs::read_exif_meta(path).orientation)
        })
    } else {
        None
    };
    let mut img = match decoded {
        Some(img) => img,
        None => image::DynamicImage::ImageRgba8(crate::extract::rgba(path)?),
    };
    if img.width().max(img.height()) > PRINT_MAX_EDGE {
        let (w, h) = crate::resize::fit_within(img.width(), img.height(), PRINT_MAX_EDGE);
        img = crate::resize::box_filter(&img, w, h);
    }
    let jpeg = crate::jpeg::encode_rgb(
        &crate::resize::flatten_onto_white(&img),
        PRINT_QUALITY,
        crate::jpeg::ChromaSampling::Full,
    )?;
    // 画素は色空間を変えずに起こしてある。**元の ICC を付け直さないと sRGB と読まれ**、Display P3・Adobe RGB の
    // PNG や WebP は画面より色が浅く刷られる（#213 のゲート1）
    Some(match icc_of(path) {
        Some(icc) => with_icc(&jpeg, &icc),
        None => jpeg,
    })
}

/// 画像の ICC プロファイル（`image` の解読器が読める形式だけ。AVIF は同梱の解読器が持たないので `None`）
fn icc_of(path: &Path) -> Option<Vec<u8>> {
    use image::ImageDecoder;
    if crate::heif::is_avif_path(path) {
        return None;
    }
    let mut decoder = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    decoder
        .icc_profile()
        .ok()
        .flatten()
        .filter(|p| !p.is_empty())
}

/// JPEG の SOI（と APP0）の直後に ICC プロファイルを APP2 で差し込む。64KB を超えるものは区画を分ける
/// （`ICC_PROFILE\0`・番号・総数の順。ICC.1 の Annex B）
fn with_icc(jpeg: &[u8], icc: &[u8]) -> Vec<u8> {
    const CHUNK: usize = 65_533 - 16;
    let mut at = 2;
    if jpeg.get(2..4) == Some(&[0xFF, 0xE0]) {
        at += 2 + usize::from(u16::from_be_bytes([jpeg[4], jpeg[5]]));
    }
    let chunks: Vec<&[u8]> = icc.chunks(CHUNK).collect();
    // 区画の数は1バイトで数える。収まらないほど大きなものは付けない（色より、刷れることを取る）
    if chunks.len() > 255 {
        return jpeg.to_vec();
    }
    let mut out = Vec::with_capacity(jpeg.len() + icc.len() + chunks.len() * 18);
    out.extend_from_slice(&jpeg[..at]);
    for (i, chunk) in chunks.iter().enumerate() {
        out.extend_from_slice(&[0xFF, 0xE2]);
        out.extend_from_slice(&((2 + 14 + chunk.len()) as u16).to_be_bytes());
        out.extend_from_slice(b"ICC_PROFILE\0");
        out.extend_from_slice(&[(i + 1) as u8, chunks.len() as u8]);
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&jpeg[at..]);
    out
}

/// JPEG から撮影情報の区画を外す。**符号化されたデータには触らない。**
///
/// 残す区画: APP0（JFIF）・APP2 のうち ICC プロファイル・APP14（Adobe。色の変換の有無を言う）・
/// 表・フレーム・スキャン。外す区画: APP1（EXIF・XMP）・APP2 のその他（MPF）・APP3〜13（IPTC は
/// APP13）・APP15・COM。**EOI の後ろも捨てる**——MPF が指す2枚目の絵（深度・ゲインマップ）は
/// そこに居て、それぞれが自分の EXIF を持つ。
///
/// JPEG として読めない（SOI が無い・区画の長さがファイルの外を指す）なら `None`。
pub fn strip_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut pos = 2;
    loop {
        // 区画の頭（0xFF の詰め物は飛ばす）
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        while bytes.get(pos + 1) == Some(&0xFF) {
            pos += 1;
        }
        let marker = *bytes.get(pos + 1)?;
        if marker == 0xD9 {
            out.extend_from_slice(&[0xFF, 0xD9]);
            return Some(out);
        }
        // 長さを持たない印（RSTn・TEM）は SOS の前には来ないはずだが、来ても写して進む
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            out.extend_from_slice(&[0xFF, marker]);
            pos += 2;
            continue;
        }
        let len = usize::from(u16::from_be_bytes([
            *bytes.get(pos + 2)?,
            *bytes.get(pos + 3)?,
        ]));
        if len < 2 {
            return None;
        }
        let end = pos + 2 + len;
        let segment = bytes.get(pos..end)?;
        let payload = &segment[4..];
        if keeps(marker, payload) {
            out.extend_from_slice(segment);
        }
        pos = end;
        if marker == 0xDA {
            // 符号化されたデータ。0xFF の後ろが 00（詰め物）・RSTn・FF なら中身、それ以外は次の区画
            pos = past_entropy_data(bytes, pos, &mut out);
            // 途中で切れた写真（EOI が無い）。ビューアが描けるところまでは刷れるよう、あるだけ返す
            if pos >= bytes.len() {
                return Some(out);
            }
        }
    }
}

/// 残す区画か
fn keeps(marker: u8, payload: &[u8]) -> bool {
    match marker {
        0xE0 => true,
        0xE2 => payload.starts_with(b"ICC_PROFILE\0"),
        0xEE => true,
        0xE1 | 0xE3..=0xED | 0xEF | 0xFE => false,
        _ => true,
    }
}

/// `pos` から符号化されたデータを写し、次の区画の頭（0xFF）の位置を返す。終わりまで無ければ末尾
fn past_entropy_data(bytes: &[u8], mut pos: usize, out: &mut Vec<u8>) -> usize {
    let start = pos;
    while pos < bytes.len() {
        if bytes[pos] == 0xFF {
            match bytes.get(pos + 1) {
                Some(0x00) | Some(0xD0..=0xD7) => pos += 2,
                // 詰め物の 0xFF は次の印の前に付いている。ここから先は区画の読み手に任せる
                Some(_) => break,
                None => {
                    pos += 1;
                    break;
                }
            }
        } else {
            pos += 1;
        }
    }
    out.extend_from_slice(&bytes[start..pos.min(bytes.len())]);
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Orientation だけを持つ EXIF の APP1（リトルエンディアン、IFD0 に1項目）
    fn orientation_app1(orientation: u8) -> Vec<u8> {
        let mut tiff = Vec::with_capacity(26);
        tiff.extend_from_slice(b"II*\0");
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 の位置
        tiff.extend_from_slice(&1u16.to_le_bytes()); // 項目の数
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // 個数
        tiff.extend_from_slice(&u16::from(orientation).to_le_bytes());
        tiff.extend_from_slice(&[0, 0]); // 4バイトの枠の残り
        tiff.extend_from_slice(&0u32.to_le_bytes()); // 次の IFD は無い
        let mut app1 = vec![0xFF, 0xE1];
        let len = (2 + 6 + tiff.len()) as u16;
        app1.extend_from_slice(&len.to_be_bytes());
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        app1
    }

    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFF, marker];
        s.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        s.extend_from_slice(payload);
        s
    }

    /// 縞のある絵（展開した画素で比べるので、平らな絵だと取り違えに気づけない）
    fn picture() -> image::RgbImage {
        image::RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 5) as u8, ((x + y) * 3) as u8])
        })
    }

    fn plain_jpeg(progressive: bool) -> Vec<u8> {
        let rgb = picture();
        let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        comp.set_size(rgb.width() as usize, rgb.height() as usize);
        comp.set_quality(90.0);
        if progressive {
            comp.set_progressive_mode();
        } else {
            comp.set_fastest_defaults();
        }
        let mut started = comp.start_compress(Vec::new()).unwrap();
        started.write_scanlines(rgb.as_raw()).unwrap();
        started.finish().unwrap()
    }

    /// 本物のカメラが書く形に近づける: APP0 の後ろに EXIF（GPS の印つき）・XMP・ICC・MPF・IPTC・
    /// COM を挟み、EOI の後ろに2枚目の絵を付ける
    fn camera_jpeg(progressive: bool) -> (Vec<u8>, Vec<u8>) {
        let plain = plain_jpeg(progressive);
        let icc = {
            let mut p = b"ICC_PROFILE\0\x01\x01".to_vec();
            p.extend_from_slice(b"fake icc profile body");
            p
        };
        // APP0 があれば、その後ろに差し込む
        let mut at = 2;
        if plain[2..4] == [0xFF, 0xE0] {
            at += 2 + usize::from(u16::from_be_bytes([plain[4], plain[5]]));
        }
        let mut jpeg = plain[..at].to_vec();
        jpeg.extend(segment(0xE1, b"Exif\0\0II*\0GPSLatitude 35.6812 secret"));
        jpeg.extend(segment(
            0xE1,
            b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>GPS secret</x:xmpmeta>",
        ));
        jpeg.extend(segment(0xE2, &icc));
        jpeg.extend(segment(0xE2, b"MPF\0II*\0 secret second image"));
        jpeg.extend(segment(0xED, b"Photoshop 3.0\0 IPTC secret"));
        jpeg.extend(segment(0xFE, b"comment secret"));
        jpeg.extend_from_slice(&plain[at..]);
        // 2枚目（MPF が指す絵）。中にも EXIF が居る
        jpeg.extend_from_slice(&[0xFF, 0xD8]);
        jpeg.extend(segment(0xE1, b"Exif\0\0 second secret"));
        jpeg.extend_from_slice(&[0xFF, 0xD9]);
        (jpeg, icc)
    }

    fn decoded(jpeg: &[u8]) -> image::RgbImage {
        image::load_from_memory(jpeg).unwrap().into_rgb8()
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn the_shooting_data_goes_and_the_pixels_stay() {
        for progressive in [false, true] {
            let (jpeg, icc) = camera_jpeg(progressive);
            let out = strip_jpeg(&jpeg).unwrap();
            assert!(!contains(&out, b"secret"), "progressive={progressive}");
            assert!(!contains(&out, b"Exif"), "向きが 1 なら APP1 は置かない");
            assert!(contains(&out, &segment(0xE2, &icc)), "ICC は区画ごと残る");
            assert_eq!(&out[out.len() - 2..], &[0xFF, 0xD9], "EOI で終わる");
            assert_eq!(decoded(&out), decoded(&jpeg), "画素は1つも変わらない");
        }
    }

    #[test]
    fn a_turned_jpeg_is_redrawn_upright_and_clean() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("写真.JPG");
        // Orientation=6（時計回りに90度回して見る）の EXIF と、秘密を持つ XMP
        let plain = plain_jpeg(false);
        let mut jpeg = plain[..2].to_vec();
        jpeg.extend(orientation_app1(6));
        jpeg.extend(segment(0xE1, b"http://ns.adobe.com/xap/1.0/\0 secret"));
        jpeg.extend_from_slice(&plain[2..]);
        std::fs::write(&p, &jpeg).unwrap();
        let out = print_jpeg(&p).unwrap();
        assert!(!contains(&out, b"secret"));
        assert!(
            !contains(&out, b"Exif"),
            "向きは画素に当てたので EXIF は要らない"
        );
        let rgb = decoded(&out);
        assert_eq!(rgb.dimensions(), (48, 64), "立っている");
        // 90度右に回すと、左上には元の左下（x=0, y=47 → 0, 235, 141）が来る
        let [r, g, b] = rgb.get_pixel(0, 0).0;
        assert!(
            r < 25 && g > 210 && (120..165).contains(&b),
            "{:?}",
            (r, g, b)
        );
    }

    #[test]
    fn an_upright_jpeg_is_not_redrawn() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jpg");
        let (jpeg, _) = camera_jpeg(false);
        std::fs::write(&p, &jpeg).unwrap();
        // 洗っただけ（符号化されたデータはそのまま）なので、洗った結果と1バイトも違わない
        assert_eq!(print_jpeg(&p).unwrap(), strip_jpeg(&jpeg).unwrap());
    }

    #[test]
    fn a_png_is_redrawn_on_white() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        let img = image::RgbaImage::from_fn(8, 8, |x, _| {
            image::Rgba([0, 0, 0, if x < 4 { 0 } else { 255 }])
        });
        img.save(&p).unwrap();
        let out = print_jpeg(&p).unwrap();
        let rgb = decoded(&out);
        assert!(rgb.get_pixel(1, 4).0.iter().all(|&c| c > 240), "透明は白");
        assert!(rgb.get_pixel(6, 4).0.iter().all(|&c| c < 15), "黒は黒");
    }

    #[test]
    fn a_redrawn_png_keeps_its_colour_profile() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("p3.png");
        // ICC を持つ PNG（中身は解読器が読めればよい。色の変換はしない）
        let icc = vec![7u8; 70_000];
        let mut bytes = Vec::new();
        {
            use image::ImageEncoder;
            let mut enc = image::codecs::png::PngEncoder::new(&mut bytes);
            enc.set_icc_profile(icc.clone()).unwrap();
            enc.write_image(
                &[200, 10, 10, 20, 200, 10],
                2,
                1,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        }
        std::fs::write(&p, &bytes).unwrap();
        let out = print_jpeg(&p).unwrap();
        // 64KB を超えるので2つに分かれ、つなぐと元に戻る
        use image::ImageDecoder;
        let mut dec = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&out)).unwrap();
        assert_eq!(dec.icc_profile().unwrap(), Some(icc));
        assert!(contains(&out, b"ICC_PROFILE\0\x02\x02"));
    }

    #[test]
    fn a_jpeg_that_cannot_be_cleaned_is_redrawn() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("padded.jpg");
        // 区画の間にゴミ（libjpeg は「marker の前に余分なバイト」と警告して読む）。洗う側は断る
        let plain = plain_jpeg(false);
        let mut jpeg = plain[..2].to_vec();
        jpeg.extend_from_slice(&[0x00, 0x00, 0x00]);
        jpeg.extend_from_slice(&plain[2..]);
        assert_eq!(strip_jpeg(&jpeg), None);
        std::fs::write(&p, &jpeg).unwrap();
        // 作り直す側の解読器（`image`）はゴミを読み飛ばす
        let out = print_jpeg(&p).unwrap();
        assert_eq!(decoded(&out).dimensions(), picture().dimensions());
    }

    #[test]
    fn a_huge_tiff_is_capped_too() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("scan.tif");
        image::RgbImage::from_pixel(PRINT_MAX_EDGE + 100, 2, image::Rgb([1, 2, 3]))
            .save(&p)
            .unwrap();
        let rgb = decoded(&print_jpeg(&p).unwrap());
        assert_eq!(rgb.width(), PRINT_MAX_EDGE);
    }

    #[test]
    fn a_huge_redraw_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("wide.png");
        image::RgbImage::from_pixel(PRINT_MAX_EDGE + 100, 2, image::Rgb([1, 2, 3]))
            .save(&p)
            .unwrap();
        let rgb = decoded(&print_jpeg(&p).unwrap());
        assert_eq!(rgb.width(), PRINT_MAX_EDGE);
    }

    #[test]
    fn what_cannot_be_printed_says_none() {
        assert_eq!(strip_jpeg(b"not a jpeg"), None);
        // 区画の長さがファイルの外を指す
        assert_eq!(
            strip_jpeg(&[0xFF, 0xD8, 0xFF, 0xE1, 0x10, 0x00, 1, 2]),
            None
        );
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("a.svg");
        std::fs::write(&svg, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert_eq!(print_jpeg(&svg), None);
        let mp4 = dir.path().join("a.mp4");
        std::fs::write(&mp4, b"video").unwrap();
        assert_eq!(print_jpeg(&mp4), None);
    }

    #[test]
    fn a_truncated_scan_is_kept_as_far_as_it_goes() {
        let plain = plain_jpeg(false);
        let cut = &plain[..plain.len() - 40];
        let out = strip_jpeg(cut).unwrap();
        assert_eq!(
            out.len(),
            cut.len(),
            "あるだけ写す（壊れた写真もビューアと同じだけ刷れる）"
        );
    }
}
