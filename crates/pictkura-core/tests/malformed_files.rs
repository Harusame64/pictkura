//! **壊れたファイルでパニックしない**ことを確かめる（`dev/loadmap.md` 1.3）。
//!
//! 数万枚のライブラリには、途中で切れた転送・規格外の書き込み・別形式に付け替え
//! られた拡張子が**必ず混ざる**。読み取りはどれも `Option` を返す形になっているが、
//! 添字が範囲を外れればその契約より先にプロセスが落ちる——**1枚のせいで
//! 取り込みごと落ちる**のがいちばん困る。
//!
//! ここは「正しく読めること」を確かめるテストではない。**何を食べても
//! 落ちないこと**だけを見る。返り値は `None` でも `Some` でも構わない。
//!
//! テストが落ちたら、それは**本当にパニックした**ということ。

// テストを組み立てる側（一時ファイルの書き出し等）の `unwrap()` は許す。
// **確かめたいのは本体が落ちないこと**で、足場が落ちたらそれは足場の不備
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

/// 種を持つだけの乱数（`rand` を足さないため）。**毎回同じ列**が出る
/// ——落ちたときに同じ入力で追いかけられる。
struct Xorshift(u64);

impl Xorshift {
    fn next_u8(&mut self) -> u8 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 24) as u8
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u8()).collect()
    }
}

/// ISO-BMFF の箱をひとつ組む。
fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// FullBox（版とフラグの4バイトが頭に付く箱）。
fn full(kind: &[u8; 4], version: u8, body: &[u8]) -> Vec<u8> {
    let mut inner = vec![version, 0, 0, 0];
    inner.extend_from_slice(body);
    bx(kind, &inner)
}

/// それらしい HEIC を組む。**中身の整合は問わない**——ここを起点に
/// 1バイトずつ切り詰めて、どこで切れても落ちないことを見る。
fn plausible_heic() -> Vec<u8> {
    let mut ftyp = b"heic".to_vec();
    ftyp.extend_from_slice(&0u32.to_be_bytes());
    ftyp.extend_from_slice(b"heicmif1miaf");

    let hdlr = full(b"hdlr", 0, b"\0\0\0\0pict\0\0\0\0\0\0\0\0\0\0\0\0");
    let pitm = full(b"pitm", 0, &1u16.to_be_bytes());

    // ipco: 寸法・色・切り出し・回転を並べる。**ここを通らないと
    // `parse_colr` / `parse_clap` が一度も動かず、検査が空振りになる**
    let mut ispe_body = 4032u32.to_be_bytes().to_vec();
    ispe_body.extend_from_slice(&3024u32.to_be_bytes());

    // colr（nclx）: BT.2020 / PQ。HDRの経路も通る
    let mut colr_body = b"nclx".to_vec();
    colr_body.extend_from_slice(&9u16.to_be_bytes());
    colr_body.extend_from_slice(&16u16.to_be_bytes());
    colr_body.extend_from_slice(&9u16.to_be_bytes());
    colr_body.push(0x80);

    // clap: 4000x3000 を中心から切り出す（分母つきの有理数8つ）
    let mut clap_body = Vec::new();
    for n in [4000i32, 1, 3000, 1, 0, 1, 0, 1] {
        clap_body.extend_from_slice(&n.to_be_bytes());
    }

    let mut ipco_body = full(b"ispe", 0, &ispe_body);
    ipco_body.extend_from_slice(&bx(b"colr", &colr_body));
    ipco_body.extend_from_slice(&bx(b"clap", &clap_body));
    ipco_body.extend_from_slice(&bx(b"irot", &[1u8]));
    let ipco = bx(b"ipco", &ipco_body);

    // ipma: アイテム1に プロパティ1〜4 を結び付ける
    let mut ipma_body = 1u32.to_be_bytes().to_vec();
    ipma_body.extend_from_slice(&1u16.to_be_bytes());
    ipma_body.push(4);
    for index in 1u8..=4 {
        ipma_body.push(0x80 | index);
    }
    let ipma = full(b"ipma", 0, &ipma_body);
    let mut iprp_body = ipco;
    iprp_body.extend_from_slice(&ipma);
    let iprp = bx(b"iprp", &iprp_body);

    // iloc: アイテム1 が mdat の中の16バイト
    let mut iloc_body = vec![0x44, 0x00];
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&0u16.to_be_bytes());
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&0u32.to_be_bytes());
    iloc_body.extend_from_slice(&16u32.to_be_bytes());
    let iloc = full(b"iloc", 0, &iloc_body);

    let mut infe_body = 1u16.to_be_bytes().to_vec();
    infe_body.extend_from_slice(&0u16.to_be_bytes());
    infe_body.extend_from_slice(b"av01");
    infe_body.push(0);
    let infe = full(b"infe", 2, &infe_body);
    let mut iinf_body = 1u16.to_be_bytes().to_vec();
    iinf_body.extend_from_slice(&infe);
    let iinf = full(b"iinf", 0, &iinf_body);

    let mut meta_body = Vec::new();
    meta_body.extend_from_slice(&hdlr);
    meta_body.extend_from_slice(&pitm);
    meta_body.extend_from_slice(&iinf);
    meta_body.extend_from_slice(&iprp);
    meta_body.extend_from_slice(&iloc);

    let mut out = bx(b"ftyp", &ftyp);
    out.extend_from_slice(&full(b"meta", 0, &meta_body));
    out.extend_from_slice(&bx(b"mdat", &[0x42; 16]));
    out
}

