//! CoreGraphics の `CGImage` を image クレートの絵へ落とす（macOS共通）。
//!
//! ImageIO（[`crate::heif`]）も AVFoundation（[`crate::shell`]）も**出口が
//! `CGImage`** で、そこから先の詰め直しは同じ。`unsafe` を1か所に閉じるために
//! ここへ出してある。
//!
//! **CoreGraphicsのビットマップ文脈へ描く**のが要点。`CGImage` の画素形式と
//! 色空間は素材任せ（HDR PQ の10bitもある）だが、**sRGBの文脈へ描けば変換は
//! CoreGraphicsがやる**。WindowsのWICが `IWICFormatConverter` で 24bppBGR へ
//! 落としているのと同じ考え方で、**出口の形をこちら側で決める**。

use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext,
    CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
};

/// `CGImage` を **sRGBのRGB8** へ落とす（Windows側の `to_image` と同じ出口）。
///
/// CoreGraphicsのビットマップ文脈は**24bppを作れない**ので、いったん
/// RGBX（32bpp・アルファ捨て）で受けてから3バイトへ詰め直す。
pub(crate) fn to_image(image: &CGImage) -> Option<image::DynamicImage> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    if width == 0 || height == 0 {
        return None;
    }
    // 展開爆弾よけ。ここを抜けると width*height*4 を確保する
    const MAX_PIXELS: usize = 512 * 1024 * 1024 / 4;
    if width.checked_mul(height)? > MAX_PIXELS {
        return None;
    }
    let stride = width.checked_mul(4)?;
    let len = stride.checked_mul(height)?;
    let mut rgbx = vec![0u8; len];

    // SAFETY: `kCGColorSpaceSRGB` はCoreGraphicsが公開する定数の名前
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // `NoneSkipLast` + `Order32Big` で、メモリ上の並びが R,G,B,X になる。
    // アルファは捨てる（Windows側も24bppBGRへ落としていて、出口を揃える）
    let bitmap_info = CGImageAlphaInfo::NoneSkipLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: `rgbx` は width*4*height バイトで、文脈より長生きする
    let context = unsafe {
        CGBitmapContextCreate(
            rgbx.as_mut_ptr().cast(),
            width,
            height,
            8,
            stride,
            Some(&space),
            bitmap_info,
        )
    }?;

    let rect = CGRect::new(
        CGPoint::new(0.0, 0.0),
        CGSize::new(width as f64, height as f64),
    );
    CGContext::draw_image(Some(&context), rect, Some(image));

    // 描き終わったことを確かめる。`CGBitmapContextGetData` が渡した先を
    // 指していなければ、CoreGraphics が別の裏付けを取ったということなので信じない
    if CGBitmapContextGetData(Some(&context)).cast::<u8>() != rgbx.as_mut_ptr() {
        return None;
    }
    drop(context);

    // RGBX → RGB（前へ詰める。確保し直さない）
    let mut out = 0usize;
    for px in 0..width * height {
        let src = px * 4;
        rgbx[out] = rgbx[src];
        rgbx[out + 1] = rgbx[src + 1];
        rgbx[out + 2] = rgbx[src + 2];
        out += 3;
    }
    rgbx.truncate(out);
    image::RgbImage::from_raw(width as u32, height as u32, rgbx).map(image::DynamicImage::ImageRgb8)
}
