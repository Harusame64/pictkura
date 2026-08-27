//! RAW画像の埋め込みプレビュー抽出（第6部 段階F）。
//!
//! **RAWを現像しない**のが方針。現像（デモザイク＋色変換）は1枚あたり数百ms〜秒で、
//! 「爆速」と両立しない。一方、実用上のRAWはほぼ例外なく
//! **カメラが書いた表示用JPEG**を内部に持っている（撮影時に背面液晶へ出すため）。
//! これを取り出せば、一覧のサムネイルもビューアの表示も**デコード1回**で足りる。
//!
//! 取り出し方は形式で違うので、**安い順に手を替える**。使える大きさ
//! （[`USABLE_LONG_EDGE`]）の絵が出た時点で打ち切る:
//!
//! 1. **EXIF/TIFFの申告**（CR2・ARW・PEF・3FR など）。
//!    TIFF系RAWは複数のIFDを持ち、それぞれがプレビューJPEGの位置と長さを申告する。
//!    ヘッダだけ読めば場所が分かるので、ここで当たれば**ファイル全体を読まない**
//! 2. **先頭16MBのブロック走査**（CR3・RAF・NEF・ORF・RW2 など）。
//!    CR3はISO-BMFF、RAFは独自ヘッダで、いずれもJPEGを丸ごと抱えている。
//!    Nikon（NEF・NRW）は**160x120の切手だけを申告して**原寸を申告の無い
//!    SubIFDに置くので、申告を鵜呑みにせずここも見る
//! 3. **全体の走査**（Ricoh GR IIIのDNG・Sigmaの sd Quattro / sd Quattro H）。
//!    原寸プレビューをファイルの後ろ半分に置く形式がある。ここまで来るのは
//!    「まだ使える絵が無い」ときだけなので、丸ごと読む値段を払う
//! 4. **非圧縮RGBの組み立て**（Leica DNG・Epson ERF・Hasselblad 3FR・
//!    Phase One IIQ・Kodak DCR）。これらは**JPEGを1枚も持たず**、プレビューを
//!    生のRGBとしてTIFFのストリップに置いている。ストリップを繋いで絵にする
//! 5. 見つからなければ諦める（サムネイル無しとして扱い、一覧には枠だけ出る）
//!
//! **メタデータ**（撮影日時・カメラ名・向き）も形式で置き場所が違う。
//! ORF・RW2はTIFFの版番号が独自でパーサに素通しされるので、
//! [`patched_tiff_metadata`] で直してから読む。CR3は箱の中
//! （[`bmff_metadata_blocks`]）、RAFはプレビューJPEGのEXIFから拾う。
//! 拾い方の順番は [`crate::thumbs::read_exif`] にある。

use std::path::Path;

use exif::{In, Tag};

/// ブロック走査で読む上限。RAWの埋め込みプレビューはファイル先頭側にあるため、
/// 全体（20〜80MB）を読まずに済ませる。CR3のPRVWもRAFのJPEGもこの範囲に入る。
const SCAN_LIMIT: usize = 16 * 1024 * 1024;

/// 走査で全体を読むときの上限。ここまで来るのは「先頭16MBに使える絵が
/// 無かった」ときだけなので、丸ごと読む値段（実測100〜350ms）を払う。
const FULL_SCAN_LIMIT: usize = 128 * 1024 * 1024;

/// 「原寸表示に使える」と見なすプレビューの長辺。これを超えたらそこで
/// 探すのをやめる。下回るときは、もっと大きい絵が無いか次の手を試す。
///
/// 1600にしたのは、下回る実物が **160x120（Nikonの切手）と720x480**
/// （Ricoh GR IIIのDNGの小プレビュー）で、上回る実物が **3200x2400**（OM-1）
/// から上に固まっているため。間に候補が無く、どこで切っても同じになる。
pub const USABLE_LONG_EDGE: u32 = 1600;

/// 版番号を直して読むときに、実際にファイルから読む先頭の量。
/// 16KBでORF・RW2とも全項目が読めたが、機種差を見込んで余裕を取る。
const PATCHED_TIFF_HEAD: usize = 256 * 1024;

/// 拡張子がRAWか（大文字小文字は無視）。
///
/// ここに無い拡張子は「普通の画像」として `image` クレートに渡る。
pub fn is_raw_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    RAW_EXTENSIONS.contains(&lower.as_str())
}

/// RAWとして扱う拡張子（小文字）。
///
/// ここに足したら [`crate::config::DEFAULT_EXTENSIONS`] にも足すこと
/// ——走査対象に入っていないRAWは**一覧に出ない**。歯止めのテストがある。
pub const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "crw", // Canon
    "nef", "nrw", // Nikon
    "arw", "srf", "sr2", // Sony
    "raf", // Fujifilm
    "orf", // OM System / Olympus
    "rw2", // Panasonic
    "pef", "ptx", // Pentax
    "srw", // Samsung
    "dng", // Adobe / 各社共通
    "raw", "rwl", // Leica
    "3fr", "fff", // Hasselblad
    "iiq", // Phase One
    "erf", // Epson
    "mrw", // Minolta
    "x3f", // Sigma
    "dcr", "kdc", // Kodak
    "mos", // Leaf
];

/// JPEGらしきバイト列か（SOIマーカー）。
fn looks_like_jpeg(bytes: &[u8]) -> bool {
    bytes.len() > 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
}

/// JPEGを1枚ぶん読み切った結果。
struct JpegSpan {
    /// SOIから数えた終端（EOIの直後）
    end: usize,
    /// `image` クレートでデコードして画面に出せるか
    displayable: bool,
}

/// バイト列の `start` から始まるJPEGを**セグメントを辿って**読み、終端と種別を返す。
///
/// 「0xFFD9 を探すだけ」ではいけない。JPEGのEXIF（APP1）には**サムネイルJPEGが
/// 丸ごと入っている**ことがあり、その内側のEOIで切ってしまうと壊れた画像になる。
/// APP1は長さ付きセグメントなので、長さで飛ばせば内側のEOIを踏まない。
///
/// 同時に SOF を見て「表示できるJPEGか」も判定する。RAWはセンサーの生データを
/// **ロスレスJPEG(SOF3)** として抱えていることがあり、大きさで選ぶと必ずそれを掴む。
fn jpeg_span(buf: &[u8], start: usize) -> Option<JpegSpan> {
    if !looks_like_jpeg(buf.get(start..)?) {
        return None;
    }
    jpeg_span_body(buf, start)
}

/// SOI（`FF D8`）が `start` にあると分かっているときの、終端探し。
/// **`start` の2バイトは見ない**ので、先頭を潰されたJPEG（Minolta MRW）を
/// 直す前に、コピーせずその場で確かめられる。
fn jpeg_span_body(buf: &[u8], start: usize) -> Option<JpegSpan> {
    let mut displayable = false;
    let mut i = start + 2;
    while i + 1 < buf.len() {
        // マーカーの前には詰め物の 0xFF が並ぶことがある
        if buf[i] != 0xFF {
            return None; // セグメントの切れ目に来ていない＝壊れている
        }
        let mut marker_pos = i;
        while marker_pos + 1 < buf.len() && buf[marker_pos + 1] == 0xFF {
            marker_pos += 1;
        }
        let marker = *buf.get(marker_pos + 1)?;
        i = marker_pos + 2;
        match marker {
            // 終端
            0xD9 => {
                return Some(JpegSpan {
                    end: i,
                    displayable,
                })
            }
            // 長さフィールドを持たないマーカー
            0x01 | 0xD0..=0xD8 => continue,
            _ => {}
        }
        let len = u16::from_be_bytes([*buf.get(i)?, *buf.get(i + 1)?]) as usize;
        if len < 2 {
            return None;
        }
        match marker {
            // 普通のJPEG（ベースライン・拡張シーケンシャル・プログレッシブ）。
            // 8ビット精度・成分1〜4のものだけ受け付ける
            0xC0..=0xC2 => {
                let sof = buf.get(i + 2..i + 8)?;
                displayable = sof[0] == 8
                    && u16::from_be_bytes([sof[1], sof[2]]) > 0
                    && u16::from_be_bytes([sof[3], sof[4]]) > 0
                    && (1..=4).contains(&sof[5]);
            }
            // ロスレス・差分・算術符号はデコードできない
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => displayable = false,
            _ => {}
        }
        i += len;

        // SOS の後ろは画素データ。次のマーカーまで読み飛ばす
        // （0xFF00 は詰め物、0xFFD0〜D7 はリスタートマーカーで区切りではない）
        if marker == 0xDA {
            while i + 1 < buf.len() {
                if buf[i] == 0xFF && !matches!(buf[i + 1], 0x00 | 0xFF | 0xD0..=0xD7) {
                    break;
                }
                i += 1;
            }
        }
    }
    None
}

/// 完結した1枚のJPEGで、かつ表示できるか。
fn is_displayable_jpeg(bytes: &[u8]) -> bool {
    matches!(jpeg_span(bytes, 0), Some(span) if span.displayable && span.end == bytes.len())
}

/// EXIF/TIFFの申告から、埋め込みJPEGの (オフセット, 長さ) 候補を集める。
///
/// 見るのは2種類:
/// - `JPEGInterchangeFormat` / `...Length`（サムネイルIFDやSubIFDのプレビュー）
/// - `StripOffsets` / `StripByteCounts` で `Compression = 6`（旧JPEG圧縮）。
///   CR2の全画素サイズのプレビューはこの形で入っている
///
/// 候補は**長さの大きい順**に返す。大きいJPEGほど元画像に近い。
fn declared_previews(exif: &exif::Exif) -> Vec<(usize, usize)> {
    /// この規格のIFD番号は 0(PRIMARY) と 1(THUMBNAIL) が定義済み。
    /// RAWはSubIFDを複数持つため、実際に出てきたIFD番号を舐める
    fn ifds(exif: &exif::Exif) -> Vec<In> {
        let mut seen: Vec<In> = Vec::new();
        for field in exif.fields() {
            if !seen.contains(&field.ifd_num) {
                seen.push(field.ifd_num);
            }
        }
        seen
    }

    let value_at = |ifd: In, tag: Tag| -> Option<usize> {
        exif.get_field(tag, ifd)?
            .value
            .get_uint(0)
            .map(|v| v as usize)
    };

    let mut candidates = Vec::new();
    for ifd in ifds(exif) {
        if let (Some(offset), Some(len)) = (
            value_at(ifd, Tag::JPEGInterchangeFormat),
            value_at(ifd, Tag::JPEGInterchangeFormatLength),
        ) {
            candidates.push((offset, len));
        }
        // JPEG圧縮のストリップ（6=旧JPEG, 7=新JPEG）は、そのままJPEGファイル
        if matches!(value_at(ifd, Tag::Compression), Some(6) | Some(7)) {
            if let (Some(offset), Some(len)) = (
                value_at(ifd, Tag::StripOffsets),
                value_at(ifd, Tag::StripByteCounts),
            ) {
                candidates.push((offset, len));
            }
        }
    }
    candidates.sort_by_key(|(_, len)| std::cmp::Reverse(*len));
    candidates
}

/// バイト列から**表示できるJPEGのうち最大のもの**を切り出す。
///
/// 形式ごとのパーサを書かずにCR3・RAF等の表示用JPEGへ届くための手段。
fn scan_largest_jpeg(buf: &[u8]) -> Option<&[u8]> {
    let mut best: Option<&[u8]> = None;
    let mut i = 0usize;
    while i + 3 < buf.len() {
        if !looks_like_jpeg(&buf[i..]) {
            i += 1;
            continue;
        }
        match jpeg_span(buf, i) {
            Some(span) => {
                let block = &buf[i..span.end];
                if span.displayable && best.is_none_or(|b| b.len() < block.len()) {
                    best = Some(block);
                }
                // 読み切れた分は飛ばす（内側のサムネイルを二重に拾わない）
                i = span.end;
            }
            // 途中で壊れていた: このSOIは諦めて次を探す
            None => i += 2,
        }
    }
    best
}

/// 非圧縮プレビューの置き場所（TIFFのストリップ）。
struct UncompressedStrips {
    width: u32,
    height: u32,
    /// 1成分あたりのビット数（8 または 16）
    bits: u32,
    offsets: Vec<u32>,
    counts: Vec<u32>,
}

/// 非圧縮RGBストリップのプレビューを組み立ててJPEGにする。
///
/// 旧世代・中判のRAW（Leica DNG・Epson ERF・Hasselblad 3FR・Phase One IIQ・
/// Kodak DCR など）は**JPEGを1枚も持たず**、プレビューを「非圧縮のRGB」として
/// TIFFのストリップに置いている。JPEGを探すだけでは永遠に見つからないので、
/// ここで組み立てる。
///
/// 選ぶのは `Compression=1`（非圧縮）かつ `PhotometricInterpretation=2`（RGB）の
/// IFD。センサーの生データ（`Photometric=32803` = CFA）は**選ばない**
/// （現像していないので絵にならない）。
fn uncompressed_preview(path: &Path, exif: &exif::Exif, big_endian: bool) -> Option<Vec<u8>> {
    /// 組み立てを許す最大バイト数。プレビューは数百KB〜数MBで、
    /// これを超えるものは生データを掴んでいる可能性が高い
    const MAX_BYTES: usize = 64 * 1024 * 1024;

    let ifds: Vec<In> = {
        let mut seen: Vec<In> = Vec::new();
        for field in exif.fields() {
            if !seen.contains(&field.ifd_num) {
                seen.push(field.ifd_num);
            }
        }
        seen
    };

    let uint = |ifd: In, tag: Tag| -> Option<u32> { exif.get_field(tag, ifd)?.value.get_uint(0) };
    let uints = |ifd: In, tag: Tag| -> Vec<u32> {
        let Some(field) = exif.get_field(tag, ifd) else {
            return Vec::new();
        };
        (0..)
            .map_while(|i| field.value.get_uint(i))
            .collect::<Vec<_>>()
    };

    // 候補のうち一番大きいものを使う
    let mut best: Option<UncompressedStrips> = None;
    for ifd in ifds {
        if uint(ifd, Tag::Compression) != Some(1) {
            continue;
        }
        if uint(ifd, Tag::PhotometricInterpretation) != Some(2) {
            continue; // RGB以外（CFAの生データ等）は絵にならない
        }
        let (Some(width), Some(height)) = (uint(ifd, Tag::ImageWidth), uint(ifd, Tag::ImageLength))
        else {
            continue;
        };
        let bits = uint(ifd, Tag::BitsPerSample).unwrap_or(8);
        let samples = uint(ifd, Tag::SamplesPerPixel).unwrap_or(3);
        // 平面分離（PlanarConfiguration=2）は面ごとに並ぶので扱わない
        if samples != 3 || !matches!(bits, 8 | 16) || uint(ifd, Tag::PlanarConfiguration) == Some(2)
        {
            continue;
        }
        let offsets = uints(ifd, Tag::StripOffsets);
        let counts = uints(ifd, Tag::StripByteCounts);
        if offsets.is_empty() || offsets.len() != counts.len() {
            continue;
        }
        let expected = (width as usize) * (height as usize) * 3 * (bits as usize / 8);
        if expected == 0 || expected > MAX_BYTES {
            continue;
        }
        let area = (width as usize) * (height as usize);
        if best
            .as_ref()
            .is_none_or(|b| (b.width as usize) * (b.height as usize) < area)
        {
            best = Some(UncompressedStrips {
                width,
                height,
                bits,
                offsets,
                counts,
            });
        }
    }

    let UncompressedStrips {
        width,
        height,
        bits,
        offsets,
        counts,
    } = best?;
    let mut raw = Vec::new();
    for (offset, count) in offsets.iter().zip(&counts) {
        raw.extend_from_slice(&read_window(path, *offset as usize, *count as usize)?);
    }

    let needed = (width as usize) * (height as usize) * 3;
    let pixels: Vec<u8> = if bits == 8 {
        raw
    } else {
        // 16ビットのプレビューは**リニア**（ガンマ未適用）で入っている。
        // 上位8ビットをそのまま使うと真っ暗な絵になるので、sRGB相当の
        // ガンマを掛けてから8ビットへ落とす（Kodak DCRで実際に発生）
        let lut: Vec<u8> = (0..=255u32)
            .map(|v| ((v as f32 / 255.0).powf(1.0 / 2.2) * 255.0).round() as u8)
            .collect();
        raw.as_chunks::<2>()
            .0
            .iter()
            .map(|p| lut[if big_endian { p[0] } else { p[1] } as usize])
            .collect()
    };
    if pixels.len() < needed {
        return None; // ストリップが欠けている
    }

    let image = image::RgbImage::from_raw(width, height, pixels[..needed].to_vec())?;
    // 非圧縮のRGBが元なので、色差は一度も間引かれていない（4:4:4のまま出す）
    encode_jpeg(
        image::DynamicImage::ImageRgb8(image),
        crate::jpeg::ChromaSampling::Full,
    )
}