/// タイルを貼り合わせる HEIC（`grid`）。iPhoneの大きな写真がこの形。
///
/// 別の解析器（`parse_grid` と `iref` の追跡）を通すために分けてある。
fn plausible_heic_grid() -> Vec<u8> {
    // タイル1枚ぶんの寸法（プロパティ1）と、貼り合わせ後の寸法（プロパティ2）。
    // iPhoneの実物も、貼り合わせ側に自分の `ispe` を持っている
    let mut tile_ispe = 2016u32.to_be_bytes().to_vec();
    tile_ispe.extend_from_slice(&1512u32.to_be_bytes());
    let mut whole_ispe = 4032u32.to_be_bytes().to_vec();
    whole_ispe.extend_from_slice(&3024u32.to_be_bytes());
    let mut ipco_body = full(b"ispe", 0, &tile_ispe);
    ipco_body.extend_from_slice(&full(b"ispe", 0, &whole_ispe));
    let ipco = bx(b"ipco", &ipco_body);
    // ipma: タイル(1)→プロパティ1、貼り合わせ(2)→プロパティ2
    let mut ipma_body = 2u32.to_be_bytes().to_vec();
    for (item, prop) in [(1u16, 0x81u8), (2u16, 0x82)] {
        ipma_body.extend_from_slice(&item.to_be_bytes());
        ipma_body.push(1);
        ipma_body.push(prop);
    }
    let mut iprp_body = ipco;
    iprp_body.extend_from_slice(&full(b"ipma", 0, &ipma_body));
    let iprp = bx(b"iprp", &iprp_body);

    // アイテム1=タイル（av01）、アイテム2=貼り合わせ（grid）
    let mut iinf_body = 2u16.to_be_bytes().to_vec();
    for (id, kind) in [(1u16, b"av01"), (2u16, b"grid")] {
        let mut infe_body = id.to_be_bytes().to_vec();
        infe_body.extend_from_slice(&0u16.to_be_bytes());
        infe_body.extend_from_slice(kind);
        infe_body.push(0);
        iinf_body.extend_from_slice(&full(b"infe", 2, &infe_body));
    }
    let iinf = full(b"iinf", 0, &iinf_body);

    // iref: grid(2) が タイル(1) から出来ている
    let mut dimg_body = 2u16.to_be_bytes().to_vec();
    dimg_body.extend_from_slice(&1u16.to_be_bytes());
    dimg_body.extend_from_slice(&1u16.to_be_bytes());
    let iref = full(b"iref", 0, &bx(b"dimg", &dimg_body));

    // iloc: 2件とも mdat の中を指す
    let mut iloc_body = vec![0x44, 0x00];
    iloc_body.extend_from_slice(&2u16.to_be_bytes());
    for (id, offset, len) in [(1u16, 8u32, 16u32), (2u16, 0u32, 8u32)] {
        iloc_body.extend_from_slice(&id.to_be_bytes());
        iloc_body.extend_from_slice(&0u16.to_be_bytes());
        iloc_body.extend_from_slice(&1u16.to_be_bytes());
        iloc_body.extend_from_slice(&offset.to_be_bytes());
        iloc_body.extend_from_slice(&len.to_be_bytes());
    }
    let iloc = full(b"iloc", 0, &iloc_body);

    let mut meta_body = full(b"hdlr", 0, b"\0\0\0\0pict\0\0\0\0\0\0\0\0\0\0\0\0");
    meta_body.extend_from_slice(&full(b"pitm", 0, &2u16.to_be_bytes()));
    meta_body.extend_from_slice(&iinf);
    meta_body.extend_from_slice(&iref);
    meta_body.extend_from_slice(&iprp);
    meta_body.extend_from_slice(&iloc);

    // mdat: 先頭8バイトが grid の中身（2x2で 4032x3024）、続きがタイルのつもり
    let mut mdat = vec![0u8, 0, 1, 1];
    mdat.extend_from_slice(&4032u16.to_be_bytes());
    mdat.extend_from_slice(&3024u16.to_be_bytes());
    mdat.extend_from_slice(&[0x42; 16]);

    let mut out = bx(b"ftyp", b"heic\0\0\0\0heicmif1");
    out.extend_from_slice(&full(b"meta", 0, &meta_body));
    out.extend_from_slice(&bx(b"mdat", &mdat));
    out
}

