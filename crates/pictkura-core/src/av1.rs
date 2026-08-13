//! AVIF（AV1）の画素展開（第7部 段階G-6）。
//!
//! HEIC（HEVC）と違い、**デコーダをアプリに同梱する**。理由は3つ:
//!
//! 1. **OSに任せると穴が空く**。Windowsは「AV1 Video Extension」が
//!    **Windows 11でも既定で入っていない**（実測で確認）。macOSのImageIOは13以降、
//!    Linuxには共通の経路が無い。つまりOS任せでは大半の環境で一覧が空になる
//! 2. **特許がロイヤリティフリー**。AOMediaのAV1はデコーダを配っても課金が無い。
//!    HEVCはプールがデコーダ配布にも課金するので（Firefoxすら同梱を避けている）、
//!    HEICは今まで通りOSに任せる。この**非対称は意図的**で、
//!    Fedora（libheifをAV1のみでビルド）やGIMPが引いているのと同じ線
//! 3. **Cライブラリが増えない**。[`rav1d`] は dav1d の純Rust移植（BSD-2-Clause）
//!
//! 実測（この開発機・4032x3024）: デコード **40〜53ms**、
//! 縮小とYUV→RGBを融合して +1〜2ms。HEICをWICで展開すると260〜480msなので、
//! **同梱した方がOS任せより速い**。
//!
//! **罠**: `irot`/`imir` はデコーダが適用しない。OSのデコーダ（WIC）は適用して返すので、
//! HEIC側の「適用済みか縦横比で見分ける」判定をここへ持ち込んではいけない。
//! ここでは**必ず自分で掛ける**。

use crate::heif::{AvifSource, Grid, Nclx};
use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::Dav1dSettings;
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_get_picture,
    dav1d_open, dav1d_picture_unref, dav1d_send_data,
};
use std::path::Path;
use std::ptr::NonNull;

/// 使うスレッド数。
///
/// サムネイル生成は**既にファイル単位でコア数ぶん並列**なので、
/// そこから更にデコーダ内で分割すると取り合いになる。
/// 一覧向けは1本、画面に出す1枚だけは全部使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threads {
    /// 1枚を最速で出す（原寸表示など、その1枚しか走っていないとき）
    All,
    /// 別のファイルと並行して走る（サムネイルの一括生成）
    One,
}

/// 一度に展開してよい画素数の上限（16384x16384＝約2.7億画素）。
///
/// 実測: 1200万画素で86MB、8Kで224MB、**1億画素で703MB**。
/// AV1が名乗れる最大（65535四方＝43億画素）をそのまま信じると
/// 28GBを要求されるので、実在する最大級の写真が余裕で通る所で頭打ちにする。
const MAX_DECODE_PIXELS: usize = 16384 * 16384;

impl Threads {
    fn count(self) -> i32 {
        match self {
            // 0 は rav1d の「自動（論理コア数）」
            Threads::All => 0,
            Threads::One => 1,
        }
    }
}

/// このビルドがAVIFを展開できるか。
///
/// デコーダを同梱しているので常に真。OSデコーダ頼みだった頃の
/// 「拡張機能が要る」案内を出さないために [`crate::heif::decoder_available`] と対で使う。
pub const fn available() -> bool {
    true
}

/// AVIFファイルを展開して**表示の向きに直した**画像を返す。
///
/// `max_edge` を渡すと、そこまで縮めながら展開する（一覧用）。
/// ちょうどその大きさになるわけではなく、**2倍程度まで粗く落とす**だけなので、
/// 最終的な縮小は呼び出し側の高品質な縮小に任せる。
pub fn decode_file(
    path: &Path,
    max_edge: Option<u32>,
    threads: Threads,
) -> Option<image::DynamicImage> {
    let source = crate::heif::read_avif_source(path)?;
    decode(&source, max_edge, threads)
}