/// 表示用JPEGの品質。**配信して数秒で捨てる絵**なので、見た目が保てる下限に置く。
pub const DISPLAY_QUALITY: u8 = 82;

/// デコード済み画像をJPEGへ（プレビューの返り値はJPEGバイト列に統一する）。
///
/// 圧縮は mozjpeg（libjpeg-turbo）に任せる。`image` クレートの純Rustエンコーダは
/// HEICの詰め直しのおよそ半分を食っていて、**同じ4:4:4で並べて3.7倍遅い**。
/// 量子化テーブルとハフマン表の作りの差で、絵は化けていない（詰め直したJPEGを
/// 読み戻して元画素と比べ、平均差が8を超えたものは20枚中0枚）。
///
/// **msの内訳はここに写さない**——`bench --heif-encode` の列を正とする
/// （PR #23 のゲート2 P2）。
///
/// `chroma` は**元の絵が既に色差を間引かれているか**で選ぶ。間引き済みのもの
/// （iPhoneのHEIC・カメラの埋め込みJPEG＝どちらも普通は4:2:0）を4:4:4で出すのは、
/// 上げ底の色差を運んでいるだけで遅い。逆に、間引かれていないものを間引くと
/// 色の境目が崩れる。
///
/// **形式から決め打ちしないこと**。Canonの `.HIF` は4:2:2で、HEIFは規格上4:4:4も
/// 許される（ゲート1のP2）。呼び出し側は元の綴りを読んで渡す
/// （[`crate::heif::stored_chroma`] / [`crate::jpeg::chroma_of`]）。読めないときと、
/// 元が間引かれていないとき（TIFF・非圧縮プレビュー）は4:4:4のまま出す。
pub(crate) fn encode_jpeg(
    img: image::DynamicImage,
    chroma: crate::jpeg::ChromaSampling,
) -> Option<Vec<u8>> {
    // JPEGは透過を持てない。alphaを捨てるだけだと、SIMDの縮小を通った
    // 透明部分が黒で残る（resize::flatten_onto_white の説明を参照）
    let rgb = crate::resize::flatten_onto_white(&img);
    if let Some(bytes) = crate::jpeg::encode_rgb(&rgb, DISPLAY_QUALITY, chroma) {
        return Some(bytes);
    }
    // libjpeg が受け取らなかったとき（壊れた画素でのlongjmpを受け止めた場合など）は
    // 純Rustのエンコーダへ戻す。3.7倍遅く、しかも間引きの指定は届かない
    // （`image` のエンコーダは常に4:4:4）が、**1枚も出せないよりはよい**
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, DISPLAY_QUALITY);
    rgb.write_with_encoder(encoder).ok()?;
    Some(bytes)
}

/// RAWから表示用のJPEGを取り出す。見つからなければNone。
///
/// 返すのは**カメラが書いたJPEGそのまま**（再エンコードしない）。
/// 向きの補正は呼び出し側が [`crate::thumbs::apply_orientation`] で行う
/// （RAWのEXIF Orientation はプレビューJPEGにも効く）。
///
/// **一番大きい絵を掴むまで手を替える**のが要点。安い手から順に試し、
/// [`USABLE_LONG_EDGE`] を超える絵が出たらそこで打ち切る。1枚目で
/// 打ち切らないのは、Nikon（NEF・NRW）が160x120の切手をTIFFに申告しつつ、
/// 原寸のJPEGを申告の無いSubIFDに置くため——**申告を鵜呑みにすると
/// 全画面表示が160x120になる**（実測: `bench --tiff-fix`）。
pub fn embedded_preview(path: &Path) -> Option<Vec<u8>> {
    embedded_preview_at_least(path, USABLE_LONG_EDGE)
}

/// 長辺が `min_long_edge` に届く絵が出たら、そこで探すのをやめる版。
///
/// 一覧のタイルのように**小さい絵で足りる**場面では、原寸のプレビューを
/// 探してファイル全体を読む（実測100〜350ms）のは払いすぎになる。
pub fn embedded_preview_at_least(path: &Path, min_long_edge: u32) -> Option<Vec<u8>> {
    /// 大きい方を残す。
    fn keep(best: &mut Option<Vec<u8>>, found: Vec<u8>) {
        if best
            .as_ref()
            .is_none_or(|b| long_edge(b) < long_edge(&found))
        {
            *best = Some(found);
        }
    }

    let mut best: Option<Vec<u8>> = None;

    // 1段目: TIFFの申告を読む。ヘッダだけで位置が分かるので、
    // 当たれば必要な範囲しか読まない
    if let Some(bytes) = declared_preview(path) {
        if long_edge(&bytes) >= min_long_edge {
            return Some(bytes);
        }
        keep(&mut best, bytes);
    }

    // 2段目: 先頭を読んでJPEGの塊を拾う（CR3・RAF・NEF等）。
    //
    // 読んだ先頭は6段目でも要るが、**そのまま持ち越さない**——3段目は最大128MBを
    // 読むので、16MBを抱えたままだとワーカーの数だけピークが積み上がる。
    // ここでHEVCの箱（数百KB）だけ取り出して、先頭は落とす。
    // **代償**: HEVCのプレビューを探せるのは先頭16MBまでになる。3段目は128MBまで
    // 読むが、6段目が使える材料はここで決まる（実物の `PRVW` は0.15MB地点）
    let hevc_boxes: Vec<Cr3Hevc> = match read_head(path, SCAN_LIMIT) {
        Some(head) => {
            if let Some(found) = scan_largest_jpeg(&head) {
                if long_edge(found) >= min_long_edge {
                    return Some(found.to_vec());
                }
                keep(&mut best, found.to_vec());
            }
            cr3_hevc_boxes(path, &head)
        }
        None => Vec::new(),
    };

    // 3段目: 先頭16MBに無かった。原寸プレビューを**後ろの方に**置く形式が
    // ある（Ricoh GR IIIのDNGは720x480の後ろ、26.8MBの位置に6000x4000）。
    //
    // **X3Fは社でも系列でも割れない**（2026-08-26にCC0の9機種で確かめた）。
    // 16MBの窓の外へ出たのは **sd Quattro（44MB）と sd Quattro H（48MB）の2機種**。
    // SD14 も末尾に置く（9.18MB・ファイルが11.6MBなので窓に収まるだけ）が、
    // **同世代・同センサーの SD15 は先頭**に置く。dp2 Quattro も先頭で、
    // sd Quattro とは中身の置き方が違う。
    //
    // つまり機種の一覧を持っても当たらない。**「窓の中に無ければ全体を読む」**
    // という今の形が、規則を当てにしないぶん正しい
    //
    // ただし**全体を読むのは原寸が要るときだけ**にする。一覧のタイル（512px）
    // でここへ来ると、160x120しか持たない社（Minolta MRW・Sony SRF・
    // Phase One IIQ 等）は1枚ごとにファイル全体を読み、しかもその先に
    // 大きい絵は無い。取り込み元のSDカードを丸ごと読み直すことになる
    //
    // **2段目で読み切ったファイルは、ここへ来ない。** 先頭16MB**以内**に収まる
    // 大きさなら2段目が既に全体を走査済みで、同じ関数に同じバイトを渡し直すだけに
    // なる（HDR PQのCR3は13.9MBなので、これが無いと必ず空振りの全走査を1回挟む）。
    // **ちょうど16MBも「読み切った」側**——`read_head(path, SCAN_LIMIT)` は
    // `SCAN_LIMIT` バイトまで読むので、境目のファイルは既に全部見えている
    if min_long_edge >= USABLE_LONG_EDGE
        && std::fs::metadata(path).is_ok_and(|m| {
            let len = m.len() as usize;
            len > SCAN_LIMIT && len <= FULL_SCAN_LIMIT
        })
    {
        if let Some(whole) = read_head(path, FULL_SCAN_LIMIT) {
            if let Some(found) = scan_largest_jpeg(&whole) {
                if long_edge(found) >= min_long_edge {
                    return Some(found.to_vec());
                }
                keep(&mut best, found.to_vec());
            }
        }
    }

    // 4段目: Minoltaの `.mrw` は、埋め込みJPEGの**先頭1バイトを潰して**書く
    if let Some(bytes) = minolta_repaired_jpeg(path) {
        if long_edge(&bytes) >= min_long_edge {
            return Some(bytes);
        }
        keep(&mut best, bytes);
    }

    // 5段目: JPEGが1枚も無い形式（Leica DNG・Epson ERF・Hasselblad 3FR・
    // Phase One IIQ・Kodak DCR）。非圧縮RGBのプレビューを組み立てる
    let uncompressed = (|| {
        let big_endian = matches!(read_window(path, 0, 2).as_deref(), Some(b"MM"));
        let file = std::fs::File::open(path).ok()?;
        let mut reader = std::io::BufReader::new(file);
        let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
        uncompressed_preview(path, &exif, big_endian)
    })();
    if let Some(bytes) = uncompressed {
        keep(&mut best, bytes);
    }

    // 6段目: **プレビューがJPEGとは限らない。** HDR PQで撮ったCanonのCR3は、
    // 10ビットのPQを積むためにプレビューをHEVCで書く（JPEGでは表現できない）。
    // ここまでの5段は全部JPEGを探しているので、そういうファイルは必ず空振りする。
    //
    // 高いのは**デコード**（PRVWを起こす取り出しは、この機械で全体0.25〜0.4秒。
    // 内訳は測っていないが、大半はWICの中に居る）なので、**ここまでの絵が
    // 求められた大きさに届いていないときだけ**払う。「1枚も取れていないとき」で
    // 門を作ると、どこかに切手のJPEGが1枚あるだけでHEVCを丸ごと飛ばし、その切手を
    // 返してしまう——直そうとしている症状がそのまま残る（ゲート1のP1）
    if best.as_ref().is_none_or(|b| long_edge(b) < min_long_edge) {
        // 起こしても**いま持っている絵より小さい**なら、`keep` が捨てるだけ。
        // 払う前に降りる
        let floor = best.as_ref().map_or(0, |b| long_edge(b));
        if let Some(bytes) = cr3_hevc_preview(hevc_boxes, floor, min_long_edge) {
            keep(&mut best, bytes);
        }
    }
    best
}

/// TIFFが**申告している**プレビューのうち、表示できて一番大きいもの。
fn declared_preview(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;
    for (offset, len) in declared_previews(&exif) {
        if len == 0 {
            continue;
        }
        // まずEXIFパーサが保持しているバッファから切り出せるか試す
        // （サムネイルIFDのJPEGはここに入っている）
        if let Some(bytes) = exif.buf().get(offset..offset.saturating_add(len)) {
            if is_displayable_jpeg(bytes) {
                return Some(bytes.to_vec());
            }
        }
        // 申告の長さが実物と合わないファイルがある（切れたJPEGになる）。
        // 少し多めに読んで、終端はJPEG自身のセグメントから決める
        if let Some(window) = read_window(path, offset, len.saturating_mul(2).max(64 * 1024)) {
            if let Some(span) = jpeg_span(&window, 0) {
                if span.displayable {
                    return Some(window[..span.end].to_vec());
                }
            }
        }
    }
    None
}

/// Minoltaの `.mrw` が持つ「先頭を潰されたJPEG」を直して返す。
///
/// MRWは埋め込みJPEGのSOI（`FF D8`）の**先頭バイトを0で潰して**書く
/// （`00 D8 FF` で始まる）。1バイト戻せば普通のJPEGとして読める。
/// **`.mrw` のときだけ**試す——この3バイトの並びは生のセンサーデータにも
/// 普通に現れるので、他形式で拾うと絵にならない塊を掴む。
fn minolta_repaired_jpeg(path: &Path) -> Option<Vec<u8>> {
    if !path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mrw"))
    {
        return None;
    }
    let head = read_head(path, SCAN_LIMIT)?;
    let mut at = 0usize;
    let mut best: Option<Vec<u8>> = None;
    // ここで `?` を使って抜けないこと——末尾に達したときに、
    // 拾ってあった絵ごと捨ててしまう
    while let Some(rest) = head.get(at..) {
        let Some(found) = rest.windows(3).position(|w| w == [0x00, 0xD8, 0xFF]) else {
            break;
        };
        let start = at + found;
        // **写す前に確かめる**。`00 D8 FF` は生のセンサーデータにも普通に
        // 現れるので、外れるたびに残り（最大16MB）を丸ごと写していると、
        // 1枚のファイルで何度も繰り返すことになる（ゲート1のP2）
        if let Some(span) = jpeg_span_body(&head, start) {
            if span.displayable {
                let mut repaired = head[start..span.end].to_vec();
                repaired[0] = 0xFF; // 潰された先頭を戻す
                if best.as_ref().is_none_or(|b| b.len() < repaired.len()) {
                    best = Some(repaired);
                }
            }
        }
        at = start + 3;
    }
    best
}