/// それらしい MP4（`moov` は**末尾**に置く。iPhoneの .MOV と同じ形）。
fn plausible_mp4() -> Vec<u8> {
    let mut mvhd_body = Vec::new();
    mvhd_body.extend_from_slice(&3_800_000_000u32.to_be_bytes()); // 作成日時
    mvhd_body.extend_from_slice(&3_800_000_000u32.to_be_bytes());
    mvhd_body.extend_from_slice(&1000u32.to_be_bytes());
    mvhd_body.extend_from_slice(&5000u32.to_be_bytes());
    mvhd_body.extend_from_slice(&[0u8; 80]);

    let mut tkhd_body = Vec::new();
    tkhd_body.extend_from_slice(&[0u8; 20]);
    tkhd_body.extend_from_slice(&[0u8; 52]);
    tkhd_body.extend_from_slice(&(1920u32 << 16).to_be_bytes());
    tkhd_body.extend_from_slice(&(1080u32 << 16).to_be_bytes());

    let trak = bx(b"trak", &full(b"tkhd", 0, &tkhd_body));
    let mut moov_body = full(b"mvhd", 0, &mvhd_body);
    moov_body.extend_from_slice(&trak);

    let mut out = bx(b"ftyp", b"qt  \0\0\0\0qt  ");
    out.extend_from_slice(&bx(b"mdat", &[0x11; 32]));
    out.extend_from_slice(&bx(b"moov", &moov_body));
    out
}