/// コンテナから集めた材料を1枚の画像に組み立てる。
pub fn decode(
    source: &AvifSource,
    max_edge: Option<u32>,
    threads: Threads,
) -> Option<image::DynamicImage> {
    // clap があれば、そこだけを読む（切り出してから縮める方が無駄が無い）
    let (rect_x, rect_y, rect_w, rect_h) = match source.info.crop {
        Some(c) => (c.x, c.y, c.width, c.height),
        None => (0, 0, source.info.stored_width, source.info.stored_height),
    };
    if rect_w == 0 || rect_h == 0 {
        return None;
    }
    // 目標の2倍まで整数間引きで落とす。半端に落とすと縞が出るので必ず整数倍
    let step = match max_edge {
        Some(edge) if edge > 0 => (rect_w.max(rect_h) / (edge.saturating_mul(2)).max(1)).max(1),
        _ => 1,
    } as usize;
    let out_w = (rect_w as usize).div_ceil(step);
    let out_h = (rect_h as usize).div_ceil(step);
    // `grid` は寸法をコンテナ側が名乗るので、デコーダの上限だけでは守れない。
    // ここを素通しにすると `vec!` の確保に失敗してプロセスごと落ちる
    if out_w.checked_mul(out_h)? > MAX_DECODE_PIXELS {
        return None;
    }
    let mut canvas = vec![0u8; out_w.checked_mul(out_h)?.checked_mul(3)?];

    let decoder = Decoder::open(threads)?;
    // grid は左上から行優先。タイル1枚ごとの寸法は1枚目から分かる
    let (cols, rows) = match source.grid {
        Some(Grid { rows, cols, .. }) => (cols as usize, rows as usize),
        None => (1, 1),
    };
    let rect = (rect_x as usize, rect_y as usize);
    let mut tile_size: Option<(usize, usize)> = None;
    for (index, obus) in source.tiles.iter().enumerate() {
        if index >= cols * rows {
            break;
        }
        // colr はコンテナ側の指定で、AV1のシーケンスヘッダより**優先**する
        let pic = decoder.decode_tile(&source.config_obus, obus, source.color)?;
        let (tw, th) = *tile_size.get_or_insert((pic.width, pic.height));
        let origin_x = (index % cols) * tw;
        let origin_y = (index / cols) * th;
        pic.blit(&mut canvas, out_w, out_h, step, rect, (origin_x, origin_y));
    }

    // 透過は主画像とは別のAV1ストリーム。**白に重ねて**不透明の絵にする
    // （一覧のタイルは背景が白なので、市松模様より素直に見える）
    if let Some(alpha) = &source.alpha {
        if let Some(obus) = alpha.tiles.first() {
            // alpha は白黒1枚。色の決まりごとは効かないので既定のまま読む
            if let Some(mask) = decoder.decode_tile(&alpha.config_obus, obus, None) {
                mask.composite_over_white(
                    &mut canvas,
                    out_w,
                    out_h,
                    step,
                    rect,
                    alpha.premultiplied,
                );
            }
        }
    }

    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(
        out_w as u32,
        out_h as u32,
        canvas,
    )?);
    // `irot`/`imir` はデコーダが適用しない。必ずここで掛ける
    Some(crate::heif::apply_transform(img, &source.info))
}

// ---------------------------------------------------------------------------
// rav1d のC API を包む
// ---------------------------------------------------------------------------

/// デコーダの文脈。生成と破棄を対にするためだけの入れ物。
struct Decoder {
    ctx: rav1d::include::dav1d::dav1d::Dav1dContext,
}

impl Decoder {
    fn open(threads: Threads) -> Option<Self> {
        // SAFETY: rav1d のC APIをそのまま呼ぶ。設定はゼロ初期化してから
        // dav1d_default_settings で埋めるので、未初期化のまま渡すことはない
        unsafe {
            let mut settings: Dav1dSettings = std::mem::zeroed();
            dav1d_default_settings(NonNull::from(&mut settings));
            settings.n_threads = threads.count();
            // **小さいファイルが巨大な寸法を宣言できる**（AV1は65535四方まで）。
            // 実測ではおおむね画素数に比例して 1億画素で703MB使うので、
            // 上限を置かないと数十GBを要求されてその場で落ちる。
            // 1億画素級の実物（12000x9000）が2倍以上の余裕で通る所に置く
            settings.frame_size_limit = MAX_DECODE_PIXELS as u32;
            // 静止画は1枚しか無いので先読みの遅延は要らない
            settings.max_frame_delay = 1;
            let mut ctx = None;
            if dav1d_open(
                Some(NonNull::from(&mut ctx)),
                Some(NonNull::from(&settings)),
            )
            .0 < 0
            {
                return None;
            }
            ctx.map(|ctx| Self { ctx })
        }
    }