/// CR3の `PRVW` / `THMB` 箱から取り出した、HEVCのプレビュー。
struct Cr3Hevc {
    /// 表示上の寸法（`CISZ` の符号化寸法とは別。1664x1080 に対し 1620x1080）
    width: u16,
    height: u16,
    /// HEVCデコーダ設定。HEIFへそのまま持っていく
    hvcc: Vec<u8>,
    /// 色空間（`nclx`）。HDR PQなら transfer=16。無いかもしれないので `Option`
    colr: Option<Vec<u8>>,
    /// チャンネル数とビット深度（HDR PQは 3チャンネル各10ビット）。同上
    pixi: Option<Vec<u8>>,
    /// 4バイト長前置のNAL列
    hevc: Vec<u8>,
}

impl Cr3Hevc {
    /// 表示寸法の長辺。**デコードせずに分かる**ので、起こす前の選別に使える。
    fn long_edge(&self) -> u32 {
        u32::from(self.width.max(self.height))
    }
}

/// CR3の中から `PRVW` / `THMB` 箱を探して中身を読む。
///
/// **HDR PQで撮ったCR3は、プレビューをJPEGではなくHEVCで書く。** 10ビットの
/// PQを積むためで、JPEGでは表現できない。この箱の中身は実質「ヘッダの無いHEIF」
/// なので、[`crate::heif::wrap_hevc_as_heif`] で包み直せばOSのデコーダに渡せる。
///
/// 箱の構造（Canon EOS R8 の実測）:
///
/// ```text
/// PRVW
///   ヘッダ16B  version/flags(4) ?(2) width(2) height(2) ?(2) 子箱の合計長(4)
///   CISZ(20)   符号化時の寸法（1664x1080）
///   hvcC(176)  HEVCデコーダ設定
///   colr(19)   nclx: primaries=9(BT.2020) transfer=16(PQ) matrix=9
///   pixi(16)   3チャンネル・各10ビット
///   IMGD(n)    先頭4Bが後続のNAL列の長さ、以降は4バイト長前置のNAL
/// ```
///
/// **版0の箱は形が違う**（通常のCR3。`PRVW` は子箱を持たずオフセット16から
/// 生JPEGが始まる。`THMB` は寸法の位置もずれる）。版1だけを受ける
/// ——版0の `PRVW` は寸法だけなら同じ位置に入っているので、値では区別できない。
fn cr3_hevc_box(head: &[u8], tag: &[u8; 4]) -> Option<Cr3Hevc> {
    /// 中に箱が入っているコンテナ。`PRVW` も `THMB` も `uuid` の下にある
    const CONTAINERS: &[&[u8; 4]] = &[b"moov", b"uuid"];

    fn walk(buf: &[u8], start: usize, end: usize, tag: &[u8; 4], depth: usize) -> Option<Cr3Hevc> {
        if depth > 4 {
            return None;
        }
        for (kind, b, e) in crate::heif::boxes(buf, start, end) {
            if &kind == tag {
                // 同じ階層に同名の箱が並ぶことは無いはずだが、1つ目の解釈に
                // 失敗しても残りを見る（`return` で打ち切らない）
                let Some(body) = buf.get(b..e) else {
                    continue; // 箱だけ飛ばす（この階層の走査は続ける）
                };
                if let Some(found) = parse(body) {
                    return Some(found);
                }
                continue;
            }
            if !CONTAINERS.iter().any(|k| **k == kind) {
                continue;
            }
            // `uuid` は先頭16バイトがUUID。ただし**プレビューを収める uuid は、
            // その後ろにさらに8バイトの欄を挟む**。実測（EOS R8）:
            //
            // ```text
            // 0x26018  00 0b fb f0  箱の大きさ
            // 0x2601c  uuid
            // 0x26020  ea f4 2b 5e 1c 98 4b 88 b9 fb b7 dc 40 6e 4d 16  UUID
            // 0x26030  00 00 00 00 00 00 00 01  ← 仕様に無い8バイト
            // 0x26038  00 0b fb d0  PRVW ...
            // ```
            //
            // `THMB` を収める uuid（`85c0b687-…`）にはこれが無い。仕様に
            // 書かれていないので、**両方の位置から読んでみて通るほうを採る**
            let starts: &[usize] = if kind == *b"uuid" { &[16, 24] } else { &[0] };
            for skip in starts {
                let Some(inner) = b.checked_add(*skip).filter(|s| *s <= e) else {
                    continue;
                };
                if let Some(found) = walk(buf, inner, e, tag, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// `IMGD` の中身からNAL列を切り出す。
    ///
    /// 先頭4Bの**申告どおりに切る**。後ろに詰め物が付いている個体で残り全部を
    /// 持っていくと、そのまま `mdat` に混ざってデコーダに拒まれる（ゲート1のP2）。
    ///
    /// **申告が使えないときは箱の残り全部へ倒す**（0、または中身に収まらない値）。
    /// 実物は「中身の長さ - 4」がそのまま入っているので、そこが食い違う個体は
    /// 形が違う——捨てて枠だけにするより、読ませてみるほうが得。
    fn imgd_nals(inner: &[u8]) -> Option<&[u8]> {
        let rest = inner.get(4..)?;
        let declared = u32::from_be_bytes(inner.get(..4)?.try_into().ok()?) as usize;
        Some(match rest.get(..declared) {
            Some(nals) if declared > 0 => nals,
            _ => rest,
        })
    }

    /// `PRVW` / `THMB` の中身（16バイトのヘッダ＋子箱の並び）を読む。
    fn parse(body: &[u8]) -> Option<Cr3Hevc> {
        // **版を確かめる。** 版0（通常のCR3）は子箱を持たず、`PRVW` は
        // オフセット16から生JPEGが始まる。**寸法は版0でも同じ位置に入っている**
        // ので、寸法を見ただけでは区別できない——`THMB` のほうは並びがずれる
        // （幅がオフセット4、高さが6）。版で弾くのが確実
        if body.first() != Some(&1) {
            return None;
        }
        let width = u16::from_be_bytes([*body.get(6)?, *body.get(7)?]);
        let height = u16::from_be_bytes([*body.get(8)?, *body.get(9)?]);
        if width == 0 || height == 0 {
            return None;
        }
        let (mut hvcc, mut colr, mut pixi, mut hevc) = (None, None, None, None);
        for (kind, b, e) in crate::heif::boxes(body, 16, body.len()) {
            let Some(inner) = body.get(b..e) else {
                continue;
            };
            match &kind {
                b"hvcC" => hvcc = Some(inner.to_vec()),
                b"colr" => colr = Some(inner.to_vec()),
                b"pixi" => pixi = Some(inner.to_vec()),
                // 先頭4Bは**後続のNAL列の長さ**（箱の中身から4を引いた値。
                // 箱そのものの長さではない）。その後ろが4バイト長前置のNAL列
                b"IMGD" => hevc = imgd_nals(inner).map(<[u8]>::to_vec),
                _ => {}
            }
        }
        Some(Cr3Hevc {
            width,
            height,
            hvcc: hvcc?,
            colr,
            pixi,
            hevc: hevc.filter(|d| !d.is_empty())?,
        })
    }

    walk(head, 0, head.len(), tag, 0)
}

/// 起こす順を決める。
///
/// **どちらを起こすかは、デコードする前に決められる。** 寸法は箱のヘッダに
/// 書いてあるので、`min_long_edge` に届かないと分かっている絵のために
/// デコード代を払わずに済む。
///
/// 順は「**足りるうち一番小さいもの**を先に、足りるものが無ければ大きいものから」。
/// 一覧のタイル（512px）が320x214のTHMBで満足してしまうと、並びがぼやけるうえ
/// `media.width/height` にもその寸法が入る（ゲート1のP2）。
fn decode_order(mut boxes: Vec<Cr3Hevc>, min_long_edge: u32) -> Vec<Cr3Hevc> {
    boxes.sort_by_key(Cr3Hevc::long_edge);
    let split = boxes.partition_point(|b| b.long_edge() < min_long_edge);
    let mut enough = boxes.split_off(split);
    boxes.reverse(); // 足りないものは大きい順（少しでもましな絵を残す）
    enough.extend(boxes);
    enough
}

/// CR3から、HEVCで入っているプレビューの箱を取り出す。
///
/// **デコードはしない。** 先頭を読んでいるあいだにここまで済ませておけば、
/// 16MBを持ち越さずに済む（3段目が最大128MBを読むので、抱えたままだと
/// ワーカーの数だけピークが積み上がる）。
fn cr3_hevc_boxes(path: &Path, head: &[u8]) -> Vec<Cr3Hevc> {
    if !is_bmff_raw_path(path) {
        return Vec::new();
    }
    [b"PRVW", b"THMB"]
        .into_iter()
        .filter_map(|tag| cr3_hevc_box(head, tag))
        .collect()
}

/// 取り出した箱を起こして、表示用JPEGとして返す。
///
/// **呼ぶのは、ここまでの5段で求められた大きさの絵が出なかったときだけ**
/// （[`embedded_preview_at_least`] の6段目）。`floor` は今持っている絵の長辺で、
/// それ以下にしかならない箱は起こさない——起こしても捨てるだけになる。
///
/// 返すのは**詰め直したJPEG**で、EXIFも色空間のタグも持たない。向きは
/// 呼び出し側がRAW側のEXIFから当てること。**絵が要らない呼び出しでここへ来ると、
/// 0.25〜0.4秒（別の機械では0.5秒近い）払って日付の無いJPEGを受け取る**ことになる
/// （[`crate::thumbs::read_exif_meta`] の、日付がプレビューにしか無い形式への
/// 救済経路。CR3は `CMT1`/`CMT2` から日付が取れるので実際には通らない）。
///
/// 色は**デコーダ任せ**。元はBT.2020のPQ（`colr` は包み直しに運ぶが、出すJPEGには
/// 付かない）で、WICはsRGBへ直して返す。直さないデコーダに同じ経路を通すと、
/// PQのままの画素がタグの無いJPEGに入る。
///
/// デコードはOS任せ（WindowsはWIC・macOSはImageIO）。**どちらも実測で通っている**
/// （macOSは2026-08-26に `Canon EOS R5m2` のCRAWで338ms）。デコーダを持たないOSでは
/// [`crate::heif::decode_mem`] が `None` を返し、従来どおり枠だけになる。
fn cr3_hevc_preview(boxes: Vec<Cr3Hevc>, floor: u32, min_long_edge: u32) -> Option<Vec<u8>> {
    if boxes.is_empty() {
        return None;
    }
    for found in &decode_order(boxes, min_long_edge) {
        if found.long_edge() <= floor {
            continue; // 起こしても今の絵に負ける
        }
        let heif = crate::heif::wrap_hevc_as_heif(
            &found.hvcc,
            found.width,
            found.height,
            found.colr.as_deref(),
            found.pixi.as_deref(),
            &found.hevc,
        );
        // 起こせないとき（コーデックが居ない・箱が壊れている）は次へ。
        // ここで諦めると、PRVWが読めないだけでサムネイルが丸ごと消える
        let Some(img) = crate::heif::decode_mem(&heif) else {
            continue;
        };
        // **元の間引きに合わせる。** Canonのこの絵は4:2:2（実測
        // `chroma_format_idc = 2`）なので、4:2:0で詰め直すと縦の色差を捨てる。
        // 読めないときは間引かない側へ倒す（ゲート1のP2）
        let chroma =
            crate::heif::chroma_from_hvcc(&found.hvcc).unwrap_or(crate::jpeg::ChromaSampling::Full);
        if let Some(jpeg) = encode_jpeg(img, chroma) {
            return Some(jpeg);
        }
    }
    None
}

/// `.mrw` の中の**TIFFの塊**（`TTW` ブロック）を返す。
///
/// MRWは独自のブロック構造（`\x00MRM` の下に `\x00PRD`・`\x00TTW`…）で、
/// EXIFは `TTW` の中に**TIFFのまま**入っている。ブロックを辿るだけなので
/// 読むのは先頭側だけで済む——`head` は呼び出し側が読んだ先頭
/// [`PATCHED_TIFF_HEAD`] バイト。
fn mrw_tiff_block(head: &[u8]) -> Option<Vec<u8>> {
    if head.get(..4)? != b"\x00MRM" {
        return None;
    }
    // 先頭の `\x00MRM` は「この後ろ全部の長さ」なので、中身は8バイト目から
    let mut at = 8usize;
    while at + 8 <= head.len() {
        let tag = head.get(at..at + 4)?;
        let len = u32::from_be_bytes(head.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        if len == 0 {
            return None;
        }
        if tag == b"\x00TTW" {
            let block = head.get(at + 8..(at + 8).saturating_add(len))?;
            return Some(block.to_vec());
        }
        at = at.checked_add(8)?.checked_add(len)?;
    }
    None
}

/// JPEGの長辺（ヘッダだけ読む。読めなければ0）。
fn long_edge(bytes: &[u8]) -> u32 {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map_or(0, |(w, h)| w.max(h))
}

/// [`patched_tiff_metadata`] の結果。
pub enum PatchedTiff {
    /// 版番号を直した（あるいはMRWの `TTW` を取り出した）先頭バイト列
    Patched(Vec<u8>),
    /// 読めたが、直す対象ではない（普通のTIFF・BigTIFF・TIFFですらない形式）
    NotApplicable,
    /// 読めなかった（権限・共有ロック・外付けが抜けた）
    Unreadable,
}

/// **中身は長さだけ出す。** `Patched` は先頭 [`PATCHED_TIFF_HEAD`] バイトを
/// 抱えているので、素朴に導出するとテストが落ちたときに256KBが流れる。
impl std::fmt::Debug for PatchedTiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Patched(buf) => write!(f, "Patched({}バイト)", buf.len()),
            Self::NotApplicable => write!(f, "NotApplicable"),
            Self::Unreadable => write!(f, "Unreadable"),
        }
    }
}

/// **TIFFのふりをしていない**TIFF系RAWのメタデータを、普通のEXIFとして
/// 読めるバイト列にして返す（`thumbs::read_exif` が使う）。
///
/// Olympus/OM の `.orf` は版番号（TIFFヘッダの3〜4バイト目。本来42）に
/// `RO`/`RS`、Panasonic の `.rw2` は `U` を書く。中身の作りは標準のTIFFなので、
/// **この2バイトを42に直すだけ**で普通のEXIFとして読める。直さないと
/// 撮影日時・カメラ名・向きが丸ごと落ち、**縦位置の写真が横倒しになる**。
///
/// 全部は読まない。返すのは先頭 [`PATCHED_TIFF_HEAD`] だけで、**ファイルと
/// 同じ長さまでゼロで嵩上げはしない**（ワーカーの数だけ数十MBを確保することに
/// なるため。ゲート1のP1）。TIFFは値をファイル内のoffsetで指すので、画素データの
/// ようにこの範囲の外を指す項目は「切れている」と言われる。読み手は
/// [`exif::Reader::continue_on_error`] でそれを飛ばし、読めた項目だけを受け取る
/// こと（`thumbs::read_raw_partial`）——撮影日時・カメラ名・向きは
/// いずれも先頭側にあるので、これで取れる
/// （実測: 先頭16KBでORF・RW2とも全項目が一致）。
///
/// **読めなかった**（`Unreadable`）と**直す対象ではない**（`NotApplicable`）は
/// 分けて返す。畳むと、一時的に読めないだけのファイルを「この形式には
/// メタデータが無い」と取り違え、後追いが「確かめた」印を付けてしまう
/// （[`crate::thumbs::backfilled_dimensions`]・ゲート1のP2）。
pub fn patched_tiff_metadata(path: &Path) -> PatchedTiff {
    // **先頭は一度だけ読む。** MRWの `TTW` 探しと版番号の直しは同じバイト列で
    // 足りるので、開き直す理由が無い（開き直しは「1度目は通ったのに2度目が
    // 落ちる」隙間を増やす）
    let Some(head) = read_head(path, PATCHED_TIFF_HEAD) else {
        return PatchedTiff::Unreadable;
    };
    // MRWは版番号ではなく**入れ物ごと独自**で、TIFFは中の `TTW` に入っている
    if let Some(block) = mrw_tiff_block(&head) {
        return PatchedTiff::Patched(block);
    }
    let Some(byte_order) = head.get(..2) else {
        return PatchedTiff::NotApplicable;
    };
    let big_endian = match byte_order {
        b"II" => false,
        b"MM" => true,
        _ => return PatchedTiff::NotApplicable, // TIFFですらない（CR3・RAF・X3F等）
    };
    // 名前は**位置**で付ける。`lo`/`hi` だと big-endian で逆になる
    let (Some(b2), Some(b3)) = (head.get(2).copied(), head.get(3).copied()) else {
        return PatchedTiff::NotApplicable;
    };
    let version = if big_endian {
        u16::from_be_bytes([b2, b3])
    } else {
        u16::from_le_bytes([b2, b3])
    };
    // 42（普通のTIFF）と43（BigTIFF）は直す対象ではない。前者はそもそも
    // ここへ来ないし、後者は構造が違うので版番号を替えても読めない
    if version == 42 || version == 43 {
        return PatchedTiff::NotApplicable;
    }
    // 返すのは**読んだ先頭ぶんだけ**。撮影日時も向きもカメラ名も先頭側に
    // あるので、ファイルと同じ長さまで0で嵩上げする必要はない——それをやると
    // サムネイルの並列ワーカーの数だけ数十MBずつ確保することになり、
    // 上限を超える大きさのファイルは向きを丸ごと落とす（ゲート1のP1）。
    // 読み手には [`exif::Reader::continue_on_error`] を使ってもらう:
    // 画素データのように**この範囲の外を指す項目は飛ばして**、
    // 先頭側に収まっている項目だけが読める
    let mut buf = head;
    let patched = if big_endian {
        [0x00, 0x2A]
    } else {
        [0x2A, 0x00]
    };
    // 版番号を読めている＝4バイト以上あることは上で確定している
    buf[2..4].copy_from_slice(&patched);
    PatchedTiff::Patched(buf)
}

/// ファイルの指定位置から最大 `len` バイト読む（足りなければ読めた分だけ）。
fn read_window(path: &Path, offset: usize, len: usize) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut buf = Vec::new();
    file.take(len.min(SCAN_LIMIT) as u64)
        .read_to_end(&mut buf)
        .ok()?;
    (!buf.is_empty()).then_some(buf)
}

/// ファイルの先頭を最大 `limit` バイト読む。
///
/// **1バイトも読めなければ `None`**（[`read_window`] と揃えた）。0バイトの
/// ファイルは「開けたが中身が無い」であって「読んで確かめられた」ではない
/// ——同期や書き戻しの途中でRAWが一時的に空になることは実際にあり、
/// そこへ後追いが「確かめた」印を付けると二度と直らない
/// （[`crate::thumbs::backfilled_dimensions`]・ゲート2のP2）。
/// 絵を探す側の呼び出しは、いずれも `None` を「ここでは見つからなかった」
/// として扱うので影響しない。
fn read_head(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(limit as u64).read_to_end(&mut buf).ok()?;
    (!buf.is_empty()).then_some(buf)
}

/// ISO-BMFF（CR3等）から**TIFF形式のメタデータブロック**を取り出す。
///
/// CR3はTIFFではないのでEXIFパーサが直接読めない。Canonは撮影情報を
/// `moov/uuid` の下の `CMT1`（IFD0: メーカー・機種・向き）と
/// `CMT2`（Exif IFD: 撮影日時）に、**中身はTIFFのまま**入れている。
/// このボックスの中身をそのままEXIFパーサへ渡せば、普通のJPEGと同じに読める。
///
/// 箱を辿るだけなので、読むのは先頭1MBだけ（画素データ `mdat` には触らない）。
///
/// **読めなかったときは `None`**、箱でなかった・箱に入っていなかったときは
/// 空の `Some`。呼ぶ側が「一時的に読めないだけ」と「この形式には無い」を
/// 見分けられないと、読めない行に「確かめた」印を付けてしまう
/// （[`crate::thumbs::backfilled_dimensions`]）。
pub fn bmff_metadata_blocks(path: &Path) -> Option<Vec<Vec<u8>>> {
    /// ISO-BMFFで中に箱が入っている（＝再帰して良い）コンテナ。
    /// `uuid` は先頭16バイトがUUIDで、その後ろに子の箱が続く
    const CONTAINERS: &[&[u8; 4]] = &[b"moov", b"trak", b"mdia", b"minf", b"stbl", b"uuid"];
    /// TIFF形式のメタデータが入っている箱（CMT1=IFD0, CMT2=Exif IFD）
    const METADATA: &[&[u8; 4]] = &[b"CMT1", b"CMT2", b"CMT3", b"CMT4"];

    fn walk(buf: &[u8], depth: usize, out: &mut Vec<Vec<u8>>) {
        // 壊れたファイルで無限に潜らない
        if depth > 6 {
            return;
        }
        let mut pos = 0usize;
        while pos + 8 <= buf.len() {
            let size = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
            let kind = &buf[pos + 4..pos + 8];
            // size=0 は「以降すべて」、size=1 は64ビット長。ここでは扱わず打ち切る
            let size = size as usize;
            if size < 8 || pos + size > buf.len() {
                return;
            }
            let body = &buf[pos + 8..pos + size];
            if METADATA.iter().any(|k| *k == kind) {
                out.push(body.to_vec());
            } else if CONTAINERS.iter().any(|k| *k == kind) {
                // uuidの先頭16バイトはUUIDそのもの。飛ばしてから中を見る
                let inner = if kind == b"uuid" && body.len() > 16 {
                    &body[16..]
                } else {
                    body
                };
                walk(inner, depth + 1, out);
            }
            pos += size;
        }
    }

    // moovはCR3ではファイル先頭側にある。画素データ（mdat）まで読まない
    let head = read_head(path, 1024 * 1024)?;
    let mut out = Vec::new();
    walk(&head, 0, &mut out);
    Some(out)
}

/// 原本（センサー）の寸法を、EXIFの申告から読む。**絵は1枚も起こさない**。
///
/// `media.width/height` に入れる値がこれ。RAWで配信するのは
/// [`embedded_preview_at_least`] が返す埋め込みプレビューで、**原本より小さいことが多い**
/// （HDR PQのCR3は1620x1080で、原本は6000x4000）。プレビューの寸法を原本として
/// 記録すると列の名前が嘘になるので、申告が読めるときはこちらを使う。
///
/// **どこに書いてあるかは社ごとに違う。** 手元のサンプル25件
/// （raw.pixls.us のCC0が24件＋EOS R8の非圧縮RAWが1件）を全部読んで確かめた結果:
///
/// | 読む場所 | 当たる社 | 例（プレビュー → 申告） |
/// |---|---|---|
/// | Exif IFDの `PixelXDimension`/`PixelYDimension`（この関数） | Canon CR2・Phase One IIQ・Sony ARW・Samsung SRW・Apple DNG | CR2(20D) 1536x1024 → 3504x2336 |
/// | ——（**DNGは社で割れる**。Appleは持ち、LeicaとBlackmagicは持たない） | | |
/// | CR3の `CMT1`（[`cr3_declared_dimensions`]） | Canon CR3 | R8 1620x1080 → 6000x4000 |
/// | **このパーサからは届かない** | Nikon NEF/NRW・Epson ERF・Hasselblad 3FR・Kodak DCR・Fujifilm RAF・Leica RWL/DNG・Panasonic RW2・Sigma X3F・Olympus ORF・Minolta MRW・Blackmagic DNG | 原寸はSubIFDの中にある |
/// | 申告はあるが読めない | Kodak KDC | Exif IFDに 3088x2310 が入っているが、`read_from_container` が `Truncated field value` でファイルごと拒む |
///
/// **IFD0の `ImageWidth` は使わない。** TIFF系RAWのIFD0は「そのIFDが持っている絵」の
/// 寸法で、そこにサムネイルを置く社がある——NEF(D2H)は160x120、Leica M8のDNGは320x240。
/// Pentax PEFは3936x2624と**実寸より大きい**（マスク領域込み）。当たる社が1つ増えるより、
/// 別の絵の寸法を原本と言い張るほうが害が大きい。
///
/// KDCの行は「無い」ではなく**取りに行けていない**（2026-08-25にゲート2が指摘し、
/// 実物で確かめた）。`patched_tiff_metadata` の側が使っている
/// `continue_on_error` を素のコンテナ読みにも回せば届くはずだが、撮影日時と
/// カメラ名の取り方まで変わるのでこのPRではやらない。
///
/// **JPEGやHEIFには使わない**（呼ぶ側の [`crate::thumbs::read_exif`] が落としている）。
/// 編集で縮めた写真は `PixelXDimension` が古いまま残ることがあり、実物と食い違う。
/// RAWは書き換えない形式なので、その心配が無い。
pub(crate) fn exif_declared_dimensions(exif: &exif::Exif) -> Option<(u32, u32)> {
    declared_dimensions(exif, Tag::PixelXDimension, Tag::PixelYDimension)
}

/// CR3の `CMT1` が申告する原本の寸法。
///
/// CR3のIFD0（＝`CMT1`）は**絵を持たない**。Canonが撮影時の寸法をそのまま書いているので、
/// TIFF系RAWと違ってサムネイルの寸法にはならない。手元の2機種で、`moov` の `CRAW`
/// トラック（＝生データそのものの寸法）と一致することを確かめた: R8が6000x4000、
/// R6が3408x2272（1.6xクロップで撮った1枚なので、3408のほうが正しい）。
///
/// 呼ぶ側（[`crate::thumbs::read_exif`]）は `CMT1`〜`CMT4` を順に渡し、最初に
/// 当たったものを採る。**`CMT1` が先頭に来る前提**で、他の箱は `ImageWidth` を
/// 持たないので実際には当たらない（手元の2機種で確認）。
pub(crate) fn cr3_declared_dimensions(exif: &exif::Exif) -> Option<(u32, u32)> {
    declared_dimensions(exif, Tag::ImageWidth, Tag::ImageLength)
}

/// 2つのタグを幅・高さとして読む。**両方揃って0でないときだけ**返す
/// （片方しか入っていない申告を「幅だけ分かった」と扱わない）。
fn declared_dimensions(exif: &exif::Exif, width: Tag, height: Tag) -> Option<(u32, u32)> {
    let uint = |tag: Tag| -> Option<u32> {
        exif.get_field(tag, In::PRIMARY)?
            .value
            .get_uint(0)
            .filter(|v| *v > 0)
    };
    Some((uint(width)?, uint(height)?))
}

/// 中身がISO-BMFF（箱の入れ子）のRAWか。いまはCanonのCR3だけ。
///
/// [`bmff_metadata_blocks`] は箱でないファイルにも空を返すが、その前に1MB読む。
/// **原寸の申告のためだけに読み直さない**よう、拡張子で先に切るために要る。
pub fn is_bmff_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cr3"))
}