/// EXIF に撮影日時だけを持つ、最小のJPEGらしきもの。
fn plausible_jpeg() -> Vec<u8> {
    let mut tiff: Vec<u8> = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x8769u16.to_le_bytes());
    tiff.extend_from_slice(&4u16.to_le_bytes());
    tiff.extend_from_slice(&1u32.to_le_bytes());
    tiff.extend_from_slice(&26u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&1u16.to_le_bytes());
    tiff.extend_from_slice(&0x9003u16.to_le_bytes());
    tiff.extend_from_slice(&2u16.to_le_bytes());
    tiff.extend_from_slice(&20u32.to_le_bytes());
    tiff.extend_from_slice(&44u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(b"2019:08:11 12:00:00");
    tiff.push(0);

    let mut out: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

/// **長さの欄に嘘が書いてある**箱たち。壊れたファイルが実際に持つ形。
fn hostile_boxes() -> Vec<(&'static str, Vec<u8>)> {
    let mut out = Vec::new();

    // size=0（「ここで終わり」）の meta が先頭に来る
    let mut zero = 0u32.to_be_bytes().to_vec();
    zero.extend_from_slice(b"meta");
    zero.extend_from_slice(&[0u8; 8]);
    out.push(("size=0", zero));

    // size=1（64ビット長）に u64::MAX 近い値
    let mut huge = 1u32.to_be_bytes().to_vec();
    huge.extend_from_slice(b"meta");
    huge.extend_from_slice(&u64::MAX.to_be_bytes());
    huge.extend_from_slice(&[0u8; 16]);
    out.push(("size=1 に u64::MAX", huge));

    // size がヘッダより小さい（8未満）
    let mut tiny = 3u32.to_be_bytes().to_vec();
    tiny.extend_from_slice(b"meta");
    tiny.extend_from_slice(&[0u8; 8]);
    out.push(("size<header", tiny));

    // size が実体より大きい
    let mut over = u32::MAX.to_be_bytes().to_vec();
    over.extend_from_slice(b"meta");
    over.extend_from_slice(&[0u8; 8]);
    out.push(("size=u32::MAX", over));

    // ftyp の後ろに、中身が空の meta
    let mut empty_meta = bx(b"ftyp", b"heic\0\0\0\0heic");
    empty_meta.extend_from_slice(&full(b"meta", 0, b""));
    out.push(("空のmeta", empty_meta));

    // iloc の長さの欄が嘘（オフセットとサイズが巨大）
    let mut iloc_body = vec![0x88, 0x00];
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&0u16.to_be_bytes());
    iloc_body.extend_from_slice(&1u16.to_be_bytes());
    iloc_body.extend_from_slice(&u64::MAX.to_be_bytes());
    iloc_body.extend_from_slice(&u64::MAX.to_be_bytes());
    let mut meta_body = full(b"pitm", 0, &1u16.to_be_bytes());
    meta_body.extend_from_slice(&full(b"iloc", 0, &iloc_body));
    let mut hostile_iloc = bx(b"ftyp", b"heic\0\0\0\0heic");
    hostile_iloc.extend_from_slice(&full(b"meta", 0, &meta_body));
    out.push(("ilocが嘘", hostile_iloc));

    // 箱が自分を指す（size がヘッダぶんだけ＝前に進まない）
    let mut loop_box = 8u32.to_be_bytes().to_vec();
    loop_box.extend_from_slice(b"meta");
    let mut looping = bx(b"ftyp", b"heic\0\0\0\0heic");
    looping.extend_from_slice(&loop_box);
    looping.extend_from_slice(&[0u8; 32]);
    out.push(("進まない箱", looping));

    out
}

/// SVGの、数字が壊れているもの。
fn hostile_svgs() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("閉じない", br#"<svg width="100" height="#.to_vec()),
        (
            "桁が溢れる",
            br#"<svg width="999999999999999999999" height="-0"/>"#.to_vec(),
        ),
        ("単位だけ", br#"<svg width="px" height="%"/>"#.to_vec()),
        (
            "viewBoxが短い",
            br#"<svg viewBox="0 0"><rect/></svg>"#.to_vec(),
        ),
        ("マルチバイトの途中", "<svg width=\"あ".as_bytes().to_vec()),
    ]
}

/// 名前から日付を読む道（段階H-2）に食わせる、危ない名前。
/// **バイト単位で切ると壊れる文字**を混ぜてある。
fn hostile_names() -> Vec<&'static str> {
    vec![
        "2019-08-11_120000.jpg",
        "あ.jpg",
        "２０１９０８１１.jpg",
        "----.jpg",
        "99999999999999999999.jpg",
        "0000-00-00_000000.jpg",
        "9999-99-99_999999.jpg",
        "IMG_.jpg",
        "２0１9-0８-1１.jpg",
        ".jpg",
    ]
}