    /// タイル1枚を展開する。`config_obus`（シーケンスヘッダ）は毎回先頭に付ける。
    fn decode_tile(&self, config_obus: &[u8], obus: &[u8], color: Option<Nclx>) -> Option<Picture> {
        let total = config_obus.len().checked_add(obus.len())?;
        if total == 0 {
            return None;
        }
        // SAFETY: dav1d_data_create が確保した領域へ、その長さだけ書き込む。
        // 受け取ったポインタは dav1d_send_data が所有権ごと引き取る
        unsafe {
            let mut data: Dav1dData = std::mem::zeroed();
            let buf = dav1d_data_create(Some(NonNull::from(&mut data)), total);
            if buf.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(config_obus.as_ptr(), buf, config_obus.len());
            std::ptr::copy_nonoverlapping(obus.as_ptr(), buf.add(config_obus.len()), obus.len());
            if dav1d_send_data(Some(self.ctx), Some(NonNull::from(&mut data))).0 < 0 {
                // 失敗したときは確保した領域がこちらに残る。返さないと積み上がる
                dav1d_data_unref(Some(NonNull::from(&mut data)));
                return None;
            }
            let mut pic: Dav1dPicture = std::mem::zeroed();
            if dav1d_get_picture(Some(self.ctx), Some(NonNull::from(&mut pic))).0 < 0 {
                return None;
            }
            // 受け取った1枚は、こちらで使わないと決めた場合でも必ず返す
            // （壊れたAVIFが並んでいると、走査のあいだ積み上がっていく）
            match Picture::new(pic, color) {
                Ok(picture) => Some(picture),
                Err(mut rejected) => {
                    dav1d_picture_unref(Some(NonNull::from(&mut *rejected)));
                    None
                }
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: open で得た文脈をそのまま返す。二重に閉じることはない
        unsafe {
            let mut ctx = Some(self.ctx);
            dav1d_close(Some(NonNull::from(&mut ctx)));
        }
    }
}

/// 展開済みの1枚。落とすときに rav1d へ返す。
struct Picture {
    raw: Dav1dPicture,
    width: usize,
    height: usize,
    /// 画素1つのバイト数（8bitなら1、10/12bitなら2）
    bytes: usize,
    /// 色差の間引き（横・縦それぞれ何ビット右シフトするか）
    ss_x: usize,
    ss_y: usize,
    /// 色差を持たない（白黒）
    mono: bool,
    /// 上位ビットへ寄せる量（10bitなら2）
    shift: u32,
    color: Color,
    /// HDRのときだけ持つ引き当て表（1枚につき1回だけ作る）
    tone: Option<Box<ToneLut>>,
}

/// タイル1枚が受け持つ出力座標の範囲（`from..to`）を求める。
///
/// 出力の升目 `x` は元画像の `rect + x * step` を代表する。その代表点が
/// タイルの内側（`origin..origin + size`）にある `x` だけを返す。
/// **grid と clap が重なるとここが一番間違えやすい**（タイルの継ぎ目が1列ずれる、
/// 端のタイルを描き落とす等）ので、切り出して単体で確かめられるようにしている。
fn tile_output_range(
    origin: usize,
    size: usize,
    rect: usize,
    step: usize,
    out: usize,
) -> (usize, usize) {
    let from = origin.saturating_sub(rect).div_ceil(step);
    let to = (origin + size).saturating_sub(rect).div_ceil(step);
    (from.min(out), to.min(out))
}

/// 明るさの伝え方。HDRは**そのまま出すと眠い絵になる**ので直しが要る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    /// ふつうのSDR（sRGB / BT.709）。何もしない
    Sdr,
    /// PQ（SMPTE ST 2084）。絶対輝度で書かれている
    Pq,
    /// HLG（ARIB STD-B67）。相対輝度で書かれている
    Hlg,
}

/// YUVからRGBへ戻すときの決まりごと。
#[derive(Debug, Clone, Copy)]
struct Color {
    /// 輝度の係数（BT.601 / 709 / 2020 で違う）
    kr: f32,
    kb: f32,
    /// 画素値が 16〜235 に収まる書き方か（AV1の既定はこちら）
    limited: bool,
    /// 変換せずそのままG・B・Rとして読む（可逆AVIFで使われる）
    identity: bool,
    /// HDRか（PQ / HLG）
    tone: Tone,
    /// 原色。sRGB以外はそれぞれ**別の行列**で移す
    gamut: Gamut,
}

/// 原色の組み合わせ。BT.2020とDisplay P3は色域が違うので同じ行列で移せない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gamut {
    /// sRGB / BT.709。付け替えない
    Srgb,
    /// BT.2020（HDRの標準）
    Bt2020,
    /// Display P3（Appleの写真でよく使われる）
    DisplayP3,
}

impl Picture {
    /// 受け取れなければ**そのまま突き返す**（呼び出し側が rav1d へ返せるように）。
    ///
    /// SAFETY: `raw` は dav1d_get_picture が成功して返したものであること。
    unsafe fn new(raw: Dav1dPicture, color: Option<Nclx>) -> Result<Self, Box<Dav1dPicture>> {
        let (width, height) = (raw.p.w.max(0) as usize, raw.p.h.max(0) as usize);
        if width == 0 || height == 0 || raw.data[0].is_none() {
            return Err(Box::new(raw));
        }
        // 0=白黒 / 1=4:2:0 / 2=4:2:2 / 3=4:4:4
        let (ss_x, ss_y, mono) = match raw.p.layout {
            0 => (0, 0, true),
            1 => (1, 1, false),
            2 => (1, 0, false),
            _ => (0, 0, false),
        };
        let bpc = raw.p.bpc.clamp(8, 16) as u32;
        // コンテナ（`colr`）の指定が最優先。無ければシーケンスヘッダを見る。
        // SAFETY: seq_hdr は get_picture が埋める。無ければ既定値で進む
        let color = match color {
            Some(n) => Color::from_cicp(n.matrix as u32, n.full_range, n.transfer, n.primaries),
            None => raw
                .seq_hdr
                .map(|h| {
                    let h = unsafe { h.as_ref() };
                    Color::from_cicp(h.mtrx, h.color_range != 0, h.trc as u16, h.pri as u16)
                })
                .unwrap_or_else(|| Color::from_cicp(2, false, 1, 1)),
        };
        let tone = (color.tone != Tone::Sdr).then(|| Box::new(ToneLut::new(color.tone)));
        Ok(Self {
            width,
            height,
            bytes: if bpc > 8 { 2 } else { 1 },
            ss_x,
            ss_y,
            mono,
            shift: bpc - 8,
            color,
            tone,
            raw,
        })
    }

