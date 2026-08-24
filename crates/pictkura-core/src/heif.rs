//! HEIC/HEIF 対応（第7部 段階G）。
//!
//! iPhone の既定形式。RAWと違って**埋め込みJPEGが1枚も無い**ので、
//! 「カメラが書いた絵を取り出すだけ」というRAWの手は使えない。
//! 実サンプル（iPhone）を解剖して分かった中身はこうなっている:
//!
//! - 主画像は `grid` アイテム＝**HEVCタイルの寄せ集め**（5712x4284 が45枚のタイル）。
//!   絵にするにはHEVCデコーダとタイル合成の両方が要る
//! - `Exif` アイテムはある（撮影日時・機種・向き）。ただし**IFD1が無い**ので
//!   JPEGサムネイルも入っていない
//! - 向きは EXIF Orientation と `irot` の**両方**に書かれていて、実測では一致していた
//!
//! そこで役割を分ける:
//!
//! 1. **メタデータ（撮影日時・機種・寸法・向き）は自前で読む**。コンテナの
//!    先頭だけ舐めれば取れるので、デコーダが無くても一覧に日付順で並べられる
//! 2. **画素はOSのデコーダに任せる**。Windowsは WIC（HEIF Image Extensions）、
//!    macOSは将来 ImageIO。自前でHEVCを持ち込むと libde265/x265 系の
//!    C依存とライセンス（LGPL/GPL）を抱えることになり、MITで配る方針と合わない
//! 3. デコーダが無い環境では**一覧には出るがサムネイルが付かない**。
//!    RAWでプレビューを持たない機種（CinemaDNG等）と同じ扱いにする
//!
//! 向きの扱いは**`irot` を正**とする（コンテナ自身の表示規則で、OSのデコーダも
//! これに従う）。EXIF Orientation は見ない。両方を掛けると二重回転になる。

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// `meta` ボックスをまるごと読む上限。iPhoneの実測で約36KB。
/// タイルが多い機種でも数百KBに収まるが、壊れたファイルで無制限に読まない
const META_LIMIT: u64 = 8 * 1024 * 1024;

/// 拡張子がHEIF系か（大文字小文字は無視）。
pub fn is_heif_extension(ext: &str) -> bool {
    const HEIF_EXTENSIONS: &[&str] = &[
        "heic", // iPhone / 一般的なHEVC静止画
        "heif", // 汎用
        "hif",  // Canon（EOS R系のHEIF記録）
    ];
    let lower = ext.to_ascii_lowercase();
    HEIF_EXTENSIONS.contains(&lower.as_str())
}

/// パスの拡張子がHEIF系か。
pub fn is_heif_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_heif_extension)
}

/// 拡張子がAVIFか。
///
/// AVIFは**HEIFと同じISO-BMFFの入れ物**で、中身の符号化がHEVCではなくAV1。
/// コンテナの読み取り（寸法・向き）はそのまま使い回せる。
/// HEIFと分けているのは、**WebViewがAVIFを直接描ける**からで、
/// 原寸表示はJPEGへ詰め直さず原本をそのまま返せる。
pub fn is_avif_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("avif")
}

/// パスの拡張子がAVIFか。
pub fn is_avif_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_avif_extension)
}

/// ISO-BMFFの静止画（HEIF系またはAVIF）か。
///
/// コンテナの読み取りとOSデコーダへの委譲は、この判定で分岐する。
pub fn is_bmff_image_path(path: &Path) -> bool {
    is_heif_path(path) || is_avif_path(path)
}

/// コンテナから読めた主画像の素性。デコードはしない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeifInfo {
    /// 格納されている画素の幅（回転前）
    pub stored_width: u32,
    /// 格納されている画素の高さ（回転前）
    pub stored_height: u32,
    /// `irot` の角度（反時計回りに 90度 × この値）
    pub rotation: u8,
    /// `imir` の鏡映（無ければ None。0=左右反転 / 1=上下反転）
    pub mirror: Option<u8>,
    /// `imir` が `irot` より先に書かれていた（順に適用する必要がある）。
    ///
    /// 鏡映と90度系の回転は**交換できない**（先に反転してから回すのと
    /// 回してから反転するのとで180度ずれる）ので、書かれた順を覚えておく。
    pub mirror_first: bool,
    /// `clap`（表示時の切り出し）。**寸法を答えるときは必ずこちらを優先する**。
    ///
    /// これを無視すると、DBへ書く寸法（`ispe` のまま）と実際に作るサムネイル
    /// （切り出し済み）で縦横比が食い違い、一覧のタイルが歪む。
    pub crop: Option<Clap>,
}

impl HeifInfo {
    /// 表示上の寸法（`clap` の切り出しと `irot` の回転を反映した幅・高さ）。
    pub fn display_size(&self) -> (u32, u32) {
        let (w, h) = match self.crop {
            Some(c) => (c.width, c.height),
            None => (self.stored_width, self.stored_height),
        };
        if self.rotation % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }
}

// ---------------------------------------------------------------------------
// ISO-BMFF（コンテナ）の読み取り
// ---------------------------------------------------------------------------

/// ボックスの中身を順に返す。壊れた長さに当たったらそこで打ち切る。
///
/// 返すのは `(種別, 本体の開始, 本体の終端)`。
pub(crate) fn boxes(buf: &[u8], start: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let size = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as u64;
        let kind = [buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]];
        let (size, header) = match size {
            // size=1 は64ビット長がヘッダの後ろに続く
            1 => {
                if pos + 16 > end {
                    return out;
                }
                let mut wide = [0u8; 8];
                wide.copy_from_slice(&buf[pos + 8..pos + 16]);
                (u64::from_be_bytes(wide), 16usize)
            }
            // size=0 は「このボックスで終わり」
            0 => ((end - pos) as u64, 8usize),
            n => (n, 8usize),
        };
        // 壊れたファイルは64ビット長に u64::MAX 近い値を書ける。
        // `pos + size` を先に計算すると桁が溢れて（debugではpanicし、
        // releaseでは折り返して）検査をすり抜けるので、必ず加算前に確かめる
        let Ok(size) = usize::try_from(size) else {
            return out;
        };
        let Some(next) = pos.checked_add(size) else {
            return out;
        };
        if size < header || next > end {
            return out;
        }
        out.push((kind, pos + header, next));
        pos = next;
    }
    out
}

/// 直下から名前の一致するボックスを1つ探す。
pub(crate) fn find_box(
    buf: &[u8],
    start: usize,
    end: usize,
    name: &[u8; 4],
) -> Option<(usize, usize)> {
    boxes(buf, start, end)
        .into_iter()
        .find(|(kind, _, _)| kind == name)
        .map(|(_, b, e)| (b, e))
}

/// ファイルの**トップレベルの箱だけ**を辿り、`meta` ボックスの中身を読む。
///
/// iPhoneの `meta` は先頭にあるが、規格上は `mdat`（数MB）の後ろに置くこともできる。
/// ヘッダ8バイトずつ読んで飛ばしていけば、画素データを1バイトも読まずに辿り着ける。
fn read_meta_box(path: &Path) -> Option<Vec<u8>> {
    read_top_level_box(path, b"meta", META_LIMIT)
}

/// ファイルの**トップレベルの箱だけ**を辿り、名前の一致する箱の中身を読む。
///
/// 画素（`mdat`）は1バイトも読まずに済む。動画の `moov` は
/// **末尾に置かれていることが多い**（iPhoneの.MOVもそう）ので、
/// ヘッダを飛ばしながら最後まで見に行けるこの形が要る。
pub(crate) fn read_top_level_box(path: &Path, name: &[u8; 4], limit: u64) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let total = file.metadata().ok()?.len();
    let mut pos = 0u64;
    while pos + 8 <= total {
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut header = [0u8; 16];
        if file.read_exact(&mut header[..8]).is_err() {
            return None;
        }
        let kind = [header[4], header[5], header[6], header[7]];
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let (size, header_len) = match size32 {
            1 => {
                file.read_exact(&mut header[8..16]).ok()?;
                let mut wide = [0u8; 8];
                wide.copy_from_slice(&header[8..16]);
                (u64::from_be_bytes(wide), 16u64)
            }
            0 => (total - pos, 8u64),
            n => (n as u64, 8u64),
        };
        // ここも同様に、加算前に桁溢れを弾く
        let next = pos.checked_add(size)?;
        if size < header_len || next > total {
            return None;
        }
        if &kind == name {
            let body_len = size - header_len;
            if body_len > limit {
                return None;
            }
            let mut body = vec![0u8; body_len as usize];
            file.seek(SeekFrom::Start(pos + header_len)).ok()?;
            file.read_exact(&mut body).ok()?;
            return Some(body);
        }
        pos = next;
    }
    None
}

/// `pitm` から主アイテムのIDを読む。
fn primary_item_id(meta: &[u8], start: usize, end: usize) -> Option<u32> {
    let (b, e) = find_box(meta, start, end, b"pitm")?;
    if e < b + 4 {
        return None;
    }
    let version = meta[b];
    let p = b + 4;
    if version == 0 {
        (p + 2 <= e).then(|| u16::from_be_bytes([meta[p], meta[p + 1]]) as u32)
    } else {
        (p + 4 <= e).then(|| u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]))
    }
}

/// `ipma` を読み、指定アイテムに紐づくプロパティ番号を**並び順のまま**返す。
///
/// 並び順には意味がある（回転と鏡映は書かれた順に適用する）。
fn property_indices(meta: &[u8], start: usize, end: usize, item: u32) -> Vec<usize> {
    // `ipma` は1つとは限らない（規格は16ビットIDの版と32ビットIDの版を
    // 別の箱に分けることを許す）。目的のアイテムが後ろの箱にいることもあるので、
    // 見つかるまで全部当たる。1つ目で打ち切ると寸法が取れず、
    // そのファイルは永久にサムネイルが付かない
    for (kind, b, e) in boxes(meta, start, end) {
        if &kind != b"ipma" {
            continue;
        }
        let found = property_indices_in(meta, b, e, item);
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// `ipma` の箱1つ分を読む。
fn property_indices_in(meta: &[u8], b: usize, e: usize, item: u32) -> Vec<usize> {
    if e < b + 8 {
        return Vec::new();
    }
    let version = meta[b];
    let flags = u32::from_be_bytes([0, meta[b + 1], meta[b + 2], meta[b + 3]]);
    // flags の最下位ビットが立っていればプロパティ番号は15ビット（2バイト）
    let wide_index = flags & 1 != 0;
    let mut p = b + 4;
    let count = u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]) as usize;
    p += 4;
    for _ in 0..count {
        let id = if version < 1 {
            if p + 2 > e {
                return Vec::new();
            }
            let v = u16::from_be_bytes([meta[p], meta[p + 1]]) as u32;
            p += 2;
            v
        } else {
            if p + 4 > e {
                return Vec::new();
            }
            let v = u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]);
            p += 4;
            v
        };
        if p >= e {
            return Vec::new();
        }
        let n = meta[p] as usize;
        p += 1;
        let mut indices = Vec::with_capacity(n);
        for _ in 0..n {
            if wide_index {
                if p + 2 > e {
                    return Vec::new();
                }
                // 最上位ビットは「必須プロパティか」の印。番号は下位15ビット
                indices.push((u16::from_be_bytes([meta[p], meta[p + 1]]) & 0x7fff) as usize);
                p += 2;
            } else {
                if p + 1 > e {
                    return Vec::new();
                }
                indices.push((meta[p] & 0x7f) as usize);
                p += 1;
            }
        }
        if id == item {
            return indices;
        }
    }
    Vec::new()
}

/// コンテナを読んで主画像の寸法と向きを得る。デコードしない。
///
/// 読むのは `meta` ボックスだけ（実測36KB）なので、数MBのファイルでも一瞬で返る。
pub fn read_info(path: &Path) -> Option<HeifInfo> {
    let meta = read_meta_box(path)?;
    // meta はフルボックス（バージョン+フラグの4バイト）の後ろに子の箱が続く
    if meta.len() < 4 {
        return None;
    }
    let (start, end) = (4usize, meta.len());
    let primary = primary_item_id(&meta, start, end)?;
    read_info_at(&meta, start, end, primary)
}

/// 読み込み済みの `meta` から、指定アイテムの寸法と向きを取り出す。
///
/// AVIFの材料集め（[`read_avif_source`]）は `meta` を一度しか読まないので、
/// ファイルを開き直さずに済むようにここを分けている。
fn read_info_at(meta: &[u8], start: usize, end: usize, primary: u32) -> Option<HeifInfo> {
    let (iprp_b, iprp_e) = find_box(meta, start, end, b"iprp")?;
    let (ipco_b, ipco_e) = find_box(meta, iprp_b, iprp_e, b"ipco")?;
    // ipco の子は「1から数える番号」で ipma から参照される
    let props = boxes(meta, ipco_b, ipco_e);

    let mut info = HeifInfo {
        stored_width: 0,
        stored_height: 0,
        rotation: 0,
        mirror: None,
        mirror_first: false,
        crop: None,
    };
    let mut seen_rotation = false;
    for index in property_indices(meta, iprp_b, iprp_e, primary) {
        let Some((kind, b, e)) = props.get(index.wrapping_sub(1)).copied() else {
            continue;
        };
        match &kind {
            // ispe: バージョン+フラグの4バイトの後ろに幅・高さ
            b"ispe" if e >= b + 12 => {
                info.stored_width =
                    u32::from_be_bytes([meta[b + 4], meta[b + 5], meta[b + 6], meta[b + 7]]);
                info.stored_height =
                    u32::from_be_bytes([meta[b + 8], meta[b + 9], meta[b + 10], meta[b + 11]]);
            }
            // irot: 1バイト。下位2ビットが「反時計回りに90度×角度」
            b"irot" if e > b => {
                info.rotation = meta[b] & 3;
                seen_rotation = true;
            }
            // imir: 1バイト。下位1ビットが鏡映の軸
            b"imir" if e > b => {
                info.mirror = Some(meta[b] & 1);
                info.mirror_first = !seen_rotation;
            }
            _ => {}
        }
    }
    if info.stored_width == 0 || info.stored_height == 0 {
        return None;
    }
    // clap は寸法が確定してからでないと画素の矩形に直せない（中心からの相対のため）
    info.crop = property_of(meta, start, end, primary, b"clap")
        .and_then(|(b, e)| parse_clap(&meta[b..e], info.stored_width, info.stored_height));
    Some(info)
}

