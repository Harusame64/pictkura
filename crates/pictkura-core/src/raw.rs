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
//! 3. **全体の走査**（Ricoh GR IIIのDNG・Sigmaのx3f）。原寸プレビューを
//!    ファイルの後ろ半分に置く形式がある。ここまで来るのは
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
/// （Ricoh・Sigmaの小プレビュー）で、上回る実物が **3200x2400**（OM-1）
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
        raw.chunks_exact(2)
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

    // 2段目: 先頭を読んでJPEGの塊を拾う（CR3・RAF・NEF等）
    if let Some(head) = read_head(path, SCAN_LIMIT) {
        if let Some(found) = scan_largest_jpeg(&head) {
            if long_edge(found) >= min_long_edge {
                return Some(found.to_vec());
            }
            keep(&mut best, found.to_vec());
        }
    }

    // 3段目: 先頭16MBに無かった。原寸プレビューを**後ろの方に**置く形式が
    // ある（Ricoh GR IIIのDNGは720x480の後ろに6000x4000、Sigmaのx3fも同様）。
    // ここまで来たのは「使える絵がまだ無い」ときだけなので、全体を読んで探す
    if std::fs::metadata(path).is_ok_and(|m| m.len() as usize <= FULL_SCAN_LIMIT) {
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
        let mut repaired = rest[found..].to_vec();
        repaired[0] = 0xFF;
        if let Some(span) = jpeg_span(&repaired, 0) {
            if span.displayable {
                repaired.truncate(span.end);
                if best.as_ref().is_none_or(|b| b.len() < repaired.len()) {
                    best = Some(repaired);
                }
            }
        }
        at = start + 3;
    }
    best
}