    /// 面の画素を8ビットに揃えて取り出す。
    ///
    /// SAFETY: `plane` は 0..3、`x`/`y` はその面の範囲内であること。
    unsafe fn sample(&self, plane: usize, x: usize, y: usize) -> u32 {
        let stride = self.raw.stride[usize::from(plane > 0)].max(0) as usize;
        let base = self.raw.data[plane].unwrap().as_ptr() as *const u8;
        let at = unsafe { base.add(y * stride + x * self.bytes) };
        if self.bytes == 1 {
            unsafe { *at as u32 }
        } else {
            // 10/12bit は下位ビットを丸めて落とす（サムネイルでは差が見えない）
            let v = unsafe { (at as *const u16).read_unaligned() } as u32;
            let half = 1u32 << self.shift >> 1;
            ((v + half) >> self.shift).min(255)
        }
    }

    /// `step` 四方の平均を取る（間引くだけだと細かい模様に縞が出る）。
    /// タイルの端では収まるぶんだけを見る。
    fn box_average(&self, x: usize, y: usize, step: usize) -> u32 {
        let (mut sum, mut count) = (0u32, 0u32);
        for dy in 0..step {
            let sy = y + dy;
            if sy >= self.height {
                break;
            }
            for dx in 0..step {
                let sx = x + dx;
                if sx >= self.width {
                    break;
                }
                // SAFETY: sx/sy はこの面の範囲に収めてある
                sum += unsafe { self.sample(0, sx, sy) };
                count += 1;
            }
        }
        if count == 0 {
            0
        } else {
            sum / count
        }
    }

    /// 出力の升目へ、このタイルが受け持つぶんを書き込む。
    ///
    /// 輝度は `step` 四方の平均を取る（間引くだけだと細かい模様に縞が出る）。
    /// 色差はもともと粗いので中心の1点で足りる。
    fn blit(
        &self,
        canvas: &mut [u8],
        out_w: usize,
        out_h: usize,
        step: usize,
        rect: (usize, usize),
        origin: (usize, usize),
    ) {
        // このタイルが覆う出力座標だけを回す（全升目を舐めるとタイル数だけ遅くなる）
        let (x0, x1) = tile_output_range(origin.0, self.width, rect.0, step, out_w);
        let (y0, y1) = tile_output_range(origin.1, self.height, rect.1, step, out_h);

        for y in y0..y1 {
            let src_y = (rect.1 + y * step)
                .saturating_sub(origin.1)
                .min(self.height - 1);
            for x in x0..x1 {
                let src_x = (rect.0 + x * step)
                    .saturating_sub(origin.0)
                    .min(self.width - 1);
                let luma = self.box_average(src_x, src_y, step);
                let (cb, cr) = if self.mono {
                    (128, 128)
                } else {
                    let cx = ((src_x + step / 2).min(self.width - 1)) >> self.ss_x;
                    let cy = ((src_y + step / 2).min(self.height - 1)) >> self.ss_y;
                    // SAFETY: 間引き後の座標なので色差の面に収まる
                    unsafe { (self.sample(1, cx, cy), self.sample(2, cx, cy)) }
                };
                let at = (y * out_w + x) * 3;
                let [r, g, b] = self.color.to_rgb(luma, cb, cr, self.tone.as_deref());
                canvas[at] = r;
                canvas[at + 1] = g;
                canvas[at + 2] = b;
            }
        }
    }
}