/// パスの拡張子がRAWか。
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_raw_extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// テスト用の小さなJPEGを作る（内容は問わないが、デコードできること）。
    fn jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::new(width, height);
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    /// TIFFのIFDエントリ1つぶん。
    struct Entry {
        tag: u16,
        /// 3=SHORT, 4=LONG
        kind: u16,
        values: Vec<u32>,
    }

    fn entry(tag: u16, kind: u16, values: &[u32]) -> Entry {
        Entry {
            tag,
            kind,
            values: values.to_vec(),
        }
    }

    /// 任意のIFDと付随データを持つTIFFを組み立てる（バイト順を選べる）。
    ///
    /// 実物のRAWは各社バラバラなので、テストは「構造」を作って確かめる。
    fn build_tiff(entries: &[Entry], blobs: &[(usize, Vec<u8>)], big_endian: bool) -> Vec<u8> {
        let u16b = |v: u16| -> Vec<u8> {
            if big_endian {
                v.to_be_bytes().to_vec()
            } else {
                v.to_le_bytes().to_vec()
            }
        };
        let u32b = |v: u32| -> Vec<u8> {
            if big_endian {
                v.to_be_bytes().to_vec()
            } else {
                v.to_le_bytes().to_vec()
            }
        };

        let mut buf: Vec<u8> = if big_endian {
            vec![0x4D, 0x4D, 0x00, 0x2A]
        } else {
            vec![0x49, 0x49, 0x2A, 0x00]
        };
        buf.extend(u32b(8));

        // 値が4バイトに収まらないエントリは、IFDの後ろへ置く
        let ifd_end = 8 + 2 + entries.len() * 12 + 4;
        let mut extra: Vec<u8> = Vec::new();
        let mut ifd: Vec<u8> = u16b(entries.len() as u16);
        for e in entries {
            ifd.extend(u16b(e.tag));
            ifd.extend(u16b(e.kind));
            ifd.extend(u32b(e.values.len() as u32));
            let width = if e.kind == 3 { 2 } else { 4 };
            let size = width * e.values.len();
            let mut packed: Vec<u8> = Vec::new();
            for v in &e.values {
                packed.extend(if e.kind == 3 {
                    u16b(*v as u16)
                } else {
                    u32b(*v)
                });
            }
            if size <= 4 {
                packed.resize(4, 0);
                ifd.extend(packed);
            } else {
                ifd.extend(u32b((ifd_end + extra.len()) as u32));
                extra.extend(packed);
            }
        }
        ifd.extend(u32b(0)); // 次のIFDは無し
        buf.extend(ifd);
        buf.extend(extra);

        // 指定オフセットへデータ（ストリップ等）を置く
        for (offset, bytes) in blobs {
            if buf.len() < *offset {
                buf.resize(*offset, 0);
            }
            buf.extend_from_slice(bytes);
        }
        buf
    }

    /// 非圧縮プレビューを持つテスト用TIFFの仕様。
    struct PreviewSpec {
        width: u32,
        height: u32,
        color: [u8; 3],
        /// ストリップ分割数
        strips: u32,
        /// 1成分あたりのビット数
        bits: u32,
        big_endian: bool,
    }

    /// 非圧縮RGBのプレビューを持つTIFFを書く（Leica DNG・Epson ERF等の縮図）。
    fn tiff_with_uncompressed_preview(
        dir: &Path,
        name: &str,
        spec: PreviewSpec,
    ) -> std::path::PathBuf {
        let PreviewSpec {
            width,
            height,
            color,
            strips,
            bits,
            big_endian,
        } = spec;
        let rows_per_strip = height.div_ceil(strips);
        let bytes_per_row = width * 3 * (bits / 8);
        let mut offsets = Vec::new();
        let mut counts = Vec::new();
        let mut blobs = Vec::new();
        let mut cursor = 4096usize; // IFDと重ならない位置から置く
        let mut remaining = height;
        for _ in 0..strips {
            let rows = rows_per_strip.min(remaining);
            remaining -= rows;
            let mut data = Vec::new();
            for _ in 0..(rows * width) {
                for channel in color {
                    if bits == 8 {
                        data.push(channel);
                    } else if big_endian {
                        data.extend_from_slice(&[channel, 0]);
                    } else {
                        data.extend_from_slice(&[0, channel]);
                    }
                }
            }
            offsets.push(cursor as u32);
            counts.push(rows * bytes_per_row);
            cursor += data.len();
            let at = *offsets.last().unwrap() as usize;
            blobs.push((at, data));
        }

        let entries = vec![
            entry(256, 4, &[width]),            // ImageWidth
            entry(257, 4, &[height]),           // ImageLength
            entry(258, 3, &[bits, bits, bits]), // BitsPerSample
            entry(259, 3, &[1]),                // Compression = 非圧縮
            entry(262, 3, &[2]),                // Photometric = RGB
            entry(273, 4, &offsets),            // StripOffsets
            entry(277, 3, &[3]),                // SamplesPerPixel
            entry(278, 4, &[rows_per_strip]),   // RowsPerStrip
            entry(279, 4, &counts),             // StripByteCounts
            entry(284, 3, &[1]),                // PlanarConfiguration
        ];
        let path = dir.join(name);
        std::fs::write(&path, build_tiff(&entries, &blobs, big_endian)).unwrap();
        path
    }

    #[test]
    fn builds_a_preview_from_uncompressed_rgb() {
        let dir = tempfile::tempdir().unwrap();
        // 複数ストリップに分かれていても1枚に繋がる
        let path = tiff_with_uncompressed_preview(
            dir.path(),
            "leica.dng",
            PreviewSpec {
                width: 32,
                height: 24,
                color: [200, 100, 50],
                strips: 4,
                bits: 8,
                big_endian: false,
            },
        );
        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let img = image::load_from_memory(&preview).unwrap().to_rgb8();
        assert_eq!((img.width(), img.height()), (32, 24));
        // JPEGは非可逆なので厳密一致はしない。色味が保たれていれば良い
        let px = img.get_pixel(16, 12).0;
        assert!(
            px[0] > 150 && (60..140).contains(&px[1]) && px[2] < 100,
            "色が保たれる: {px:?}"
        );
    }

    #[test]
    fn a_big_endian_uncompressed_preview_reads_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = tiff_with_uncompressed_preview(
            dir.path(),
            "epson.erf",
            PreviewSpec {
                width: 16,
                height: 16,
                color: [30, 180, 90],
                strips: 1,
                bits: 8,
                big_endian: true,
            },
        );
        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let img = image::load_from_memory(&preview).unwrap();
        assert_eq!((img.width(), img.height()), (16, 16));
    }

    #[test]
    fn a_sixteen_bit_uncompressed_preview_is_read_with_its_brightness_corrected() {
        let dir = tempfile::tempdir().unwrap();
        // 16ビットのプレビューはリニア（ガンマ未適用）で入っている。
        // 上位8ビットをそのまま使うと真っ暗になるので補正が要る
        let path = tiff_with_uncompressed_preview(
            dir.path(),
            "kodak.dcr",
            PreviewSpec {
                width: 16,
                height: 16,
                color: [40, 40, 40],
                strips: 1,
                bits: 16,
                big_endian: true,
            },
        );
        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let img = image::load_from_memory(&preview).unwrap().to_rgb8();
        let px = img.get_pixel(8, 8).0;
        assert!(px[0] > 80, "リニアのまま暗く出ていない: {px:?}");
    }

    #[test]
    fn raw_sensor_data_is_not_used_as_a_preview() {
        // Photometric=32803（CFA）は現像していないので絵にならない。
        // 非圧縮でも選んではいけない
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("raw-only.dng");
        let entries = vec![
            entry(256, 4, &[64]),
            entry(257, 4, &[64]),
            entry(258, 3, &[8, 8, 8]),
            entry(259, 3, &[1]),
            entry(262, 3, &[32803]), // CFA
            entry(273, 4, &[4096]),
            entry(277, 3, &[3]),
            entry(279, 4, &[64 * 64 * 3]),
        ];
        let blob = vec![128u8; 64 * 64 * 3];
        std::fs::write(&path, build_tiff(&entries, &[(4096, blob)], false)).unwrap();
        assert!(embedded_preview(&path).is_none());
    }

    #[test]
    fn previews_come_out_of_the_real_samples() {
        // 実物のRAWは各社バラバラなので、手元にサンプルがある人だけ走る
        // （raw.pixls.us のCC0サンプルを想定。環境変数が無ければskip）
        let Ok(dir) = std::env::var("PICTKURA_RAW_SAMPLES") else {
            return;
        };
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(is_raw_extension)
            {
                continue;
            }
            // プレビューを持たないファイルもあるので、
            // 「取れたなら必ずデコードできる」ことを確かめる
            if let Some(preview) = embedded_preview(&path) {
                image::load_from_memory(&preview)
                    .unwrap_or_else(|e| panic!("{}: デコードできない: {e}", path.display()));
                checked += 1;
            }
        }
        assert!(checked > 0, "サンプルが1件も見つからない: {dir}");
    }

    /// 「IFD0にプレビューJPEGを申告するTIFF」を組み立てる（CR2やNEFの縮図）。
    fn tiff_with_declared_preview(dir: &Path, name: &str, jpeg: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut buf: Vec<u8> = Vec::new();
        // TIFFヘッダ（リトルエンディアン、IFD0は8バイト目から）
        buf.extend_from_slice(b"II\x2a\x00");
        buf.extend_from_slice(&8u32.to_le_bytes());

        // IFD0: エントリ2つ（JPEGInterchangeFormat=513, その長さ=514）
        let entry_count: u16 = 2;
        let ifd_size = 2 + entry_count as usize * 12 + 4;
        let jpeg_offset = (8 + ifd_size) as u32;

        buf.extend_from_slice(&entry_count.to_le_bytes());
        for (tag, value) in [(513u16, jpeg_offset), (514u16, jpeg.len() as u32)] {
            buf.extend_from_slice(&tag.to_le_bytes());
            buf.extend_from_slice(&4u16.to_le_bytes()); // type = LONG
            buf.extend_from_slice(&1u32.to_le_bytes()); // count
            buf.extend_from_slice(&value.to_le_bytes());
        }
        buf.extend_from_slice(&0u32.to_le_bytes()); // 次のIFDは無し
        buf.extend_from_slice(jpeg);

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&buf).unwrap();
        path
    }

    #[test]
    fn recognises_raw_extensions() {
        for ext in ["CR2", "cr3", "arw", "NEF", "dng", "raf"] {
            assert!(is_raw_extension(ext), "{ext} はRAW");
        }
        for ext in ["jpg", "png", "webp", "mp4", ""] {
            assert!(!is_raw_extension(ext), "{ext} はRAWではない");
        }
    }

    #[test]
    fn lifts_the_preview_out_of_the_tiff_declaration() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = jpeg_bytes(160, 120);
        let path = tiff_with_declared_preview(dir.path(), "sample.cr2", &jpeg);

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        assert_eq!(preview, jpeg, "カメラが書いたJPEGをそのまま返す");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 120));
    }

    #[test]
    fn a_stamp_sized_declaration_still_sends_us_looking_for_a_bigger_picture() {
        // Nikon（NEF・NRW）の縮図: TIFFには160x120の切手だけを申告し、
        // 原寸のJPEGは申告の無い場所（SubIFD）に置く。申告を鵜呑みにすると
        // **全画面表示が160x120**になる（実測: Z6III・COOLPIX A1000）
        let dir = tempfile::tempdir().unwrap();
        let stamp = jpeg_bytes(160, 120);
        let full = jpeg_bytes(1600, 1200);
        let path = tiff_with_declared_preview(dir.path(), "sample.nef", &stamp);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&full).unwrap();
        drop(file);

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (1600, 1200),
            "申告の切手ではなく、後ろにある原寸を選ぶ"
        );
    }

    #[test]
    fn the_search_stops_once_a_usable_size_is_found() {
        // 逆に、申告が原寸級ならファイル全体を読み直さない（CR2・ARW・PEF）。
        // 後ろにもっと大きい絵があっても、探しに行く値段（実測100〜350ms）は
        // 払わない
        let dir = tempfile::tempdir().unwrap();
        let declared = jpeg_bytes(1600, 1200);
        let bigger = jpeg_bytes(2400, 1800);
        let path = tiff_with_declared_preview(dir.path(), "sample.cr2", &declared);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&bigger).unwrap();
        drop(file);

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (1600, 1200),
            "使える大きさが申告されていたら、それで打ち切る"
        );
    }

    #[test]
    fn a_jpeg_with_a_broken_head_is_repaired_past_the_wrong_candidates() {
        // Minoltaの `.mrw` は埋め込みJPEGのSOI（`FF D8`）の**先頭1バイトを
        // 0で潰して**書く。この3バイトの並びは生のセンサーデータにも普通に
        // 現れるので、外れの候補が何度も当たる。**写す前に確かめる**ように
        // しないと、外れるたびに残り（最大16MB）を丸ごと写す（ゲート1のP2）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.mrw");
        let mut buf = b" MRM".to_vec();
        for _ in 0..64 {
            buf.extend_from_slice(&[0x00, 0xD8, 0xFF]); // センサーデータ側の空似
            buf.extend_from_slice(&[0x12; 4096]);
        }
        let mut jpeg = jpeg_bytes(640, 480);
        jpeg[0] = 0x00; // 実物と同じように潰しておく
        buf.extend_from_slice(&jpeg);
        std::fs::write(&path, &buf).unwrap();

        let preview = embedded_preview(&path).expect("直して取り出せる");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (640, 480));
    }

    #[test]
    fn the_whole_file_is_not_read_when_the_grid_size_suffices() {
        // 一覧のタイル（512px）は取り込み元のSDカードにも並ぶ。160x120しか
        // 持たない社（Minolta MRW・Sony SRF・Phase One IIQ）でここから
        // 全体走査に降りると、**1枚ごとにファイル全体を読む**うえに、
        // その先に大きい絵は無い（ゲート1のP1）
        let dir = tempfile::tempdir().unwrap();
        let stamp = jpeg_bytes(160, 120);
        let far = jpeg_bytes(1600, 1200);
        let path = tiff_with_declared_preview(dir.path(), "sample.mrw", &stamp);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        // 先頭16MBの走査には入らない位置へ大きい絵を置く
        file.write_all(&vec![0u8; SCAN_LIMIT]).unwrap();
        file.write_all(&far).unwrap();
        drop(file);

        let tile = embedded_preview_at_least(&path, 512).expect("小さくても絵は返る");
        assert_eq!(
            image::load_from_memory(&tile).unwrap().width(),
            160,
            "タイルの大きさで足りる場面では、後ろまで探しに行かない"
        );

        let full = embedded_preview(&path).expect("プレビューが取れる");
        assert_eq!(
            image::load_from_memory(&full).unwrap().width(),
            1600,
            "原寸が要る場面では、今までどおり後ろまで探す"
        );
    }

    #[test]
    fn with_only_small_pictures_the_largest_is_returned() {
        // どこにも原寸が無い形式もある。手を尽くしても届かないときは
        // **一番大きかった絵**を返す（何も出さないよりよい）
        let dir = tempfile::tempdir().unwrap();
        let stamp = jpeg_bytes(160, 120);
        let small = jpeg_bytes(720, 480);
        let path = tiff_with_declared_preview(dir.path(), "sample.dng", &stamp);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(&small).unwrap();
        drop(file);

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (720, 480));
    }

    #[test]
    fn tiff_with_its_own_version_number_is_patched_to_be_readable() {
        // ORF（Olympus/OM）は "IIRO"、RW2（Panasonic）は "IIU\x00" と、
        // TIFFの版番号（本来42）に独自の値を書く。直さないと撮影日時も
        // カメラ名も向きも丸ごと落ち、**縦位置の写真が横倒しになる**
        let dir = tempfile::tempdir().unwrap();
        for (name, big_endian, version) in [
            ("sample.orf", false, *b"RO"),
            ("sample.orf", true, *b"OR"),
            ("sample.rw2", false, [b'U', 0x00]),
        ] {
            let mut buf = build_tiff(&[entry(274, 3, &[6])], &[], big_endian);
            buf[2] = version[0];
            buf[3] = version[1];
            let path = dir.path().join(name);
            std::fs::write(&path, &buf).unwrap();

            // 素のパーサは読めない（だから直す必要がある）
            let file = std::fs::File::open(&path).unwrap();
            assert!(
                exif::Reader::new()
                    .read_from_container(&mut std::io::BufReader::new(file))
                    .is_err(),
                "{name}: 版番号が独自のままでは読めない"
            );

            let PatchedTiff::Patched(patched) = patched_tiff_metadata(&path) else {
                panic!("{name}: 版番号を直せる");
            };
            let exif = exif::Reader::new().read_raw(patched).expect("直せば読める");
            assert_eq!(
                exif.get_field(Tag::Orientation, In::PRIMARY)
                    .and_then(|f| f.value.get_uint(0)),
                Some(6),
                "{name}: 向きが読める"
            );
        }
    }

    #[test]
    fn patching_the_version_number_does_not_allocate_the_whole_file() {
        // サムネイルのワーカーは並列に走る。1枚ごとにファイル長ぶん0で
        // 嵩上げすると、数十MBのORF・RW2が同時に何枚も乗るうえ、上限を
        // 超える大きさのファイルは向きを丸ごと落とす（ゲート1のP1）
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.orf");
        let mut buf = build_tiff(&[entry(274, 3, &[6])], &[], false);
        buf[2] = b'R';
        buf[3] = b'O';
        buf.resize(4 * 1024 * 1024, 0); // 画素データのつもりの重し
        std::fs::write(&path, &buf).unwrap();

        let PatchedTiff::Patched(patched) = patched_tiff_metadata(&path) else {
            panic!("版番号を直せる");
        };
        assert!(
            patched.len() <= PATCHED_TIFF_HEAD,
            "読むのは先頭ぶんだけ: {}",
            patched.len()
        );
    }

    #[cfg(windows)]
    #[test]
    fn an_unreadable_orf_is_told_apart_from_a_non_applicable_one() {
        // 「直す対象ではない」に畳むと、一時的に読めないだけのORF・RW2に
        // 後追いが「確かめた」印を付けて二度と直らない（ゲート1の4周目のP2）
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.orf");
        let mut buf = build_tiff(&[entry(274, 3, &[6])], &[], false);
        buf[2] = b'R';
        buf[3] = b'O';
        std::fs::write(&path, &buf).unwrap();
        assert!(matches!(
            patched_tiff_metadata(&path),
            PatchedTiff::Patched(_)
        ));

        // 共有を許さずに開いたまま持つ
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(
            matches!(patched_tiff_metadata(&path), PatchedTiff::Unreadable),
            "読めないのと直す対象でないのは別"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_cr3_whose_boxes_cannot_be_walked_is_told_apart_from_an_empty_one() {
        // CR3はコンテナ読みも版番号の直しも素通りするので、ここが3度目の
        // `File::open` になる。**空の `Vec` に畳むと**、前の2回が通った後に
        // 共有ロックが掛かった1枚を「箱にメタデータが無いCR3」と取り違え、
        // 後追いが「確かめた」印を付けて二度と直らない
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.cr3");
        // 箱としては正しいが `CMT*` を持たない個体（＝空の `Some` が正解）
        std::fs::write(&path, b"\x00\x00\x00\x0cftypcrx ").unwrap();
        assert_eq!(
            bmff_metadata_blocks(&path).expect("読める").len(),
            0,
            "読めて、箱が無いだけ"
        );

        // 共有を許さずに開いたまま持つ
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(
            bmff_metadata_blocks(&path).is_none(),
            "読めないのと箱が無いのは別"
        );
    }

    #[test]
    fn an_ordinary_tiff_is_left_alone() {
        // 版番号42はそのまま読めるので直す対象ではない。BigTIFF（43）は
        // 構造が違い、版番号を替えても読めないので触らない
        let dir = tempfile::tempdir().unwrap();
        for version in [42u16, 43] {
            let mut buf = build_tiff(&[entry(274, 3, &[6])], &[], false);
            buf[2..4].copy_from_slice(&version.to_le_bytes());
            let path = dir.path().join("sample.dng");
            std::fs::write(&path, &buf).unwrap();
            assert!(
                matches!(patched_tiff_metadata(&path), PatchedTiff::NotApplicable),
                "版{version}"
            );
        }
        // TIFFですらないファイル（CR3・RAF・X3F）も対象外
        let path = dir.path().join("sample.cr3");
        std::fs::write(&path, b"\x00\x00\x00\x18ftypcrx ").unwrap();
        assert!(matches!(
            patched_tiff_metadata(&path),
            PatchedTiff::NotApplicable
        ));
    }

    #[test]
    fn jpeg_blocks_are_found_even_without_a_declaration() {
        // CR3やRAFのように、TIFFの申告が無くJPEGを抱えているだけのファイル
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.cr3");
        let small = jpeg_bytes(16, 16);
        let large = jpeg_bytes(320, 240);
        let mut buf = b"ftypcrx \x00\x00\x00\x00".to_vec();
        buf.extend_from_slice(&small);
        buf.extend_from_slice(b"\x00\x00moov");
        buf.extend_from_slice(&large);
        std::fs::write(&path, &buf).unwrap();

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (320, 240),
            "小さい方ではなく大きい方を選ぶ"
        );
    }

    #[test]
    fn lifts_the_metadata_boxes_out_of_bmff() {
        // CR3の構造を最小限で再現: moov > uuid > CMT1
        fn box_of(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.cr3");

        let cmt1 = box_of(b"CMT1", b"II*\x00metadata-here");
        let mut uuid_body = vec![0u8; 16]; // UUID
        uuid_body.extend_from_slice(&cmt1);
        let moov = box_of(b"moov", &box_of(b"uuid", &uuid_body));
        let mut file = box_of(b"ftyp", b"crx ");
        file.extend_from_slice(&moov);
        std::fs::write(&path, &file).unwrap();

        let blocks = bmff_metadata_blocks(&path).expect("読める");
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].starts_with(b"II"), "TIFFの中身がそのまま出る");
    }

    /// Exif IFD（`PixelXDimension` 等）をぶら下げたTIFFを組み立てる。
    ///
    /// `PixelXDimension` はIFD0に置いてもパーサが別のタグとして読む
    /// （文脈が違う）ので、実物と同じく `ExifIFDPointer` の先へ置く。
    fn tiff_with_exif_ifd(ifd0: &[Entry], exif_ifd: &[(u16, u32)]) -> Vec<u8> {
        /// Exif IFDを置く場所。IFD0とその付随データより十分後ろ
        const AT: usize = 0x400;

        let mut sub: Vec<u8> = (exif_ifd.len() as u16).to_le_bytes().to_vec();
        for (tag, value) in exif_ifd {
            sub.extend(tag.to_le_bytes());
            sub.extend(4u16.to_le_bytes()); // LONG
            sub.extend(1u32.to_le_bytes()); // 値は1つ
            sub.extend(value.to_le_bytes()); // 4バイトに収まるので直接置く
        }
        sub.extend(0u32.to_le_bytes()); // 次のIFDは無し

        let mut entries: Vec<Entry> = Vec::new();
        for e in ifd0 {
            entries.push(entry(e.tag, e.kind, &e.values));
        }
        entries.push(entry(0x8769, 4, &[AT as u32])); // ExifIFDPointer
        build_tiff(&entries, &[(AT, sub)], false)
    }

    #[test]
    fn reads_the_original_size_from_the_exif_ifd_declaration() {
        // Canon CR2・Sony ARW・Phase One IIQ 等はここに原寸を書く。
        // IFD0のImageWidthは埋め込みプレビューの寸法（実測: CR2 20Dで1536x1024）
        let buf = tiff_with_exif_ifd(
            &[entry(256, 4, &[1536]), entry(257, 4, &[1024])],
            &[(0xA002, 3504), (0xA003, 2336)],
        );
        let exif = exif::Reader::new().read_raw(buf).unwrap();
        assert_eq!(exif_declared_dimensions(&exif), Some((3504, 2336)));
    }

    #[test]
    fn imagewidth_in_ifd0_is_not_read_as_the_original_size() {
        // Nikon NEFのIFD0は160x120の切手を指す。ここを原寸と信じると、
        // 2400万画素の写真が160x120としてDBに入る
        let buf = build_tiff(&[entry(256, 4, &[160]), entry(257, 4, &[120])], &[], false);
        let exif = exif::Reader::new().read_raw(buf).unwrap();
        assert_eq!(exif_declared_dimensions(&exif), None);
        // CR3の `CMT1` だけは別。あちらのIFD0は絵を持たない
        assert_eq!(cr3_declared_dimensions(&exif), Some((160, 120)));
    }

    #[test]
    fn a_declaration_with_only_one_side_is_not_used() {
        let buf = tiff_with_exif_ifd(&[], &[(0xA002, 3504)]);
        let exif = exif::Reader::new().read_raw(buf).unwrap();
        assert_eq!(exif_declared_dimensions(&exif), None);
    }

    #[test]
    fn a_declaration_of_zero_is_not_used() {
        let buf = tiff_with_exif_ifd(&[], &[(0xA002, 0), (0xA003, 2336)]);
        let exif = exif::Reader::new().read_raw(buf).unwrap();
        assert_eq!(exif_declared_dimensions(&exif), None);
    }

    #[test]
    fn only_cr3_sends_us_to_the_bmff_boxes() {
        // 原寸の申告のために、TIFF系RAWで1MB読み直さないための門
        assert!(is_bmff_raw_path(Path::new("a.CR3")));
        assert!(is_bmff_raw_path(Path::new("a.cr3")));
        assert!(!is_bmff_raw_path(Path::new("a.cr2")));
        assert!(!is_bmff_raw_path(Path::new("a.nef")));
    }

    #[test]
    fn a_broken_bmff_still_terminates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.cr3");
        // サイズが自分より大きい箱（壊れている）
        let mut buf = 999_999u32.to_be_bytes().to_vec();
        buf.extend_from_slice(b"moov");
        std::fs::write(&path, &buf).unwrap();
        assert!(bmff_metadata_blocks(&path).expect("読める").is_empty());
    }

    #[test]
    fn a_file_holding_no_jpeg_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.arw");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        assert!(embedded_preview(&path).is_none());
    }

    #[test]
    fn a_jpeg_cut_off_partway_is_not_picked_up() {
        // SOIはあるがEOIが無い（壊れたRAW）。無理に返すとデコードで落ちる
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.nef");
        let mut buf = vec![0u8; 64];
        buf.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]);
        buf.extend_from_slice(&[0u8; 512]);
        std::fs::write(&path, &buf).unwrap();
        assert!(embedded_preview(&path).is_none());
    }

    /// 表示用JPEGは mozjpeg で詰める（`image` クレートのエンコーダに戻っていない）。
    ///
    /// 同じ4:4:4で並べて3.7倍の差（内訳は `bench --heif-encode` の列）。
    /// 黙って戻ると詰め直しが3.7倍に伸びるが、**絵は出る**のでテストでしか
    /// 気づけない
    #[test]
    fn the_display_jpeg_is_packed_with_mozjpeg() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x * 4) as u8, (y * 5) as u8, 200])
        }));
        let ours = encode_jpeg(img.clone(), crate::jpeg::ChromaSampling::Full).unwrap();

        let mut theirs = Vec::new();
        let encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut theirs, DISPLAY_QUALITY);
        img.to_rgb8().write_with_encoder(encoder).unwrap();
        assert_ne!(ours, theirs, "image クレートのエンコーダに戻っている");

        let back = image::load_from_memory(&ours).unwrap();
        assert_eq!((back.width(), back.height()), (64, 48));
    }

    /// 間引きの指定が本体からエンコーダまで届いている。
    ///
    /// 届いていないと libjpeg v6 の既定（4:2:0）で全部が出る——TIFFや非圧縮
    /// プレビューを4:4:4で出す判断が、黙って無かったことになる
    #[test]
    fn the_chroma_setting_reaches_the_encoder() {
        // **色差が大きく動く絵**にすると、間引きの差が大きさに出る
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(256, 256, |x, y| {
            image::Rgb([128, (x % 256) as u8, (y % 256) as u8])
        }));
        let full = encode_jpeg(img.clone(), crate::jpeg::ChromaSampling::Full)
            .unwrap()
            .len();
        let half = encode_jpeg(img, crate::jpeg::ChromaSampling::Half)
            .unwrap()
            .len();
        assert!(
            half < full,
            "間引きが効いていない: 4:4:4={full} 4:2:0={half}"
        );
    }

    /// 透過は白へ重ねてから詰める（JPEGは透過を持てない）。
    ///
    /// エンコーダを替えても、ここは通り道が変わっただけで意味は同じ
    #[test]
    fn transparency_is_flattened_onto_white_before_packing() {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            32,
            32,
            image::Rgba([0, 0, 0, 0]),
        ));
        let bytes = encode_jpeg(img, crate::jpeg::ChromaSampling::Half).unwrap();
        let back = image::load_from_memory(&bytes).unwrap().to_rgb8();
        let p = back.get_pixel(16, 16);
        assert!(
            p.0.iter().all(|c| *c > 240),
            "透明な部分が白になっていない: {:?}",
            p.0
        );
    }
}

