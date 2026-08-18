//! JPEGを「間引きながら」展開する。
//!
//! サムネイルに要るのは512px程度なのに、`image` クレート（中身は zune-jpeg）は
//! 常に原寸で起こす。19MPの写真なら、捨てるためだけに1900万画素を作っている。
//!
//! JPEGは8x8のブロックを周波数へ変換して持っているので、**低い周波数だけを
//! 逆変換すれば 1/8 の絵がそのまま出てくる**。縮小してから捨てるのではなく、
//! 最初から小さく起こす。
//!
//! 展開は libjpeg-turbo（mozjpeg）に任せる。理由は2つ:
//! - `scale_num/8` で 1/8 まで落として展開できる（純Rustのデコーダで
//!   これができるのは `jpeg-decoder` だけだが、1画素あたりが遅くて相殺される）
//! - ハフマン復号そのものが速い。**ここが展開時間の大半**で、間引いても減らない
//!
//! 実測（19MP・40枚の平均）: zune-jpeg 原寸 92ms → mozjpeg 1/8 展開 40ms。
//!
//! 扱えない中身（CMYK・壊れたファイル）は `None` を返し、
//! 呼び出し側が `image::open` へ戻る。
//!
//! **罠**: libjpeg はエラーを `longjmp` で返す作りで、mozjpeg クレートは
//! それを `resume_unwind`（＝パニック）に変換して寄こす。クレート自身の
//! ドキュメントも「Result はほぼ役に立たない。壊れたファイルではパニックする」と
//! 断っている。写真ライブラリには壊れたJPEGが1枚くらい混ざっているもので、
//! そのままではサムネイルのワーカースレッドが落ちる。ここで受け止めて
//! `None` に均し、呼び出し側から見れば「扱えなかった」だけにする。

use std::path::Path;

use image::DynamicImage;
use mozjpeg::{ColorSpace, Compress, Decompress};

/// 元ファイルが名乗ってよい画素数の上限（**間引く前**の値で見る）。
///
/// **`dec.rgb()` を呼ぶ前に見ること**。あそこで `jpeg_start_decompress` が走り、
/// プログレッシブJPEGでは**間引き前の寸法で**係数の配列がまるごと確保される。
/// 間引き後の寸法で見ても、その確保は止められない。
/// 30000x20000 と名乗る小さなファイルを置かれるだけで、ワーカーの数だけ
/// GB級の確保が走る（ワーカーは `available_parallelism()/2` 本動く）。
///
/// 値は置き換え前の `image::open` に合わせた。あちらは `Limits` の
/// `max_alloc = 512MB` で、RGB8なら約1億7900万画素で断っていた。
/// 3コンポーネントのプログレッシブなら係数配列が約1GBに収まる大きさでもある。
const MAX_SOURCE_PIXELS: usize = 178_956_970;

/// 展開後に確保してよい画素数の上限（間引いた**後**の値で見る）。
///
/// [`MAX_SOURCE_PIXELS`] を通っても、間引かない（原寸で返す）場合は
/// そのままの大きさが出てくる。二重の歯止めとして残す。
const MAX_DECODED_PIXELS: usize = 8192 * 8192;

/// 拡張子がJPEGか。
pub fn is_jpeg_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "jpe" | "jfif")
    )
}

/// 長辺が `max_edge` 以上になる**最小の**大きさで展開する。
///
/// 返る絵はたいてい `max_edge` より少し大きい（1/8・2/8…8/8 のいずれかに
/// しか落とせないため）。ぴったりの寸法にするのは呼び出し側の縮小に任せる
/// ——ここで半端な倍率まで面倒を見ると、せっかく省いた計算が戻ってくる。
///
/// 元が `max_edge` より小さければ、原寸のまま返る（拡大はしない）。
pub fn decode_scaled(path: &Path, max_edge: u32) -> Option<DynamicImage> {
    catching(|| scaled(Decompress::new_path(path).ok()?, max_edge))
}

/// [`decode_scaled`] のバイト列版。
///
/// RAWの埋め込みプレビューはファイルの中に埋まったJPEGで、
/// 原寸（6000x4000など）で入っていることが多い。ここも間引いて起こす。
pub fn decode_scaled_mem(bytes: &[u8], max_edge: u32) -> Option<DynamicImage> {
    catching(|| scaled(Decompress::new_mem(bytes).ok()?, max_edge))
}

