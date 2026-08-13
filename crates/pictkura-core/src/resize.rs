//! 縮小（リサイズ）。サムネイル生成で一番CPUを食う部分。
//!
//! `image` クレートの `imageops` は素のスカラー実装で、12MP を 512px へ
//! 落とすだけで数百ms かかる。ここでは同じ結果を狙いつつ、SIMD
//! （`fast_image_resize`。SSE4.1/AVX2/NEON を**実行時に**選ぶ）で実装する。
//!
//! 実行時に選ぶのが要点で、ビルド時に `-C target-cpu` を上げると
//! 古いCPUで起動できないバイナリになる。配布物は素の x86-64 のまま、
//! 速い道は動いているCPUを見てから選ぶ。

use fast_image_resize::images::{Image as FirImage, ImageRef as FirImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::DynamicImage;

/// 長辺が `max_edge` に収まる縮小後の寸法（拡大はしないので、
/// 元がすでに小さければそのままの寸法を返す）。
///
/// `image` クレートの `resize` と同じ丸め方をする（縦横比を保ち、
/// はみ出さない側に倒す）。
pub fn fit_within(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    if width.max(height) <= max_edge || width == 0 || height == 0 {
        return (width, height);
    }
    let ratio = (max_edge as f64 / width as f64).min(max_edge as f64 / height as f64);
    let w = ((width as f64 * ratio).round() as u32).max(1);
    let h = ((height as f64 * ratio).round() as u32).max(1);
    (w, h)
}

/// SIMDのLanczos3で縮小する。
///
/// `image` の `resize(.., Lanczos3)` の置き換え。中身は
/// 「元の画素を出力画素の面積へ重み付きで畳み込む」同じ計算で、
/// 係数を整数化して16画素ずつまとめて掛ける点だけが違う。
///
/// 対応していない画素形式（16bit等）は `image` 側の実装へ戻す。
pub fn lanczos3(img: &DynamicImage, dst_w: u32, dst_h: u32) -> DynamicImage {
    resize_with(
        img,
        dst_w,
        dst_h,
        ResizeAlg::Convolution(FilterType::Lanczos3),
    )
    .unwrap_or_else(|| img.resize_exact(dst_w, dst_h, image::imageops::FilterType::Lanczos3))
}

/// SIMDの面積平均で縮小する（`DynamicImage::thumbnail` の置き換え）。
///
/// 大きく落とすときはこちらが速く、モアレも出にくい。仕上げの
/// Lanczos3 の前段（2倍サイズへの荒落とし）に使う。
pub fn box_filter(img: &DynamicImage, dst_w: u32, dst_h: u32) -> DynamicImage {
    resize_with(img, dst_w, dst_h, ResizeAlg::Convolution(FilterType::Box))
        .unwrap_or_else(|| img.thumbnail_exact(dst_w, dst_h))
}

/// 透過を白へ重ねて、不透明のRGBにする。
///
/// サムネイルは最後にRGBへ落とすが、`to_rgb8()` は alpha を**捨てるだけ**で
/// 重ねてはくれない。透明部分にどんなRGBが残るかは元ファイル次第になる。
///
/// **ここを省くと透明PNGのサムネイルが真っ黒になる**。SIMDの縮小は
/// 「色にalphaを掛けてから縮め、後で割り戻す」という正しい手順を踏むので、
/// 完全に透明な画素は色が0（＝黒）で戻ってくる。掛けずに縮める実装
///（`image` の imageops）では元のRGBがそのまま残るため、差が出る。
///
/// 一覧のタイルは背景が白なので、市松模様を敷くより白へ重ねる方が素直
///（AVIFの `av1::composite_over_white` と同じ考え方）。
pub fn flatten_onto_white(img: &DynamicImage) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        let a = u32::from(src.0[3]);
        let over = |c: u8| -> u8 {
            // c*a/255 + 255*(255-a)/255 を1回の割り算で。+127 は四捨五入
            ((u32::from(c) * a + 255 * (255 - a) + 127) / 255).min(255) as u8
        };
        *dst = image::Rgb([over(src.0[0]), over(src.0[1]), over(src.0[2])]);
    }
    out
}

/// 長辺が `max_edge` に収まるまで面積平均で落とす（拡大はしない）。
///
/// `DynamicImage::thumbnail` の置き換え。すでに収まっていれば何もしない。
pub fn shrink_to_fit(img: &DynamicImage, max_edge: u32) -> DynamicImage {
    if img.width().max(img.height()) <= max_edge {
        return img.clone();
    }
    let (w, h) = fit_within(img.width(), img.height(), max_edge);
    box_filter(img, w, h)
}