#[cfg(test)]
mod cr3_hevc_tests {
    //! CanonのHDR PQのCR3（プレビューがHEVC）の取り出しと、HEIFへの包み直し。
    //!
    //! 実物のCR3は1ファイル14MBあってリポジトリに置けないので、**箱だけを合成**して
    //! 組み立ての筋を固める。画素は起こさない——起こせるかどうかはOSのデコーダ次第で、
    //! CIには居ない（実物での確認は `PICTKURA_RAW_SAMPLES` のオプトイン側）。

    use super::cr3_hevc_box;
    use crate::heif::wrap_hevc_as_heif;

    /// ISO-BMFFの箱を1つ組む。
    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// 版1の `PRVW` / `THMB` の中身（16バイトのヘッダ＋子箱）。
    fn preview_body(width: u16, height: u16, hvcc: &[u8], hevc: &[u8]) -> Vec<u8> {
        let mut b = vec![0x01, 0, 0, 0, 0, 0x02];
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&[0xff, 0xff]);
        b.extend_from_slice(&0u32.to_be_bytes()); // 子箱の合計長（読まない）
        b.extend_from_slice(&boxed(b"CISZ", &[0u8; 12]));
        b.extend_from_slice(&boxed(b"hvcC", hvcc));
        b.extend_from_slice(&boxed(b"colr", b"nclx\x00\x09\x00\x10\x00\x09\x80"));
        b.extend_from_slice(&boxed(b"pixi", &[0, 0, 0, 0, 3, 10, 10, 10]));
        // IMGDの先頭4Bは後続のNAL列の長さ（＝箱の中身の長さ - 4）。
        // その後ろが4バイト長前置のNAL
        let mut imgd = (hevc.len() as u32).to_be_bytes().to_vec();
        imgd.extend_from_slice(hevc);
        b.extend_from_slice(&boxed(b"IMGD", &imgd));
        b
    }

    /// 4バイト長前置のNAL 1本（IDR_W_RADL のヘッダ相当）。
    fn one_nal() -> Vec<u8> {
        let payload = [0x26u8, 0x01, 0xac, 0x19];
        let mut nal = (payload.len() as u32).to_be_bytes().to_vec();
        nal.extend_from_slice(&payload);
        nal
    }

    /// `uuid` にプレビューを収めたCR3もどき。`extra` は仕様に無い余分な欄の長さ。
    fn fake_cr3(kind: &[u8; 4], extra: usize, width: u16, height: u16) -> Vec<u8> {
        let body = preview_body(width, height, &[0x01, 0x04, 0x08], &one_nal());
        let mut uuid_body = vec![0u8; 16 + extra];
        uuid_body.extend_from_slice(&boxed(kind, &body));
        let mut out = boxed(b"ftyp", b"crx crx isom");
        out.extend_from_slice(&boxed(b"uuid", &uuid_body));
        out
    }

    /// ISO-BMFFの箱を辿って、指定した型の中身を返す（テスト用の読み手）。
    fn find(buf: &[u8], kind: &[u8; 4]) -> Option<Vec<u8>> {
        let mut pos = 0usize;
        while pos + 8 <= buf.len() {
            let size = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
            if size < 8 || pos + size > buf.len() {
                return None;
            }
            let body = &buf[pos + 8..pos + size];
            if &buf[pos + 4..pos + 8] == kind {
                return Some(body.to_vec());
            }
            // `meta` は版と旗の4バイト、`iinf` はさらに件数の2バイトを挟んでから
            // 子箱が並ぶ。`iprp` / `ipco` はそのまま
            let inner = match &buf[pos + 4..pos + 8] {
                b"meta" => body.get(4..),
                b"iinf" => body.get(6..),
                b"iprp" | b"ipco" => Some(body),
                _ => None,
            };
            if let Some(inner) = inner {
                if let Some(found) = find(inner, kind) {
                    return Some(found);
                }
            }
            pos += size;
        }
        None
    }

    #[test]
    fn the_preview_is_found_even_with_the_extra_uuid_field() {
        // THMB を収める uuid には無く、PRVW を収める uuid には8バイト入っている。
        // どちらも読めないと、HDR PQのCR3で原寸が切手に落ちる
        for extra in [0usize, 8] {
            let buf = fake_cr3(b"PRVW", extra, 1620, 1080);
            let found = cr3_hevc_box(&buf, b"PRVW")
                .unwrap_or_else(|| panic!("余分な欄 {extra} バイトで見つからない"));
            assert_eq!((found.width, found.height), (1620, 1080));
            assert!(found.colr.is_some() && found.pixi.is_some());
            assert!(!found.hevc.is_empty());
        }
    }

    #[test]
    fn only_the_named_boxes_are_read() {
        let buf = fake_cr3(b"THMB", 0, 320, 214);
        assert!(cr3_hevc_box(&buf, b"THMB").is_some());
        assert!(cr3_hevc_box(&buf, b"PRVW").is_none());
    }

    #[test]
    fn a_version_zero_box_is_not_picked_up() {
        // 通常のCR3は**版0**で、`PRVW` は子箱を持たずオフセット16から生JPEGが
        // 始まる。寸法（1620x1080）は版1と同じ位置に入っているので、**値では
        // 見分けが付かない**——版で弾く
        let mut body = vec![0x00, 0, 0, 0, 0, 0x01, 0x06, 0x54, 0x04, 0x38, 0xff, 0xff];
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&[0xff, 0xd8, 0xff, 0xdb, 0x00, 0x84]); // 生JPEGの先頭
        let mut uuid_body = vec![0u8; 16];
        uuid_body.extend_from_slice(&boxed(b"PRVW", &body));
        let buf = boxed(b"uuid", &uuid_body);
        assert!(
            cr3_hevc_box(&buf, b"PRVW").is_none(),
            "版0を拾ってはいけない"
        );

        // 版0の THMB は並びがずれていて、**幅がオフセット4（160）・高さが6（120）**。
        // 版1として読むとオフセット6/8を見るので120と0になる。
        // **とはいえ弾いているのは版**で、寸法は見ていない——「寸法が版1と
        // 同じ位置でも版で落とす」を見張っているのは上の `PRVW` の側。
        // こちらは版0の実物の並びを添えた補強
        let mut thmb = vec![0x00, 0, 0, 0, 0x00, 0xa0, 0x00, 0x78, 0x00, 0x00];
        thmb.extend_from_slice(&[0u8; 6]);
        let mut uuid_body = vec![0u8; 16];
        uuid_body.extend_from_slice(&boxed(b"THMB", &thmb));
        assert!(cr3_hevc_box(&boxed(b"uuid", &uuid_body), b"THMB").is_none());
    }

    #[test]
    fn nothing_is_picked_up_without_hevc() {
        // 版1の形をしていても、`hvcC` と `IMGD` が無ければ絵にならない
        let mut body = vec![0x01, 0, 0, 0, 0, 0x02, 0x06, 0x54, 0x04, 0x38, 0xff, 0xff];
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&boxed(b"CISZ", &[0u8; 12]));
        let mut uuid_body = vec![0u8; 16];
        uuid_body.extend_from_slice(&boxed(b"PRVW", &body));
        assert!(cr3_hevc_box(&boxed(b"uuid", &uuid_body), b"PRVW").is_none());
    }

    #[test]
    fn the_cut_follows_the_imgd_declaration() {
        /// `IMGD` の中身を指定して、版1の `PRVW` を1つ持つCR3もどきを組む。
        fn cr3_with_imgd(imgd: &[u8]) -> Vec<u8> {
            let mut body = vec![0x01, 0, 0, 0, 0, 0x02];
            body.extend_from_slice(&1620u16.to_be_bytes());
            body.extend_from_slice(&1080u16.to_be_bytes());
            body.extend_from_slice(&[0xff, 0xff]);
            body.extend_from_slice(&0u32.to_be_bytes());
            body.extend_from_slice(&boxed(b"hvcC", &[0x01, 0x04, 0x08]));
            body.extend_from_slice(&boxed(b"IMGD", imgd));
            let mut uuid_body = vec![0u8; 16];
            uuid_body.extend_from_slice(&boxed(b"PRVW", &body));
            boxed(b"uuid", &uuid_body)
        }
        let nal = one_nal();

        // 後ろに詰め物が付いた個体。残り全部を持っていくと `mdat` に混ざって
        // デコーダに拒まれる——**申告どおりに切る**
        let mut padded = (nal.len() as u32).to_be_bytes().to_vec();
        padded.extend_from_slice(&nal);
        padded.extend_from_slice(&[0xaa; 16]);
        let found = cr3_hevc_box(&cr3_with_imgd(&padded), b"PRVW").expect("取り出せる");
        assert_eq!(found.hevc, nal, "詰め物まで持っていっている");

        // 申告が中身に収まらないときは**捨てずに**箱の残り全部へ倒す
        let mut bogus = u32::MAX.to_be_bytes().to_vec();
        bogus.extend_from_slice(&nal);
        let found = cr3_hevc_box(&cr3_with_imgd(&bogus), b"PRVW").expect("諦めてはいけない");
        assert_eq!(found.hevc, nal);

        // 申告が0のときも同じ（空にすると絵が丸ごと消える）
        let mut zero = 0u32.to_be_bytes().to_vec();
        zero.extend_from_slice(&nal);
        let found = cr3_hevc_box(&cr3_with_imgd(&zero), b"PRVW").expect("諦めてはいけない");
        assert_eq!(found.hevc, nal);
    }

    #[test]
    fn chroma_sampling_is_read_from_the_hvcc() {
        use crate::jpeg::ChromaSampling;
        // hvcC の17バイト目の下位2ビットが chroma_format_idc。
        // 0=モノクロ / 1=4:2:0 / 2=4:2:2 / 3=4:4:4
        let hvcc = |idc: u8| {
            let mut b = vec![0u8; 17];
            b[0] = 1; // configurationVersion
            b[16] = 0xfc | idc;
            b
        };
        assert_eq!(
            crate::heif::chroma_from_hvcc(&hvcc(1)),
            Some(ChromaSampling::Half)
        );
        // Canonのプレビューはこれ。間引くと縦の色差が消える
        assert_eq!(
            crate::heif::chroma_from_hvcc(&hvcc(2)),
            Some(ChromaSampling::Full)
        );
        assert_eq!(
            crate::heif::chroma_from_hvcc(&hvcc(3)),
            Some(ChromaSampling::Full)
        );
        // 版が違う・切り詰められている＝**当てずっぽうで読まずに黙る**
        assert_eq!(crate::heif::chroma_from_hvcc(&[2, 0, 0]), None);
        assert_eq!(crate::heif::chroma_from_hvcc(&hvcc(2)[..16]), None);
    }

    #[test]
    fn the_smallest_one_that_suffices_is_decoded_first() {
        use super::decode_order;
        let sized = |width: u16, height: u16| super::Cr3Hevc {
            width,
            height,
            hvcc: Vec::new(),
            colr: None,
            pixi: None,
            hevc: Vec::new(),
        };
        let boxes = || vec![sized(1620, 1080), sized(320, 214)];
        let edges = |v: Vec<super::Cr3Hevc>| v.iter().map(|b| b.long_edge()).collect::<Vec<_>>();

        // 一覧のタイル（512px）。THMBは320しか無いので**PRVWを起こす**
        // ——ここを間違えると並びがぼやけ、media.width/height も320x214になる
        assert_eq!(edges(decode_order(boxes(), 512)), vec![1620, 320]);
        // 原寸（1600px）。THMBは論外
        assert_eq!(edges(decode_order(boxes(), 1600)), vec![1620, 320]);
        // 小さくてよい場面（256px）。**安いほうで足りる**ので THMB が先
        assert_eq!(edges(decode_order(boxes(), 256)), vec![320, 1620]);
        // どちらも足りない。せめて大きいほうから
        assert_eq!(edges(decode_order(boxes(), 4000)), vec![1620, 320]);
    }

    #[test]
    fn the_rewrapped_heif_parses_as_boxes() {
        let buf = fake_cr3(b"PRVW", 8, 1620, 1080);
        let found = cr3_hevc_box(&buf, b"PRVW").expect("見つかる");
        let hevc_len = found.hevc.len();
        let heif = wrap_hevc_as_heif(
            &found.hvcc,
            found.width,
            found.height,
            found.colr.as_deref(),
            found.pixi.as_deref(),
            &found.hevc,
        );

        // ブランドは `heix`。`heic` は Main / Main Still の約束で、
        // Canonのこの絵（Main 10・4:2:2）は範囲の外
        assert_eq!(&heif[4..8], b"ftyp");
        assert_eq!(&heif[8..12], b"heix");
        assert!(find(&heif, b"pitm").is_some(), "pitmが要る");

        // アイテムの型は `hvc1`（パラメータセットが `hvcC` 側にある形）
        let infe = find(&heif, b"infe").expect("infeが要る");
        assert_eq!(&infe[8..12], b"hvc1", "アイテムの型が hvc1 でない");

        // ipma の紐付け: プロパティ4つ、先頭（hvcC）だけ必須の旗が立つ
        let ipma = find(&heif, b"ipma").expect("ipmaが要る");
        assert_eq!(u16::from_be_bytes(ipma[8..10].try_into().unwrap()), 1);
        let count = ipma[10] as usize;
        assert_eq!(count, 4, "hvcC/ispe/colr/pixi の4つ");
        assert_eq!(ipma[11], 0x81, "hvcC が必須になっていない");
        assert_eq!(&ipma[12..11 + count], &[2, 3, 4], "紐付けの順がずれている");

        // ispe は**表示寸法**（CISZ の符号化寸法ではない）
        let ispe = find(&heif, b"ispe").expect("ispeが要る");
        assert_eq!(u32::from_be_bytes(ispe[4..8].try_into().unwrap()), 1620);
        assert_eq!(u32::from_be_bytes(ispe[8..12].try_into().unwrap()), 1080);

        // colr と pixi は**中身をそのまま**運ぶこと。pixi はフルボックスだが、
        // CR3から取り出した時点で version/flags を含んでいる。ここで足し直すと
        // 二重になり、読み手はチャンネル数を0と読む（ゲート1のP1）
        let pixi = find(&heif, b"pixi").expect("pixiが要る");
        assert_eq!(
            pixi,
            vec![0, 0, 0, 0, 3, 10, 10, 10],
            "pixiの中身が変わっている"
        );
        let colr = find(&heif, b"colr").expect("colrが要る");
        assert_eq!(&colr[..4], b"nclx");
        assert_eq!(
            u16::from_be_bytes(colr[6..8].try_into().unwrap()),
            16,
            "PQのtransferが消えている"
        );

        // iloc が指す位置に、mdat の中身がそのまま居ること。
        // ここがずれると、デコーダは無音で別のバイト列を読む
        let iloc = find(&heif, b"iloc").expect("ilocが要る");
        let offset = u32::from_be_bytes(iloc[14..18].try_into().unwrap()) as usize;
        let length = u32::from_be_bytes(iloc[18..22].try_into().unwrap()) as usize;
        assert_eq!(length, hevc_len);
        assert_eq!(&heif[offset - 4..offset], b"mdat");
        assert_eq!(&heif[offset..offset + length], &found.hevc[..]);
    }

    #[test]
    fn a_broken_box_does_not_keep_us_descending() {
        // 大きさが嘘の箱で止まること
        assert!(cr3_hevc_box(&[0xff; 64], b"PRVW").is_none());

        // **深さの制限の境目を押さえる。** 一番奥に本物の PRVW を置いて、
        // 何重までなら届くかを見る。`is_none()` だけだと「もっと緩い制限」でも
        // 通ってしまうので、**届く側と届かない側の両方**を確かめる
        let wrap = |times: usize| {
            let mut nested = fake_cr3(b"PRVW", 8, 1620, 1080);
            for _ in 0..times {
                let mut body = vec![0u8; 16];
                body.extend_from_slice(&nested);
                nested = boxed(b"uuid", &body);
            }
            nested
        };
        assert!(cr3_hevc_box(&wrap(0), b"PRVW").is_some(), "素なら見つかる");
        // 境目そのものを押さえる。3重までは届き、4重で届かなくなる
        assert!(cr3_hevc_box(&wrap(3), b"PRVW").is_some(), "3重は届くはず");
        assert!(
            cr3_hevc_box(&wrap(4), b"PRVW").is_none(),
            "深さの制限が効いていない"
        );
    }
}