/// 拡張子を付け替えて、**その形式のつもりで**読ませる。
/// 実際に起きる: `.HEIC` を `.jpg` に直しただけのファイルは普通に出回る。
const EXTS: &[&str] = &[
    "heic", "heif", "avif", "jpg", "jpeg", "png", "mov", "mp4", "cr3", "cr2", "arw", "nef", "dng",
    "svg", "webp", "gif", "tif",
];

/// すべての読み取り口へ、そのファイルを食わせる。
fn feed_every_reader(path: &Path) {
    use pictkura_core::*;

    // 形式の判定（パスだけを見るもの）
    let _ = heif::is_heif_path(path);
    let _ = heif::is_avif_path(path);
    let _ = heif::is_bmff_image_path(path);
    let _ = video::is_video_path(path);
    let _ = video::is_bmff_video_path(path);
    let _ = video::plays_in_webview(path);
    let _ = raw::is_raw_path(path);
    let _ = jpeg::is_jpeg_path(path);
    let _ = svg::is_svg_path(path);

    // 中身を読むもの
    let _ = heif::read_info(path);
    let _ = heif::display_dimensions(path);
    let _ = heif::stored_chroma(path);
    let _ = heif::read_avif_source(path);
    let _ = video::read_info(path);
    let _ = raw::embedded_preview(path);
    let _ = raw::bmff_metadata_blocks(path);
    let _ = svg::dimensions(path);
    let _ = namedate::guess_taken_at(path);
    let _ = thumbs::read_exif(path);
    let _ = thumbs::read_exif_info(path);
    let _ = thumbs::needs_display_transcode(path);
    let _ = jpeg::decode_scaled(path, 256);

    if let Ok(bytes) = std::fs::read(path) {
        let _ = jpeg::decode_scaled_mem(&bytes, 256);
        let _ = jpeg::chroma_of(&bytes);
    }
}

/// 切り詰めの検査で使う拡張子。**総当たりにしない**——1バイトずつ削る掛け算で
/// 時間が伸びるだけで、判定はどれも同じ入口へ落ちる。
/// 中身と食い違う `.jpg` / `.arw` を1つずつ混ぜて、付け替えの経路も通す。
const CUT_EXTS: &[&str] = &["heic", "avif", "mov", "mp4", "jpg", "arw"];

/// 1つのバイト列を、指定の拡張子で置いて食わせる。
fn feed_as(dir: &Path, label: &str, bytes: &[u8], exts: &[&str]) {
    for ext in exts {
        let path = dir.join(format!("壊れた.{ext}"));
        std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("{label} を書けない: {e}"));
        feed_every_reader(&path);
        let _ = std::fs::remove_file(&path);
    }
}

/// すべての拡張子で置いて食わせる（中身と拡張子の食い違いを総当たりで見る）。
fn feed_as_every_extension(dir: &Path, label: &str, bytes: &[u8]) {
    feed_as(dir, label, bytes, EXTS);
}

/// **この検査そのものが空振りしていないこと**を先に固定する。
///
/// 細工したファイルが先頭4バイトで弾かれていたら、下の「1バイトずつ切り詰める」
/// 検査は**解析器の中を一度も歩いていない**——落ちないのは当たり前で、
/// 何も確かめていないことになる。
#[test]
fn 細工したファイルが本当に読めている() {
    let dir = tempfile::tempdir().unwrap();

    let heic = dir.path().join("細工.heic");
    std::fs::write(&heic, plausible_heic()).unwrap();
    let info = pictkura_core::heif::read_info(&heic);
    let info = info.expect("細工したHEICが解析器に届いていない");
    // **奥の解析器まで歩いている**ことを見る。`clap`（切り出し）が読めていれば
    // `parse_clap` を通っており、下の切り詰め検査もそこを歩く
    assert!(
        info.crop.is_some(),
        "clap が解析されていない（検査が空振り）"
    );
    assert_eq!(info.rotation, 1, "irot が解析されていない");

    let grid = dir.path().join("細工の貼り合わせ.heic");
    std::fs::write(&grid, plausible_heic_grid()).unwrap();
    assert!(
        pictkura_core::heif::read_info(&grid).is_some(),
        "貼り合わせのHEICが解析器に届いていない"
    );
    // `grid` の中身（タイルの並びと出来上がりの寸法）まで読めていること
    assert_eq!(
        pictkura_core::heif::display_dimensions(&grid),
        Some((4032, 3024)),
        "parse_grid が動いていない（検査が空振り）"
    );

    let mp4 = dir.path().join("細工.mp4");
    std::fs::write(&mp4, plausible_mp4()).unwrap();
    assert!(
        pictkura_core::video::read_info(&mp4).is_some(),
        "細工したMP4が解析器に届いていない"
    );

    let jpg = dir.path().join("細工.jpg");
    std::fs::write(&jpg, plausible_jpeg()).unwrap();
    assert!(
        pictkura_core::thumbs::read_exif(&jpg).taken_at_ms.is_some(),
        "細工したJPEGのEXIFが読めていない"
    );
}