/// `hvcC`（HEVCDecoderConfigurationRecord）の中で `chroma_format_idc` が居る位置。
///
/// 先頭から順に configurationVersion(1) / profile_space+tier+profile_idc(1) /
/// compatibility_flags(4) / constraint_indicator(6) / level_idc(1) /
/// min_spatial_segmentation_idc(2) / parallelismType(1) と並び、その次。
const HVCC_CHROMA_OFFSET: usize = 16;

/// 格納されている画素の色差の間引きを読む。デコードしない。
///
/// 詰め直しでどこまで間引いてよいかを決めるのに要る。**元が4:2:0なら4:2:0で
/// 出しても失うものは無い**が、HEIFは4:2:0とは限らない——Canonの `.HIF`
/// （EOS R系）は**4:2:2**で書かれ、規格上は4:4:4も許される。そこを間引くと
/// 色の境目が目に見えて崩れる。
///
/// **分からないときは `None`**。呼び出し側は間引かない側（4:4:4）へ倒すこと。
///
/// **`ipco` にある `hvcC` を全部見てはいけない**。iPhoneのHEICは主画像のほかに
/// **HDRゲインマップや深度**を同じファイルへ入れ、それらは**モノクロHEVC**
/// （`chroma_format_idc = 0`）で自分の `hvcC` を持つ。実ファイルでは `hvcC` が
/// 6個・値が `1,0,1,0,1,1` と混ざっていた。全部が揃うことを求めると、
/// **ゲインマップ付き（iOS 14.1以降のHDR写真はほぼ全部）で常に「分からない」に
/// 落ちる**——安全側ではあるが、速さの取り分がまるごと消える
/// （PR #23 のゲート2 P2。**出力サイズが4:4:4の値と一致することで見つかった**）。
///
/// そこで**主アイテムと、それが `dimg` で参照するタイルだけ**を見る。
/// iPhoneの主画像は `grid`（タイルの寄せ集め）で `hvcC` を自分では持たないので、
/// タイル側まで辿る必要がある。ゲインマップは主アイテムから `dimg` では
/// 参照されない（`auxl` で主画像を指す側）ので、これで外れる。
///
/// **派生を辿るのは1段だけ**。`iden`（切り抜き等）を挟んで `grid` がぶら下がる
/// 入れ子では `hvcC` に届かず「分からない」に落ちる——遅いが正しい側なので
/// そのままにしてある（iPhoneにもCanonにもこの形は無い）。
pub fn stored_chroma(path: &Path) -> Option<crate::jpeg::ChromaSampling> {
    let meta = read_meta_box(path)?;
    if meta.len() < 4 {
        return None;
    }
    let (start, end) = (4usize, meta.len());
    let primary = primary_item_id(&meta, start, end)?;

    // 主アイテム自身（単一画像のHEIF）と、grid のタイル
    let mut items = vec![primary];
    items.extend(derived_from(&meta, start, end, primary));

    let mut found: Option<u8> = None;
    for item in items {
        let Some((b, e)) = property_of(&meta, start, end, item, b"hvcC") else {
            continue;
        };
        let idc = hvcc_chroma_idc(meta.get(b..e)?)?;
        if found.is_some_and(|seen| seen != idc) {
            return None; // タイルごとに違う＝一枚の絵として何とも言えない
        }
        found = Some(idc);
    }
    Some(chroma_of_idc(found?))
}

/// `hvcC` の中身から `chroma_format_idc` を読む。
///
/// 版が違えば並びも違うかもしれない。切り詰められた箱も同じ
/// ——**当てずっぽうで読むより黙る**。
fn hvcc_chroma_idc(hvcc: &[u8]) -> Option<u8> {
    if hvcc.len() <= HVCC_CHROMA_OFFSET || hvcc.first() != Some(&1) {
        return None;
    }
    Some(hvcc[HVCC_CHROMA_OFFSET] & 0b11)
}

/// 0=モノクロ / 1=4:2:0 / 2=4:2:2 / 3=4:4:4。**4:2:0以外は間引かない側へ**。
fn chroma_of_idc(idc: u8) -> crate::jpeg::ChromaSampling {
    match idc {
        1 => crate::jpeg::ChromaSampling::Half,
        _ => crate::jpeg::ChromaSampling::Full,
    }
}

/// `hvcC` の中身だけを手に持っているときの [`stored_chroma`]。
///
/// CR3のHDR PQのプレビューは箱から `hvcC` を直接取り出すので、ファイルを
/// 開き直さずに聞ける。**Canonのこの絵は4:2:2**（実測 `chroma_format_idc = 2`）
/// なので、4:2:0で詰め直すと縦の色差を捨てることになる。
///
/// **分からないときは `None`**。呼び出し側は間引かない側（4:4:4）へ倒すこと。
pub fn chroma_from_hvcc(hvcc: &[u8]) -> Option<crate::jpeg::ChromaSampling> {
    hvcc_chroma_idc(hvcc).map(chroma_of_idc)
}

/// 表示上の寸法（回転を反映）。コンテナが読めなければ None。
pub fn display_dimensions(path: &Path) -> Option<(u32, u32)> {
    read_info(path).map(|i| i.display_size())
}

// ---------------------------------------------------------------------------
// AVIF: 主画像の材料をコンテナから取り出す
// ---------------------------------------------------------------------------

/// アイテム1つの実体として読み込む上限。壊れたファイルで数GBを抱え込まないため
const MAX_ITEM_BYTES: usize = 256 * 1024 * 1024;

/// 1枚を組み立てるのに読む実体の総量。`grid` はタイルが数万枚まで宣言できるので、
/// 1枚ずつの上限だけでは総量が抑えられない
const MAX_TOTAL_ITEM_BYTES: usize = 512 * 1024 * 1024;

/// `grid` アイテムの並べ方。タイルを左上から行優先で敷き詰めて切り落とす。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub rows: u32,
    pub cols: u32,
    /// 敷き詰めた後に切り落とす表示寸法（タイル全体より小さいことがある）
    pub width: u32,
    pub height: u32,
}

/// `clap`（表示時の切り出し）。中心からの相対で書かれているので画素の矩形に直して持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clap {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// AVIFの主画像を組み立てるのに要る材料。画素はまだ展開していない。
#[derive(Debug, Clone)]
pub struct AvifSource {
    /// 寸法と向き（`ispe` / `irot` / `imir`）
    pub info: HeifInfo,
    /// `av1C` の configOBUs（シーケンスヘッダ）。タイルの手前に必ず流す
    pub config_obus: Vec<u8>,
    /// 主画像のOBU。`grid` なら左上から行優先、そうでなければ1本
    pub tiles: Vec<Vec<u8>>,
    /// `grid` のときの並べ方
    pub grid: Option<Grid>,
    /// `colr`（nclx）が書いている色の素性。**シーケンスヘッダより優先する**
    pub color: Option<Nclx>,
    /// 透過（`auxl` で主画像に紐づく別アイテム）。無ければ不透明
    pub alpha: Option<AvifAlpha>,
}

/// `colr`（nclx）が書いている色の素性。
///
/// AVIFの規格では**シーケンスヘッダ（AV1側）よりこちらが優先**。
/// 実際に食い違うファイルがあり、そのときシーケンスヘッダを信じると色がずれる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nclx {
    pub primaries: u16,
    pub transfer: u16,
    pub matrix: u16,
    /// 画素値が 0〜255 いっぱいを使う書き方か
    pub full_range: bool,
}

/// 透過を持つAVIFの、alpha側の材料。
///
/// alphaは**主画像とは別のAV1ストリーム**（白黒1枚）として入っていて、
/// `auxl` で主画像に紐づけられている。
#[derive(Debug, Clone)]
pub struct AvifAlpha {
    pub config_obus: Vec<u8>,
    pub tiles: Vec<Vec<u8>>,
    /// 色に既にalphaが掛けられている（`prem`）。掛け直してはいけない
    pub premultiplied: bool,
}

/// `iloc` の1アイテム分の在りか。
struct ItemLocation {
    /// 0=ファイル先頭からのオフセット / 1=`idat` の中 / 2=他アイテムの中（非対応）
    construction: u8,
    base: u64,
    extents: Vec<(u64, u64)>,
}

/// `iloc` を読み、アイテムIDごとの在りかを返す。
fn item_locations(meta: &[u8], start: usize, end: usize) -> Vec<(u32, ItemLocation)> {
    let mut out = Vec::new();
    let Some((b, e)) = find_box(meta, start, end, b"iloc") else {
        return out;
    };
    if e < b + 8 {
        return out;
    }
    let version = meta[b];
    let mut p = b + 4;
    let offset_size = (meta[p] >> 4) as usize;
    let length_size = (meta[p] & 15) as usize;
    let base_size = (meta[p + 1] >> 4) as usize;
    // 幅は4ビットの生の値なので、壊れたファイルは 15 と書ける。
    // そのまま読むと9バイト目で u64 が溢れる（debugではpanic、releaseでは折り返す）。
    // 規格が許すのは 0/4/8 だけなので、それ以外は読まない
    let valid = |n: usize| matches!(n, 0 | 4 | 8);
    if !valid(offset_size) || !valid(length_size) || !valid(base_size) {
        return out;
    }
    // version 0 は index_size のニブルを使わない（予約領域）
    let index_size = if version >= 1 {
        (meta[p + 1] & 15) as usize
    } else {
        0
    };
    if !valid(index_size) {
        return out;
    }
    p += 2;
    // 幅がファイルごとに違う（0/4/8バイト）ので、読み進めながら都度取り出す
    let read = |p: &mut usize, n: usize| -> Option<u64> {
        if n == 0 {
            return Some(0);
        }
        if *p + n > e {
            return None;
        }
        let v = meta[*p..*p + n]
            .iter()
            .fold(0u64, |a, &x| (a * 256) + x as u64);
        *p += n;
        Some(v)
    };
    let count = if version < 2 {
        read(&mut p, 2)
    } else {
        read(&mut p, 4)
    };
    let Some(count) = count else { return out };
    for _ in 0..count {
        let id = if version < 2 {
            read(&mut p, 2)
        } else {
            read(&mut p, 4)
        };
        let Some(id) = id else { return out };
        let construction = if version >= 1 {
            // 上位12ビットは予約。下位4ビットが construction_method
            match read(&mut p, 2) {
                Some(v) => (v & 15) as u8,
                None => return out,
            }
        } else {
            0
        };
        // data_reference_index（外部ファイル参照。ここでは扱わない）
        if read(&mut p, 2).is_none() {
            return out;
        }
        let Some(base) = read(&mut p, base_size) else {
            return out;
        };
        let Some(extent_count) = read(&mut p, 2) else {
            return out;
        };
        let mut extents = Vec::new();
        for _ in 0..extent_count {
            if read(&mut p, index_size).is_none() {
                return out;
            }
            let (Some(offset), Some(length)) =
                (read(&mut p, offset_size), read(&mut p, length_size))
            else {
                return out;
            };
            extents.push((offset, length));
        }
        out.push((
            id as u32,
            ItemLocation {
                construction,
                base,
                extents,
            },
        ));
    }
    out
}

/// `iinf` を読み、アイテムIDと種別（`av01` / `grid` など）の対応を返す。
fn item_types(meta: &[u8], start: usize, end: usize) -> Vec<(u32, [u8; 4])> {
    let mut out = Vec::new();
    let Some((b, e)) = find_box(meta, start, end, b"iinf") else {
        return out;
    };
    if e < b + 4 {
        return out;
    }
    // FullBox のヘッダ4バイトの後ろに entry_count（版で幅が違う）が続き、
    // その後は `infe` の並び。数は `infe` を数えれば分かるので読み飛ばす
    let skip = if meta[b] == 0 { 6 } else { 8 };
    for (kind, ib, ie) in boxes(meta, b + skip, e) {
        if &kind != b"infe" || ie < ib + 4 {
            continue;
        }
        let version = meta[ib];
        let mut p = ib + 4;
        let id = if version >= 3 {
            if p + 4 > ie {
                continue;
            }
            let v = u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]);
            p += 4;
            v
        } else {
            if p + 2 > ie {
                continue;
            }
            let v = u16::from_be_bytes([meta[p], meta[p + 1]]) as u32;
            p += 2;
            v
        };
        // protection_index
        p += 2;
        if p + 4 > ie {
            continue;
        }
        out.push((id, [meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]));
    }
    out
}