#[cfg(test)]
mod cr3_hevc_sample_tests {
    //! 実物のHDR PQのCR3で確かめる（オプトイン）。
    //!
    //! ここは**OSのデコーダに依存する**——WindowsのWICと、別インストールの
    //! HEVCコーデックが要る。居ない環境（macOSを含む）では取れないのが正しいので、
    //! そこは失敗にしない。
    //!
    //! **ただし「居ないから飛ばす」の判定を、試験対象で作らないこと。**
    //! 製品側の包み直しや箱の走査が壊れたときに「デコーダが居ない」と読み替えて
    //! 静かに緑になる——オプトインテストの存在理由そのものを潰す
    //! （ゲート2・ゲート3が揃って指摘）。そこで、
    //!
    //! - サンプルの選別は **`hvcC` の在処をバイト列で直に探す**（`cr3_hevc_box` を通さない）
    //! - コーデックの有無は **テストの中で別に組んだ最小のHEIF**で確かめる
    //!   （`wrap_hevc_as_heif` を通さない）
    //!
    //! こうすると、製品側が壊れたときは「飛ばす」ではなく**落ちる**。

    use super::{cr3_hevc_box, embedded_preview, long_edge, read_head, SCAN_LIMIT};

    /// テスト用の、製品側とは別に書いた箱組み。
    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// テスト用の、製品側とは別に書いた最小のHEIF。
    ///
    /// **`wrap_hevc_as_heif` を呼ばない**のが要点。これが起きるのに製品側が
    /// 起きないなら、環境ではなく製品側が壊れている。
    fn independent_heif(hvcc: &[u8], width: u16, height: u16, hevc: &[u8]) -> Vec<u8> {
        fn full(kind: &[u8; 4], version: u8, body: &[u8]) -> Vec<u8> {
            let mut inner = vec![version, 0, 0, 0];
            inner.extend_from_slice(body);
            boxed(kind, &inner)
        }
        let ftyp = boxed(b"ftyp", b"mif1\x00\x00\x00\x00mif1heix");
        let mut hdlr_body = 0u32.to_be_bytes().to_vec();
        hdlr_body.extend_from_slice(b"pict");
        hdlr_body.extend_from_slice(&[0u8; 12]);
        hdlr_body.push(0);
        let hdlr = full(b"hdlr", 0, &hdlr_body);
        let pitm = full(b"pitm", 0, &1u16.to_be_bytes());
        let iinf = {
            let mut infe = 1u16.to_be_bytes().to_vec();
            infe.extend_from_slice(&0u16.to_be_bytes());
            infe.extend_from_slice(b"hvc1");
            infe.push(0);
            let mut b = 1u16.to_be_bytes().to_vec();
            b.extend_from_slice(&full(b"infe", 2, &infe));
            full(b"iinf", 0, &b)
        };
        let iprp = {
            let mut ispe = u32::from(width).to_be_bytes().to_vec();
            ispe.extend_from_slice(&u32::from(height).to_be_bytes());
            let mut props = boxed(b"hvcC", hvcc);
            props.extend_from_slice(&full(b"ispe", 0, &ispe));
            let ipco = boxed(b"ipco", &props);
            let mut assoc = 1u32.to_be_bytes().to_vec();
            assoc.extend_from_slice(&1u16.to_be_bytes());
            assoc.extend_from_slice(&[2, 0x81, 2]);
            let mut out = ipco;
            out.extend_from_slice(&full(b"ipma", 0, &assoc));
            boxed(b"iprp", &out)
        };
        let build = |offset: u32| {
            let mut b = vec![0x44, 0x00];
            b.extend_from_slice(&1u16.to_be_bytes());
            b.extend_from_slice(&1u16.to_be_bytes());
            b.extend_from_slice(&0u16.to_be_bytes());
            b.extend_from_slice(&1u16.to_be_bytes());
            b.extend_from_slice(&offset.to_be_bytes());
            b.extend_from_slice(&(hevc.len() as u32).to_be_bytes());
            let iloc = full(b"iloc", 0, &b);
            let mut meta = hdlr.clone();
            meta.extend_from_slice(&pitm);
            meta.extend_from_slice(&iinf);
            meta.extend_from_slice(&iprp);
            meta.extend_from_slice(&iloc);
            let meta = full(b"meta", 0, &meta);
            let at = (ftyp.len() + meta.len() + 8) as u32;
            (meta, at)
        };
        let (_, at) = build(0);
        let (meta, _) = build(at);
        let mut out = ftyp.clone();
        out.extend_from_slice(&meta);
        out.extend_from_slice(&(hevc.len() as u32 + 8).to_be_bytes());
        out.extend_from_slice(b"mdat");
        out.extend_from_slice(hevc);
        out
    }

