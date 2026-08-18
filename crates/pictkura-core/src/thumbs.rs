//! 爆速サムネイル生成。
//!
//! 爆速の原則:
//! - EXIF埋め込みサムネイルがあれば**再エンコードせずそのまま保存**（フルデコード回避）
//! - なければ `image` クレートでデコード＋縮小（バックグラウンドワーカーで実行）
//! - 可視領域のIDを優先処理するジョブキュー
//! - ついでにEXIF撮影日時と画像サイズ（幅・高さ）も抽出してDBへ書く

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use chrono::TimeZone;
use exif::{In, Tag};

use crate::db::{Db, DbError};

#[derive(Debug, thiserror::Error)]
pub enum ThumbError {
    #[error("DB操作に失敗: {0}")]
    Db(#[from] DbError),
    #[error("I/O失敗: {0}")]
    Io(#[from] std::io::Error),
    #[error("画像処理に失敗: {0}")]
    Image(#[from] image::ImageError),
    #[error("レコードが見つからない: id={0}")]
    NotFound(i64),
}

/// EXIFから抽出したメタデータ。
#[derive(Debug)]
pub struct ExifData {
    /// 撮影日時（Unixエポックミリ秒）。EXIFにタイムゾーンはないため、
    /// **ローカルタイムゾーンの壁時計時刻**として解釈する（表示側の `new Date()` と一致させる）
    pub taken_at_ms: Option<i64>,
    /// 埋め込みサムネイルのJPEGバイナリ
    pub thumbnail: Option<Vec<u8>>,
    /// EXIF Orientation（1〜8、1=回転なし）
    pub orientation: u8,
    /// カメラ名（メーカー＋機種）。検索とファセットのためDBへ正規化保存する
    pub camera: Option<String>,
}

impl Default for ExifData {
    fn default() -> Self {
        Self {
            taken_at_ms: None,
            thumbnail: None,
            orientation: 1,
            camera: None,
        }
    }
}

/// 詳細ビューアに出す撮影情報（第4部 段階D）。
///
/// **DBの列には持たない**。1000万件に数十バイトずつの列を足すとDBが数百MB膨らむ一方、
/// 表示するのは常に1枚だけなので、ビューアを開いた瞬間に実ファイルのEXIFヘッダを
/// 読む（数百µs）。常に最新で、スキーマも増えない。
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ExifInfo {
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// "f/2.8" 等、単位付きの表示用文字列
    pub aperture: Option<String>,
    /// "1/250 s"
    pub shutter: Option<String>,
    /// "ISO 100"
    pub iso: Option<String>,
    /// "24 mm"
    pub focal: Option<String>,
    /// 撮影地（緯度・経度）。地図リンク用
    pub gps: Option<(f64, f64)>,
}

/// 単位付き表示の種類（[`humanize`] 用）。
#[derive(Clone, Copy)]
enum Unit {
    /// 絞り: `f/5.0`
    Aperture,
    /// シャッター速度: `1/60 s`
    Shutter,
    /// 焦点距離: `24 mm`
    Focal,
}

/// EXIFの数値を人が読む形に整える。
///
/// パーサの整形（`display_value().with_unit()`）は機種によって形がぶれる:
///
/// - **分数のまま出る**: 文脈を跨いで拾ったフィールド（CR3の `CMT2` など）は
///   パーサが意味を知らないので `5/1` `24/1` のように出る
/// - **桁が溢れる**: 有理数を割った結果がそのまま並ぶ。iPhoneの焦点距離は
///   `5.960000038146973 mm` になる（実測）
///
/// 先頭の数を読めたときだけ整形し直し、読めなければそのまま通す。
fn humanize(shown: String, unit: Unit) -> String {
    // 単位が付いている場合は先頭の数だけを見る（"5.96 mm" → "5.96"）。
    // 絞りは "f/" が前に付く形で出るので、それも剥がしてから数として読む
    // （剥がさないと "f/1.7799999713897705" の桁溢れを直せない）
    let head = shown.split_whitespace().next().unwrap_or_default();
    let head = head.strip_prefix("f/").unwrap_or(head);
    let value = match head.split_once('/') {
        Some((n, d)) => {
            let n: f64 = n.trim().parse().ok().unwrap_or(f64::NAN);
            let d: f64 = d.trim().parse().ok().unwrap_or(f64::NAN);
            (d != 0.0).then_some(n / d)
        }
        None => head.parse::<f64>().ok(),
    }
    .filter(|v| v.is_finite());
    let Some(value) = value else {
        return shown; // 数として読めない（"f/5.0" 等の整形済み表記）
    };
    match unit {
        Unit::Aperture => format!("f/{value:.1}"),
        Unit::Focal => format!("{} mm", trim_decimals(value)),
        // シャッターは 1/60 のような表記のまま単位だけ足す
        // （`shown` ではなく `head` を使う。既に " s" が付いていると二重になる）
        Unit::Shutter if value < 1.0 => format!("{head} s"),
        // 1秒以上は小数で出る（"1.3 s"）。桁を落とすと 1.3秒が「1 s」になる
        Unit::Shutter => format!("{} s", trim_decimals(value)),
    }
}

/// 小数第2位まで出し、意味のない末尾の0を落とす（`24` / `5.96`）。
fn trim_decimals(value: f64) -> String {
    let s = format!("{value:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// タグを**文脈を跨いで**引く。
///
/// CR3の `CMT2` は「Exif IFDの中身」だけを箱に入れているため、TIFFとして読むと
/// パーサからは *IFD0にExifのタグ番号が並んでいる* ように見える。
/// EXIFのタグは (文脈, 番号) の組なので、文脈が違うと同じ番号でも別タグ扱いになる。
/// 通常のファイルでは1つ目の検索で当たるので、余計なコストにならない。
fn field_any_context(exif: &exif::Exif, tag: Tag) -> Option<&exif::Field> {
    exif.get_field(tag, In::PRIMARY).or_else(|| {
        exif.get_field(Tag(exif::Context::Tiff, tag.number()), In::PRIMARY)
            .or_else(|| exif.get_field(Tag(exif::Context::Exif, tag.number()), In::PRIMARY))
    })
}

/// EXIFのASCIIフィールドを文字列として取り出す（末尾のNUL・空白は落とす）。
fn ascii_field(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let field = field_any_context(exif, tag)?;
    let exif::Value::Ascii(values) = &field.value else {
        return None;
    };
    let raw = values.first()?;
    let s = String::from_utf8_lossy(raw);
    let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// メーカー名と機種名からカメラの表示名を組み立てる。
/// 機種名が既にメーカー名で始まる場合（"NIKON CORPORATION" + "NIKON D850"）は
/// 重ねずに機種名だけを使う。
fn camera_name(make: Option<String>, model: Option<String>) -> Option<String> {
    match (make, model) {
        (Some(make), Some(model)) => {
            // メーカー名の先頭語（NIKON CORPORATION → NIKON）で判定する
            let head = make.split_whitespace().next().unwrap_or(&make);
            if model.to_lowercase().starts_with(&head.to_lowercase()) {
                Some(model)
            } else {
                Some(format!("{make} {model}"))
            }
        }
        (None, Some(model)) => Some(model),
        (Some(make), None) => Some(make),
        (None, None) => None,
    }
}

/// GPS座標（度分秒のRational3つ＋方位）を10進度へ変換する。
fn gps_coord(exif: &exif::Exif, coord: Tag, reference: Tag) -> Option<f64> {
    let field = exif.get_field(coord, In::PRIMARY)?;
    let exif::Value::Rational(parts) = &field.value else {
        return None;
    };
    let dms: Vec<f64> = parts.iter().take(3).map(|r| r.to_f64()).collect();
    if dms.is_empty() {
        return None;
    }
    let degrees = dms.first().copied().unwrap_or(0.0)
        + dms.get(1).copied().unwrap_or(0.0) / 60.0
        + dms.get(2).copied().unwrap_or(0.0) / 3600.0;
    // 南緯・西経は負の値にする
    let negative = ascii_field(exif, reference)
        .map(|r| matches!(r.to_ascii_uppercase().as_str(), "S" | "W"))
        .unwrap_or(false);
    Some(if negative { -degrees } else { degrees })
}

/// 詳細ビューア用に、1ファイルの撮影情報を読む。
/// EXIFが無い・壊れている場合は空の情報を返す（エラーにしない）。
pub fn read_exif_info(path: &Path) -> ExifInfo {
    if let Ok(file) = std::fs::File::open(path) {
        let mut reader = BufReader::new(file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) {
            return exif_info_from(&exif);
        }
    }

    // CR3のようにTIFFではないRAWは、箱に分かれて入っているメタデータを
    // それぞれ読んで合成する（機種はCMT1、撮影パラメータはCMT2…）
    let mut info = ExifInfo::default();
    for block in crate::raw::bmff_metadata_blocks(path) {
        let Ok(exif) = exif::Reader::new().read_raw(block) else {
            continue;
        };
        let part = exif_info_from(&exif);
        info.camera = info.camera.or(part.camera);
        info.lens = info.lens.or(part.lens);
        info.aperture = info.aperture.or(part.aperture);
        info.shutter = info.shutter.or(part.shutter);
        info.iso = info.iso.or(part.iso);
        info.focal = info.focal.or(part.focal);
        info.gps = info.gps.or(part.gps);
    }
    info
}

/// パース済みEXIFから表示用の撮影情報を組み立てる。
fn exif_info_from(exif: &exif::Exif) -> ExifInfo {
    // display_value().with_unit() は "f/2.8" "1/250 s" "24 mm" のように
    // 単位付きで整形してくれるので、表示用はそのまま使う
    let shown = |tag: Tag| {
        field_any_context(exif, tag).map(|f| f.display_value().with_unit(exif).to_string())
    };
    ExifInfo {
        camera: camera_name(ascii_field(exif, Tag::Make), ascii_field(exif, Tag::Model)),
        lens: ascii_field(exif, Tag::LensModel),
        aperture: shown(Tag::FNumber).map(|v| humanize(v, Unit::Aperture)),
        shutter: shown(Tag::ExposureTime).map(|v| humanize(v, Unit::Shutter)),
        iso: shown(Tag::PhotographicSensitivity),
        focal: shown(Tag::FocalLength).map(|v| humanize(v, Unit::Focal)),
        gps: gps_coord(exif, Tag::GPSLatitude, Tag::GPSLatitudeRef).zip(gps_coord(
            exif,
            Tag::GPSLongitude,
            Tag::GPSLongitudeRef,
        )),
    }
}

/// EXIFの暦日時（タイムゾーンなし）をローカル時刻としてUnixエポックミリ秒へ変換する。
/// DST切替で曖昧な時刻は早い方を採用し、存在しない時刻はNoneを返す。
fn exif_dt_to_local_ms(dt: &exif::DateTime) -> Option<i64> {
    use chrono::offset::LocalResult;
    match chrono::Local.with_ymd_and_hms(
        dt.year as i32,
        dt.month as u32,
        dt.day as u32,
        dt.hour as u32,
        dt.minute as u32,
        dt.second as u32,
    ) {
        LocalResult::Single(d) => Some(d.timestamp_millis()),
        LocalResult::Ambiguous(d, _) => Some(d.timestamp_millis()),
        LocalResult::None => None,
    }
}

/// ファイルからEXIF情報（撮影日時・埋め込みサムネイル）を読む。
/// EXIFがない・壊れている場合は空の結果を返す（エラーにしない）。
///
/// RAWは形式によってコンテナのEXIFが読めない（CR3はISO-BMFFでTIFFではない）。
/// その場合は**埋め込みプレビューJPEGのEXIF**を読む。カメラが書いたJPEGなので
/// 撮影日時・カメラ名・向きは実体と同じものが入っている（第6部 段階F）。
pub fn read_exif(path: &Path) -> ExifData {
    let data = read_exif_container(path);

    // HEIFは**コンテナ自身が向きを持つ**（`irot`）。OSのデコーダはそれを適用して
    // 返すので、EXIF Orientationをそのまま流すと絵に二重に掛かって横倒しになる。
    // 撮影日時・カメラ名はEXIFのものをそのまま使う（第7部 段階G）
    if crate::heif::is_bmff_image_path(path) {
        return ExifData {
            orientation: 1,
            // 埋め込みJPEGサムネイルは（あっても）回転前の絵なので使わない。
            // 一覧用の小さい絵は heif::decode_thumbnail から作る
            thumbnail: None,
            ..data
        };
    }

    if data.taken_at_ms.is_some() || !crate::raw::is_raw_path(path) {
        return data;
    }
    let mut result = data;

    // CR3等のISO-BMFFは、TIFF形式のメタデータを箱に入れて持っている。
    // 複数の箱に分かれている（機種はCMT1、撮影日時はCMT2）ので、
    // 取れた項目だけを拾って埋めていく
    for block in crate::raw::bmff_metadata_blocks(path) {
        let Ok(exif) = exif::Reader::new().read_raw(block) else {
            continue;
        };
        let from_block = exif_data_from(&exif);
        result.taken_at_ms = result.taken_at_ms.or(from_block.taken_at_ms);
        result.camera = result.camera.or(from_block.camera);
        if result.orientation == 1 {
            result.orientation = from_block.orientation;
        }
    }

    // サムネイルは埋め込みプレビューJPEGを使う（カメラが書いた表示用の絵）。
    // 撮影日時が箱から取れなかった形式では、プレビューのEXIFも当たってみる
    if let Some(preview) = crate::raw::embedded_preview(path) {
        if result.taken_at_ms.is_none() {
            let mut cursor = std::io::Cursor::new(&preview);
            if let Ok(exif) = exif::Reader::new().read_from_container(&mut cursor) {
                let from_preview = exif_data_from(&exif);
                result.taken_at_ms = from_preview.taken_at_ms;
                result.camera = result.camera.or(from_preview.camera);
                if result.orientation == 1 {
                    result.orientation = from_preview.orientation;
                }
            }
        }
        result.thumbnail = Some(preview);
    }
    result
}

/// コンテナ（ファイル本体）のEXIFヘッダを読む。
fn read_exif_container(path: &Path) -> ExifData {
    let Ok(file) = std::fs::File::open(path) else {
        return ExifData::default();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return ExifData::default();
    };
    exif_data_from(&exif)
}

/// パース済みEXIFから必要な項目を取り出す。
fn exif_data_from(exif: &exif::Exif) -> ExifData {
    let taken_at_ms = field_any_context(exif, Tag::DateTimeOriginal)
        .or_else(|| field_any_context(exif, Tag::DateTime))
        .and_then(|field| match &field.value {
            exif::Value::Ascii(v) => v.first().cloned(),
            _ => None,
        })
        .and_then(|ascii| exif::DateTime::from_ascii(&ascii).ok())
        .and_then(|dt| exif_dt_to_local_ms(&dt));

    let orientation = field_any_context(exif, Tag::Orientation)
        .and_then(|f| f.value.get_uint(0))
        .map(|v| v as u8)
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1);

    // IFD1（サムネイルIFD）のJPEGオフセット＋長さから埋め込みJPEGを切り出す
    let thumbnail = (|| {
        let offset = exif
            .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)?
            .value
            .get_uint(0)? as usize;
        let len = exif
            .get_field(Tag::JPEGInterchangeFormatLength, In::THUMBNAIL)?
            .value
            .get_uint(0)? as usize;
        let buf = exif.buf();
        buf.get(offset..offset + len).map(|b| b.to_vec())
    })();

    ExifData {
        taken_at_ms,
        thumbnail,
        orientation,
        camera: camera_name(ascii_field(exif, Tag::Make), ascii_field(exif, Tag::Model)),
    }
}

/// RAWの**原寸表示用**JPEGを取り出す（第6部 段階F）。
///
/// ビューアは `.CR3` や `.DNG` をそのまま描けないので、埋め込みプレビューを返す。
/// このとき**向きを適用してから返す**必要がある: 埋め込みプレビューはカメラが
/// 回転前の状態で書いており、しかもEXIFを持たないことが多い（CanonのCR3や
/// AppleのProRAWがそう）。普通のJPEGならブラウザがEXIFを見て回してくれるが、
/// 向き情報の無いJPEGを渡すと**縦位置の写真が横倒しで表示される**。
///
/// 回転が要らない（Orientation=1）ときは、カメラが書いたJPEGをそのまま返す
/// （再エンコードしない）。
pub fn raw_display_jpeg(path: &Path) -> Option<Vec<u8>> {
    let exif = read_exif(path);
    let bytes = exif.thumbnail?;
    if exif.orientation == 1 {
        return Some(bytes);
    }
    let img = image::load_from_memory(&bytes).ok()?;
    // 元はカメラが書いたJPEG＝すでに4:2:0。4:4:4で詰め直しても、
    // 上げ底の色差を運んで遅くなるだけ
    crate::raw::encode_jpeg(
        apply_orientation(img, exif.orientation),
        crate::jpeg::ChromaSampling::Half,
    )
}

/// HEIFを原寸表示用のJPEGにする（第7部 段階G）。
///
/// `.heic` をそのまま返してもWebViewは描けないので、OSのデコーダで展開して
/// JPEGに詰め直す。向きは [`crate::heif::decode`] が適用済み。
pub fn heif_display_jpeg(path: &Path) -> Option<Vec<u8>> {
    // HEVCの主用途が4:2:0で、iPhoneのHEICもそう。間引き直しても失うものは無い
    crate::raw::encode_jpeg(
        crate::heif::decode(path)?,
        crate::jpeg::ChromaSampling::Half,
    )
}

/// 拡張子がTIFFか。
fn is_tiff_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tif") || e.eq_ignore_ascii_case("tiff"))
}

/// 原本をそのまま返してもWebViewが描けない形式か。
///
/// 描けないもの（RAW・HEIC・TIFF）は [`display_jpeg`] でJPEGへ詰め直す。
/// AVIF・SVG・BMP・GIF はブラウザが直接描けるので原本を返す。
pub fn needs_display_transcode(path: &Path) -> bool {
    crate::raw::is_raw_path(path) || crate::heif::is_heif_path(path) || is_tiff_path(path)
}

/// 原寸表示用のJPEGを作る。[`needs_display_transcode`] が真の形式に使う。
pub fn display_jpeg(path: &Path) -> Option<Vec<u8>> {
    if crate::raw::is_raw_path(path) {
        return raw_display_jpeg(path);
    }
    if crate::heif::is_heif_path(path) {
        return heif_display_jpeg(path);
    }
    // TIFFは image クレートが読む。向きはEXIFのOrientationに従う。
    // 1億画素のスキャンTIFFのような極端なものでも、配信するJPEGは
    // 画面で見る大きさに収める（原寸のまま詰め直すと数百MBを抱える）
    // **変えるときは `ui/src/App.tsx` の `DISPLAY_MAX_EDGE` も直す**。
    // ビューアの先読みは、TIFFが配信後に丸められる寸法で画素の予算を数えている
    const MAX_DISPLAY_EDGE: u32 = 4096;
    let exif = read_exif(path);
    let img = image::open(path).ok()?;
    let img = if img.width().max(img.height()) > MAX_DISPLAY_EDGE {
        let (w, h) = crate::resize::fit_within(img.width(), img.height(), MAX_DISPLAY_EDGE);
        crate::resize::box_filter(&img, w, h)
    } else {
        img
    };
    // TIFFは色差を間引かない形式。スキャンした文字や線画が混ざるので、
    // ここだけ4:4:4のまま出す（圧縮は50msほど高くつくが、通る枚数が少ない）
    crate::raw::encode_jpeg(
        apply_orientation(img, exif.orientation),
        crate::jpeg::ChromaSampling::Full,
    )
}

/// EXIF Orientation（1〜8）をデコード済み画像へ適用する。
pub fn apply_orientation(img: image::DynamicImage, orientation: u8) -> image::DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// サムネイルの元になる画像を得る。
///
/// **RAWは現像しない**。カメラが埋め込んだ表示用JPEG（`ExifData::thumbnail`）を
/// 「その画像」として扱う。デモザイクは1枚数百ms〜秒かかり、爆速と両立しないうえ、
/// 一覧に出す絵としては埋め込みプレビューで十分（第6部 段階F）。
fn source_image(
    path: &Path,
    exif: &ExifData,
    max_edge: Option<u32>,
) -> Result<image::DynamicImage, ThumbError> {
    // AVIFは**同梱のデコーダ**で展開する（第7部 段階G-6）。OSの拡張機能に
    // 頼らないので環境を選ばず、しかも縮小を展開と融合できるぶん速い。
    // 一覧用なら目標の2倍まで落として返るので、この後の縮小が軽くなる
    if crate::heif::is_avif_path(path) {
        return crate::av1::decode_file(path, max_edge, crate::av1::Threads::One)
            .ok_or_else(|| ThumbError::Io(std::io::Error::other("AVIFを展開できない")));
    }
    // HEIF（HEVC）はOSのデコーダに任せる（向きは適用済みで返る）。
    // HEVCは特許プールがデコーダの配布にも課金するので同梱しない
    if crate::heif::is_heif_path(path) {
        return crate::heif::decode(path).ok_or_else(|| {
            ThumbError::Io(std::io::Error::other(
                "デコードできない（HEIFの拡張機能が要る）",
            ))
        });
    }
    if crate::raw::is_raw_path(path) {
        let bytes = exif
            .thumbnail
            .as_ref()
            .ok_or_else(|| ThumbError::Io(std::io::Error::other("RAWにプレビューが無い")))?;
        // 埋め込みプレビューは原寸のJPEGで入っていることが多い。ここも間引く
        if let Some(edge) = max_edge {
            if let Some(img) = crate::jpeg::decode_scaled_mem(bytes, edge) {
                return Ok(img);
            }
        }
        return Ok(image::load_from_memory(bytes)?);
    }
    // JPEGは**間引きながら**展開する（第8部）。原寸で起こしてから縮めるより
    // 2倍以上速く、しかも中間の縮小が1段減るぶん結果もわずかに素直になる。
    // 扱えない中身（CMYK等）は `None` が返るので、そのまま下の image::open へ落ちる
    if let Some(edge) = max_edge {
        if crate::jpeg::is_jpeg_path(path) {
            if let Some(img) = crate::jpeg::decode_scaled(path, edge) {
                return Ok(img);
            }
        }
    }
    Ok(image::open(path)?)
}

/// 画像の寸法（回転前）。RAWは埋め込みプレビューの寸法を使う。
fn source_dimensions(path: &Path, exif: &ExifData) -> Result<(u32, u32), ThumbError> {
    // HEIFはコンテナの `ispe` を読むだけで分かる（デコード不要・実測0.2ms）。
    // 返すのは**回転を反映した表示上の寸法**なので、呼び出し側で入れ替えない
    // SVGはルート要素の width/height（無ければ viewBox）から読む
    if crate::svg::is_svg_path(path) {
        return crate::svg::dimensions(path)
            .ok_or_else(|| ThumbError::Io(std::io::Error::other("SVGの寸法が読めない")));
    }
    if crate::heif::is_bmff_image_path(path) {
        return crate::heif::display_dimensions(path)
            .ok_or_else(|| ThumbError::Io(std::io::Error::other("HEIFのコンテナが読めない")));
    }
    if crate::raw::is_raw_path(path) {
        let bytes = exif
            .thumbnail
            .as_ref()
            .ok_or_else(|| ThumbError::Io(std::io::Error::other("RAWにプレビューが無い")))?;
        // ヘッダだけ読む（デコードしない）
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(ThumbError::Io)?;
        return Ok(reader.into_dimensions()?);
    }
    Ok(image::image_dimensions(path)?)
}

/// process_oneの結果: どの品質段階まで進んだか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbOutcome {
    /// メタデータ（幅・高さ・撮影日時）のみ書いた。サムネイルは未生成
    /// （埋め込みサムネイルが無く、高品質生成は可視要求までしない: 段階B-3）
    MetadataOnly,
    /// クラウドにしか実体が無いので、自動パスでは何もしなかった。
    /// ユーザーがその場所を見たとき（可視要求）に初めて処理する
    Deferred,
    /// サムネイルが要らない形式（SVG）。原本をそのまま一覧へ出す
    NotNeeded,
    /// EXIF埋め込みの即席サムネイルを書いた（高品質版の再処理が必要）
    Provisional,
    /// 高品質サムネイルまで生成完了
    Final,
}

/// サムネイルの保存先。`thumbs/{id % 256}/{id}.<ext>` に分散配置し、
/// 1フォルダに数十万ファイルが並ぶ問題（FS性能劣化）を回避する。
fn thumb_path_for(thumbs_dir: &Path, id: i64, ext: &str) -> PathBuf {
    thumbs_dir
        .join(format!("{:02x}", (id % 256) as u8))
        .join(format!("{id}.{ext}"))
}

/// 動画1件を処理する（第9部）。
///
/// 画像と違って**段階を分けない**。理由は2つ:
///
/// - コンテナ読み（`moov`）は画素を1バイトも読まないので、寸法・長さ・撮影日時は
///   ヘッダ読みと同じ速さで揃う（実測0.2ms）
/// - 絵はOSのサムネイル機構から借りる。自前で展開しないので、
///   「即席で出しておいて後から作り直す」意味が無い
///
/// OSが絵を出せなかったときはメタデータだけ書いて終わる。一覧には
/// 枠が出て絵の場所は空くが、**汎用アイコンで埋めるよりは良い**。
///
/// 絵の取得は画像と同じく**可視要求（`want_final`）まで待つ**。
/// OSに問い合わせると1本あたり190ms（Windowsのサムネイルキャッシュに
/// 載っていれば29ms）かかり、起動直後に全部やると数百本で数十秒動き続ける。
/// 段階B-3でやめたはずの「全件事前生成」がここだけ復活してしまう。
fn process_video(
    db: &mut Db,
    thumbs_dir: &Path,
    thumb_size: u32,
    id: i64,
    record: &crate::MediaRecord,
    want_final: bool,
) -> Result<ThumbOutcome, ThumbError> {
    let src = &record.path;
    let mut info = crate::video::read_info(src).unwrap_or_default();

    // コンテナから読めなかったぶんをOSのプロパティで補う（段階H）。
    // 自前のコンテナ読みが効かない相手が2種類ある:
    //
    // - **`.m2ts` / `.avi`** … MPEG-TS も RIFF も ISO-BMFF ではないので
    //   `read_info` が何も返せない。Shellは長さも寸法も返す
    //   （実測: 合成した m2ts で 10秒・1920x1080）
    // - **実体がクラウドにしか無い動画** … 開けないのでコンテナを読めていない。
    //   Shellは同期クライアントが置いたぶんだけを返すので、実体を落とさずに
    //   寸法と撮影日時が埋まる（長さは返らない。詳しくは `shell::metadata`）
    if info.width == 0 || info.taken_at_ms.is_none() || info.duration_ms.is_none() {
        if let Some(meta) = crate::shell::metadata(src) {
            if info.width == 0 {
                info.width = meta.width;
                info.height = meta.height;
            }
            info.taken_at_ms = info.taken_at_ms.or(meta.taken_at_ms);
            info.duration_ms = info.duration_ms.or(meta.duration_ms);
        }
    }

    // それでも寸法が読めなければ0のまま。グリッドは既定の縦横比で並べる
    db.update_metadata(
        id,
        i64::from(info.width),
        i64::from(info.height),
        info.taken_at_ms
            .or_else(|| crate::namedate::guess_taken_at(src))
            .or(Some(record.mtime_ms)),
        None,
    )?;
    db.update_duration(id, info.duration_ms)?;

    // 自動パスはここまで。寸法が入ったので、一覧は枠を確保して並べられる
    if !want_final {
        return Ok(ThumbOutcome::MetadataOnly);
    }

    let Some(img) = crate::shell::thumbnail(src, thumb_size) else {
        // **ここを Ok で返してはいけない**。thumb_state は0のままなので、
        // 可視領域の通知（`thumb_state < 2` を投げ直す）が同じIDを拾い続け、
        // OSへの問い合わせ→media-updated→再投入 が延々回る。
        //
        // Err にすると ThumbQueue が失敗として数え、セッション内で
        // MAX_FAILURES 回で見切りをつける（壊れた画像と同じ扱い）。
        // 状態を0のまま残すのは意図的で、HEVCの拡張機能を後から入れた場合に
        // **次回起動で取り直せる**ようにするため
        return Err(ThumbError::Io(std::io::Error::other(
            "OSがこの動画のサムネイルを出せない（対応するデコーダが無い可能性）",
        )));
    };

    let thumb = crate::resize::shrink_to_fit(&img, thumb_size);
    let webp_path = thumb_path_for(thumbs_dir, id, "webp");
    std::fs::create_dir_all(webp_path.parent().unwrap())?;
    let rgb = crate::resize::flatten_onto_white(&thumb);
    let encoded = webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height()).encode(82.0);
    std::fs::write(&webp_path, &*encoded)?;

    // OSから借りた絵がそのまま最終形なので、高品質（2）として記録する。
    // LRUの管理対象に入れておけば、増えすぎたときに消してもらえる
    db.update_thumb_path(id, &webp_path, 2, Some(encoded.len() as i64))?;
    if let Some(old) = &record.thumb_path {
        if old != &webp_path {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(ThumbOutcome::Final)
}

/// 1件分のサムネイル＋メタデータ処理（2段階）。
///
/// - 段階0→1: ヘッダ読みで幅・高さ、EXIF撮影日時をDBへ。埋め込みサムネイルが
///   あれば**即書き出して返す**（一覧の初期表示を最速にする）
/// - 段階1→2: フル画像をデコードし、Lanczos3で高品質サムネイルを生成して差し替える
///
/// `want_final` = false（バックグラウンドの自動パス）では高品質生成へ進まない:
/// 埋め込みサムネイルの無い画像（PNG・スクリーンショット等）をここでフルデコード
/// すると、全件事前生成をやめた意味（CPU・ディスク節約）が失われるため、
/// メタデータだけ書いて [`ThumbOutcome::MetadataOnly`] を返す。
/// 高品質は可視領域の要求（優先キュー経由、want_final = true）でのみ生成する。
pub fn process_one(
    db: &mut Db,
    thumbs_dir: &Path,
    thumb_size: u32,
    id: i64,
    want_final: bool,
) -> Result<ThumbOutcome, ThumbError> {
    let record = db.get_by_id(id)?.ok_or(ThumbError::NotFound(id))?;
    let src = &record.path;

    // 実体がクラウドにしか無いファイルは、**開いた瞬間にダウンロードが走る**。
    // 起動直後の自動パスがこれを片端から開くと、ユーザーが何もしていないのに
    // 数GBの通信とディスク消費が始まる（実測: ライブラリ内のHEIC 3322枚のうち
    // 94%がクラウドのみだった）。自動パスでは触らず、ユーザーがその場所を
    // 実際に見たとき（可視要求 = want_final）に初めて取りに行く。
    // Windowsのサムネイルキャッシュも同じ理由でオフライン属性のファイルを飛ばす。
    //
    // 代償: 幅・高さ・撮影日時が埋まらないので、一覧では既定の縦横比で並び、
    // 日付は `COALESCE(taken_at_ms, mtime_ms)` の mtime になる。可視要求で
    // 実物を読んだ瞬間に撮影日へ直るため、**タイルが別の日へ移ることがある**。
    // 同期クライアント経由のmtimeは撮影日に近いことが多く実害は小さいが、
    // これを消すにはファイルを開かずにサムネイルを得る経路
    //（WindowsのShellサムネイル）が要る。plan.md 第7部の未着手に置いた
    if !want_final && crate::cloud::is_cloud_only_path(src) {
        // 絵は諦めるが、**素性はOSが持っている**（段階H）。同期クライアントが
        // 置いたぶんだけが返るので、ここで問い合わせても
        // 1バイトも落ちない（根拠は `shell::metadata` の説明。長さだけが
        // 返らない＝コンテナを解釈するハンドラが動いていない）。実測ではクラウドのみのHEIC・MOVでも
        // 寸法と撮影日時が返り、上の「並びがmtimeに落ちる」代償がほぼ消える。
        //
        // 一度埋まれば `width` が入るので次の起動からは聞き直さない。
        // Shellも何も持っていないファイルだけは毎回聞くことになるが、
        // 1件あたり20ms程度で、キューに載るのは起動時の1回きり
        // `taken_at_ms == mtime_ms` も「読めなくて落ちた」印なので拾い直す。
        // これが無いと、一度実体が落ちてきた後に同期クライアントへ戻された
        // ファイル（Storage Sense等）が、寸法も日時も入っているせいで
        // 永久にこの門を素通りする（段階H-2）
        if record.width.is_none()
            || record.taken_at_ms.is_none()
            || record.taken_at_ms == Some(record.mtime_ms)
        {
            let mut meta = crate::shell::metadata(src).unwrap_or_default();
            // OSも知らないならファイル名に聞く（段階H-2）。開かずに済む点は同じ
            meta.taken_at_ms = meta
                .taken_at_ms
                .or_else(|| crate::namedate::guess_taken_at(src));
            if !meta.is_empty() {
                db.update_shell_metadata(
                    id,
                    i64::from(meta.width),
                    i64::from(meta.height),
                    meta.taken_at_ms,
                )?;
                if meta.duration_ms.is_some() {
                    db.update_duration(id, meta.duration_ms)?;
                }
            }
        }
        return Ok(ThumbOutcome::Deferred);
    }

    // 動画は画素の道が根本的に違う（第9部）。EXIFも無いので、
    // コンテナを読んで寸法・長さ・撮影日時を取り、絵はOSから借りる
    if crate::video::is_video_path(src) {
        return process_video(db, thumbs_dir, thumb_size, id, &record, want_final);
    }

    let exif_data = read_exif(src);
    let (width, height) = source_dimensions(src, &exif_data)?;
    // RAWは埋め込みプレビューが「表示用の絵」そのもの。即席として書き出すと
    // フルサイズJPEG（数MB）がサムネイル置き場に溜まるので、最初から縮小して作る
    let is_raw = crate::raw::is_raw_path(src);
    // HEIFも元ファイルをWebViewが描けない。サムネイルを作らないと一覧に何も出ない。
    // AVIFはブラウザが描けるので、ここではHEIFだけを「一覧に何も出せない形式」
    // として扱う。デコーダを同梱した後（段階G-6）もこの線引きは変えない:
    // AVIFまで含めると、起動直後の背景パスで全AVIFを展開してしまい、
    // 段階B-3のオンデマンド化が効かなくなる（展開は可視要求まで待つ）
    let is_heif = crate::heif::is_heif_path(src);
    // Orientation 5〜8 は90度系の回転 → 表示上の幅・高さは入れ替わる
    let (disp_w, disp_h) = if (5..=8).contains(&exif_data.orientation) {
        (height, width)
    } else {
        (width, height)
    };
    db.update_metadata(
        id,
        disp_w as i64,
        disp_h as i64,
        // EXIF → OSのプロパティ → 名前 → mtime（段階H-2）。`or_else` なので
        // EXIFで決まればOSには聞きに行かない。
        //
        // **OSを挟むのが要点**。XMPにしか撮影日を持たない書き出し（Lightroom・
        // Google フォト）は `read_exif` が何も返さないが、Shellのハンドラは読める。
        // ここを飛ばすと、背景の門がOSから正しい日付を入れた後で、可視要求の
        // この経路が mtime で**上書きし直してしまう**（タイルが同期日へ飛ぶ）
        exif_data
            .taken_at_ms
            .or_else(|| crate::shell::metadata(src).and_then(|m| m.taken_at_ms))
            .or_else(|| crate::namedate::guess_taken_at(src))
            .or(Some(record.mtime_ms)),
        exif_data.camera.as_deref(),
    )?;

    // SVGはブラウザがそのまま描けるので、サムネイルを作らない。
    // 一覧にも原本を配れば足りるうえ、拡大しても劣化しない。
    //
    // ここで**終わった印**（THUMB_STATE_NOT_NEEDED）を付けるのが要点。
    // 可視領域の通知は `thumb_state < 2` の行を投げ直すので、0のままだと
    // スクロールのたびにファイルを開き直してDBへ書き続ける（無限に繰り返す）
    if crate::svg::is_svg_path(src) {
        db.update_thumb_state(id, THUMB_STATE_NOT_NEEDED)?;
        return Ok(ThumbOutcome::NotNeeded);
    }

    // 旧配置（フラット/旧拡張子）のサムネイルは新パス書き込み後に消す
    let cleanup_old = |new_path: &Path| {
        if let Some(old) = &record.thumb_path {
            if old != new_path {
                let _ = std::fs::remove_file(old);
            }
        }
    };

    // 段階1(HEIF): iPhoneのHEICは主画像とは別に**小さいHEVCのサムネイル**を持つ。
    // JPEGではないのでバイト列のままでは使えないが、小さいまま展開すれば
    // 主画像を展開するより速い（実測 148ms 対 455ms）。
    // 展開済みなので再エンコードは避けられず、そのままWebPで書く
    if record.thumb_state == 0 && is_heif {
        if let Some(img) = crate::heif::decode_thumbnail(src) {
            let webp_path = thumb_path_for(thumbs_dir, id, "webp");
            std::fs::create_dir_all(webp_path.parent().unwrap())?;
            let rgb = crate::resize::flatten_onto_white(&img);
            let encoded = webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height()).encode(82.0);
            std::fs::write(&webp_path, &*encoded)?;
            // 即席（state=1）なので、可視になったら主画像から作り直す
            db.update_thumb_path(id, &webp_path, 1, None)?;
            cleanup_old(&webp_path);
            return Ok(ThumbOutcome::Provisional);
        }
    }

    // 段階1: まだサムネイルが無ければ、埋め込みサムネイルを即席として書き出す（爆速パス）。
    // EXIF埋め込みJPEGは再エンコードせずそのまま書く（デコード回避）ため拡張子はjpg
    if record.thumb_state == 0 && !is_raw && !is_heif {
        if let Some(bytes) = &exif_data.thumbnail {
            if let Ok(embedded) = image::load_from_memory(bytes) {
                let jpg_path = thumb_path_for(thumbs_dir, id, "jpg");
                std::fs::create_dir_all(jpg_path.parent().unwrap())?;
                if exif_data.orientation == 1 {
                    std::fs::write(&jpg_path, bytes)?;
                } else {
                    let rotated = apply_orientation(embedded, exif_data.orientation);
                    let mut out = std::io::BufWriter::new(std::fs::File::create(&jpg_path)?);
                    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
                    rotated.to_rgb8().write_with_encoder(encoder)?;
                }
                db.update_thumb_path(id, &jpg_path, 1, None)?;
                cleanup_old(&jpg_path);
                return Ok(ThumbOutcome::Provisional);
            }
        }
    }

    // 自動パス（want_final = false）はここまで: 高品質生成は可視要求まで保留。
    // ただしRAWは、埋め込みプレビューのデコード1回で済むうえ、
    // これを作らないと一覧に何も出せない（元ファイルはWebViewが表示できない）ので進む。
    // HEIFも同じ理由で進む（ここへ来るのは埋め込みサムネイルが取れなかったときだけ）
    if !want_final && !is_raw && !is_heif {
        return Ok(ThumbOutcome::MetadataOnly);
    }

    // 段階2: フル画像から高品質サムネイルを生成する。
    // 大きい画像はまず高速に2倍サイズへ落とし、仕上げにLanczos3でシャープに縮小する
    let img = source_image(src, &exif_data, Some(thumb_size))?;
    let (iw, ih) = (img.width(), img.height());
    let thumb = if iw.max(ih) <= thumb_size {
        img // 元がサムネイルより小さい: 拡大はしない
    } else {
        // 目標の3倍より大きいまま仕上げると、Lanczos3の窓が広がって重いうえ
        // 細かい模様に縞が出る。先に面積平均でざっと落としてから仕上げる。
        // JPEGとAVIFは展開の時点で目標付近まで落ちているので通らない。
        // HEIFはOSのデコーダに大きさを指定できず原寸で返るので、ここを通る
        let pre = if iw.max(ih) > thumb_size * 3 {
            let (pw, ph) = crate::resize::fit_within(iw, ih, thumb_size * 2);
            crate::resize::box_filter(&img, pw, ph)
        } else {
            img
        };
        let (tw, th) = crate::resize::fit_within(pre.width(), pre.height(), thumb_size);
        crate::resize::lanczos3(&pre, tw, th)
    };

    // WebP(lossy)で書く: JPEG比でほぼ半分の容量（100万枚で数十GBの差になる）
    let webp_path = thumb_path_for(thumbs_dir, id, "webp");
    std::fs::create_dir_all(webp_path.parent().unwrap())?;
    let rgb = crate::resize::flatten_onto_white(&apply_orientation(thumb, exif_data.orientation));
    let encoded = webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height()).encode(82.0);
    std::fs::write(&webp_path, &*encoded)?;

    // 高品質サムネイルはLRUキャッシュ管理対象: サイズを記録する（段階B-3）
    db.update_thumb_path(id, &webp_path, 2, Some(encoded.len() as i64))?;
    cleanup_old(&webp_path);
    Ok(ThumbOutcome::Final)
}

/// 「サムネイルは要らない」印（SVGのように原本をそのまま出せる形式）。
///
/// 2（高品質生成済み）より大きくすることで、
/// - 可視領域の再投入（`thumb_state < 2`）に入らない
/// - LRUの削除対象（`thumb_state = 2`）にも入らない
///
/// の両方を満たす。原本をサムネイル置き場と誤認して消す事故を避けるため、
/// `thumb_path` は書かない。
pub const THUMB_STATE_NOT_NEEDED: i64 = 3;

/// 同一IDの処理失敗をこの回数まで許容する（超えたら以後の投入を無視する）。
/// 壊れた画像が可視領域にあると、UIの可視通知のたびに再投入されて
/// フルデコード失敗を延々繰り返すため、セッション内で見切りをつける。
const MAX_FAILURES: u32 = 3;

/// 可視領域優先のジョブキュー。
struct QueueState {
    /// 優先ジョブ（可視領域のID）。先頭から処理され、高品質生成まで行う
    priority: VecDeque<i64>,
    /// 通常ジョブ（バックグラウンドの自動パス）。メタデータ＋即席まで
    pending: VecDeque<i64>,
    /// キュー内のID（重複投入防止）
    queued: HashSet<i64>,
    /// ワーカーが処理中のID。complete()まで再投入を防ぎ、
    /// 同一サムネイルファイルへの並行書き込みを避ける
    processing: HashSet<i64>,
    /// ID別の処理失敗回数（壊れた画像の無限リトライ防止。セッション内のみ）
    failures: HashMap<i64, u32>,
    shutdown: bool,
}

impl QueueState {
    fn blocked(&self, id: i64) -> bool {
        self.failures.get(&id).copied().unwrap_or(0) >= MAX_FAILURES
    }
}

struct QueueInner {
    state: Mutex<QueueState>,
    cvar: Condvar,
}

#[derive(Clone)]
pub struct ThumbQueue {
    inner: Arc<QueueInner>,
}

impl Default for ThumbQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(QueueInner {
                state: Mutex::new(QueueState {
                    priority: VecDeque::new(),
                    pending: VecDeque::new(),
                    queued: HashSet::new(),
                    processing: HashSet::new(),
                    failures: HashMap::new(),
                    shutdown: false,
                }),
                cvar: Condvar::new(),
            }),
        }
    }

    /// ポイズニングされていてもロックを取得する（パニックの連鎖を防ぐ）。
    fn lock_state(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// ジョブを末尾に追加する（キュー済み・処理中・失敗上限超えのIDは無視）。
    pub fn enqueue(&self, ids: &[i64]) {
        let mut state = self.lock_state();
        for &id in ids {
            if !state.blocked(id) && !state.processing.contains(&id) && state.queued.insert(id) {
                state.pending.push_back(id);
            }
        }
        drop(state);
        self.inner.cvar.notify_all();
    }

    /// 可視領域のIDを最優先に引き上げる。キューにないIDは無視する。
    /// 優先キューのジョブは高品質生成まで行う（段階B-3のオンデマンド経路）。
    pub fn prioritize(&self, ids: &[i64]) {
        let mut state = self.lock_state();
        let target: HashSet<i64> = ids.iter().copied().collect();
        state.pending.retain(|id| !target.contains(id));
        state.priority.retain(|id| !target.contains(id));
        // 表示順（引数順）を保って先頭へ
        for &id in ids.iter().rev() {
            if state.queued.contains(&id) {
                state.priority.push_front(id);
            }
        }
        drop(state);
        self.inner.cvar.notify_all();
    }

    /// 次のジョブを取り出す。キューが空なら待機し、shutdownでNoneを返す。
    /// 戻り値の bool は「優先キュー由来（＝高品質生成まで行う）」か。
    pub fn pop(&self) -> Option<(i64, bool)> {
        let mut state = self.lock_state();
        loop {
            if state.shutdown {
                return None;
            }
            let next = match state.priority.pop_front() {
                Some(id) => Some((id, true)),
                None => state.pending.pop_front().map(|id| (id, false)),
            };
            if let Some((id, want_final)) = next {
                state.queued.remove(&id);
                state.processing.insert(id);
                return Some((id, want_final));
            }
            state = self
                .inner
                .cvar
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    /// 処理完了（成功・失敗とも）を通知し、そのIDの再投入を許可する。
    pub fn complete(&self, id: i64) {
        self.lock_state().processing.remove(&id);
    }

    /// 処理失敗を記録する。上限（MAX_FAILURES）を超えたIDは以後投入されない。
    pub fn record_failure(&self, id: i64) {
        *self.lock_state().failures.entry(id).or_insert(0) += 1;
    }

    /// このIDがキュー内または処理中か（LRU削除の競合回避用）。
    pub fn is_active(&self, id: i64) -> bool {
        let state = self.lock_state();
        state.queued.contains(&id) || state.processing.contains(&id)
    }

    /// キュー内＋処理中のIDのスナップショット。
    /// LRU削除が「これから生成される/されつつあるサムネイル」を巻き込まないための除外集合。
    pub fn active_ids(&self) -> HashSet<i64> {
        let state = self.lock_state();
        state.queued.union(&state.processing).copied().collect()
    }

    pub fn shutdown(&self) {
        self.lock_state().shutdown = true;
        self.inner.cvar.notify_all();
    }
}

/// サムネイル生成ワーカー群。
pub struct ThumbnailService {
    queue: ThumbQueue,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl ThumbnailService {
    /// ワーカースレッドを起動する。各ワーカーは自前のDB接続を持つ（WALで並行動作）。
    /// `on_done(id)` は1件完了ごとに呼ばれる（UIへの通知用）。
    pub fn start(
        db_path: PathBuf,
        thumbs_dir: PathBuf,
        thumb_size: u32,
        worker_count: usize,
        on_done: impl Fn(i64) + Send + Sync + 'static,
    ) -> Self {
        let queue = ThumbQueue::new();
        let on_done = Arc::new(on_done);
        let count = if worker_count == 0 {
            std::thread::available_parallelism()
                .map(|n| (n.get() / 2).max(2))
                .unwrap_or(2)
        } else {
            worker_count
        };

        let workers = (0..count)
            .map(|_| {
                let queue = queue.clone();
                let db_path = db_path.clone();
                let thumbs_dir = thumbs_dir.clone();
                let on_done = on_done.clone();
                std::thread::spawn(move || {
                    let Ok(mut db) = Db::open(&db_path) else {
                        return;
                    };
                    while let Some((id, want_final)) = queue.pop() {
                        // 段階B-3: 自動パス（want_final=false）はメタデータ＋即席まで。
                        // 高品質生成は可視領域の要求（優先キュー）時のみ行う
                        let result = process_one(&mut db, &thumbs_dir, thumb_size, id, want_final);
                        queue.complete(id);
                        // 失敗（壊れた画像等）は回数を記録する。上限を超えたIDは
                        // 以後の再投入が無視される（無限リトライ防止）
                        if result.is_err() {
                            queue.record_failure(id);
                        }
                        // **成否にかかわらず通知する**。途中で失敗しても
                        // 寸法・撮影日時までは書けていることがあり（動画で
                        // 絵が取れなかった場合がまさにそれ）、黙っていると
                        // グリッドが既定の縦横比と古い日付のまま取り残される。
                        // 再投入は上の失敗回数が止めるので、通知が無限に続くことはない
                        on_done(id);
                    }
                })
            })
            .collect();

        Self { queue, workers }
    }

    pub fn enqueue(&self, ids: &[i64]) {
        self.queue.enqueue(ids);
    }

    pub fn prioritize(&self, ids: &[i64]) {
        self.queue.prioritize(ids);
    }

    /// キュー内＋処理中のIDのスナップショット（LRU削除の除外集合用）。
    pub fn active_ids(&self) -> HashSet<i64> {
        self.queue.active_ids()
    }

    /// このIDがキュー内または処理中か（LRU削除ファイル消去直前の再確認用）。
    pub fn is_active(&self, id: i64) -> bool {
        self.queue.is_active(id)
    }

    /// 全ワーカーを停止して合流する。
    pub fn shutdown(self) {
        self.queue.shutdown();
        for w in self.workers {
            let _ = w.join();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn 分数のままの値に単位が付く() {
        // CR3のように文脈を跨いで拾った値（パーサが意味を知らない形）
        assert_eq!(humanize("5/1".into(), Unit::Aperture), "f/5.0");
        assert_eq!(humanize("63/10".into(), Unit::Aperture), "f/6.3");
        assert_eq!(humanize("24/1".into(), Unit::Focal), "24 mm");
        assert_eq!(humanize("1/60".into(), Unit::Shutter), "1/60 s");
        assert_eq!(humanize("2/1".into(), Unit::Shutter), "2 s");
        // 既に整形済みの表記は触らない（普通のJPEGの経路）
        assert_eq!(humanize("f/2.8".into(), Unit::Aperture), "f/2.8");
        assert_eq!(humanize("24 mm".into(), Unit::Focal), "24 mm");
        assert_eq!(humanize("1/250 s".into(), Unit::Shutter), "1/250 s");
    }

    use super::*;
    use crate::scanner::ScannedFile;

    fn make_test_jpeg(path: &Path, w: u32, h: u32) {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        img.save(path).unwrap();
    }

    #[test]
    fn exif日時はローカル壁時計として往復できる() {
        use chrono::{Datelike, TimeZone, Timelike};
        let dt = exif::DateTime::from_ascii(b"2024:08:11 15:30:45").unwrap();
        let ms = exif_dt_to_local_ms(&dt).unwrap();
        // ローカルタイムゾーンで読み戻すと元の壁時計時刻に一致する
        // （フロントの new Date(ms) のローカル表示と揃うことを保証）
        let back = chrono::Local.timestamp_millis_opt(ms).unwrap();
        assert_eq!(
            (back.year(), back.month(), back.day()),
            (2024, 8, 11),
            "日付が一致"
        );
        assert_eq!(
            (back.hour(), back.minute(), back.second()),
            (15, 30, 45),
            "時刻が一致"
        );
    }

    #[test]
    fn orientation適用で寸法が入れ替わる() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(40, 30));
        let rotated = apply_orientation(img.clone(), 6);
        assert_eq!((rotated.width(), rotated.height()), (30, 40));
        let flipped = apply_orientation(img.clone(), 2);
        assert_eq!((flipped.width(), flipped.height()), (40, 30));
        let same = apply_orientation(img, 1);
        assert_eq!((same.width(), same.height()), (40, 30));
    }

    #[test]
    fn カメラ名はメーカーの重複を畳む() {
        let name = |mk: Option<&str>, md: Option<&str>| {
            camera_name(mk.map(String::from), md.map(String::from))
        };
        // 機種名が既にメーカー名で始まる → 機種名だけ（"NIKON CORPORATION NIKON D850" にしない）
        assert_eq!(
            name(Some("NIKON CORPORATION"), Some("NIKON D850")),
            Some("NIKON D850".into())
        );
        assert_eq!(
            name(Some("SONY"), Some("ILCE-7M3")),
            Some("SONY ILCE-7M3".into())
        );
        assert_eq!(
            name(Some("Apple"), Some("iPhone 15 Pro")),
            Some("Apple iPhone 15 Pro".into())
        );
        // 片方しか無い場合はある方を使う
        assert_eq!(name(None, Some("EOS R5")), Some("EOS R5".into()));
        assert_eq!(name(Some("Canon"), None), Some("Canon".into()));
        assert_eq!(name(None, None), None);
    }

    #[test]
    fn exif情報の無いファイルは空のinfoになる() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.jpg");
        make_test_jpeg(&path, 32, 24);
        let info = read_exif_info(&path);
        assert!(info.camera.is_none() && info.gps.is_none() && info.lens.is_none());
        // 存在しないファイルでもパニックしない
        let missing = read_exif_info(&dir.path().join("nope.jpg"));
        assert!(missing.camera.is_none());
    }

    #[test]
    fn exifなしのファイルは空のexifデータ() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.jpg");
        make_test_jpeg(&path, 64, 48);
        let data = read_exif(&path);
        assert!(data.taken_at_ms.is_none());
        assert!(data.thumbnail.is_none());
    }

    #[test]
    fn process_oneで寸法抽出とサムネイル生成ができる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.jpg");
        make_test_jpeg(&src, 800, 600);

        let db_path = dir.path().join("test.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src.clone(),
            size: 1,
            mtime_ms: 42,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;

        let thumbs = dir.path().join("thumbs");
        // EXIF埋め込みサムネイルなし → 一発で高品質(Final)まで進む
        let outcome = process_one(&mut db, &thumbs, 320, id, true).unwrap();
        assert_eq!(outcome, ThumbOutcome::Final);

        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.width, Some(800));
        assert_eq!(rec.height, Some(600));
        // EXIFなし → mtimeがtaken_atへフォールバック
        assert_eq!(rec.taken_at_ms, Some(42));
        assert_eq!(rec.thumb_state, 2);
        let thumb_path = rec.thumb_path.unwrap();
        assert!(thumb_path.exists());
        // 分散配置（thumbs/{id%256}/{id}.webp）で書かれている
        assert_eq!(thumb_path.extension().unwrap(), "webp");
        assert_eq!(
            thumb_path.parent().unwrap(),
            thumbs.join(format!("{:02x}", (id % 256) as u8)),
        );
        let (tw, th) = image::image_dimensions(&thumb_path).unwrap();
        assert!(tw <= 320 && th <= 320);
    }

    #[test]
    fn 旧配置のサムネイルは再生成時に削除される() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.jpg");
        make_test_jpeg(&src, 800, 600);

        let db_path = dir.path().join("test.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src.clone(),
            size: 1,
            mtime_ms: 42,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;

        // 旧配置（フラットなthumbs/{id}.jpg）にサムネイルがある状態を再現
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let old_path = thumbs.join(format!("{id}.jpg"));
        std::fs::write(&old_path, b"old-thumb").unwrap();
        db.update_thumb_path(id, &old_path, 1, None).unwrap();

        process_one(&mut db, &thumbs, 320, id, true).unwrap();
        assert!(!old_path.exists(), "旧サムネイルが削除される");
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert!(rec.thumb_path.unwrap().extension().unwrap() == "webp");
    }

    #[test]
    fn キューは可視優先で払い出す() {
        let queue = ThumbQueue::new();
        queue.enqueue(&[1, 2, 3, 4, 5]);
        queue.prioritize(&[4, 3]);
        // 優先キュー由来は want_final = true（高品質まで生成）
        assert_eq!(queue.pop(), Some((4, true)));
        assert_eq!(queue.pop(), Some((3, true)));
        assert_eq!(queue.pop(), Some((1, false)));
        assert_eq!(queue.pop(), Some((2, false)));
        assert_eq!(queue.pop(), Some((5, false)));
    }

    #[test]
    fn キューは重複投入を無視する() {
        let queue = ThumbQueue::new();
        queue.enqueue(&[1, 2]);
        queue.enqueue(&[2, 3]);
        assert_eq!(queue.pop(), Some((1, false)));
        assert_eq!(queue.pop(), Some((2, false)));
        assert_eq!(queue.pop(), Some((3, false)));
    }

    #[test]
    fn 処理中のidはcompleteまで再投入されない() {
        let queue = ThumbQueue::new();
        queue.enqueue(&[1]);
        assert_eq!(queue.pop(), Some((1, false))); // 1は処理中になる
        queue.enqueue(&[1]); // 処理中 → 無視される
        queue.enqueue(&[2]);
        assert_eq!(queue.pop(), Some((2, false)));
        queue.complete(1); // 処理完了 → 再投入可能に
        queue.enqueue(&[1]);
        assert_eq!(queue.pop(), Some((1, false)));
    }

    #[test]
    fn shutdownでpopがnoneを返す() {
        let queue = ThumbQueue::new();
        let q2 = queue.clone();
        let handle = std::thread::spawn(move || q2.pop());
        std::thread::sleep(std::time::Duration::from_millis(50));
        queue.shutdown();
        assert_eq!(handle.join().unwrap(), None);
    }

    #[test]
    fn サービスの自動パスはメタデータのみで高品質は作らない() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("svc.db");
        let mut db = Db::open(&db_path).unwrap();

        let mut files = Vec::new();
        for i in 0..6 {
            let p = dir.path().join(format!("img{i}.jpg"));
            make_test_jpeg(&p, 400, 300);
            files.push(ScannedFile {
                path: p,
                size: 1,
                mtime_ms: i,
            });
        }
        db.upsert_files(&files).unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().map(|r| r.id).collect();

        let done = Arc::new(Mutex::new(Vec::new()));
        let done2 = done.clone();
        let svc = ThumbnailService::start(
            db_path.clone(),
            dir.path().join("thumbs"),
            160,
            2,
            move |id| done2.lock().unwrap().push(id),
        );
        svc.enqueue(&ids);

        // 全件完了を待つ（最大5秒）
        for _ in 0..100 {
            if done.lock().unwrap().len() == ids.len() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        svc.shutdown();
        assert_eq!(done.lock().unwrap().len(), ids.len());

        let db = Db::open(&db_path).unwrap();
        for rec in db.list_all().unwrap() {
            // 埋め込みサムネイルなし＋自動パス → メタデータのみ（段階B-3）
            assert_eq!(rec.width, Some(400), "メタデータは抽出される");
            assert!(rec.thumb_path.is_none(), "高品質は自動では作らない");
            assert_eq!(rec.thumb_state, 0);
        }
    }

    #[test]
    fn 可視要求で高品質まで生成される() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("vis.db");
        let mut db = Db::open(&db_path).unwrap();
        let p = dir.path().join("img.jpg");
        make_test_jpeg(&p, 400, 300);
        db.upsert_files(&[ScannedFile {
            path: p,
            size: 1,
            mtime_ms: 1,
        }])
        .unwrap();
        let ids: Vec<i64> = db.list_all().unwrap().iter().map(|r| r.id).collect();

        let done = Arc::new(Mutex::new(0usize));
        let done2 = done.clone();
        let svc = ThumbnailService::start(
            db_path.clone(),
            dir.path().join("thumbs"),
            160,
            1,
            move |_| *done2.lock().unwrap() += 1,
        );
        // 可視フロー（UIと同じ）: 高品質になるまで enqueue＋prioritize を繰り返す
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let db = Db::open(&db_path).unwrap();
            let rec = db.get_by_id(ids[0]).unwrap().unwrap();
            if rec.thumb_state == 2 {
                assert!(rec.thumb_path.unwrap().exists());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "高品質生成がタイムアウト"
            );
            svc.enqueue(&ids);
            svc.prioritize(&ids);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        svc.shutdown();
    }

    #[test]
    fn 自動パスは埋め込みなし画像をメタデータのみで止める() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("plain.jpg");
        make_test_jpeg(&src, 640, 480);
        let db_path = dir.path().join("meta.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src,
            size: 1,
            mtime_ms: 5,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;

        let thumbs = dir.path().join("thumbs");
        let outcome = process_one(&mut db, &thumbs, 320, id, false).unwrap();
        assert_eq!(outcome, ThumbOutcome::MetadataOnly);
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.width, Some(640));
        assert_eq!((rec.thumb_state, rec.thumb_path), (0, None));

        // 可視要求（want_final）で高品質まで進む
        let outcome = process_one(&mut db, &thumbs, 320, id, true).unwrap();
        assert_eq!(outcome, ThumbOutcome::Final);
        assert_eq!(db.get_by_id(id).unwrap().unwrap().thumb_state, 2);
    }

    /// 動画はコンテナも絵も読めなくても、処理として破綻しない（第9部）。
    ///
    /// 中身が動画でないファイルを渡すと、寸法0・長さ不明のまま
    /// メタデータだけが入る。撮影日時はmtimeへ落ちる。
    /// 一覧では既定の縦横比の枠が並び、絵の場所が空くのが期待動作
    #[test]
    fn 動画はコンテナが読めなくてもメタデータまでは書く() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("broken.mp4");
        std::fs::write(&src, b"not really a video").unwrap();
        let db_path = dir.path().join("video.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src,
            size: 1,
            mtime_ms: 12_345,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;
        let thumbs = dir.path().join("thumbs");

        // 自動パスは寸法・長さを書いて止まる（OSへの問い合わせはしない）
        let outcome = process_one(&mut db, &thumbs, 320, id, false).unwrap();
        assert_eq!(outcome, ThumbOutcome::MetadataOnly);
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.duration_ms, None, "読めないコンテナの長さはNULL");
        assert_eq!(rec.taken_at_ms, Some(12_345), "撮影日時はmtimeへ落ちる");

        // 可視要求でOSが絵を出せなければ**失敗として返す**。
        // Okで返すと、可視通知のたびに同じIDが投げ直されて延々回る
        assert!(
            process_one(&mut db, &thumbs, 320, id, true).is_err(),
            "絵が取れないときはErrで返し、キューに失敗を数えさせる"
        );
        // 状態は0のまま＝デコーダを後から入れれば次回取り直せる
        assert_eq!(db.get_by_id(id).unwrap().unwrap().thumb_state, 0);
    }

    /// 原本をそのまま返して良い形式か（第7部の形式追加）。
    /// SVGは「サムネイル不要」の印が付き、可視要求で作り直されない。
    ///
    /// 印が無いと `thumb_state < 2` の再投入に毎回引っかかり、
    /// スクロールのたびにファイルを開き直してDBへ書き続ける（レビュー指摘）。
    #[test]
    fn svgはサムネイル不要の印で止まる() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("v.svg");
        std::fs::write(&src, br#"<svg width="800" height="600"></svg>"#).unwrap();
        let db_path = dir.path().join("svg.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src,
            size: 1,
            mtime_ms: 5,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;
        let thumbs = dir.path().join("thumbs");

        for want_final in [false, true] {
            let outcome = process_one(&mut db, &thumbs, 320, id, want_final).unwrap();
            assert_eq!(outcome, ThumbOutcome::NotNeeded);
            let rec = db.get_by_id(id).unwrap().unwrap();
            assert_eq!(rec.thumb_state, THUMB_STATE_NOT_NEEDED);
            assert_eq!(rec.thumb_path, None, "原本をサムネイル置き場に見せない");
            assert_eq!(rec.width, Some(800));
        }
    }

    #[test]
    fn 詰め直しが要る形式だけを見分ける() {
        // WebViewが描けない: JPEGへ詰め直す
        for name in [
            "a.cr3", "a.CR2", "a.heic", "a.HEIF", "a.hif", "a.tif", "a.TIFF",
        ] {
            assert!(
                needs_display_transcode(Path::new(name)),
                "{name} は詰め直しが要る"
            );
        }
        // ブラウザが直接描ける: 原本を返す
        for name in [
            "a.jpg", "a.png", "a.webp", "a.bmp", "a.gif", "a.svg", "a.avif",
        ] {
            assert!(
                !needs_display_transcode(Path::new(name)),
                "{name} は原本を返せる"
            );
        }
    }

    #[test]
    fn 撮影情報の数値を人が読む形に整える() {
        // 分数のまま出る形（CR3の CMT2 など）
        assert_eq!(humanize("24/1".into(), Unit::Focal), "24 mm");
        assert_eq!(humanize("5/1".into(), Unit::Aperture), "f/5.0");
        assert_eq!(humanize("1/250".into(), Unit::Shutter), "1/250 s");

        // 桁が溢れる形（iPhoneの焦点距離。実測値）
        assert_eq!(
            humanize("5.960000038146973 mm".into(), Unit::Focal),
            "5.96 mm"
        );
        // 単位が既に付いていても二重にしない
        assert_eq!(humanize("1/250 s".into(), Unit::Shutter), "1/250 s");
        assert_eq!(humanize("24 mm".into(), Unit::Focal), "24 mm");

        // 数として読めない整形済み表記はそのまま通す
        assert_eq!(humanize("f/2.8".into(), Unit::Aperture), "f/2.8");

        // 0除算で壊れない
        assert_eq!(humanize("5/0".into(), Unit::Focal), "5/0");
    }

    /// クラウドのみのファイルは自動パスで開かない（ダウンロードを誘発しない）。
    ///
    /// 実物のプレースホルダは同期クライアントしか作れないので、
    /// `OFFLINE` 属性を立てたローカルファイルで代用する。
    #[cfg(windows)]
    #[test]
    fn 自動パスはクラウドのみのファイルに触らない() {
        use std::os::windows::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("cloud.jpg");
        make_test_jpeg(&src, 640, 480);
        let wide: Vec<u16> = src
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
        unsafe {
            windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(
                wide.as_ptr(),
                FILE_ATTRIBUTE_OFFLINE,
            );
        }

        let db_path = dir.path().join("cloud.db");
        let mut db = Db::open(&db_path).unwrap();
        db.upsert_files(&[ScannedFile {
            path: src,
            size: 1,
            mtime_ms: 5,
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;
        let thumbs = dir.path().join("thumbs");

        // 自動パスは何もしない（寸法もサムネイルも書かない＝ファイルを開いていない）
        let outcome = process_one(&mut db, &thumbs, 320, id, false).unwrap();
        assert_eq!(outcome, ThumbOutcome::Deferred);
        let rec = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(rec.width, None);
        assert_eq!((rec.thumb_state, rec.thumb_path), (0, None));

        // ユーザーがその場所を見たら（可視要求）通常どおり処理する
        let outcome = process_one(&mut db, &thumbs, 320, id, true).unwrap();
        assert_eq!(outcome, ThumbOutcome::Final);
        assert_eq!(db.get_by_id(id).unwrap().unwrap().width, Some(640));
    }

    #[test]
    fn 失敗上限を超えたidは再投入されない() {
        let queue = ThumbQueue::new();
        for _ in 0..3 {
            queue.record_failure(7);
        }
        queue.enqueue(&[7, 8]);
        assert_eq!(
            queue.pop(),
            Some((8, false)),
            "失敗上限超えの7は投入されない"
        );
        queue.complete(8);
        // 2回までの失敗なら再投入できる
        queue.record_failure(8);
        queue.record_failure(8);
        queue.enqueue(&[8]);
        assert_eq!(queue.pop(), Some((8, false)));
    }
}