/// `iref` の `dimg`（派生アイテムが参照する元アイテム）を**並び順のまま**返す。
///
/// `grid` のタイルはこの順に左上から行優先で並ぶ。
fn derived_from(meta: &[u8], start: usize, end: usize, item: u32) -> Vec<u32> {
    references_from(meta, start, end, b"dimg", item)
}

/// `iref` の指定種別で、**`item` が指している先**を並び順のまま返す。
fn references_from(meta: &[u8], start: usize, end: usize, kind: &[u8; 4], item: u32) -> Vec<u32> {
    let Some((b, e)) = find_box(meta, start, end, b"iref") else {
        return Vec::new();
    };
    if e < b + 4 {
        return Vec::new();
    }
    let wide = meta[b] >= 1;
    for (k, rb, re) in boxes(meta, b + 4, e) {
        if &k != kind {
            continue;
        }
        let mut p = rb;
        let id = if wide {
            if p + 4 > re {
                continue;
            }
            let v = u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]]);
            p += 4;
            v
        } else {
            if p + 2 > re {
                continue;
            }
            let v = u16::from_be_bytes([meta[p], meta[p + 1]]) as u32;
            p += 2;
            v
        };
        if id != item || p + 2 > re {
            continue;
        }
        let count = u16::from_be_bytes([meta[p], meta[p + 1]]) as usize;
        p += 2;
        let step = if wide { 4 } else { 2 };
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if p + step > re {
                break;
            }
            out.push(if wide {
                u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]])
            } else {
                u16::from_be_bytes([meta[p], meta[p + 1]]) as u32
            });
            p += step;
        }
        return out;
    }
    Vec::new()
}

/// `iref` の指定種別で、**`target` を指しているアイテム**を集める。
///
/// `dimg`（[`derived_from`]）と向きが逆。透過は「alpha側が主画像を指す」
/// 形（`auxl`）で紐づくので、こちらの引き方が要る。
fn references_to(meta: &[u8], start: usize, end: usize, kind: &[u8; 4], target: u32) -> Vec<u32> {
    let Some((b, e)) = find_box(meta, start, end, b"iref") else {
        return Vec::new();
    };
    if e < b + 4 {
        return Vec::new();
    }
    let wide = meta[b] >= 1;
    let step = if wide { 4 } else { 2 };
    let read = |p: usize| -> u32 {
        if wide {
            u32::from_be_bytes([meta[p], meta[p + 1], meta[p + 2], meta[p + 3]])
        } else {
            u16::from_be_bytes([meta[p], meta[p + 1]]) as u32
        }
    };
    let mut out = Vec::new();
    for (k, rb, re) in boxes(meta, b + 4, e) {
        if &k != kind || rb + step + 2 > re {
            continue;
        }
        let from = read(rb);
        let mut p = rb + step;
        let count = u16::from_be_bytes([meta[p], meta[p + 1]]) as usize;
        p += 2;
        for _ in 0..count {
            if p + step > re {
                break;
            }
            if read(p) == target {
                out.push(from);
                break;
            }
            p += step;
        }
    }
    out
}

/// アイテムの実体を取り出す。`idat` の中にあるものはファイルを読まずに済む。
fn read_item(
    file: &mut std::fs::File,
    meta: &[u8],
    idat: Option<(usize, usize)>,
    loc: &ItemLocation,
) -> Option<Vec<u8>> {
    // construction_method=2（他アイテムの中）は実物を見たことがないので扱わない
    if loc.construction > 1 {
        return None;
    }
    let mut out = Vec::new();
    for &(offset, length) in &loc.extents {
        let at = loc.base.checked_add(offset)?;
        let length = usize::try_from(length).ok()?;
        if out.len().checked_add(length)? > MAX_ITEM_BYTES {
            return None;
        }
        if loc.construction == 1 {
            let (ib, ie) = idat?;
            let b = ib.checked_add(usize::try_from(at).ok()?)?;
            let e = b.checked_add(length)?;
            if e > ie {
                return None;
            }
            out.extend_from_slice(&meta[b..e]);
        } else {
            file.seek(SeekFrom::Start(at)).ok()?;
            let mut buf = vec![0u8; length];
            file.read_exact(&mut buf).ok()?;
            out.extend_from_slice(&buf);
        }
    }
    Some(out)
}

/// `grid` アイテムの中身（並べ方）を読む。
fn parse_grid(payload: &[u8]) -> Option<Grid> {
    if payload.len() < 8 {
        return None;
    }
    let flags = payload[1];
    let rows = payload[2] as u32 + 1;
    let cols = payload[3] as u32 + 1;
    // flags の最下位ビットが立っていれば出力寸法は32ビット
    let (width, height) = if flags & 1 != 0 {
        if payload.len() < 16 {
            return None;
        }
        (
            u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]),
            u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]),
        )
    } else {
        (
            u16::from_be_bytes([payload[4], payload[5]]) as u32,
            u16::from_be_bytes([payload[6], payload[7]]) as u32,
        )
    };
    (width > 0 && height > 0).then_some(Grid {
        rows,
        cols,
        width,
        height,
    })
}

/// `colr` を読む。`nclx` 以外（埋め込みICCプロファイル）は扱わない。
fn parse_colr(body: &[u8]) -> Option<Nclx> {
    // 先頭4バイトが種別。`rICC`/`prof` はICCプロファイルで、
    // 正しく扱うにはカラーマネジメントが要るので手を出さない
    if body.len() < 11 || &body[0..4] != b"nclx" {
        return None;
    }
    let at = |i: usize| u16::from_be_bytes([body[i], body[i + 1]]);
    Some(Nclx {
        primaries: at(4),
        transfer: at(6),
        matrix: at(8),
        // 最上位ビットが full_range_flag、残り7ビットは予約
        full_range: body[10] & 0x80 != 0,
    })
}