#[test]
fn 空とごみは何形式として読ませても落ちない() {
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Xorshift(0x1234_5678_9abc_def0);

    feed_as_every_extension(dir.path(), "空", &[]);
    feed_as_every_extension(dir.path(), "ゼロ", &[0u8; 64]);
    feed_as_every_extension(dir.path(), "0xFF", &[0xFFu8; 64]);
    for len in [1usize, 3, 7, 8, 9, 15, 16, 17, 63, 255, 1024] {
        let bytes = rng.bytes(len);
        feed_as_every_extension(dir.path(), &format!("乱数{len}"), &bytes);
    }
}

#[test]
fn 途中で切れた画像や動画でも落ちない() {
    let dir = tempfile::tempdir().unwrap();
    for (label, whole) in [
        ("heic", plausible_heic()),
        ("heic-grid", plausible_heic_grid()),
        ("mp4", plausible_mp4()),
        ("jpeg", plausible_jpeg()),
    ] {
        // **1バイトずつ切り詰める**。転送が途中で止まったファイルそのもの
        for cut in 0..whole.len() {
            feed_as(
                dir.path(),
                &format!("{label}[..{cut}]"),
                &whole[..cut],
                CUT_EXTS,
            );
        }
        feed_as_every_extension(dir.path(), label, &whole);
    }
}

#[test]
fn 長さの欄が嘘の箱でも落ちない() {
    let dir = tempfile::tempdir().unwrap();
    for (label, bytes) in hostile_boxes() {
        feed_as_every_extension(dir.path(), label, &bytes);
        // 後ろが切れている場合も見る
        for cut in [0usize, 4, 8, 12] {
            if cut < bytes.len() {
                feed_as_every_extension(dir.path(), label, &bytes[..bytes.len() - cut]);
            }
        }
    }
}

#[test]
fn 壊れたsvgでも落ちない() {
    let dir = tempfile::tempdir().unwrap();
    for (label, bytes) in hostile_svgs() {
        feed_as_every_extension(dir.path(), label, &bytes);
        for cut in 0..bytes.len() {
            feed_as_every_extension(dir.path(), label, &bytes[..cut]);
        }
    }
}

#[test]
fn 危ない名前でも日付の読み取りが落ちない() {
    let dir = tempfile::tempdir().unwrap();
    for name in hostile_names() {
        let path = dir.path().join(name);
        std::fs::write(&path, b"x").unwrap();
        feed_every_reader(&path);
        let _ = std::fs::remove_file(&path);
    }
    // 拡張子だけ・点だけ・とても長い名前
    for name in [".", "..", "a", &"x".repeat(200)] {
        let path = dir.path().join(format!("{name}.jpg"));
        if std::fs::write(&path, b"x").is_ok() {
            feed_every_reader(&path);
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[test]
fn 実在しないファイルでも落ちない() {
    let dir = tempfile::tempdir().unwrap();
    for ext in EXTS {
        feed_every_reader(&dir.path().join(format!("居ない.{ext}")));
    }
    // フォルダを画像として読ませる（右クリックの経路で起きうる）
    feed_every_reader(dir.path());
}