    /// プレビューの箱から、HEVCと表示寸法を**箱の走査を使わずに**素朴に切り出す。
    ///
    /// `cr3_hevc_box` が壊れてもここは動くので、環境の判定に使える。
    ///
    /// **一式は必ず同じ箱から取る。** `hvcC` を位置順で、寸法をタグの優先順で
    /// 別々に拾うと、`PRVW` のストリームに `THMB` の320x214を貼るような組み合わせが
    /// できてしまう。`ispe` と符号化寸法を突き合わせる読み手（将来のImageIO）では
    /// それが常に false になり、その環境でテストが飛び続ける（ゲート3のP3）。
    /// そこで**先に見つかったプレビューの箱に絞ってから**中身を読む。
    fn crude_preview(head: &[u8]) -> Option<(Vec<u8>, Vec<u8>, u16, u16)> {
        /// 型の位置から、その箱の中身を切り出す。
        fn body_at(buf: &[u8], at: usize) -> Option<&[u8]> {
            // 箱の大きさは型の4バイト手前。**引き算を先にしない**
            // ——`at < 4` だとデバッグビルドで桁溢れのpanicになる
            let start = at.checked_sub(4)?;
            let size = u32::from_be_bytes(buf.get(start..at)?.try_into().ok()?) as usize;
            buf.get(at + 4..start.checked_add(size)?)
        }
        fn find(buf: &[u8], tag: &[u8; 4]) -> Option<usize> {
            buf.windows(4).position(|w| w == tag)
        }

        /// 1つの箱から一式（`hvcC`・NAL列・表示寸法）を取る。
        fn one(head: &[u8], at: usize) -> Option<(Vec<u8>, Vec<u8>, u16, u16)> {
            let body = body_at(head, at)?;
            // 型の直後が16バイトのヘッダ。版1ならオフセット6/8に表示寸法が入る
            if body.first() != Some(&1) {
                return None;
            }
            let width = u16::from_be_bytes([*body.get(6)?, *body.get(7)?]);
            let height = u16::from_be_bytes([*body.get(8)?, *body.get(9)?]);
            if width == 0 || height == 0 {
                return None;
            }
            let hvcc = body_at(body, find(body, b"hvcC")?)?.to_vec();
            let imgd = body_at(body, find(body, b"IMGD")?)?;
            Some((hvcc, imgd.get(4..)?.to_vec(), width, height))
        }

        // 候補は**位置の昇順に見て、一式が揃った最初の箱**を採る。1つ目が版0でも
        // もう一方へ落ちる——「`THMB` は版0のJPEG・`PRVW` だけ版1のHEVC」という
        // 個体が出たときに、環境の判定が false に倒れてサンプルのテストが
        // 無言で飛ぶのを避ける（ゲート2のP3）
        let mut ats: Vec<usize> = [b"PRVW", b"THMB"]
            .into_iter()
            .filter_map(|tag| find(head, tag))
            .collect();
        ats.sort_unstable();
        ats.into_iter().find_map(|at| one(head, at))
    }