/// `.mrw` の中の**TIFFの塊**（`TTW` ブロック）を返す。
///
/// MRWは独自のブロック構造（` MRM` の下に ` PRD`・` TTW`…）で、
/// EXIFは `TTW` の中に**TIFFのまま**入っている。ブロックを辿るだけなので
/// 読むのは先頭側だけで済む。
fn mrw_tiff_block(path: &Path) -> Option<Vec<u8>> {
    let head = read_head(path, PATCHED_TIFF_HEAD)?;
    if head.get(..4)? != b" MRM" {
        return None;
    }
    // 先頭の ` MRM` は「この後ろ全部の長さ」なので、中身は8バイト目から
    let mut at = 8usize;
    while at + 8 <= head.len() {
        let tag = head.get(at..at + 4)?;
        let len = u32::from_be_bytes(head.get(at + 4..at + 8)?.try_into().ok()?) as usize;
        if len == 0 {
            return None;
        }
        if tag == b" TTW" {
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

/// **TIFFのふりをしていない**TIFF系RAWのメタデータを、普通のEXIFとして
/// 読めるバイト列にして返す（`thumbs::read_exif` が使う）。
///
/// Olympus/OM の `.orf` は版番号（TIFFヘッダの3〜4バイト目。本来42）に
/// `RO`/`RS`、Panasonic の `.rw2` は `U` を書く。中身の作りは標準のTIFFなので、
/// **この2バイトを42に直すだけ**で普通のEXIFとして読める。直さないと
/// 撮影日時・カメラ名・向きが丸ごと落ち、**縦位置の写真が横倒しになる**。
///
/// 全部は読まない。先頭 [`PATCHED_TIFF_HEAD`] だけを読み、残りは**ゼロで
/// 嵩上げする**。TIFFは値をファイル内のoffsetで指すので、長さが足りないと
/// パーサが「Truncated field value」で全体を投げ出してしまう。嵩上げした
/// 先を指す項目は空になるだけで、手前にある撮影日時・カメラ名・向きは読める
/// （実測: 先頭16KBでORF・RW2とも全項目が一致）。
pub fn patched_tiff_metadata(path: &Path) -> Option<Vec<u8>> {
    // MRWは版番号ではなく**入れ物ごと独自**で、TIFFは中の `TTW` に入っている
    if let Some(block) = mrw_tiff_block(path) {
        return Some(block);
    }
    let head = read_window(path, 0, PATCHED_TIFF_HEAD)?;
    let byte_order = head.get(..2)?;
    let big_endian = match byte_order {
        b"II" => false,
        b"MM" => true,
        _ => return None, // TIFFですらない（CR3・RAF・X3F等）
    };
    let version = if big_endian {
        u16::from_be_bytes([*head.get(2)?, *head.get(3)?])
    } else {
        u16::from_le_bytes([*head.get(2)?, *head.get(3)?])
    };
    // 42（普通のTIFF）と43（BigTIFF）は直す対象ではない。前者はそもそも
    // ここへ来ないし、後者は構造が違うので版番号を替えても読めない
    if version == 42 || version == 43 {
        return None;
    }
    let len = std::fs::metadata(path).ok()?.len() as usize;
    // 嵩上げはファイルと同じ長さぶん確保する。`.raw` のように**中身が何でも
    // ありうる拡張子**も走査対象なので、上限を付けないと数GBのファイル1つで
    // 確保が破裂する。超えるものは諦める（メタデータが空になるだけ）
    if len > FULL_SCAN_LIMIT {
        return None;
    }
    let mut buf = vec![0u8; len.max(head.len())];
    buf.get_mut(..head.len())?.copy_from_slice(&head);
    let patched = if big_endian {
        [0x00, 0x2A]
    } else {
        [0x2A, 0x00]
    };
    buf.get_mut(2..4)?.copy_from_slice(&patched);
    Some(buf)
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
fn read_head(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(limit as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// ISO-BMFF（CR3等）から**TIFF形式のメタデータブロック**を取り出す。
///
/// CR3はTIFFではないのでEXIFパーサが直接読めない。Canonは撮影情報を
/// `moov/uuid` の下の `CMT1`（IFD0: メーカー・機種・向き）と
/// `CMT2`（Exif IFD: 撮影日時）に、**中身はTIFFのまま**入れている。
/// このボックスの中身をそのままEXIFパーサへ渡せば、普通のJPEGと同じに読める。
///
/// 箱を辿るだけなので読むのはヘッダ周辺だけ（数十KB）。
pub fn bmff_metadata_blocks(path: &Path) -> Vec<Vec<u8>> {
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
    let Some(head) = read_head(path, 1024 * 1024) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(&head, 0, &mut out);
    out
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
    fn 非圧縮rgbのプレビューを組み立てる() {
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
    fn ビッグエンディアンの非圧縮プレビューも読める() {
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
    fn 十六ビットの非圧縮プレビューは明るさを補正して読む() {
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
    fn センサーの生データはプレビューに使わない() {
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
    fn 実サンプルでプレビューが取れる() {
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
    fn raw拡張子を見分ける() {
        for ext in ["CR2", "cr3", "arw", "NEF", "dng", "raf"] {
            assert!(is_raw_extension(ext), "{ext} はRAW");
        }
        for ext in ["jpg", "png", "webp", "mp4", ""] {
            assert!(!is_raw_extension(ext), "{ext} はRAWではない");
        }
    }

    #[test]
    fn tiffの申告からプレビューを取り出す() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = jpeg_bytes(160, 120);
        let path = tiff_with_declared_preview(dir.path(), "sample.cr2", &jpeg);

        let preview = embedded_preview(&path).expect("プレビューが取れる");
        assert_eq!(preview, jpeg, "カメラが書いたJPEGをそのまま返す");
        let decoded = image::load_from_memory(&preview).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (160, 120));
    }

    #[test]
    fn 申告が切手でも大きい絵を探しに行く() {
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
    fn 使える大きさが取れたらそこで打ち切る() {
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
    fn 小さい絵しか無ければ一番大きいものを返す() {
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
    fn 版番号が独自のtiffも読めるように直す() {
        // ORF（Olympus/OM）は "IIRO"、RW2（Panasonic）は "IIU " と、
        // TIFFの版番号（本来42）に独自の値を書く。直さないと撮影日時も
        // カメラ名も向きも丸ごと落ち、**縦位置の写真が横倒しになる**
        let dir = tempfile::tempdir().unwrap();
        for (name, big_endian, version) in [
            ("sample.orf", false, [b'R', b'O']),
            ("sample.orf", true, [b'O', b'R']),
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

            let patched = patched_tiff_metadata(&path).expect("直せる");
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
    fn 普通のtiffは直さない() {
        // 版番号42はそのまま読めるので直す対象ではない。BigTIFF（43）は
        // 構造が違い、版番号を替えても読めないので触らない
        let dir = tempfile::tempdir().unwrap();
        for version in [42u16, 43] {
            let mut buf = build_tiff(&[entry(274, 3, &[6])], &[], false);
            buf[2..4].copy_from_slice(&version.to_le_bytes());
            let path = dir.path().join("sample.dng");
            std::fs::write(&path, &buf).unwrap();
            assert!(patched_tiff_metadata(&path).is_none(), "版{version}");
        }
        // TIFFですらないファイル（CR3・RAF・X3F）も対象外
        let path = dir.path().join("sample.cr3");
        std::fs::write(&path, b"   ftypcrx ").unwrap();
        assert!(patched_tiff_metadata(&path).is_none());
    }

    #[test]
    fn 申告が無くてもjpegの塊を拾う() {
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
    fn bmffのメタデータ箱を取り出す() {
        // CR3の構造を最小限で再現: moov > uuid > CMT1
        fn box_of(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.cr3");

        let cmt1 = box_of(b"CMT1", b"II* metadata-here");
        let mut uuid_body = vec![0u8; 16]; // UUID
        uuid_body.extend_from_slice(&cmt1);
        let moov = box_of(b"moov", &box_of(b"uuid", &uuid_body));
        let mut file = box_of(b"ftyp", b"crx ");
        file.extend_from_slice(&moov);
        std::fs::write(&path, &file).unwrap();

        let blocks = bmff_metadata_blocks(&path);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].starts_with(b"II"), "TIFFの中身がそのまま出る");
    }

    #[test]
    fn 壊れたbmffでも止まる() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.cr3");
        // サイズが自分より大きい箱（壊れている）
        let mut buf = 999_999u32.to_be_bytes().to_vec();
        buf.extend_from_slice(b"moov");
        std::fs::write(&path, &buf).unwrap();
        assert!(bmff_metadata_blocks(&path).is_empty());
    }

    #[test]
    fn jpegを含まないファイルはnone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.arw");
        std::fs::write(&path, vec![0u8; 4096]).unwrap();
        assert!(embedded_preview(&path).is_none());
    }

    #[test]
    fn 途中で切れたjpegは拾わない() {
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
    fn 表示用jpegはmozjpegで詰める() {
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
    fn 間引きの指定がエンコーダまで届く() {
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
    fn 透過は白へ重ねてから詰める() {
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