/// `clap` を画素の矩形に直す。中心からの相対で書かれているので寸法が要る。
fn parse_clap(body: &[u8], stored_width: u32, stored_height: u32) -> Option<Clap> {
    if body.len() < 32 {
        return None;
    }
    // 32バイトあることは上で確かめている。それでも取り出しを `unwrap` に
    // 頼らないのは、**上の条件を後から緩めたときに黙って落ちる**のを避けるため
    // ——0が返っても、分母なら `ratio` が `None`、分子なら幅・高さが0になって
    // 下の `width < 1.0` で落ちる。どちらも「切り出さない」に着く
    let num = |i: usize| -> i32 {
        body.get(i * 4..i * 4 + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(i32::from_be_bytes)
            .unwrap_or(0)
    };
    let ratio = |n: i32, d: i32| -> Option<f64> { (d != 0).then(|| n as f64 / d as f64) };
    let width = ratio(num(0), num(1))?;
    let height = ratio(num(2), num(3))?;
    let off_x = ratio(num(4), num(5))?;
    let off_y = ratio(num(6), num(7))?;
    // 規格の定義: 中心は ((格納寸法 - 1) / 2) + オフセット
    let center_x = (stored_width as f64 - 1.0) / 2.0 + off_x;
    let center_y = (stored_height as f64 - 1.0) / 2.0 + off_y;
    let x = (center_x - (width - 1.0) / 2.0).round();
    let y = (center_y - (height - 1.0) / 2.0).round();
    if width < 1.0 || height < 1.0 || x < 0.0 || y < 0.0 {
        return None;
    }
    let (x, y, w, h) = (x as u32, y as u32, width as u32, height as u32);
    // 枠からはみ出す指定は無視する（切り出さない方がまだ絵になる）
    (x.checked_add(w)? <= stored_width && y.checked_add(h)? <= stored_height).then_some(Clap {
        x,
        y,
        width: w,
        height: h,
    })
}

/// 指定アイテムに紐づくプロパティを名前で**すべて**返す。
///
/// `colr` のように**同じ名前の箱を複数置ける**ものがある（規格は色の種類ごとに
/// 1つずつ許す）。1つ目で打ち切ると、ICCプロファイルを持つファイルで
/// nclx の指定を取り落とす（libavifはICCの方を先に書く）。
fn properties_of(
    meta: &[u8],
    start: usize,
    end: usize,
    item: u32,
    name: &[u8; 4],
) -> Vec<(usize, usize)> {
    let Some((iprp_b, iprp_e)) = find_box(meta, start, end, b"iprp") else {
        return Vec::new();
    };
    let Some((ipco_b, ipco_e)) = find_box(meta, iprp_b, iprp_e, b"ipco") else {
        return Vec::new();
    };
    let props = boxes(meta, ipco_b, ipco_e);
    property_indices(meta, iprp_b, iprp_e, item)
        .into_iter()
        .filter_map(|index| match props.get(index.wrapping_sub(1)) {
            Some(&(kind, b, e)) if &kind == name => Some((b, e)),
            _ => None,
        })
        .collect()
}

/// 指定アイテムに紐づくプロパティを名前で1つ探す。
fn property_of(
    meta: &[u8],
    start: usize,
    end: usize,
    item: u32,
    name: &[u8; 4],
) -> Option<(usize, usize)> {
    let (iprp_b, iprp_e) = find_box(meta, start, end, b"iprp")?;
    let (ipco_b, ipco_e) = find_box(meta, iprp_b, iprp_e, b"ipco")?;
    let props = boxes(meta, ipco_b, ipco_e);
    property_indices(meta, iprp_b, iprp_e, item)
        .into_iter()
        .find_map(|index| match props.get(index.wrapping_sub(1)) {
            Some(&(kind, b, e)) if &kind == name => Some((b, e)),
            _ => None,
        })
}

/// 指定アイテムの `av1C` から configOBUs を取り出す。
fn av1_config(meta: &[u8], start: usize, end: usize, item: u32) -> Option<Vec<u8>> {
    property_of(meta, start, end, item, b"av1C").map(|(b, e)| {
        // 先頭4バイトは marker/version と符号化の素性。その後ろが configOBUs
        meta[(b + 4).min(e)..e].to_vec()
    })
}

/// AVIFの主画像を組み立てる材料をコンテナから取り出す。
///
/// `grid` で分割された画像もタイルを並び順のまま集める。展開は [`crate::av1`] が行う。
pub fn read_avif_source(path: &Path) -> Option<AvifSource> {
    let meta = read_meta_box(path)?;
    if meta.len() < 4 {
        return None;
    }
    let (start, end) = (4usize, meta.len());
    let primary = primary_item_id(&meta, start, end)?;
    let info = read_info_at(&meta, start, end, primary)?;
    let idat = find_box(&meta, start, end, b"idat");
    let locations = item_locations(&meta, start, end);
    let locate = |id: u32| locations.iter().find(|(i, _)| *i == id).map(|(_, l)| l);
    let types = item_types(&meta, start, end);

    let mut file = std::fs::File::open(path).ok()?;

    // 主画像が `grid` なら、実際の画素は参照先のタイルにある
    let is_grid = types.iter().any(|(i, k)| *i == primary && k == b"grid");
    let (grid, tile_ids) = if is_grid {
        let payload = read_item(&mut file, &meta, idat, locate(primary)?)?;
        let grid = parse_grid(&payload)?;
        let ids = derived_from(&meta, start, end, primary);
        // タイルが足りない grid は敷き詰められないので諦める
        if ids.len() < (grid.rows as usize).checked_mul(grid.cols as usize)? {
            return None;
        }
        (Some(grid), ids)
    } else {
        (None, vec![primary])
    };

    // av1C は主画像に付いているのが普通だが、`grid` のときはタイル側にしか無い
    let config_obus = av1_config(&meta, start, end, primary).or_else(|| {
        tile_ids
            .first()
            .and_then(|&id| av1_config(&meta, start, end, id))
    })?;

    let mut tiles = Vec::with_capacity(tile_ids.len());
    let mut total = 0usize;
    for id in tile_ids {
        let tile = read_item(&mut file, &meta, idat, locate(id)?)?;
        // 1枚ずつの上限だけだと、タイルが多い `grid` で総量が青天井になる
        total = total.checked_add(tile.len())?;
        if total > MAX_TOTAL_ITEM_BYTES {
            return None;
        }
        tiles.push(tile);
    }
    // colr は主画像に付く。無ければシーケンスヘッダ側の指定に従う。
    // **ICCプロファイルの colr が先に来ることがある**ので、名前で1つ目を
    // 取るのではなく nclx として読める方を選ぶ
    let color = properties_of(&meta, start, end, primary, b"colr")
        .into_iter()
        .find_map(|(b, e)| parse_colr(&meta[b..e]));
    let alpha = read_alpha(
        &mut file, &meta, start, end, idat, &locations, &types, primary, total,
    );
    Some(AvifSource {
        info,
        config_obus,
        tiles,
        grid,
        color,
        alpha,
    })
}

/// 透過（alpha）の材料を集める。無ければ None。
///
/// alphaは `auxl` で主画像を指す**別のアイテム**として入っている。
/// 種別の見分けは `auxC` のURN（`...auxiliary:alpha`）で行う。
/// `grid` に分かれた透過は実物を見たことがないので扱わない
/// （その場合は透過を諦めるだけで、色は普通に出る）。
#[allow(clippy::too_many_arguments)]
fn read_alpha(
    file: &mut std::fs::File,
    meta: &[u8],
    start: usize,
    end: usize,
    idat: Option<(usize, usize)>,
    locations: &[(u32, ItemLocation)],
    types: &[(u32, [u8; 4])],
    primary: u32,
    // 主画像で既に読んだ量。透過を足しても総量の上限を超えないこと
    used: usize,
) -> Option<AvifAlpha> {
    let candidates = references_to(meta, start, end, b"auxl", primary);
    let alpha_id = candidates.into_iter().find(|&id| {
        // auxC: FullBoxの4バイトの後ろにヌル終端のURN
        property_of(meta, start, end, id, b"auxC").is_some_and(|(b, e)| {
            let urn = &meta[(b + 4).min(e)..e];
            urn.windows(5).any(|w| w == b"alpha")
        }) && types.iter().any(|(i, k)| *i == id && k == b"av01")
    })?;
    let loc = locations
        .iter()
        .find(|(i, _)| *i == alpha_id)
        .map(|(_, l)| l)?;
    let tile = read_item(file, meta, idat, loc)?;
    if used.checked_add(tile.len())? > MAX_TOTAL_ITEM_BYTES {
        return None;
    }
    Some(AvifAlpha {
        config_obus: av1_config(meta, start, end, alpha_id)?,
        tiles: vec![tile],
        // `prem` は**主画像からalphaへ**張られる（`auxl` と向きが逆）。
        // 逆に引くと常に「掛かっていない」と判定してしまい、
        // 掛け済みの色にもう一度掛けて半透明部だけ薄くなる
        premultiplied: references_from(meta, start, end, b"prem", primary).contains(&alpha_id),
    })
}

// ---------------------------------------------------------------------------
// 画素のデコード（OS任せ）
// ---------------------------------------------------------------------------

/// このOS・この環境でHEIFをデコードできるか。
///
/// UIで「表示するには拡張機能が要ります」と案内するために使う。
pub fn decoder_available() -> bool {
    #[cfg(windows)]
    {
        windows_wic::available()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// この環境でHEIF（HEVC）の画素を**実際に**展開できるか、見本1枚で確かめる。
///
/// [`decoder_available`] は「画像デコードの土台（WIC）が居るか」しか分からない。
/// **HEVCのコーデックは別インストール**なので、在るかどうかは
/// 実ファイルを読ませてみるまで確定しない。
/// 主画像は重い（実測260〜480ms）ので、まず埋め込みサムネイル（同22ms）で試す。
pub fn can_decode(path: &Path) -> bool {
    decode_thumbnail(path).is_some() || decode(path).is_some()
}

/// WICの「縮小しながら展開」への対応を調べた結果（0.2 HEICの詰め直し・計測用）。
///
/// JPEGなら1/8まで安く起こせる（[`crate::jpeg::decode_scaled`]）が、
/// HEVCに同じ仕掛けがあるかは**デコーダに聞かないと分からない**。
/// 聞き方は `IWICBitmapSourceTransform::GetClosestSize` で、
/// 希望の寸法を渡すと「実際に出せる一番近い寸法」が返る。
/// 原寸がそのまま返るなら、間引いた展開はできない＝縮小してもデコードは安くならない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaledDecodeProbe {
    /// フレームが `IWICBitmapSourceTransform` を実装しているか
    pub has_transform: bool,
    /// 原寸（格納されている向きのまま）
    pub full: (u32, u32),
    /// こちらが希望した寸法
    pub requested: (u32, u32),
    /// デコーダが出せると答えた寸法
    pub closest: (u32, u32),
    /// デコーダが出せると答えた画素形式（`GUID_WICPixelFormat*` の名前か生のGUID）
    pub closest_format: String,
}

impl ScaledDecodeProbe {
    /// 希望どおり縮めて起こせるか。
    ///
    /// **`None` は「聞けていない」**。元が既に `max_edge` に収まっていると
    /// 希望寸法＝原寸になり、デコーダは当然そのまま返す。これを false（＝
    /// 縮小に対応していない）と読むと、小さい素材で測っただけで
    /// 「WICは縮小デコードできない」と断じてしまう。
    pub fn scales(&self) -> Option<bool> {
        if !self.has_transform {
            return Some(false);
        }
        if self.requested == self.full {
            return None;
        }
        Some(self.closest != self.full)
    }
}

/// 長辺 `max_edge` で起こせるかをデコーダに聞く（Windowsのみ）。
///
/// **計測用**。実際に縮小デコードする経路は用意していない——
/// [`ScaledDecodeProbe::scales`] が真になる環境が見つかってから考える。
pub fn probe_scaled_decode(path: &Path, max_edge: u32) -> Option<ScaledDecodeProbe> {
    #[cfg(windows)]
    {
        windows_wic::probe_scaled_decode(path, max_edge)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, max_edge);
        None
    }
}

/// 長辺 `max_edge` を目指して**縮小しながら**デコードする（Windowsのみ）。
///
/// **収まるとは限らない**。出せる寸法を決めるのはデコーダで、縮小に
/// 対応していなければ原寸がそのまま返る。呼ぶ側は返った絵の寸法を見ること。
///
/// [`probe_scaled_decode`] が「対応あり」と答えても、デコーダが内部で
/// 原寸まで起こしてから縮めているだけなら1msも得しない。**本当に安いかは
/// 時間を測るまで分からない**ので、まず測るためにこれを足した
/// （`bench --heif-encode`）。使うと決めるまで本体からは呼ばない。
///
/// 向きは [`decode`] と同じくコンテナの `irot`/`imir` を適用済み。
pub fn decode_scaled(path: &Path, max_edge: u32) -> Option<image::DynamicImage> {
    #[cfg(windows)]
    {
        let img = windows_wic::decode_scaled(path, max_edge)?;
        Some(match read_info(path) {
            Some(info) => apply_container_transform(img, &info),
            None => img,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = (path, max_edge);
        None
    }
}

/// HEIFをデコードして**表示の向きに直した**画像を返す。
///
/// `irot`/`imir` はここで適用済みなので、呼び出し側でEXIF Orientationを
/// 重ねてはいけない（二重回転になる）。
pub fn decode(path: &Path) -> Option<image::DynamicImage> {
    #[cfg(windows)]
    {
        let img = windows_wic::decode(path)?;
        Some(match read_info(path) {
            Some(info) => apply_container_transform(img, &info),
            None => img,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// ISO-BMFFの箱を1つ組む。
fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// 版と旗を持つ箱（FullBox）を1つ組む。
fn full_box(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut inner = vec![version];
    inner.extend_from_slice(&flags.to_be_bytes()[1..]);
    inner.extend_from_slice(body);
    boxed(kind, &inner)
}

/// 裸のHEVCビットストリームを、画像1枚だけの最小のHEIFに包む。
///
/// CanonのHDR PQのCR3は、プレビューを「ヘッダの無いHEIF」として抱えている
/// （[`crate::raw`] の `cr3_hevc_box` が取り出す）。OSのデコーダに渡すための
/// 入れ物なので、要るものしか入れない。
///
/// - `hvcc` は HEVCDecoderConfigurationRecord の**中身**（箱ヘッダを含まない）
/// - `colr` / `pixi` も同様に中身。**`pixi` は版と旗を既に含んでいる**ので、
///   ここで足し直してはいけない（足すとチャンネル数が0と読まれる）
/// - `hevc` は4バイト長前置のNAL列
///
/// **向きは入れない**。CR3の向きはEXIF側にあり、呼び出し側で当てる。
pub(crate) fn wrap_hevc_as_heif(
    hvcc: &[u8],
    width: u16,
    height: u16,
    colr: Option<&[u8]>,
    pixi: Option<&[u8]>,
    hevc: &[u8],
) -> Vec<u8> {
    /// 画像アイテムの番号。1枚しか入れないので固定
    const ITEM: u16 = 1;

    // **ブランドは `heix`。** `heic` は HEVC Main / Main Still を約束するが、
    // Canonのこの絵は Main 10・4:2:2（Range Extensions）なので範囲の外に出る。
    // WICはブランドを見ないので `heic` でも通ったが、ブランドで選別する読み手
    // （macOSのImageIOなど）に拒まれる余地を残す（ゲート2のP3・ゲート3のP2）
    let ftyp = {
        let mut b = b"heix".to_vec();
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(b"mif1heix");
        boxed(b"ftyp", &b)
    };

    let hdlr = {
        let mut b = 0u32.to_be_bytes().to_vec();
        b.extend_from_slice(b"pict");
        b.extend_from_slice(&[0u8; 12]);
        b.push(0); // 名前は空文字列
        full_box(b"hdlr", 0, 0, &b)
    };
    let pitm = full_box(b"pitm", 0, 0, &ITEM.to_be_bytes());
    let iinf = {
        let mut infe = ITEM.to_be_bytes().to_vec();
        infe.extend_from_slice(&0u16.to_be_bytes()); // protection_index
        infe.extend_from_slice(b"hvc1");
        infe.push(0); // item_name
        let infe = full_box(b"infe", 2, 0, &infe);
        let mut b = 1u16.to_be_bytes().to_vec();
        b.extend_from_slice(&infe);
        full_box(b"iinf", 0, 0, &b)
    };

    // ipco に積んだ順がそのまま ipma の番号（1始まり）になる。
    // hvcC は**必須**（essential）にする——読めないデコーダに絵を出させない
    let mut ipco = Vec::new();
    let mut assoc: Vec<u8> = Vec::new();
    let mut count = 0u8;
    let mut add = |ipco: &mut Vec<u8>, assoc: &mut Vec<u8>, bytes: Vec<u8>, essential: bool| {
        ipco.extend_from_slice(&bytes);
        count += 1;
        assoc.push(if essential { 0x80 | count } else { count });
    };
    add(&mut ipco, &mut assoc, boxed(b"hvcC", hvcc), true);
    let ispe = {
        let mut b = u32::from(width).to_be_bytes().to_vec();
        b.extend_from_slice(&u32::from(height).to_be_bytes());
        full_box(b"ispe", 0, 0, &b)
    };
    add(&mut ipco, &mut assoc, ispe, false);
    if let Some(colr) = colr {
        add(&mut ipco, &mut assoc, boxed(b"colr", colr), false);
    }
    if let Some(pixi) = pixi {
        // **`full_box` で包み直さない。** 取り出した中身が版と旗を既に持っている
        add(&mut ipco, &mut assoc, boxed(b"pixi", pixi), false);
    }
    let iprp = {
        let ipco = boxed(b"ipco", &ipco);
        let mut b = 1u32.to_be_bytes().to_vec(); // entry_count
        b.extend_from_slice(&ITEM.to_be_bytes());
        b.push(count);
        b.extend_from_slice(&assoc);
        let ipma = full_box(b"ipma", 0, 0, &b);
        let mut out = ipco;
        out.extend_from_slice(&ipma);
        boxed(b"iprp", &out)
    };

    // iloc は mdat の中身を**ファイル先頭からの絶対値**で指す。オフセット欄は
    // 4バイト固定なので、`meta` の大きさは指す値によらない——一度組んで測れば、
    // その値で組み直しても長さは変わらない
    let build = |data_offset: u32| -> (Vec<u8>, u32) {
        let iloc = {
            let mut b = vec![0x44, 0x00]; // offset_size=4 length_size=4 / base_offset_size=0
            b.extend_from_slice(&1u16.to_be_bytes()); // item_count
            b.extend_from_slice(&ITEM.to_be_bytes());
            b.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
            b.extend_from_slice(&1u16.to_be_bytes()); // extent_count
            b.extend_from_slice(&data_offset.to_be_bytes());
            b.extend_from_slice(&(hevc.len() as u32).to_be_bytes());
            full_box(b"iloc", 0, 0, &b)
        };
        let mut meta = Vec::new();
        meta.extend_from_slice(&hdlr);
        meta.extend_from_slice(&pitm);
        meta.extend_from_slice(&iinf);
        meta.extend_from_slice(&iprp);
        meta.extend_from_slice(&iloc);
        let meta = full_box(b"meta", 0, 0, &meta);
        let offset = (ftyp.len() + meta.len() + 8) as u32;
        (meta, offset)
    };
    let (_, offset) = build(0);
    let (meta, again) = build(offset);
    debug_assert_eq!(offset, again, "ilocのオフセット欄は4バイト固定のはず");

    let mut out = ftyp;
    out.extend_from_slice(&meta);
    out.extend_from_slice(&(hevc.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(b"mdat");
    out.extend_from_slice(hevc);
    debug_assert_eq!(offset as usize, out.len() - hevc.len());
    out
}

/// **メモリ上の**HEIFをデコードする。
///
/// CanonのHDR PQのCR3は、プレビューをHEVCで抱えている（[`crate::raw`] の
/// `cr3_hevc_preview`）。取り出したHEVCはその場でHEIFに包み直すので、
/// 一時ファイルを作らずに渡せる口が要る。**サムネイル1枚ごとにディスクへ
/// 書き戻すのは、数万枚を並べる作りと合わない**。
///
/// 向きの補正（`irot`/`imir`）はしない。包み直したHEIFにその情報を入れていない
/// ——向きはRAW側のEXIFから読んで、呼び出し側で当てる。
///
/// **macOSは未対応**（[`decode`] と同じく将来 ImageIO）。`None` を返すので、
/// 呼び出し側では「デコーダが無い環境」と同じ扱いになる。
pub fn decode_mem(bytes: &[u8]) -> Option<image::DynamicImage> {
    #[cfg(windows)]
    {
        windows_wic::decode_mem(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = bytes;
        None
    }
}

/// 埋め込みサムネイル（あれば）を小さいままデコードする。
///
/// iPhoneのHEICは主画像とは別に**小さいタイル集合**をサムネイルとして持っている。
/// 主画像（5712x4284・45タイル）を展開せずに済むので一覧が速い。
pub fn decode_thumbnail(path: &Path) -> Option<image::DynamicImage> {
    #[cfg(windows)]
    {
        let img = windows_wic::decode_thumbnail(path)?;
        Some(match read_info(path) {
            Some(info) => apply_container_transform(img, &info),
            None => img,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// デコーダが返した絵に、こちらで向きを直す必要があるか。
///
/// **OSのデコーダは普通コンテナの指示（`irot`/`imir`）を適用して返す**
/// （WindowsのWICは実測でそうだった）。それを知らずに重ねて掛けると二重回転になる。
/// かといって「必ず適用済み」と決め打つと、そうでないデコーダで倒れたまま出る。
///
/// そこで**縦横比で見分ける**。90度系の回転が指示されているのに、返ってきた絵が
/// まだ「格納された向き」のままなら、適用されていないのでこちらで直す。
/// サムネイルのように**倍率が違っても判定できる**のが要点で、
/// 原寸と実寸を突き合わせる方式だとサムネイルだけ二重回転する（実測で踏んだ）。
// デコーダを持たないOSでは呼ばれないが、macOS対応を足すときにそのまま使う
#[cfg_attr(not(windows), allow(dead_code))]
fn needs_manual_transform(width: u32, height: u32, info: &HeifInfo) -> bool {
    // 180度・鏡映のみ・正方形は、寸法が変わらないので見分けられない。
    // 判定できないときは「デコーダが適用済み」に倒す（普通のデコーダの挙動）
    if info.rotation % 2 != 1 || info.stored_width == info.stored_height {
        return false;
    }
    (width > height) == (info.stored_width > info.stored_height)
}

/// コンテナが指示する向きへ直す（デコーダが適用済みなら何もしない）。
#[cfg_attr(not(windows), allow(dead_code))]
fn apply_container_transform(img: image::DynamicImage, info: &HeifInfo) -> image::DynamicImage {
    if !needs_manual_transform(img.width(), img.height(), info) {
        return img;
    }
    apply_transform(img, info)
}

/// コンテナが指示する向きへ**必ず**直す。
///
/// 同梱のAV1デコーダ（[`crate::av1`]）は `irot`/`imir` を適用しないので、
/// OSデコーダ向けの「適用済みか見分ける」判定を挟まずにこちらを直接使う。
pub fn apply_transform(img: image::DynamicImage, info: &HeifInfo) -> image::DynamicImage {
    // axis=0 は垂直な軸での鏡映＝左右反転
    let mirror = |img: image::DynamicImage| match info.mirror {
        Some(0) => img.fliph(),
        Some(_) => img.flipv(),
        None => img,
    };
    let rotate = |img: image::DynamicImage| match info.rotation {
        1 => img.rotate270(), // 反時計回り90度
        2 => img.rotate180(),
        3 => img.rotate90(), // 反時計回り270度＝時計回り90度
        _ => img,
    };
    // 書かれた順に適用する（入れ替えると90度系では180度ずれる）
    if info.mirror_first {
        rotate(mirror(img))
    } else {
        mirror(rotate(img))
    }
}

// ---------------------------------------------------------------------------
// Windows: WIC（Windows Imaging Component）
// ---------------------------------------------------------------------------

/// Windowsの画像デコーダを直接叩く。
///
/// エクスプローラやフォトが使っているのと同じ経路で、
/// **HEIF Image Extensions**（Microsoft Store・Windows 11は既定で同梱）が
/// 入っていればHEICを読める。自前でHEVCデコーダを抱えないので、
/// 配布物が増えずライセンスの問題も起きない。
#[cfg(windows)]
mod windows_wic {
    use std::path::Path;

    use windows::core::{Interface, HSTRING};
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat24bppBGR, GUID_WICPixelFormat32bppBGR,
        GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameDecode, IWICBitmapSource,
        IWICBitmapSourceTransform, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeCustom, WICBitmapTransformRotate0, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Shell::SHCreateMemStream;

    thread_local! {
        /// COMはスレッドごとに初期化が要る。サムネイルワーカーは複数走るので
        /// スレッドローカルに1回だけ済ませる（解放はプロセス終了任せ）
        static COM_READY: bool = init_com();
    }

    fn init_com() -> bool {
        // 既に別のモード（STA）で初期化済みなら RPC_E_CHANGED_MODE が返るが、
        // COM自体は使えるので成功扱いにする
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() }
    }

    fn factory() -> Option<IWICImagingFactory> {
        COM_READY.with(|_| ());
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() }
    }

    /// WICが使えるか。デコーダの有無はファイルを読ませないと分からないので、
    /// ここでは「ファクトリが作れるか」までを見る。
    pub fn available() -> bool {
        factory().is_some()
    }

    /// `IWICBitmapSource` をRGB8の画像に落とす。
    fn to_image(source: &IWICBitmapSource) -> Option<image::DynamicImage> {
        unsafe {
            let factory = factory()?;
            let converter = factory.CreateFormatConverter().ok()?;
            converter
                .Initialize(
                    source,
                    &GUID_WICPixelFormat24bppBGR,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .ok()?;
            let (mut width, mut height) = (0u32, 0u32);
            converter.GetSize(&mut width, &mut height).ok()?;
            if width == 0 || height == 0 {
                return None;
            }
            let stride = width.checked_mul(3)?;
            let len = (stride as usize).checked_mul(height as usize)?;
            let mut buf = vec![0u8; len];
            converter
                .CopyPixels(std::ptr::null(), stride, &mut buf)
                .ok()?;
            // WICはBGR順。imageクレートはRGB順なので入れ替える
            for px in buf.chunks_exact_mut(3) {
                px.swap(0, 2);
            }
            image::RgbImage::from_raw(width, height, buf).map(image::DynamicImage::ImageRgb8)
        }
    }

    /// ファイルを開いて先頭フレームを得る。
    fn frame(path: &Path) -> Option<IWICBitmapFrameDecode> {
        unsafe {
            let factory = factory()?;
            let decoder = factory
                .CreateDecoderFromFilename(
                    &HSTRING::from(path.as_os_str()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .ok()?;
            decoder.GetFrame(0).ok()
        }
    }

    /// 主画像をデコードする。
    pub fn decode(path: &Path) -> Option<image::DynamicImage> {
        let frame = frame(path)?;
        to_image(&frame.cast::<IWICBitmapSource>().ok()?)
    }

    /// メモリ上のバイト列から先頭フレームを得る（[`super::decode_mem`]）。
    ///
    /// `SHCreateMemStream` は**渡したバイト列を複製する**ので、この呼び出しの
    /// あいだだけ生きていればよい。
    pub fn decode_mem(bytes: &[u8]) -> Option<image::DynamicImage> {
        unsafe {
            let factory = factory()?;
            let stream = SHCreateMemStream(Some(bytes))?;
            let decoder = factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
                .ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            to_image(&frame.cast::<IWICBitmapSource>().ok()?)
        }
    }

    /// 縮小デコードの可否をデコーダに聞く（[`super::probe_scaled_decode`]）。
    ///
    /// `GetClosestSize` は**問い合わせるだけ**で、画素は起こさない。
    /// 対応していないデコーダは原寸をそのまま書き戻してくる。
    pub fn probe_scaled_decode(path: &Path, max_edge: u32) -> Option<super::ScaledDecodeProbe> {
        let frame = frame(path)?;
        let (mut fw, mut fh) = (0u32, 0u32);
        unsafe { frame.GetSize(&mut fw, &mut fh).ok()? };
        if fw == 0 || fh == 0 {
            return None;
        }
        let (rw, rh) = crate::resize::fit_within(fw, fh, max_edge.max(1));
        let Ok(transform) = frame.cast::<IWICBitmapSourceTransform>() else {
            return Some(super::ScaledDecodeProbe {
                has_transform: false,
                full: (fw, fh),
                requested: (rw, rh),
                closest: (fw, fh),
                closest_format: "（問い合わせ先が無い）".to_string(),
            });
        };
        let (mut cw, mut ch) = (rw, rh);
        unsafe { transform.GetClosestSize(&mut cw, &mut ch).ok()? };
        let mut format = GUID_WICPixelFormat24bppBGR;
        let format = match unsafe { transform.GetClosestPixelFormat(&mut format) } {
            Ok(()) => pixel_format_name(&format),
            Err(e) => format!("聞けない（{e}）"),
        };
        Some(super::ScaledDecodeProbe {
            has_transform: true,
            full: (fw, fh),
            requested: (rw, rh),
            closest: (cw, ch),
            closest_format: format,
        })
    }

    /// よく出る画素形式に名前を付ける（それ以外は生のGUIDを見せる）。
    fn pixel_format_name(format: &windows::core::GUID) -> String {
        // GUIDの定数はパターンに書けない（束縛と区別が付かず、常に1つ目が当たる）
        if *format == GUID_WICPixelFormat24bppBGR {
            "24bppBGR".to_string()
        } else if *format == GUID_WICPixelFormat32bppBGR {
            "32bppBGR".to_string()
        } else if *format == GUID_WICPixelFormat32bppBGRA {
            "32bppBGRA".to_string()
        } else {
            format!("{format:?}")
        }
    }

    /// 縮小しながらデコードする（[`super::decode_scaled`]）。
    ///
    /// `IWICBitmapSourceTransform::CopyPixels` は寸法を渡せる唯一の入口で、
    /// デコーダが対応していれば展開そのものを小さく済ませられる。
    /// 出せる寸法・画素形式はデコーダが決めるので、両方とも先に聞く。
    pub fn decode_scaled(path: &Path, max_edge: u32) -> Option<image::DynamicImage> {
        let frame = frame(path)?;
        let (mut fw, mut fh) = (0u32, 0u32);
        unsafe { frame.GetSize(&mut fw, &mut fh).ok()? };
        if fw == 0 || fh == 0 {
            return None;
        }
        let transform = frame.cast::<IWICBitmapSourceTransform>().ok()?;

        let (rw, rh) = crate::resize::fit_within(fw, fh, max_edge.max(1));
        let (mut w, mut h) = (rw, rh);
        unsafe { transform.GetClosestSize(&mut w, &mut h).ok()? };
        if w == 0 || h == 0 {
            return None;
        }

        // 希望の形式が通らなければデコーダの都合に合わせる。
        // BGRの3バイトとBGRAの4バイトだけ相手にし、他（10bitなど）は諦めて
        // 呼び出し側を原寸の経路へ戻す
        let mut format = GUID_WICPixelFormat24bppBGR;
        unsafe { transform.GetClosestPixelFormat(&mut format).ok()? };
        let bytes_per_pixel = if format == GUID_WICPixelFormat24bppBGR {
            3usize
        } else if format == GUID_WICPixelFormat32bppBGR || format == GUID_WICPixelFormat32bppBGRA {
            // 4バイト形式は先頭3バイトがBGRで、4本目は未使用（BGR）か
            // アルファ（BGRA）。どちらも捨てて詰め直す——HEIFは規格上
            // 透過を持てる（`auxl` の補助アイテム。AVIF側の `read_alpha` が
            // 読んでいるのがそれ）が、iPhoneのHEICは持たないし、
            // **既存の原寸経路（`to_image`）も24bppBGRへ変換して捨てている**
            4usize
        } else {
            return None;
        };

        let stride = (w as usize).checked_mul(bytes_per_pixel)?;
        let mut buf = vec![0u8; stride.checked_mul(h as usize)?];
        unsafe {
            transform
                .CopyPixels(
                    std::ptr::null(),
                    w,
                    h,
                    &format,
                    WICBitmapTransformRotate0,
                    u32::try_from(stride).ok()?,
                    &mut buf,
                )
                .ok()?;
        }

        // WICはBGR順。imageクレートはRGB順なので入れ替える
        // （4バイト形式は4本目を落として詰め直す）
        if bytes_per_pixel == 3 {
            for px in buf.chunks_exact_mut(3) {
                px.swap(0, 2);
            }
        } else {
            let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
            for px in buf.chunks_exact(4) {
                rgb.extend_from_slice(&[px[2], px[1], px[0]]);
            }
            buf = rgb;
        }
        image::RgbImage::from_raw(w, h, buf).map(image::DynamicImage::ImageRgb8)
    }

    /// 埋め込みサムネイルをデコードする（無ければ None）。
    pub fn decode_thumbnail(path: &Path) -> Option<image::DynamicImage> {
        let frame = frame(path)?;
        let thumb = unsafe { frame.GetThumbnail().ok()? };
        to_image(&thumb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 縮小デコードの可否は三値。**「縮小を頼んでいない」を「できない」と
    /// 読まない**ことが要点（素材がたまたま小さいだけで、WICが縮小に
    /// 対応していないと断じてしまう）。WICの要らない純粋な分岐なので
    /// この環境でも回せる
    #[test]
    fn scaled_decode_probe_is_three_valued() {
        let probe = |has_transform, full, requested, closest| ScaledDecodeProbe {
            has_transform,
            full,
            requested,
            closest,
            closest_format: String::new(),
        };
        // 頼んで、縮んで返ってきた
        assert_eq!(
            probe(true, (4000, 3000), (2048, 1536), (2048, 1536)).scales(),
            Some(true)
        );
        // 頼んだのに原寸が返ってきた
        assert_eq!(
            probe(true, (4000, 3000), (2048, 1536), (4000, 3000)).scales(),
            Some(false)
        );
        // そもそも頼んでいない（元が既に小さい）＝何も言えない
        assert_eq!(
            probe(true, (1600, 1200), (1600, 1200), (1600, 1200)).scales(),
            None
        );
        // 問い合わせ先が無いなら、頼めるかによらず「できない」
        assert_eq!(
            probe(false, (4000, 3000), (2048, 1536), (4000, 3000)).scales(),
            Some(false)
        );
    }

    /// テスト用に最小限のHEIFコンテナを組み立てる。
    ///
    /// 実物のHEICは数MBあってリポジトリに置けないので、
    /// **パーサが読む箱だけ**を持つ合成ファイルをその場で作る。
    fn synth_heif(rotation: Option<u8>, mirror: Option<u8>, width: u32, height: u32) -> Vec<u8> {
        synth_heif_opts(rotation, mirror, width, height, false)
    }

    /// `decoy_ipma` を立てると、**目的のアイテムを含まない `ipma` を先に置く**。
    /// 規格は `ipma` を複数置くことを許すので、1つ目で打ち切ってはいけない。
    fn synth_heif_opts(
        rotation: Option<u8>,
        mirror: Option<u8>,
        width: u32,
        height: u32,
        decoy_ipma: bool,
    ) -> Vec<u8> {
        fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }

        // ipco: プロパティを並べる。番号は1から数える
        let mut ipco = Vec::new();
        let mut ispe_body = vec![0u8; 4]; // version + flags
        ispe_body.extend_from_slice(&width.to_be_bytes());
        ispe_body.extend_from_slice(&height.to_be_bytes());
        ipco.extend_from_slice(&boxed(b"ispe", &ispe_body));
        let mut indices: Vec<u8> = vec![1];
        let mut next = 2u8;
        if let Some(axis) = mirror {
            ipco.extend_from_slice(&boxed(b"imir", &[axis]));
            indices.push(next);
            next += 1;
        }
        if let Some(angle) = rotation {
            ipco.extend_from_slice(&boxed(b"irot", &[angle]));
            indices.push(next);
        }

        // ipma: アイテム1 に上のプロパティを紐づける
        let mut ipma_body = vec![0u8; 4]; // version=0, flags=0（番号は1バイト）
        ipma_body.extend_from_slice(&1u32.to_be_bytes()); // エントリ数
        ipma_body.extend_from_slice(&1u16.to_be_bytes()); // item_id = 1
        ipma_body.push(indices.len() as u8);
        ipma_body.extend_from_slice(&indices);

        let mut iprp = boxed(b"ipco", &ipco);
        if decoy_ipma {
            // 別のアイテム（id=9）だけを載せた ipma を先に置く
            let mut decoy = vec![0u8; 4];
            decoy.extend_from_slice(&1u32.to_be_bytes());
            decoy.extend_from_slice(&9u16.to_be_bytes());
            decoy.push(1);
            decoy.push(1);
            iprp.extend_from_slice(&boxed(b"ipma", &decoy));
        }
        iprp.extend_from_slice(&boxed(b"ipma", &ipma_body));

        let mut pitm_body = vec![0u8; 4]; // version=0
        pitm_body.extend_from_slice(&1u16.to_be_bytes());

        let mut meta_body = vec![0u8; 4]; // meta はフルボックス
        meta_body.extend_from_slice(&boxed(b"pitm", &pitm_body));
        meta_body.extend_from_slice(&boxed(b"iprp", &iprp));

        let mut file = boxed(b"ftyp", b"heic\0\0\0\0mif1heic");
        // 画素データを模した箱を meta の前に置く（トップレベルを飛ばせること）
        file.extend_from_slice(&boxed(b"mdat", &vec![0u8; 4096]));
        file.extend_from_slice(&boxed(b"meta", &meta_body));
        file
    }

    /// `ipco` に今いくつ入っているか（プロパティ番号は1から数えるので、
    /// 足した直後に数えればその番号になる）。
    fn ipco_count(ipco: &[u8]) -> u8 {
        let mut n = 0u8;
        let mut p = 0usize;
        while p + 8 <= ipco.len() {
            let size = u32::from_be_bytes([ipco[p], ipco[p + 1], ipco[p + 2], ipco[p + 3]]);
            if size < 8 {
                break;
            }
            p += size as usize;
            n += 1;
        }
        n
    }

    /// `hvcC` を持つHEIFの組み立て方。
    ///
    /// 実物のHEICは、主画像が `grid`（タイルの寄せ集め）で、**HDRゲインマップや
    /// 深度が別アイテムとして同じファイルに入る**。この形を作れないと、
    /// 「`ipco` の `hvcC` を全部見る」実装の穴（PR #23 のゲート2 P2）が再現できない
    struct HvccSpec {
        /// 主アイテム自身が持つ `hvcC`。`grid` のときは持たない
        primary: Option<u8>,
        /// 主アイテムが `dimg` で参照するタイル
        tiles: Vec<u8>,
        /// 主アイテムから参照されない別アイテム（ゲインマップ・深度）
        aux: Vec<u8>,
        /// `hvcC` の版。1以外は読まない
        version: u8,
        /// `chroma_format_idc` の手前で切り詰める
        truncate: bool,
    }

    impl Default for HvccSpec {
        fn default() -> Self {
            Self {
                primary: None,
                tiles: Vec::new(),
                aux: Vec::new(),
                version: 1,
                truncate: false,
            }
        }
    }

    fn synth_heif_hvcc(spec: HvccSpec) -> Vec<u8> {
        fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }
        fn add_hvcc(ipco: &mut Vec<u8>, prop: &mut u8, spec: &HvccSpec, idc: u8) -> u8 {
            let mut body = vec![0u8; HVCC_CHROMA_OFFSET + 1];
            body[0] = spec.version;
            // 上位6ビットは予約（全部1）
            body[HVCC_CHROMA_OFFSET] = 0b1111_1100 | (idc & 0b11);
            if spec.truncate {
                body.truncate(HVCC_CHROMA_OFFSET);
            }
            ipco.extend_from_slice(&boxed(b"hvcC", &body));
            *prop += 1;
            *prop
        }

        let mut ipco = Vec::new();
        let mut prop = 0u8;
        let mut assoc: Vec<(u16, u8)> = Vec::new(); // (アイテム番号, プロパティ番号)
        if let Some(idc) = spec.primary {
            let p = add_hvcc(&mut ipco, &mut prop, &spec, idc);
            assoc.push((1, p));
        }
        let mut item = 1u16;
        let mut tiles = Vec::new();
        for &idc in &spec.tiles {
            item += 1;
            let p = add_hvcc(&mut ipco, &mut prop, &spec, idc);
            assoc.push((item, p));
            tiles.push(item);
        }
        for &idc in &spec.aux {
            item += 1;
            let p = add_hvcc(&mut ipco, &mut prop, &spec, idc);
            assoc.push((item, p));
        }

        let mut ipma_body = vec![0u8; 4]; // version=0, flags=0（番号は1バイト）
        ipma_body.extend_from_slice(&(assoc.len() as u32).to_be_bytes());
        for (id, index) in &assoc {
            ipma_body.extend_from_slice(&id.to_be_bytes());
            ipma_body.push(1); // このアイテムに紐づけるプロパティは1つ
            ipma_body.push(*index);
        }

        let mut iprp = boxed(b"ipco", &ipco);
        iprp.extend_from_slice(&boxed(b"ipma", &ipma_body));

        let mut pitm_body = vec![0u8; 4]; // version=0
        pitm_body.extend_from_slice(&1u16.to_be_bytes());

        let mut meta_body = vec![0u8; 4]; // meta はフルボックス
        meta_body.extend_from_slice(&boxed(b"pitm", &pitm_body));
        meta_body.extend_from_slice(&boxed(b"iprp", &iprp));
        if !tiles.is_empty() {
            let mut dimg = 1u16.to_be_bytes().to_vec(); // 参照元＝主アイテム
            dimg.extend_from_slice(&(tiles.len() as u16).to_be_bytes());
            for t in &tiles {
                dimg.extend_from_slice(&t.to_be_bytes());
            }
            let mut iref_body = vec![0u8; 4]; // version=0（番号は2バイト）
            iref_body.extend_from_slice(&boxed(b"dimg", &dimg));
            meta_body.extend_from_slice(&boxed(b"iref", &iref_body));
        }

        let mut file = boxed(b"ftyp", b"heic    mif1heic");
        file.extend_from_slice(&boxed(b"meta", &meta_body));
        file
    }

    /// 元の色差の間引きをコンテナから読む。
    ///
    /// **形式で決め打ちしてはいけない**——HEIFは4:2:0とは限らず、Canonの `.HIF`
    /// は4:2:2で書かれる。4:2:0だと思い込んで詰め直すと、色の境目が目に見えて
    /// 崩れる（PR #23 のゲート1 P2）
    #[test]
    fn 元の色差の間引きを読む() {
        use crate::jpeg::ChromaSampling;
        let cases = [
            (0u8, ChromaSampling::Full), // モノクロ
            (1, ChromaSampling::Half),   // 4:2:0（iPhone）
            (2, ChromaSampling::Full),   // 4:2:2（Canonの.HIF）
            (3, ChromaSampling::Full),   // 4:4:4
        ];
        for (idc, want) in cases {
            let file = synth_heif_hvcc(HvccSpec {
                primary: Some(idc),
                ..HvccSpec::default()
            });
            let (_dir, path) = write_temp(&file, "a.heic");
            assert_eq!(stored_chroma(&path), Some(want), "chroma_format_idc={idc}");
        }
    }

    /// **HDRゲインマップに引きずられない**。
    ///
    /// iPhoneのHEICは主画像が `grid` で、ゲインマップと深度が別アイテムとして
    /// 同じファイルに入る。**それらはモノクロHEVC（idc=0）**なので、`ipco` の
    /// `hvcC` を全部見て「揃っていること」を求めると、**HDR写真では常に
    /// 分からない**に落ちる。実ファイルは `hvcC` 6個・値 `1,0,1,0,1,1` だった。
    /// 主アイテムと、それが `dimg` で参照するタイルだけを見ること
    /// （PR #23 のゲート2 P2）
    #[test]
    fn ゲインマップに引きずられない() {
        let file = synth_heif_hvcc(HvccSpec {
            primary: None, // grid は自分では hvcC を持たない
            tiles: vec![1, 1, 1, 1],
            aux: vec![0, 0], // ゲインマップと深度
            ..HvccSpec::default()
        });
        let (_dir, path) = write_temp(&file, "hdr.heic");
        assert_eq!(
            stored_chroma(&path),
            Some(crate::jpeg::ChromaSampling::Half),
            "ゲインマップのモノクロに引きずられている"
        );
    }

    /// 答えられないときは黙る（呼び出し側が間引かない側へ倒せるように）。
    ///
    /// 「たぶん4:2:0だろう」で埋めると、**間引かない形式を静かに間引く**
    #[test]
    fn 色差が分からないときは黙る() {
        // hvcC が1つも無い（壊れたコンテナ）
        let (_d1, p1) = write_temp(&synth_heif_hvcc(HvccSpec::default()), "none.heic");
        assert_eq!(stored_chroma(&p1), None);
        // タイルごとに違う値が書いてある
        let file = synth_heif_hvcc(HvccSpec {
            tiles: vec![1, 3],
            ..HvccSpec::default()
        });
        let (_d2, p2) = write_temp(&file, "mixed.heic");
        assert_eq!(stored_chroma(&p2), None);
        // 知らない版＝並びが違うかもしれないので読まない
        let file = synth_heif_hvcc(HvccSpec {
            primary: Some(1),
            version: 2,
            ..HvccSpec::default()
        });
        let (_d3, p3) = write_temp(&file, "v2.heic");
        assert_eq!(stored_chroma(&p3), None);
        // 切り詰められていて、読みたいバイトが入っていない
        let file = synth_heif_hvcc(HvccSpec {
            primary: Some(1),
            truncate: true,
            ..HvccSpec::default()
        });
        let (_d4, p4) = write_temp(&file, "short.heic");
        assert_eq!(stored_chroma(&p4), None);
    }

    fn write_temp(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    /// テスト用に最小限のAVIFコンテナを組み立てる。
    ///
    /// 実物のAVIFは実体（OBU）が数百KBあってリポジトリに置けないので、
    /// **パーサが読む箱だけ**を持つ合成ファイルをその場で作る。
    /// 中身のOBUは目印のバイト列で足りる（ここでは展開まではしない）。
    ///
    /// `grid` を渡すと、主アイテムを `grid` にしてタイルを `dimg` で参照する形にする
    /// （grid本体は `idat` の中に置く＝construction_method=1 の経路も通る）。
    fn synth_avif(
        width: u32,
        height: u32,
        config: &[u8],
        tiles: &[&[u8]],
        grid: Option<(u8, u8, u16, u16)>,
        clap: Option<[u32; 8]>,
    ) -> Vec<u8> {
        synth_avif_full(AvifSpec {
            width,
            height,
            config,
            tiles,
            grid,
            clap,
            ..AvifSpec::default()
        })
    }

    /// 透過（`auxl` の別アイテム）と `colr` を持つ1枚。
    fn synth_avif_alpha(alpha: &[u8], colr: Option<[u16; 3]>) -> Vec<u8> {
        synth_avif_alpha_opts(alpha, colr, false)
    }

    fn synth_avif_alpha_opts(alpha: &[u8], colr: Option<[u16; 3]>, premultiplied: bool) -> Vec<u8> {
        synth_avif_full(AvifSpec {
            width: 64,
            height: 48,
            config: b"C",
            tiles: &[b"T"],
            alpha: Some(alpha),
            premultiplied,
            colr,
            ..AvifSpec::default()
        })
    }

    /// 切り出しと回転が両方付いた1枚（寸法の答え方が一番ややこしい組み合わせ）。
    fn synth_avif_rotated(width: u32, height: u32, clap: [u32; 8], rotation: u8) -> Vec<u8> {
        synth_avif_full(AvifSpec {
            width,
            height,
            config: b"C",
            tiles: &[b"T"],
            clap: Some(clap),
            rotation: Some(rotation),
            ..AvifSpec::default()
        })
    }

    /// 合成AVIFに入れるものの指定。組み合わせが多いので個別の引数にしない。
    #[derive(Default)]
    struct AvifSpec<'a> {
        width: u32,
        height: u32,
        config: &'a [u8],
        tiles: &'a [&'a [u8]],
        /// (行, 列, 出力幅, 出力高さ)
        grid: Option<(u8, u8, u16, u16)>,
        clap: Option<[u32; 8]>,
        rotation: Option<u8>,
        alpha: Option<&'a [u8]>,
        /// 色に既にalphaが掛けてある（`prem`）
        premultiplied: bool,
        /// (原色, 伝え方, 係数)
        colr: Option<[u16; 3]>,
        /// nclx の手前にICCプロファイルの `colr` を置く（libavifの並びを再現する）
        icc_first: bool,
    }

    fn synth_avif_full(spec: AvifSpec<'_>) -> Vec<u8> {
        let AvifSpec {
            width,
            height,
            config,
            tiles,
            grid,
            clap,
            rotation,
            alpha,
            premultiplied,
            colr,
            icc_first,
        } = spec;
        fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out
        }

        // アイテム番号: 主画像=1、gridならタイルは2以降
        let tile_ids: Vec<u16> = if grid.is_some() {
            (0..tiles.len() as u16).map(|i| i + 2).collect()
        } else {
            vec![1]
        };

        // ---- mdat（タイルの実体）。ファイル先頭からの絶対位置で iloc に書く
        let ftyp = boxed(b"ftyp", b"avif\0\0\0\0avifmif1miaf");
        let mdat_data_at = ftyp.len() + 8;
        let mut mdat_body = Vec::new();
        let mut extents: Vec<(u32, u32)> = Vec::new();
        for tile in tiles {
            extents.push(((mdat_data_at + mdat_body.len()) as u32, tile.len() as u32));
            mdat_body.extend_from_slice(tile);
        }
        // 透過は主画像とは別のアイテム。番号は最後に取る
        let alpha_id = alpha.map(|a| {
            let id = tile_ids.iter().copied().max().unwrap_or(1) + 1;
            extents.push(((mdat_data_at + mdat_body.len()) as u32, a.len() as u32));
            mdat_body.extend_from_slice(a);
            id
        });

        // ---- idat（grid本体をここへ置く）
        let mut idat_body = Vec::new();
        if let Some((rows, cols, w, h)) = grid {
            idat_body.push(0); // version
            idat_body.push(0); // flags（出力寸法は16ビット）
            idat_body.push(rows - 1);
            idat_body.push(cols - 1);
            idat_body.extend_from_slice(&w.to_be_bytes());
            idat_body.extend_from_slice(&h.to_be_bytes());
        }

        // ---- ipco: 1=ispe, 2=av1C, 3=clap
        let mut ipco = Vec::new();
        let mut ispe_body = vec![0u8; 4];
        ispe_body.extend_from_slice(&width.to_be_bytes());
        ispe_body.extend_from_slice(&height.to_be_bytes());
        ipco.extend_from_slice(&boxed(b"ispe", &ispe_body));
        let mut av1c_body = vec![0x81, 0x00, 0x00, 0x00]; // marker/version と素性の4バイト
        av1c_body.extend_from_slice(config);
        ipco.extend_from_slice(&boxed(b"av1C", &av1c_body));
        if let Some(values) = clap {
            let mut body = Vec::new();
            for v in values {
                body.extend_from_slice(&v.to_be_bytes());
            }
            ipco.extend_from_slice(&boxed(b"clap", &body));
        }
        if let Some(angle) = rotation {
            ipco.extend_from_slice(&boxed(b"irot", &[angle]));
        }
        let mut icc_index = 0u8;
        if icc_first {
            // 中身は問わない（こちらは読まない側の箱）
            ipco.extend_from_slice(&boxed(b"colr", b"prof    "));
            icc_index = ipco_count(&ipco);
        }
        let mut colr_index = 0u8;
        if let Some([primaries, transfer, matrix]) = colr {
            let mut body = b"nclx".to_vec();
            body.extend_from_slice(&primaries.to_be_bytes());
            body.extend_from_slice(&transfer.to_be_bytes());
            body.extend_from_slice(&matrix.to_be_bytes());
            body.push(0x80); // full_range を立てる
            ipco.extend_from_slice(&boxed(b"colr", &body));
            colr_index = ipco_count(&ipco);
        }
        let mut auxc_index = 0u8;
        if alpha.is_some() {
            let mut body = vec![0u8; 4];
            body.extend_from_slice(b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha ");
            ipco.extend_from_slice(&boxed(b"auxC", &body));
            auxc_index = ipco_count(&ipco);
        }

        // ---- ipma: 主画像は ispe（+clap）、実体を持つアイテムに av1C
        let mut entries: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut primary_props = vec![1u8];
        if grid.is_none() {
            primary_props.push(2);
        }
        if clap.is_some() {
            primary_props.push(3);
        }
        if rotation.is_some() {
            primary_props.push(if clap.is_some() { 4 } else { 3 });
        }
        if icc_index > 0 {
            primary_props.push(icc_index);
        }
        if colr_index > 0 {
            primary_props.push(colr_index);
        }
        entries.push((1, primary_props));
        if grid.is_some() {
            for &id in &tile_ids {
                entries.push((id, vec![1, 2]));
            }
        }
        if let Some(id) = alpha_id {
            // alpha も寸法と av1C を持ち、加えて「これは透過だ」という印が付く
            entries.push((id, vec![1, 2, auxc_index]));
        }
        let mut ipma_body = vec![0u8; 4];
        ipma_body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (id, props) in &entries {
            ipma_body.extend_from_slice(&id.to_be_bytes());
            ipma_body.push(props.len() as u8);
            ipma_body.extend_from_slice(props);
        }
        let mut iprp = boxed(b"ipco", &ipco);
        iprp.extend_from_slice(&boxed(b"ipma", &ipma_body));

        // ---- iinf: アイテムの種別（grid か av01 か）
        let mut infes = Vec::new();
        let infe = |id: u16, kind: &[u8; 4]| {
            let mut body = vec![2u8, 0, 0, 0]; // version=2
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&0u16.to_be_bytes()); // protection_index
            body.extend_from_slice(kind);
            body.push(0); // 名前（空）
            boxed(b"infe", &body)
        };
        infes.extend_from_slice(&infe(1, if grid.is_some() { b"grid" } else { b"av01" }));
        if grid.is_some() {
            for &id in &tile_ids {
                infes.extend_from_slice(&infe(id, b"av01"));
            }
        }
        if let Some(id) = alpha_id {
            infes.extend_from_slice(&infe(id, b"av01"));
        }
        let mut iinf_body = vec![0u8; 4];
        iinf_body.extend_from_slice(
            &((1 + tile_ids.len() * usize::from(grid.is_some()) + usize::from(alpha.is_some()))
                as u16)
                .to_be_bytes(),
        );
        iinf_body.extend_from_slice(&infes);

        // ---- iloc: version=1（construction_method を持つ）、4バイトのoffset/length
        let mut iloc_body = vec![1u8, 0, 0, 0];
        iloc_body.push(0x44); // offset_size=4, length_size=4
        iloc_body.push(0x00); // base_offset_size=0, index_size=0
        let item_count =
            1 + tile_ids.len() * usize::from(grid.is_some()) + usize::from(alpha.is_some());
        iloc_body.extend_from_slice(&(item_count as u16).to_be_bytes());
        let put = |body: &mut Vec<u8>, id: u16, construction: u16, off: u32, len: u32| {
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&construction.to_be_bytes());
            body.extend_from_slice(&0u16.to_be_bytes()); // data_reference_index
            body.extend_from_slice(&1u16.to_be_bytes()); // extent_count
            body.extend_from_slice(&off.to_be_bytes());
            body.extend_from_slice(&len.to_be_bytes());
        };
        if grid.is_some() {
            // 主アイテム（grid本体）は idat の中
            put(&mut iloc_body, 1, 1, 0, idat_body.len() as u32);
            for (i, &id) in tile_ids.iter().enumerate() {
                let (off, len) = extents[i];
                put(&mut iloc_body, id, 0, off, len);
            }
        } else {
            let (off, len) = extents[0];
            put(&mut iloc_body, 1, 0, off, len);
        }
        if let Some(id) = alpha_id {
            let (off, len) = extents[extents.len() - 1];
            put(&mut iloc_body, id, 0, off, len);
        }

        // ---- iref: grid が参照するタイルの並び
        let mut iref_body = vec![0u8; 4];
        if grid.is_some() {
            let mut dimg = Vec::new();
            dimg.extend_from_slice(&1u16.to_be_bytes()); // from_item
            dimg.extend_from_slice(&(tile_ids.len() as u16).to_be_bytes());
            for &id in &tile_ids {
                dimg.extend_from_slice(&id.to_be_bytes());
            }
            iref_body.extend_from_slice(&boxed(b"dimg", &dimg));
        }
        if let Some(id) = alpha_id {
            // auxl は「alpha側が主画像を指す」向き（dimg と逆）
            let mut auxl = id.to_be_bytes().to_vec();
            auxl.extend_from_slice(&1u16.to_be_bytes()); // 参照は1件
            auxl.extend_from_slice(&1u16.to_be_bytes()); // 主画像
            iref_body.extend_from_slice(&boxed(b"auxl", &auxl));
            if premultiplied {
                // prem は逆に「主画像がalphaを指す」向き
                let mut prem = 1u16.to_be_bytes().to_vec();
                prem.extend_from_slice(&1u16.to_be_bytes());
                prem.extend_from_slice(&id.to_be_bytes());
                iref_body.extend_from_slice(&boxed(b"prem", &prem));
            }
        }

        let mut pitm_body = vec![0u8; 4];
        pitm_body.extend_from_slice(&1u16.to_be_bytes());

        let mut meta_body = vec![0u8; 4];
        meta_body.extend_from_slice(&boxed(b"pitm", &pitm_body));
        meta_body.extend_from_slice(&boxed(b"iinf", &iinf_body));
        meta_body.extend_from_slice(&boxed(b"iloc", &iloc_body));
        if grid.is_some() || alpha.is_some() {
            meta_body.extend_from_slice(&boxed(b"iref", &iref_body));
        }
        if grid.is_some() {
            meta_body.extend_from_slice(&boxed(b"idat", &idat_body));
        }
        meta_body.extend_from_slice(&boxed(b"iprp", &iprp));

        let mut file = ftyp;
        file.extend_from_slice(&boxed(b"mdat", &mdat_body));
        file.extend_from_slice(&boxed(b"meta", &meta_body));
        file
    }

    #[test]
    fn avifの主画像の実体と設定を取り出す() {
        let bytes = synth_avif(4032, 3024, b"SEQHDR", &[b"TILE-A"], None, None);
        let (_dir, path) = write_temp(&bytes, "a.avif");
        let source = read_avif_source(&path).expect("読めるはず");
        assert_eq!(
            (source.info.stored_width, source.info.stored_height),
            (4032, 3024)
        );
        assert_eq!(source.config_obus, b"SEQHDR");
        assert_eq!(source.tiles, vec![b"TILE-A".to_vec()]);
        assert!(source.grid.is_none());
        assert!(source.info.crop.is_none());
    }

    #[test]
    fn gridのタイルを並び順のまま集める() {
        let tiles: Vec<&[u8]> = vec![b"T0", b"T1", b"T2", b"T3", b"T4", b"T5"];
        let bytes = synth_avif(4000, 3000, b"CFG", &tiles, Some((2, 3, 4000, 3000)), None);
        let (_dir, path) = write_temp(&bytes, "g.avif");
        let source = read_avif_source(&path).expect("読めるはず");
        assert_eq!(
            source.grid,
            Some(Grid {
                rows: 2,
                cols: 3,
                width: 4000,
                height: 3000
            })
        );
        // 並びが崩れると絵がバラバラに貼られるので、順序まで確かめる
        assert_eq!(
            source.tiles,
            tiles.iter().map(|t| t.to_vec()).collect::<Vec<_>>()
        );
        // av1C はタイル側にしか無い（gridアイテムには付かない）
        assert_eq!(source.config_obus, b"CFG");
    }

    #[test]
    fn タイルが足りないgridは諦める() {
        // 2x3 = 6枚要るのに4枚しか無い
        let tiles: Vec<&[u8]> = vec![b"T0", b"T1", b"T2", b"T3"];
        let bytes = synth_avif(4000, 3000, b"CFG", &tiles, Some((2, 3, 4000, 3000)), None);
        let (_dir, path) = write_temp(&bytes, "short.avif");
        assert!(read_avif_source(&path).is_none());
    }

    #[test]
    fn clapを画素の矩形に直す() {
        // 幅1000/高さ800を、中心をずらさずに 800x600 で切り出す
        let clap = [800, 1, 600, 1, 0, 1, 0, 1];
        let bytes = synth_avif(1000, 800, b"C", &[b"T"], None, Some(clap));
        let (_dir, path) = write_temp(&bytes, "clap.avif");
        let source = read_avif_source(&path).expect("読めるはず");
        assert_eq!(
            source.info.crop,
            Some(Clap {
                x: 100,
                y: 100,
                width: 800,
                height: 600
            })
        );
    }

    #[test]
    fn clapは表示寸法にも効く() {
        // 1000x800 を 800x600 に切り出す指定。DBへ書く寸法は切り出し後でないと、
        // 実際に作るサムネイル（切り出し済み）と縦横比が食い違ってタイルが歪む
        let clap = [800, 1, 600, 1, 0, 1, 0, 1];
        let bytes = synth_avif(1000, 800, b"C", &[b"T"], None, Some(clap));
        let (_dir, path) = write_temp(&bytes, "clapdim.avif");
        let info = read_info(&path).expect("読めるはず");
        assert_eq!((info.stored_width, info.stored_height), (1000, 800));
        assert_eq!(info.display_size(), (800, 600));
        assert_eq!(display_dimensions(&path), Some((800, 600)));
    }

    #[test]
    fn 切り出しと九十度回転が両方効く() {
        // 回転が入ると、切り出し後の幅と高さが入れ替わる
        let clap = [800, 1, 600, 1, 0, 1, 0, 1];
        let bytes = synth_avif_rotated(1000, 800, clap, 1);
        let (_dir, path) = write_temp(&bytes, "claprot.avif");
        let info = read_info(&path).expect("読めるはず");
        assert_eq!(info.display_size(), (600, 800));
    }

    #[test]
    fn 透過のアイテムを見つける() {
        let bytes = synth_avif_alpha(b"ALPHA-STREAM", None);
        let (_dir, path) = write_temp(&bytes, "alpha.avif");
        let source = read_avif_source(&path).expect("読めるはず");
        let alpha = source.alpha.expect("透過が見つかるはず");
        assert_eq!(alpha.tiles, vec![b"ALPHA-STREAM".to_vec()]);
        assert!(!alpha.premultiplied);
        // 主画像の実体を透過と取り違えていないこと
        assert_eq!(source.tiles, vec![b"T".to_vec()]);
    }

    #[test]
    fn 掛け済みの透過を見分ける() {
        // `prem` は**主画像からalphaへ**張られる。逆に引くと常に
        // 「掛かっていない」と判定して、半透明の部分だけ二重に薄くなる
        let bytes = synth_avif_alpha_opts(b"A", None, true);
        let (_dir, path) = write_temp(&bytes, "prem.avif");
        let alpha = read_avif_source(&path)
            .unwrap()
            .alpha
            .expect("透過があるはず");
        assert!(alpha.premultiplied);

        // 張られていなければ偽のまま
        let plain = synth_avif_alpha(b"A", None);
        let (_dir2, path2) = write_temp(&plain, "noprem.avif");
        assert!(
            !read_avif_source(&path2)
                .unwrap()
                .alpha
                .unwrap()
                .premultiplied
        );
    }

    #[test]
    fn 透過が無ければnone() {
        let bytes = synth_avif(64, 48, b"C", &[b"T"], None, None);
        let (_dir, path) = write_temp(&bytes, "noalpha.avif");
        assert!(read_avif_source(&path).unwrap().alpha.is_none());
    }

    #[test]
    fn colrはシーケンスヘッダより優先される素性として読める() {
        // BT.2020 / PQ / BT.2020非定輝度
        let bytes = synth_avif_alpha(b"A", Some([9, 16, 9]));
        let (_dir, path) = write_temp(&bytes, "colr.avif");
        let color = read_avif_source(&path)
            .unwrap()
            .color
            .expect("colrが読めるはず");
        assert_eq!(color.primaries, 9);
        assert_eq!(color.transfer, 16);
        assert_eq!(color.matrix, 9);
        assert!(color.full_range);
    }

    #[test]
    fn iccのcolrが先にあってもnclxを取り落とさない() {
        // 規格は色の種類ごとに `colr` を1つずつ許し、**libavifはICCを先に書く**。
        // 名前で1つ目を取ると、ICCを持つファイルだけ色の指定が丸ごと消える
        let bytes = synth_avif_full(AvifSpec {
            width: 64,
            height: 48,
            config: b"C",
            tiles: &[b"T"],
            colr: Some([9, 16, 9]),
            icc_first: true,
            ..AvifSpec::default()
        });
        let (_dir, path) = write_temp(&bytes, "icc.avif");
        let color = read_avif_source(&path)
            .unwrap()
            .color
            .expect("ICCの後ろのnclxを見つけること");
        assert_eq!(color.transfer, 16);
    }

    #[test]
    fn nclx以外のcolrは読まない() {
        // ICCプロファイル（rICC）はカラーマネジメントが要るので手を出さない
        assert!(parse_colr(b"rICC       ").is_none());
        assert!(parse_colr(b"nclx").is_none(), "短すぎる指定は読まない");
    }

    #[test]
    fn 枠からはみ出すclapは無視する() {
        // 中心を大きくずらして枠外へ出す指定。切り出さない方がまだ絵になる
        let clap = [800, 1, 600, 1, 400, 1, 0, 1];
        let bytes = synth_avif(1000, 800, b"C", &[b"T"], None, Some(clap));
        let (_dir, path) = write_temp(&bytes, "bad_clap.avif");
        let source = read_avif_source(&path).expect("読めるはず");
        assert!(source.info.crop.is_none());
    }

    #[test]
    fn 実体が無いアイテムは読めないと返す() {
        // iloc が指す先がファイルの外
        let mut bytes = synth_avif(100, 100, b"C", &[b"TILE"], None, None);
        bytes.truncate(bytes.len() - 1);
        let (_dir, path) = write_temp(&bytes, "broken.avif");
        // 箱が切れているので、寸法すら読めないか、実体が取れないかのどちらか
        assert!(read_avif_source(&path).is_none());
    }

    #[test]
    fn 拡張子を見分ける() {
        assert!(is_heif_extension("heic"));
        assert!(is_heif_extension("HEIC"));
        assert!(is_heif_extension("hif"));
        assert!(!is_heif_extension("jpg"));
        assert!(!is_heif_extension("heicx"));
    }

    #[test]
    fn 回転なしの寸法をそのまま読む() {
        let (_dir, path) = write_temp(&synth_heif(None, None, 4032, 3024), "a.heic");
        let info = read_info(&path).expect("読めるはず");
        assert_eq!((info.stored_width, info.stored_height), (4032, 3024));
        assert_eq!(info.rotation, 0);
        assert_eq!(info.display_size(), (4032, 3024));
    }

    #[test]
    fn 九十度系の回転で縦横が入れ替わる() {
        for angle in [1u8, 3] {
            let (_dir, path) = write_temp(&synth_heif(Some(angle), None, 5712, 4284), "b.heic");
            let info = read_info(&path).expect("読めるはず");
            assert_eq!(info.rotation, angle);
            assert_eq!(info.display_size(), (4284, 5712), "angle={angle}");
        }
    }

    #[test]
    fn 百八十度の回転では寸法が変わらない() {
        let (_dir, path) = write_temp(&synth_heif(Some(2), None, 4032, 3024), "c.heic");
        let info = read_info(&path).expect("読めるはず");
        assert_eq!(info.display_size(), (4032, 3024));
    }

    #[test]
    fn 鏡映も読む() {
        let (_dir, path) = write_temp(&synth_heif(Some(1), Some(0), 100, 50), "d.heic");
        let info = read_info(&path).expect("読めるはず");
        assert_eq!(info.mirror, Some(0));
        assert_eq!(info.rotation, 1);
        // synth_heif は imir を irot より先に置く
        assert!(info.mirror_first);
    }

    #[test]
    fn 画素データの後ろにmetaがあっても辿れる() {
        // synth_heif は mdat を meta の前に置いている。
        // トップレベルの箱をヘッダだけで飛ばせていれば読める
        let (_dir, path) = write_temp(&synth_heif(None, None, 8, 8), "e.heic");
        assert!(read_info(&path).is_some());
    }

    /// 回転判定は倍率に依存してはいけない。
    ///
    /// 一度「原寸の格納寸法と実寸が入れ替わっているか」で判定して、
    /// 原寸では当たるのにサムネイルでは外れ、**サムネイルだけ二重回転**した。
    #[test]
    fn 回転済みかの判定は倍率に依存しない() {
        let info = HeifInfo {
            stored_width: 5712,
            stored_height: 4284,
            rotation: 3,
            mirror: None,
            mirror_first: false,
            crop: None,
        };
        // デコーダが回転を適用済み（縦になっている）→ こちらでは触らない
        assert!(!needs_manual_transform(4284, 5712, &info), "原寸・適用済み");
        assert!(!needs_manual_transform(312, 416, &info), "サムネ・適用済み");
        // 適用されていない（横のまま）→ こちらで回す
        assert!(needs_manual_transform(5712, 4284, &info), "原寸・未適用");
        assert!(needs_manual_transform(416, 312, &info), "サムネ・未適用");
    }

    #[test]
    fn 寸法で見分けられないときはデコーダに任せる() {
        // 180度回転は縦横が変わらないので、適用済みかを寸法から判断できない
        let half = HeifInfo {
            stored_width: 4032,
            stored_height: 3024,
            rotation: 2,
            mirror: None,
            mirror_first: false,
            crop: None,
        };
        assert!(!needs_manual_transform(4032, 3024, &half));

        // 正方形も同様
        let square = HeifInfo {
            stored_width: 1000,
            stored_height: 1000,
            rotation: 1,
            mirror: None,
            mirror_first: false,
            crop: None,
        };
        assert!(!needs_manual_transform(1000, 1000, &square));
    }

    #[test]
    fn 回転を適用すると表示寸法になる() {
        let info = HeifInfo {
            stored_width: 40,
            stored_height: 30,
            rotation: 3,
            mirror: None,
            mirror_first: false,
            crop: None,
        };
        // デコーダが未適用の絵を渡す
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(40, 30));
        let out = apply_container_transform(img, &info);
        assert_eq!((out.width(), out.height()), info.display_size());

        // 適用済みの絵はそのまま
        let done = image::DynamicImage::ImageRgb8(image::RgbImage::new(30, 40));
        let out = apply_container_transform(done, &info);
        assert_eq!((out.width(), out.height()), (30, 40));
    }

    /// 64ビット長に巨大な値が書かれていても桁溢れで落ちない（Codexレビュー指摘）。
    ///
    /// `size=1` は「長さはこの後ろの64ビット」の意味。ここに `u64::MAX` 近い値を
    /// 書かれると、`位置 + 長さ` を素直に計算した時点で溢れる。
    #[test]
    fn 巨大な箱の長さでも桁が溢れない() {
        // ftyp の後ろに「64ビット長 = u64::MAX」の箱を置く
        let mut file = 16u32.to_be_bytes().to_vec();
        file.extend_from_slice(b"ftyp");
        file.extend_from_slice(b"heic    ");
        file.extend_from_slice(&1u32.to_be_bytes());
        file.extend_from_slice(b"meta");
        file.extend_from_slice(&u64::MAX.to_be_bytes());
        file.extend_from_slice(&[0u8; 64]);

        let (_dir, path) = write_temp(&file, "huge.heic");
        assert_eq!(read_info(&path), None);

        // メモリ上のパーサ側も同じ形で落ちない
        assert!(boxes(&file, 0, file.len()).len() <= 1);
    }

    /// `ipma` が複数あっても、目的のアイテムを載せた箱まで探す（レビュー指摘）。
    ///
    /// 1つ目で打ち切ると `ispe` が取れず、そのファイルは寸法が読めない
    /// ＝サムネイル生成が3回失敗して以後ずっと出なくなる。
    #[test]
    fn ipmaが複数あっても目的のアイテムを見つける() {
        let bytes = synth_heif_opts(Some(3), None, 4032, 3024, true);
        let (_dir, path) = write_temp(&bytes, "multi.heic");
        let info = read_info(&path).expect("2つ目のipmaから読めるはず");
        assert_eq!((info.stored_width, info.stored_height), (4032, 3024));
        assert_eq!(info.rotation, 3);
    }

    /// 鏡映と回転は**書かれた順**に適用する（レビュー指摘）。
    ///
    /// 90度系の回転と鏡映は交換できない。順を無視すると180度ずれる。
    #[test]
    fn 鏡映と回転の順序が結果に反映される() {
        // 2x1 の絵。左右で色を変えて、反転したか分かるようにする
        let mut img = image::RgbImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([0, 255, 0]));
        let img = image::DynamicImage::ImageRgb8(img);

        let base = HeifInfo {
            stored_width: 2,
            stored_height: 1,
            rotation: 1,
            mirror: Some(0),
            mirror_first: true,
            crop: None,
        };
        let first = apply_container_transform(img.clone(), &base);
        let second = apply_container_transform(
            img,
            &HeifInfo {
                mirror_first: false,
                ..base
            },
        );
        assert_eq!(first.width(), second.width());
        assert_ne!(
            first.to_rgb8().into_raw(),
            second.to_rgb8().into_raw(),
            "順序を入れ替えたら結果も変わること"
        );
    }

    #[test]
    fn 壊れたファイルでも落ちない() {
        let (_dir, path) = write_temp(b"not a heif file at all", "f.heic");
        assert_eq!(read_info(&path), None);

        // 箱の長さが嘘（実体より大きい）
        let mut broken = 0xFFFF_FFFFu32.to_be_bytes().to_vec();
        broken.extend_from_slice(b"meta");
        let (_dir2, path2) = write_temp(&broken, "g.heic");
        assert_eq!(read_info(&path2), None);
    }
}