    /// この環境でHEVCの静止画を起こせるか。**包み直しを1行も通さずに**確かめる。
    fn codec_available(head: &[u8]) -> bool {
        let Some((hvcc, hevc, width, height)) = crude_preview(head) else {
            return false;
        };
        let heif = independent_heif(&hvcc, width, height, &hevc);
        // **ここだけは製品側（`decode_mem`）に落ちる。** 完全な分離はできないので、
        // 「コーデックが無い」と「`decode_mem` が壊れた」は区別できない。
        // 包み直しの側は独立なので、2周目に潰した穴の大半はここで塞がっている
        crate::heif::decode_mem(&heif).is_some()
    }

    /// 切手を足す相手として**一番小さいサンプル**を選ぶ。
    ///
    /// 切手はコピーの**末尾**に付くので、元が [`SCAN_LIMIT`] を超えていると
    /// 切手が走査窓の外に落ち、仕掛けの確認（[`with_trailing_jpeg`] の末尾）が
    /// 必ず外れる。`read_dir` の順は台で変わるので `samples[0]` に任せると、
    /// **同じサンプル一式でもWindowsでは通ってmacOSでは落ちる**——実際そうなっていた
    /// （Windowsに置いてあったHDR PQのCR3は13.9MiBのEOS R8だけで、macOS側は
    /// 26.3MiBのEOS R5 Mark IIだけだった）。
    fn smallest_sample(
        samples: &[(std::path::PathBuf, Vec<u8>)],
    ) -> &(std::path::PathBuf, Vec<u8>) {
        samples
            .iter()
            .min_by_key(|(path, _)| file_len(path))
            .expect("呼ぶ側が空でないことを確かめている")
    }

    fn file_len(path: &std::path::Path) -> u64 {
        std::fs::metadata(path).map_or(u64::MAX, |m| m.len())
    }

    /// 小さいJPEGを1枚後ろにくっつけたコピーを作る。
    ///
    /// HDR PQのCR3に切手のJPEGが同居している場合を模す。走査（2段目）はこれを
    /// 拾うので、`best` が `Some` になってHEVCの経路が飛ばされないかを見る。
    fn with_trailing_jpeg(src: &std::path::Path, dir: &std::path::Path) -> std::path::PathBuf {
        let mut buf = std::fs::read(src).expect("読める");
        let stamp = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            64,
            64,
            image::Rgb([200, 40, 40]),
        ));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode_image(&stamp)
            .expect("詰められる");
        buf.extend_from_slice(&jpeg);
        let dst = dir.join("with_stamp.CR3");
        std::fs::write(&dst, buf).expect("書ける");

        // **仕掛けが効いていることを先に確かめる。** 走査がこの切手を拾わないなら、
        // このテストは何も見張っていない（黙って通り続ける）
        let head = read_head(&dst, SCAN_LIMIT).expect("読める");
        let found = super::scan_largest_jpeg(&head).expect("切手が走査に掛かること");
        assert_eq!(long_edge(found), 64, "拾ったのが切手ではない");
        dst
    }

    /// サンプルのフォルダから、HEVCのプレビューを持つCR3を集める。
    ///
    /// 選別は **`hvcC` がファイルに在るか**だけで決める。`cr3_hevc_box` を使うと、
    /// 走査や版判定が壊れたときにサンプル0件＝無言で成功になる。
    fn hdr_pq_samples() -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let Ok(dir) = std::env::var("PICTKURA_RAW_SAMPLES") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| !e.eq_ignore_ascii_case("cr3"))
            {
                continue;
            }
            let Some(head) = read_head(&path, SCAN_LIMIT) else {
                continue;
            };
            // **ISO-BMFFであることも確かめる。** 拡張子と `hvcC` の4バイトだけだと、
            // 通常のCR3の圧縮データに偶然その並びが出たファイルが混ざる
            // （16MBで約0.4%）。それが先頭に来ると環境の判定がゴミを掴んで
            // 「コーデックが無い」に倒れ、本物まで無言で飛ぶ（ゲート2のP3）
            let is_bmff = head.get(4..8) == Some(b"ftyp");
            if is_bmff && head.windows(4).any(|w| w == b"hvcC") {
                out.push((path, head));
            }
        }
        out
    }

    #[test]
    fn a_hdr_pq_cr3_yields_a_picture_that_can_be_shown() {
        let samples = hdr_pq_samples();
        if samples.is_empty() {
            return; // サンプルが置かれていない
        }
        if !codec_available(&samples[0].1) {
            eprintln!("HEVCの静止画を起こせない環境なので飛ばす");
            return;
        }
        for (path, head) in &samples {
            // ここから先は**製品側**。飛ばさずに落とす
            let found = cr3_hevc_box(head, b"PRVW")
                .unwrap_or_else(|| panic!("{}: PRVWを取り出せない", path.display()));

            // **Canonのこの絵は4:2:2**。ここがずれると出力が黙って4:2:0に落ちる
            assert_eq!(
                crate::heif::chroma_from_hvcc(&found.hvcc),
                Some(crate::jpeg::ChromaSampling::Full),
                "{}: chroma_format_idc が 4:2:2 でない",
                path.display()
            );

            let preview = embedded_preview(path)
                .unwrap_or_else(|| panic!("{}: 起こせるはずが取れない", path.display()));
            let img = image::load_from_memory(&preview)
                .unwrap_or_else(|e| panic!("{}: 包み直した絵が壊れている: {e}", path.display()));

            // 箱のヘッダが言う表示寸法と、実際に起きた絵の寸法が一致すること。
            // ずれるなら符号化寸法（CISZ 1664x1080）が出ている＝右端にゴミが載る
            assert_eq!(
                (img.width(), img.height()),
                (u32::from(found.width), u32::from(found.height)),
                "{}: 起きた絵の寸法が箱の申告と違う",
                path.display()
            );

            // 一覧のタイルの経路（512px）。ここが320x214のTHMBで満足すると、
            // 並びがぼやけたまま気付けない（ゲート1のP2）
            let tile = super::embedded_preview_at_least(path, 512)
                .unwrap_or_else(|| panic!("{}: タイル用の絵が取れない", path.display()));
            assert!(
                long_edge(&tile) >= 512,
                "{}: 512pxを求めたのに長辺{}しか無い",
                path.display(),
                long_edge(&tile)
            );
        }
    }

    #[test]
    fn hevc_is_decoded_even_with_a_stamp_sized_jpeg_alongside() {
        /// 切手1枚ぶんの余地。64x64の単色JPEGは1KBに満たないが、
        /// 窓の縁ぎりぎりで切られると仕掛けの確認が理由の分からない形で外れる
        const STAMP_ROOM: u64 = 64 * 1024;

        let samples = hdr_pq_samples();
        if samples.is_empty() {
            return;
        }
        let (src, head) = smallest_sample(&samples);
        if !codec_available(head) {
            return;
        }
        // 一番小さいものでも窓に入らないなら、**理由を言ってから**降りる。
        // 黙って通ると「見張っている」と「見張れていない」が区別できない
        let len = file_len(src);
        if len + STAMP_ROOM > SCAN_LIMIT as u64 {
            eprintln!(
                "{}: 一番小さいHDR PQのサンプルでも{}MiBあり、\
                 切手が走査窓（{}MiB）の外に出るので飛ばす",
                src.display(),
                len / (1024 * 1024),
                SCAN_LIMIT / (1024 * 1024)
            );
            return;
        }
        let tmp = tempfile::tempdir().expect("一時フォルダ");
        let stamped = with_trailing_jpeg(src, tmp.path());
        let got = embedded_preview(&stamped).expect("切手があっても絵は返る");
        assert!(
            long_edge(&got) >= super::USABLE_LONG_EDGE,
            "{}: 切手（64x64）を掴んで長辺{}で止まっている",
            stamped.display(),
            long_edge(&got)
        );
    }
}