/// libjpeg 由来のパニックを `None` に均す。
///
/// `resume_unwind` はパニックハンドラを通らないので、コンソールには何も出ない。
/// 巻き戻しの途中で `Decompress`／`Compress` の Drop が走り、
/// libjpeg 側の確保は解放される。
fn catching<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    // AssertUnwindSafe: 失敗したら結果を丸ごと捨てるだけで、
    // 途中まで書いた状態を外へ持ち出さない
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()?
}

/// 色差（Cb/Cr）をどれだけ間引くか。
///
/// **エンコーダを比べるときは必ず揃えること**。間引きは画素数そのものを
/// 減らすので、ここが違うと「速い」のがエンコーダの手柄なのか
/// 間引きの手柄なのか分からなくなる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromaSampling {
    /// 4:4:4（間引かない）。`image` クレートのエンコーダはこれで固定
    Full,
    /// 4:2:0（縦横とも半分）。カメラが書くJPEGもiPhoneのHEICもこちら
    Half,
}

/// RGB8の画素をJPEGへ詰める（libjpeg-turbo）。
///
/// `image` クレートの純Rustエンコーダの代わり。**展開ではなく圧縮のほうが
/// 原寸表示の値段の大半**で、HEICの詰め直し 953ms のうち約525msがここだった。
///
/// mozjpeg の既定はtrellis量子化とスキャン最適化（プログレッシブ）を回して
/// ファイルを小さくするが、その分だけ遅い。ここは**配信して数秒で捨てる絵**なので
/// [`Compress::set_fastest_defaults`] で libjpeg v6 相当まで落とす
/// ——速さが要るのであって、数十KBの節約に用は無い。
///
/// **設定の順番に意味がある**。`set_fastest_defaults` は内部で `jpeg_set_defaults`
/// を呼び直すので、**品質は75へ戻り、間引きは色空間の既定へ戻る**
/// （寸法はここでは触られないが、まとめて後に置くほうが間違えにくい）。
/// だから品質と間引きは必ずその後に設定する（テストで固定した）。
pub fn encode_rgb(rgb: &image::RgbImage, quality: u8, chroma: ChromaSampling) -> Option<Vec<u8>> {
    let (w, h) = (rgb.width(), rgb.height());
    if w == 0 || h == 0 {
        return None;
    }
    catching(|| {
        let mut comp = Compress::new(ColorSpace::JCS_RGB);
        comp.set_fastest_defaults();
        comp.set_size(w as usize, h as usize);
        comp.set_quality(f32::from(quality));
        // 引数は「輝度1画素あたりの色差“画素”の大きさ」。(1,1)で等倍＝4:4:4、
        // (2,2)で縦横半分＝4:2:0。libjpeg v6 の既定は 4:2:0 だが、
        // 既定に頼らず毎回書く——**比較の土台になる値**なので黙って変わると困る
        let size = match chroma {
            ChromaSampling::Full => (1, 1),
            ChromaSampling::Half => (2, 2),
        };
        comp.set_chroma_sampling_pixel_sizes(size, size);
        let mut started = comp.start_compress(Vec::new()).ok()?;
        started.write_scanlines(rgb.as_raw()).ok()?;
        started.finish().ok()
    })
}

/// JPEGのヘッダから色差の間引きを読む（`SOF` の標本化係数）。デコードしない。
///
/// 詰め直しでどこまで間引いてよいかを決めるのに要る。カメラがRAWへ埋める
/// プレビューはたいてい4:2:0だが、**4:2:2で書く機種がある**——そこを間引くと
/// 色の境目が崩れる。「たぶん4:2:0だろう」で決め打ちせず、書いてあるものを読む。
///
/// **分からないときは `None`**。呼び出し側は間引かない側（4:4:4）へ倒すこと。
pub fn chroma_of(bytes: &[u8]) -> Option<ChromaSampling> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut pos = 2usize;
    while pos + 2 <= bytes.len() {
        if bytes[pos] != 0xFF {
            return None; // 並びが壊れている
        }
        let marker = bytes[pos + 1];
        // マーカーの前には詰め物の 0xFF がいくつ入ってもよい
        if marker == 0xFF {
            pos += 1;
            continue;
        }
        pos += 2;
        // 長さを持たないマーカー（TEM・RSTn・SOI・EOI）
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            continue;
        }
        // SOS まで来たら、その先は走査データ＝SOFは無かった
        if marker == 0xDA {
            return None;
        }
        if pos + 2 > bytes.len() {
            return None;
        }
        let len = usize::from(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]));
        if len < 2 {
            return None;
        }
        // SOF0〜SOF15。C4=DHT・C8=JPG・CC=DAC はSOFではない
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            return chroma_of_sof(bytes.get(pos + 2..pos + len)?);
        }
        pos += len;
    }
    None
}