impl Picture {
    /// 透過を白へ重ねて、不透明の絵にする。
    ///
    /// alphaは主画像と同じ寸法の**白黒1枚**として別に入っている。
    /// 一覧のタイルは背景が白なので、市松模様を敷くより白に重ねる方が素直。
    ///
    /// **罠**: `prem`（既に色へalphaが掛けてある）のときに掛け直すと、
    /// 半透明の部分だけ二重に薄くなる。
    fn composite_over_white(
        &self,
        canvas: &mut [u8],
        out_w: usize,
        out_h: usize,
        step: usize,
        rect: (usize, usize),
        premultiplied: bool,
    ) {
        let (x0, x1) = tile_output_range(0, self.width, rect.0, step, out_w);
        let (y0, y1) = tile_output_range(0, self.height, rect.1, step, out_h);
        for y in y0..y1 {
            let src_y = (rect.1 + y * step).min(self.height - 1);
            for x in x0..x1 {
                let src_x = (rect.0 + x * step).min(self.width - 1);
                // 色と同じ範囲を平均する。左上1点だけ見ると、細かい透過の縁が
                // 「全部不透明」か「全部透明」に倒れてギザギザになる
                let raw = self.box_average(src_x, src_y, step);
                // alphaは規格上フルレンジ。限定レンジで書かれていたら伸ばす
                let a = if self.color.limited {
                    (((raw as f32 - 16.0) * (255.0 / 219.0)) as i32).clamp(0, 255) as u32
                } else {
                    raw
                };
                if a >= 255 {
                    continue;
                }
                let at = (y * out_w + x) * 3;
                for c in 0..3 {
                    let v = canvas[at + c] as u32;
                    // 掛かっていなければ掛けてから、白（255）を残りの割合で足す
                    let kept = if premultiplied { v } else { v * a / 255 };
                    canvas[at + c] = (kept + 255 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: get_picture が返した1枚をちょうど1回返す
        unsafe { dav1d_picture_unref(Some(NonNull::from(&mut self.raw))) }
    }
}

impl Color {
    /// CICPの matrix_coefficients から係数を決める。
    ///
    /// **罠**: 2（unspecified）は実際に多い。libavif に合わせて BT.601 に倒す。
    /// ここを BT.709 にすると肌の色がずれる。
    /// また AV1 の既定は**限定レンジ**で、full だと思って伸ばすと白飛びする。
    fn from_cicp(matrix: u32, full_range: bool, transfer: u16, primaries: u16) -> Self {
        // 16=PQ / 18=HLG。それ以外はSDRとして扱う
        let tone = match transfer {
            16 => Tone::Pq,
            18 => Tone::Hlg,
            _ => Tone::Sdr,
        };
        // 原色の付け替えはHDRのときだけ行う
        //（SDRの広色域は実物が稀で、触ると普通の写真に影響が出かねない）
        let gamut = match (tone, primaries) {
            (Tone::Sdr, _) => Gamut::Srgb,
            (_, 9) => Gamut::Bt2020,
            // 11=DCI-P3 / 12=Display P3。BT.2020とは色域が違う
            (_, 11 | 12) => Gamut::DisplayP3,
            _ => Gamut::Srgb,
        };
        // 0 は「変換しない」＝ G・B・R がそのまま入っている（可逆AVIF）
        if matrix == 0 {
            return Self {
                kr: 0.0,
                kb: 0.0,
                limited: false,
                identity: true,
                tone,
                gamut,
            };
        }
        let (kr, kb) = match matrix {
            1 => (0.2126, 0.0722),      // BT.709
            9 | 10 => (0.2627, 0.0593), // BT.2020
            _ => (0.299, 0.114),        // BT.601（5/6 と unspecified）
        };
        Self {
            kr,
            kb,
            limited: !full_range,
            identity: false,
            tone,
            gamut,
        }
    }

    fn to_rgb(self, y: u32, u: u32, v: u32, lut: Option<&ToneLut>) -> [u8; 3] {
        if self.identity {
            // AV1のidentityは Y=G / U=B / V=R の順で入っている。
            // **可逆でもHDRはあり得る**（`avifenc --lossless` は係数を0にする）ので、
            // ここで返してしまうとPQ/HLGのまま出て眠い絵になる
            let rgb = [v as f32, y as f32, u as f32];
            return match lut {
                Some(lut) => self.to_sdr(rgb, lut),
                None => rgb.map(|c| c.clamp(0.0, 255.0) as u8),
            };
        }
        let (mut y, mut u, mut v) = (y as f32, u as f32 - 128.0, v as f32 - 128.0);
        if self.limited {
            // 16〜235（色差は16〜240）を0〜255へ伸ばす
            y = (y - 16.0) * (255.0 / 219.0);
            u *= 255.0 / 224.0;
            v *= 255.0 / 224.0;
        }
        let kg = 1.0 - self.kr - self.kb;
        let r = y + 2.0 * (1.0 - self.kr) * v;
        let b = y + 2.0 * (1.0 - self.kb) * u;
        let g = y
            - (2.0 * (1.0 - self.kb) * self.kb / kg) * u
            - (2.0 * (1.0 - self.kr) * self.kr / kg) * v;
        if let Some(lut) = lut {
            return self.to_sdr([r, g, b], lut);
        }
        [
            r.clamp(0.0, 255.0) as u8,
            g.clamp(0.0, 255.0) as u8,
            b.clamp(0.0, 255.0) as u8,
        ]
    }

    /// HDRの信号をSDRの絵に直す。
    ///
    /// そのまま8ビットとして書き出すと、PQ/HLGは**全体が眠く（低コントラストに）なる**。
    /// 直し方はこう:
    ///
    /// 1. 信号を実際の明るさへ戻す（PQ/HLGそれぞれの逆変換）
    /// 2. **拡散白**（紙の白に相当する明るさ）が1.0になるよう正規化する。
    ///    PQは203ニト、HLGは信号75%がそれにあたる（BT.2408）
    /// 3. 原色がBT.2020ならsRGBの原色へ移す（しないと色が濃すぎる）
    /// 4. 明るい側だけを滑らかに畳む（[`roll_off`]）。
    ///    中間調は触らないので、普通の写真としての見え方が変わらない
    /// 5. sRGBの伝え方で8ビットへ戻す
    ///
    /// 一覧に出す絵としてはこれで十分で、厳密な色再現は狙わない。
    fn to_sdr(self, rgb: [f32; 3], lut: &ToneLut) -> [u8; 3] {
        // 入り口の信号は8ビットに揃えてあるので、そのまま表を引ける
        let linear = rgb.map(|v| lut.linear(v));
        let rgb = match self.gamut {
            Gamut::Srgb => linear,
            Gamut::Bt2020 => convert(linear, BT2020_TO_SRGB),
            Gamut::DisplayP3 => convert(linear, P3_TO_SRGB),
        };
        rgb.map(|v| lut.encode(v))
    }
}

/// HDRをSDRへ直すための引き当て表。
///
/// 変換にはべき乗が何度も要り、**毎画素で計算すると桁違いに遅い**
/// （実測: 190万画素で260ms。同じ枚数のSDRは20ms）。
/// 入り口の信号は8ビットに揃えてあるので、そこは256通りを先に引き当て表にできる。
/// 出口（畳み込み＋sRGBの符号化）は原色の付け替えで値が混ざるため表を引けないので、
/// **平方根で刻んだ**表にして暗部の刻みを細かく保つ。
struct ToneLut {
    /// 信号（0〜255）→ 拡散白を1.0とした明るさ
    linear: [f32; 256],
    /// 明るさ → 8ビットの信号（平方根で刻む）
    encode: [u8; Self::STEPS],
}

impl ToneLut {
    const STEPS: usize = 1024;
    /// 表が覆う明るさの上限。拡散白の8倍まで見ればハイライトは足りる
    const MAX: f32 = 8.0;

    fn new(tone: Tone) -> Self {
        let mut linear = [0.0f32; 256];
        for (i, slot) in linear.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            *slot = match tone {
                Tone::Pq => pq_to_linear(x) / 203.0,
                Tone::Hlg => hlg_to_linear(x) / hlg_to_linear(0.75),
                Tone::Sdr => x,
            };
        }
        let mut encode = [0u8; Self::STEPS];
        for (i, slot) in encode.iter_mut().enumerate() {
            let t = i as f32 / (Self::STEPS - 1) as f32;
            let v = t * t * Self::MAX;
            *slot = (srgb_encode(roll_off(v)) * 255.0).clamp(0.0, 255.0) as u8;
        }
        Self { linear, encode }
    }

    /// 信号を明るさへ。**表の間は直線で埋める**。
    ///
    /// 切り捨てで引くと、PQのように傾きが急な所で1段ぶんの差が大きく効く
    /// （実測でPSNRが1.3dB落ちた）。補間は掛け算1回で済む。
    fn linear(&self, signal: f32) -> f32 {
        let x = signal.clamp(0.0, 255.0);
        let i = x as usize;
        if i >= 255 {
            return self.linear[255];
        }
        let t = x - i as f32;
        self.linear[i] + (self.linear[i + 1] - self.linear[i]) * t
    }

    /// 明るさを8ビットの信号へ戻す。
    fn encode(&self, v: f32) -> u8 {
        if v <= 0.0 {
            return self.encode[0];
        }
        let t = (v / Self::MAX).min(1.0).sqrt();
        self.encode[(t * (Self::STEPS - 1) as f32) as usize]
    }
}

/// PQ（SMPTE ST 2084）の信号を明るさ（ニト）へ戻す。
fn pq_to_linear(x: f32) -> f32 {
    const M1: f32 = 0.159_301_76;
    const M2: f32 = 78.84375;
    const C1: f32 = 0.8359375;
    const C2: f32 = 18.851_562;
    const C3: f32 = 18.6875;
    let e = x.powf(1.0 / M2);
    let num = (e - C1).max(0.0);
    let den = C2 - C3 * e;
    if den <= 0.0 {
        return 0.0;
    }
    10000.0 * (num / den).powf(1.0 / M1)
}

/// HLG（ARIB STD-B67）の信号を、表示側の明るさへ戻す。
fn hlg_to_linear(x: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    let scene = if x <= 0.5 {
        x * x / 3.0
    } else {
        (((x - C) / A).exp() + B) / 12.0
    };
    // OOTF: 1000ニトの表示に対するシステムガンマ
    scene.powf(1.2)
}

/// BT.2020 → sRGB（BT.709）の原色変換。
const BT2020_TO_SRGB: [[f32; 3]; 3] = [
    [1.6605, -0.5876, -0.0728],
    [-0.1246, 1.1329, -0.0083],
    [-0.0182, -0.1006, 1.1187],
];

/// Display P3 → sRGB の原色変換。**BT.2020とは色域が違う**ので使い回せない
/// （同じ行列を当てると、鮮やかな色が目に見えてずれる）。
const P3_TO_SRGB: [[f32; 3]; 3] = [
    [1.2249, -0.2247, 0.0],
    [-0.0420, 1.0419, 0.0],
    [-0.0197, -0.0786, 1.0979],
];

/// 線形RGBに3x3の行列を掛ける。
fn convert([r, g, b]: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        m[0][0] * r + m[0][1] * g + m[0][2] * b,
        m[1][0] * r + m[1][1] * g + m[1][2] * b,
        m[2][0] * r + m[2][1] * g + m[2][2] * b,
    ]
}

/// 明るい側を滑らかに畳む。
///
/// 畳み始めを拡散白（1.0）ちょうどに置くと、**それより上が全部真っ白に潰れる**
/// （畳んだ先の余地が無くなるため）。かといって早く畳み始めると、
/// 拡散白そのものが暗くなり、**SDRの写真と並べたときに白がくすんで見える**。
/// 0.9 から畳むと拡散白は8ビットで251（255ではない）に収まり、
/// おおむね1.4倍までのハイライトに階調が残る。この辺りが折り合い。
fn roll_off(x: f32) -> f32 {
    /// ここから上を畳み始める
    const KNEE: f32 = 0.9;
    if x <= KNEE {
        return x.max(0.0);
    }
    KNEE + (1.0 - KNEE) * (1.0 - (-(x - KNEE) / (1.0 - KNEE)).exp())
}

/// sRGBの伝え方（線形→信号）。
fn srgb_encode(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// タイルの範囲が「隙間なく・重なりなく」出力を覆うことを確かめる。
    fn 継ぎ目を確かめる(tiles: &[(usize, usize)], rect: usize, step: usize, out: usize) {
        let mut covered = vec![0u8; out];
        for &(origin, size) in tiles {
            let (from, to) = tile_output_range(origin, size, rect, step, out);
            for slot in &mut covered[from..to] {
                *slot += 1;
            }
        }
        assert!(
            covered.iter().all(|&n| n == 1),
            "隙間か重なりがある: {covered:?}"
        );
    }

    #[test]
    fn タイルの範囲が隙間なく出力を覆う() {
        // 512四方が4枚（2048px）を、1/1・1/2・1/4に落として貼る
        let tiles: Vec<(usize, usize)> = (0..4).map(|i| (i * 512, 512)).collect();
        for step in [1, 2, 4] {
            継ぎ目を確かめる(&tiles, 0, step, 2048 / step);
        }
    }

    #[test]
    fn 切り出しが効いていても継ぎ目がずれない() {
        // clap で 100..1700 を切り出した状態（rect が 0 でない）
        let tiles: Vec<(usize, usize)> = (0..4).map(|i| (i * 512, 512)).collect();
        for step in [1, 2, 3] {
            継ぎ目を確かめる(&tiles, 100, step, 1600 / step);
        }
    }

    #[test]
    fn 端のタイルは出力の外へはみ出さない() {
        // 最後のタイルが出力より大きい（grid の切り落とし）
        let (from, to) = tile_output_range(1536, 512, 0, 1, 2000);
        assert_eq!((from, to), (1536, 2000));
        // 出力より手前で終わるタイルは全部入る
        assert_eq!(tile_output_range(0, 512, 0, 1, 2000), (0, 512));
    }

    #[test]
    fn 切り出しより手前のタイルは1画素も描かない() {
        // rect=1000 なのに 0..512 のタイル＝完全に切り出しの外
        let (from, to) = tile_output_range(0, 512, 1000, 1, 500);
        assert_eq!(from, to, "描く範囲が空でないと左端に別のタイルが写り込む");
    }

    #[test]
    fn pqは拡散白を白として出す() {
        // PQで203ニト（拡散白）の信号は、SDRのほぼ白になるべき。
        // ここが暗いと、HDR写真の一覧だけ全体的に眠くなる
        let signal = pq_signal(203.0);
        let c = Color::from_cicp(1, true, 16, 1);
        let [r, g, b] = c.to_sdr([signal * 255.0; 3], &ToneLut::new(Tone::Pq));
        assert!(r > 235 && r == g && g == b, "拡散白が白にならない: {r}");
    }

    #[test]
    fn pqのハイライトは白飛びせずに畳まれる() {
        let c = Color::from_cicp(1, true, 16, 1);
        let lut = ToneLut::new(Tone::Pq);
        let white = c.to_sdr([pq_signal(203.0) * 255.0; 3], &lut)[0];
        let bright = c.to_sdr([pq_signal(1000.0) * 255.0; 3], &lut)[0];
        assert!(bright > white, "明るい方が明るく出ること");
        // 4000ニトまで行っても振り切らずに差が残る（畳んでいる証拠）
        let brighter = c.to_sdr([pq_signal(4000.0) * 255.0; 3], &lut)[0];
        assert!(brighter >= bright);
    }

    #[test]
    fn hlgも拡散白が白になる() {
        // HLGは信号75%が拡散白
        let c = Color::from_cicp(1, true, 18, 1);
        let [r, ..] = c.to_sdr([0.75 * 255.0; 3], &ToneLut::new(Tone::Hlg));
        assert!(r > 235, "拡散白が白にならない: {r}");
    }

    #[test]
    fn 可逆のhdrも直す() {
        // `avifenc --lossless` は係数を0（変換しない）にする。
        // 「変換しない」で早々に返すと、PQのまま出て眠い絵になる
        let c = Color::from_cicp(0, true, 16, 9);
        let lut = ToneLut::new(Tone::Pq);
        let signal = (pq_signal(203.0) * 255.0) as u32;
        let plain = c.to_rgb(signal, signal, signal, None);
        let mapped = c.to_rgb(signal, signal, signal, Some(&lut));
        assert_ne!(plain, mapped, "HDRの直しが素通しになっている");
        assert!(mapped[0] > 235, "拡散白が白にならない: {:?}", mapped);
    }

    #[test]
    fn 拡散白は白のすぐ手前に収まる() {
        // 畳み始めを1.0ちょうどに置くと上が全部潰れ、早すぎると白がくすむ。
        // 実際にどこへ落ちるかを固定して、次に触る人が驚かないようにする
        let c = Color::from_cicp(1, true, 16, 1);
        let white = c.to_sdr([pq_signal(203.0) * 255.0; 3], &ToneLut::new(Tone::Pq))[0];
        assert!((248..=253).contains(&white), "拡散白が {white} になった");
    }

    #[test]
    fn sdrはhdrの処理を通らない() {
        // 伝え方がSDRなら、値がそのまま出る（余計な変換を挟まない）
        let c = Color::from_cicp(1, true, 1, 1);
        assert_eq!(c.to_rgb(128, 128, 128, None), [128, 128, 128]);
    }

    /// 指定した明るさ（ニト）に対応するPQの信号値（0〜1）。
    fn pq_signal(nits: f32) -> f32 {
        const M1: f32 = 0.159_301_76;
        const M2: f32 = 78.84375;
        const C1: f32 = 0.8359375;
        const C2: f32 = 18.851_562;
        const C3: f32 = 18.6875;
        let y = (nits / 10000.0).powf(M1);
        ((C1 + C2 * y) / (1.0 + C3 * y)).powf(M2)
    }

    #[test]
    fn 名乗るだけの巨大な寸法は展開しない() {
        // 小さいファイルが65535四方を名乗れる（AV1の上限）。
        // 素通しにすると確保に失敗してプロセスごと落ちる
        let source = AvifSource {
            info: crate::heif::HeifInfo {
                stored_width: 65535,
                stored_height: 65535,
                rotation: 0,
                mirror: None,
                mirror_first: false,
                crop: None,
            },
            config_obus: Vec::new(),
            tiles: vec![Vec::new()],
            grid: None,
            color: None,
            alpha: None,
        };
        assert!(
            decode(&source, None, Threads::One).is_none(),
            "原寸は断ること"
        );
        // 一覧用は間引きで小さくなるので、断らずに枠は作れる
        let small = decode(&source, Some(512), Threads::One);
        assert!(small.is_none() || small.unwrap().width() <= 2048);
    }

    #[test]
    fn 広色域は原色ごとに別の行列を使う() {
        // BT.2020 と Display P3 は色域が違うので、同じ行列で移すと色がずれる
        let green = [0.0, 1.0, 0.0];
        let bt2020 = convert(green, BT2020_TO_SRGB);
        let p3 = convert(green, P3_TO_SRGB);
        assert_ne!(bt2020, p3);
        // BT.2020の方が広いので、sRGBへ移すと緑がより強く引き伸ばされる
        assert!(bt2020[1] > p3[1]);
    }

    #[test]
    fn 原色の指定で色域を選ぶ() {
        assert_eq!(Color::from_cicp(1, true, 16, 9).gamut, Gamut::Bt2020);
        assert_eq!(Color::from_cicp(1, true, 16, 12).gamut, Gamut::DisplayP3);
        assert_eq!(Color::from_cicp(1, true, 16, 1).gamut, Gamut::Srgb);
        // SDRでは付け替えない（普通の写真に影響を出さない）
        assert_eq!(Color::from_cicp(1, true, 1, 9).gamut, Gamut::Srgb);
    }

    #[test]
    fn 限定レンジの黒と白が振り切る() {
        let c = Color::from_cicp(1, false, 1, 1);
        assert_eq!(c.to_rgb(16, 128, 128, None), [0, 0, 0]);
        assert_eq!(c.to_rgb(235, 128, 128, None), [255, 255, 255]);
    }

    #[test]
    fn フルレンジは伸ばさない() {
        let c = Color::from_cicp(1, true, 1, 1);
        assert_eq!(c.to_rgb(0, 128, 128, None), [0, 0, 0]);
        assert_eq!(c.to_rgb(255, 128, 128, None), [255, 255, 255]);
    }

    #[test]
    fn identityはgbrとしてそのまま読む() {
        let c = Color::from_cicp(0, true, 1, 1);
        // Y=G / U=B / V=R
        assert_eq!(c.to_rgb(10, 20, 30, None), [30, 10, 20]);
    }

    #[test]
    fn 未指定はbt601に倒す() {
        let unspecified = Color::from_cicp(2, false, 1, 1);
        let bt601 = Color::from_cicp(6, false, 1, 1);
        assert_eq!(unspecified.kr, bt601.kr);
        assert_eq!(unspecified.kb, bt601.kb);
        // BT.709 とは別物であること（ここを取り違えると肌の色がずれる）
        assert_ne!(unspecified.kr, Color::from_cicp(1, false, 1, 1).kr);
    }
}