/// 実際に `fast_image_resize` を呼ぶ部分。
///
/// 8bitのグレー/グレー+A/RGB/RGBAだけを受ける。それ以外（16bit・f32）は
/// `None` を返して呼び出し側のフォールバックに任せる——ここで
/// RGB8へ落として渡すと、返り値の形式が入力と変わってしまう。
fn resize_with(img: &DynamicImage, dst_w: u32, dst_h: u32, alg: ResizeAlg) -> Option<DynamicImage> {
    if dst_w == 0 || dst_h == 0 {
        return None;
    }
    let (pixel_type, src_buf): (PixelType, &[u8]) = match img {
        DynamicImage::ImageLuma8(b) => (PixelType::U8, b.as_raw()),
        DynamicImage::ImageLumaA8(b) => (PixelType::U8x2, b.as_raw()),
        DynamicImage::ImageRgb8(b) => (PixelType::U8x3, b.as_raw()),
        DynamicImage::ImageRgba8(b) => (PixelType::U8x4, b.as_raw()),
        _ => return None,
    };
    let src = FirImageRef::new(img.width(), img.height(), src_buf, pixel_type).ok()?;
    let mut dst = FirImage::new(dst_w, dst_h, pixel_type);
    // 透過は「色へalphaを掛けてから縮小し、後で割り戻す」のが正しい。
    // 掛けずに縮小すると、透明部分の色（多くは黒や白）が縁ににじむ。
    // fast_image_resize は既定でこれをやるので、そのまま任せる
    Resizer::new()
        .resize(&src, &mut dst, &ResizeOptions::new().resize_alg(alg))
        .ok()?;
    let buf = dst.into_vec();
    Some(match pixel_type {
        PixelType::U8 => DynamicImage::ImageLuma8(image::ImageBuffer::from_raw(dst_w, dst_h, buf)?),
        PixelType::U8x2 => {
            DynamicImage::ImageLumaA8(image::ImageBuffer::from_raw(dst_w, dst_h, buf)?)
        }
        PixelType::U8x3 => {
            DynamicImage::ImageRgb8(image::ImageBuffer::from_raw(dst_w, dst_h, buf)?)
        }
        PixelType::U8x4 => {
            DynamicImage::ImageRgba8(image::ImageBuffer::from_raw(dst_w, dst_h, buf)?)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        }))
    }

    #[test]
    fn fit_within_keeps_aspect_and_never_upscales() {
        assert_eq!(fit_within(4000, 3000, 512), (512, 384));
        assert_eq!(fit_within(3000, 4000, 512), (384, 512));
        // 元が小さければそのまま（拡大はしない）
        assert_eq!(fit_within(100, 80, 512), (100, 80));
        // 極端に細長くても0にはしない
        assert_eq!(fit_within(10000, 3, 512), (512, 1));
    }

    #[test]
    fn lanczos3_matches_requested_size_and_keeps_format() {
        let out = lanczos3(&gradient(800, 600), 200, 150);
        assert_eq!((out.width(), out.height()), (200, 150));
        assert!(matches!(out, DynamicImage::ImageRgb8(_)));
    }

    #[test]
    fn flatten_puts_transparent_areas_on_white() {
        let src = DynamicImage::ImageRgba8(image::ImageBuffer::from_fn(4, 1, |x, _| match x {
            0 => image::Rgba([255, 0, 0, 255]),   // 不透明の赤
            1 => image::Rgba([0, 0, 0, 0]),       // 完全に透明（色は黒）
            2 => image::Rgba([255, 255, 255, 0]), // 完全に透明（色は白）
            _ => image::Rgba([0, 0, 0, 128]),     // 半透明の黒
        }));
        let out = flatten_onto_white(&src);
        assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0], "不透明はそのまま");
        assert_eq!(out.get_pixel(1, 0).0, [255, 255, 255], "透明は白");
        assert_eq!(out.get_pixel(2, 0).0, [255, 255, 255], "透明は色によらず白");
        let half = out.get_pixel(3, 0).0;
        assert!(
            (120..=135).contains(&half[0]),
            "半透明の黒は灰色になるはず: {half:?}"
        );
    }

    /// alphaの無い絵は素通し（余計な変換をしない）
    #[test]
    fn flatten_passes_through_opaque_images() {
        let src = gradient(8, 8);
        assert_eq!(flatten_onto_white(&src), src.to_rgb8());
    }

    /// SIMDで縮めた透明PNGが黒くならないこと（この経路の回帰そのもの）
    #[test]
    fn shrinking_a_transparent_image_stays_white() {
        let src = DynamicImage::ImageRgba8(image::ImageBuffer::from_fn(256, 256, |x, _| {
            if x < 128 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 0])
            }
        }));
        let small = lanczos3(&src, 64, 64);
        let out = flatten_onto_white(&small);
        assert_eq!(out.get_pixel(63, 32).0, [255, 255, 255], "透明側が白でない");
    }

    #[test]
    fn shrink_to_fit_only_shrinks() {
        let big = shrink_to_fit(&gradient(1000, 500), 200);
        assert_eq!((big.width(), big.height()), (200, 100));
        // すでに収まっていれば寸法は変わらない
        let small = shrink_to_fit(&gradient(100, 50), 200);
        assert_eq!((small.width(), small.height()), (100, 50));
    }

    #[test]
    fn box_filter_matches_requested_size() {
        let out = box_filter(&gradient(800, 600), 100, 75);
        assert_eq!((out.width(), out.height()), (100, 75));
    }

    /// 対応外の画素形式（16bit）でも、フォールバックで結果は返る
    #[test]
    fn falls_back_for_unsupported_pixel_type() {
        let src = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(64, 64, |x, y| {
            image::Rgb([(x * 512) as u16, (y * 512) as u16, 0])
        }));
        let out = lanczos3(&src, 16, 16);
        assert_eq!((out.width(), out.height()), (16, 16));
    }

    /// 縮小結果が「だいたい合っている」ことを、image クレートの実装との
    /// 平均差で確かめる（係数の整数化ぶんの誤差しか出ないはず）
    #[test]
    fn close_to_reference_implementation() {
        let src = gradient(640, 480);
        let ours = lanczos3(&src, 160, 120).to_rgb8();
        let reference = src
            .resize_exact(160, 120, image::imageops::FilterType::Lanczos3)
            .to_rgb8();
        let diff: u64 = ours
            .as_raw()
            .iter()
            .zip(reference.as_raw())
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let mean = diff as f64 / ours.as_raw().len() as f64;
        assert!(mean < 2.0, "平均差が大きすぎる: {mean}");
    }
}