/// `SOF` の中身から間引きを読む。
///
/// 並びは precision(1) / height(2) / width(2) / 成分数(1) ののち、
/// 成分ごとに id(1) / 標本化係数(1) / 量子化表番号(1)。
/// 標本化係数は上位4ビットが水平、下位4ビットが垂直。
fn chroma_of_sof(body: &[u8]) -> Option<ChromaSampling> {
    let count = usize::from(*body.get(5)?);
    // グレースケール（1成分）やCMYK（4成分）は「4:2:0ではない」と分かる。
    // 間引かない側で答える——分からないのではなく、間引く相手ではない
    if count != 3 {
        return Some(ChromaSampling::Full);
    }
    let comps = body.get(6..6 + count * 3)?;
    let factor = |i: usize| comps[i * 3 + 1];
    // 輝度が縦横2倍で、色差が両方とも等倍のときだけ 4:2:0
    if factor(0) == 0x22 && factor(1) == 0x11 && factor(2) == 0x11 {
        Some(ChromaSampling::Half)
    } else {
        Some(ChromaSampling::Full)
    }
}

fn scaled<R: std::io::BufRead>(mut dec: Decompress<R>, max_edge: u32) -> Option<DynamicImage> {
    // CMYK/YCCK（印刷用）は libjpeg が RGB へ変換できない。数も少ないので
    // 相手にせず、image クレート側へ回す
    if !matches!(
        dec.color_space(),
        ColorSpace::JCS_GRAYSCALE | ColorSpace::JCS_YCbCr | ColorSpace::JCS_RGB
    ) {
        return None;
    }

    let (w, h) = dec.size();
    let long = w.max(h);
    if long == 0 || w.checked_mul(h)? > MAX_SOURCE_PIXELS {
        return None;
    }
    // 長辺が目標以上になる最小の分子（分母は8）。届かなければ原寸
    let numerator = (1..=8u8)
        .find(|n| long * usize::from(*n) >= max_edge.max(1) as usize * 8)
        .unwrap_or(8);
    dec.scale(numerator);

    let mut started = dec.rgb().ok()?;
    let (dw, dh) = (started.width(), started.height());
    if dw.checked_mul(dh)? > MAX_DECODED_PIXELS {
        return None;
    }
    let pixels: Vec<u8> = started.read_scanlines().ok()?;
    // finish() は末尾まで読めたかの確認。途中で切れた画像は
    // 上半分だけ絵が入った状態になるので、通さない
    started.finish().ok()?;

    let buf =
        image::ImageBuffer::from_raw(u32::try_from(dw).ok()?, u32::try_from(dh).ok()?, pixels)?;
    Some(DynamicImage::ImageRgb8(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_jpeg(dir: &Path, name: &str, w: u32, h: u32) -> std::path::PathBuf {
        let img = image::DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(w, h, |x, y| {
            // 一様な色だと縮小の良し悪しが出ないので模様を入れる
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 253) as u8])
        }));
        let path = dir.join(name);
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn recognises_jpeg_extensions() {
        assert!(is_jpeg_path(Path::new("a.JPG")));
        assert!(is_jpeg_path(Path::new("a.jpeg")));
        assert!(!is_jpeg_path(Path::new("a.png")));
        assert!(!is_jpeg_path(Path::new("a")));
    }

    /// 目標より小さくならず、かつ原寸よりは小さくなること
    #[test]
    fn decodes_at_least_the_requested_edge() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "big.jpg", 2048, 1536);
        let img = decode_scaled(&path, 512).expect("展開できるはず");
        assert!(
            img.width().max(img.height()) >= 512,
            "小さすぎる: {}x{}",
            img.width(),
            img.height()
        );
        assert!(
            img.width() < 2048,
            "間引かれていない: {}x{}",
            img.width(),
            img.height()
        );
        let ratio = img.width() as f64 / img.height() as f64;
        assert!(
            (ratio - 2048.0 / 1536.0).abs() < 0.02,
            "縦横比が崩れた: {ratio}"
        );
    }

    /// ちょうど境目（長辺が目標の8倍ぴったり）で、1/8が選ばれること
    #[test]
    fn picks_the_smallest_scale_that_still_fits() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "exact.jpg", 4096, 3072);
        let img = decode_scaled(&path, 512).unwrap();
        assert_eq!(img.width(), 512, "1/8 が選ばれていない: {}", img.width());
    }

    /// 原寸が目標より小さければ、そのままの寸法で返る（拡大はしない）
    #[test]
    fn keeps_small_images_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "small.jpg", 200, 150);
        let img = decode_scaled(&path, 512).expect("展開できるはず");
        assert_eq!((img.width(), img.height()), (200, 150));
    }

    /// グレースケールJPEGもRGBとして受け取れる
    #[test]
    fn decodes_grayscale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gray.jpg");
        image::DynamicImage::ImageLuma8(image::ImageBuffer::from_fn(1024, 768, |x, y| {
            image::Luma([((x + y) % 256) as u8])
        }))
        .save(&path)
        .unwrap();
        let img = decode_scaled(&path, 256).expect("展開できるはず");
        assert!(img.width() >= 256);
    }

    /// JPEGでないものを渡しても落ちない
    #[test]
    fn returns_none_for_non_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not.jpg");
        std::fs::write(&path, b"this is not a jpeg").unwrap();
        assert!(decode_scaled(&path, 512).is_none());
    }

    /// 途中で切れたJPEGでパニックしない（libjpeg は longjmp で投げてくる）。
    ///
    /// 絵が返るか `None` かは libjpeg の寛容さ次第で、どちらでも構わない。
    /// 落ちないことだけが要件——ここはワーカースレッドの中で動く
    #[test]
    fn survives_truncated_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "cut.jpg", 1024, 768);
        let bytes = std::fs::read(&path).unwrap();
        let cut = dir.path().join("truncated.jpg");
        std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
        let _ = decode_scaled(&cut, 512);
    }

    /// 巨大だと名乗るJPEGは、展開を**始める前に**断る。
    ///
    /// ヘッダだけ差し替えた小さなファイルで、`dec.rgb()` へ進まないことを見る
    /// （進むとプログレッシブでは間引き前の寸法で係数配列が確保される）
    #[test]
    fn refuses_absurdly_large_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "src.jpg", 64, 64);
        let mut bytes = std::fs::read(&path).unwrap();
        // SOF0（0xFFC0）の直後は 長さ2 + 精度1 の後に 高さ2・幅2 が並ぶ
        let sof = bytes
            .windows(2)
            .position(|w| w == [0xFF, 0xC0])
            .expect("SOF0が見つからない");
        let at = sof + 5;
        bytes[at..at + 4].copy_from_slice(&[0x4E, 0x20, 0x75, 0x30]); // 20000 x 30000
        let huge = dir.path().join("huge.jpg");
        std::fs::write(&huge, &bytes).unwrap();

        assert!(
            decode_scaled(&huge, 512).is_none(),
            "6億画素を名乗るJPEGを受け入れてしまった"
        );
    }

    /// 頭から壊れているファイルでもパニックしない
    #[test]
    fn survives_garbage_with_jpeg_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.jpg");
        std::fs::write(&path, vec![0xFFu8; 4096]).unwrap();
        assert!(decode_scaled(&path, 512).is_none());
    }

    /// mozjpegで詰めたJPEGが、そのまま読み戻せること（寸法・色が化けていない）
    #[test]
    fn encodes_rgb_that_decodes_back() {
        let rgb = image::RgbImage::from_fn(320, 240, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 253) as u8])
        });
        let bytes = encode_rgb(&rgb, 82, ChromaSampling::Half).expect("詰められるはず");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEGのSOIが無い");
        let back = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (320, 240));
        let mean: f64 = back
            .as_raw()
            .iter()
            .zip(rgb.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as f64)
            .sum::<f64>()
            / back.as_raw().len() as f64;
        assert!(mean < 24.0, "元の絵と違いすぎる: 平均差 {mean}");
    }

    /// 詰めたJPEGの間引きを、ヘッダから読み戻せる。
    ///
    /// **書いた側と読む側を突き合わせる**形にしてある。`chroma_of` は
    /// RAWの埋め込みプレビューをどこまで間引いてよいかの判断に使うので、
    /// ここが逆さまだと**間引き済みの絵をもう一度間引く**（色の境目が崩れる）
    #[test]
    fn chroma_of_reads_back_what_we_wrote() {
        let rgb = image::RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([128, (x % 256) as u8, (y % 256) as u8])
        });
        for want in [ChromaSampling::Full, ChromaSampling::Half] {
            let bytes = encode_rgb(&rgb, 82, want).unwrap();
            assert_eq!(chroma_of(&bytes), Some(want), "{want:?} を読み戻せない");
        }
    }

    /// JPEGでないものを渡されても答えを作らない（間引かない側へ倒すため）
    #[test]
    fn chroma_of_says_nothing_about_non_jpeg() {
        assert_eq!(chroma_of(&[]), None);
        assert_eq!(chroma_of(b"not a jpeg at all"), None);
        // SOIだけあって SOF が来ないまま走査データに入る
        assert_eq!(chroma_of(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02]), None);
    }

    /// 4:2:2（輝度だけ横に2倍）は**間引かない側**と答える。
    ///
    /// Canonの一部がこれで書く。`0x21` を「2が付いているから4:2:0」と
    /// 読むと、色差を横半分に潰したうえで縦にも潰すことになる
    #[test]
    fn chroma_of_treats_422_as_full() {
        // precision(1) height(2) width(2) 成分数(1) ＋ 成分3つ
        let body = [
            8, 0, 48, 0, 64, 3, //
            1, 0x21, 0, //
            2, 0x11, 0, //
            3, 0x11, 0,
        ];
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xC0];
        jpeg.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
        jpeg.extend_from_slice(&body);
        assert_eq!(chroma_of(&jpeg), Some(ChromaSampling::Full));
    }

    /// 品質を上げれば大きくなる（set_quality が効いている＝
    /// set_fastest_defaults の後に呼べている）
    #[test]
    fn quality_reaches_the_encoder() {
        let rgb = image::RgbImage::from_fn(256, 256, |x, y| {
            image::Rgb([(x * y % 256) as u8, (x % 256) as u8, (y % 256) as u8])
        });
        let low = encode_rgb(&rgb, 40, ChromaSampling::Half).unwrap().len();
        let high = encode_rgb(&rgb, 95, ChromaSampling::Half).unwrap().len();
        assert!(high > low, "品質が効いていない: q40={low} q95={high}");
    }

    /// 色差の間引きが効いている（4:4:4 のほうが大きい）。
    ///
    /// `set_fastest_defaults` の**後**に設定できていないと、libjpeg v6 の既定
    /// （4:2:0）のまま両方が同じ大きさになる——エンコーダを比べるときに、
    /// これが黙って揃っていないと数字の意味が変わる
    #[test]
    fn chroma_sampling_reaches_the_encoder() {
        // 色差だけが動く絵（輝度をほぼ一定に保つと間引きの差が出やすい）
        let rgb = image::RgbImage::from_fn(256, 256, |x, y| {
            image::Rgb([128, (x % 256) as u8, (y % 256) as u8])
        });
        let full = encode_rgb(&rgb, 82, ChromaSampling::Full).unwrap().len();
        let half = encode_rgb(&rgb, 82, ChromaSampling::Half).unwrap().len();
        assert!(
            full > half,
            "間引きが効いていない: 4:4:4={full} 4:2:0={half}"
        );
    }

    /// 0画素は libjpeg へ渡す前に断る（渡すとlongjmpで飛んでくる）
    #[test]
    fn refuses_empty_images() {
        assert!(encode_rgb(&image::RgbImage::new(0, 0), 82, ChromaSampling::Half).is_none());
    }

    /// 間引いた絵が、原寸から縮めた絵とだいたい一致すること（色が化けていない）
    #[test]
    fn scaled_decode_matches_full_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_jpeg(dir.path(), "cmp.jpg", 1024, 768);
        let scaled = decode_scaled(&path, 128).unwrap();
        let full = image::open(&path).unwrap();
        let reference = crate::resize::lanczos3(&full, scaled.width(), scaled.height()).to_rgb8();
        let ours = scaled.to_rgb8();
        let mean: f64 = ours
            .as_raw()
            .iter()
            .zip(reference.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as f64)
            .sum::<f64>()
            / ours.as_raw().len() as f64;
        assert!(mean < 12.0, "原寸から縮めた絵と違いすぎる: 平均差 {mean}");
    }
}
