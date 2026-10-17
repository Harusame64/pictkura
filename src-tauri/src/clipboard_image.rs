//! 画像をクリップボードへ置く（Windows。pictkura-dev #47）。
//!
//! arboard の `set_image` は、**持ち主の窓なし（`OpenClipboard(NULL)`）で開いて空にしてから** PNG への圧縮と
//! DIB の組み立てをする（45MP の RAW で約0.5秒、release）。持ち主 NULL で開く者どうしは互いを締め出さないので、
//! その間に同じく NULL で開いて読むアプリが入り込み、こちらの `SetClipboardData` が「スレッドはクリップボードを
//! 開いていません (os error 1418)」で落ちた（win の実測 10-17、10回中10回）。
//!
//! だからここでは、**PNG と DIB を開く前に作り終え**、**主窓を持ち主にして**開く（普通のアプリと同じ作法）。
//! 開いている間は空にして置くだけ（約0.06秒）で、その間は誰も入り込めない。持ち主だけ NULL にした版も
//! 10回中10回通ったが、それは割り込める間が短くなっただけで、締め出してはいない——だから窓を渡す。
//! 主窓が持ち主だと、次に誰かが空にするとき主スレッドへ知らせ（`WM_DESTROYCLIPBOARD`）が届くが、続けて2回
//! コピーしても開いている間は 0.09 → 0.11 秒だった（同じ実測）。
//! 置く形式と順番は arboard と同じ（PNG が先——先頭の形式を取る貼り先が多く、PNG の方が通りがよい）。

use image::RgbaImage;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{BITMAPV5HEADER, BI_BITFIELDS};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_DIBV5`。定数は `Win32_System_Ole` に居るが、その機能を足すと大きなモジュールを丸ごと組むことになる
const CF_DIBV5: u32 = 17;
/// `LCS_sRGB`（windows クレートに無い）
const LCS_SRGB: u32 = 0x7352_4742;
/// `LCS_GM_IMAGES`
const LCS_GM_IMAGES: i32 = 4;
/// 開けなかったときに試し直す回数と間隔（合わせて約0.5秒）。arboard（Chromium と同じ）は 5回×5ms。
/// 大きな絵を置いた直後は、履歴のアプリ等が 180MB の DIB を読み終えるまで開けないことがある（ゲート2）——
/// こちらは開いた後が短いので、その前で待つ側に回る
const OPEN_ATTEMPTS: usize = 25;
const OPEN_RETRY_MS: u64 = 20;

/// 置く前の確保。置けたら持ち主はクリップボードに移るので、`release` で手放す。置けずに落ちたら自分で解放する
struct Global {
    h: HGLOBAL,
    owned: bool,
}

impl Global {
    /// `len` バイトを確保し、`fill` で**確保した場所へ直に**書く。間に1枚写しを挟むと、45MP で 180MB 余分に持つ
    fn filled(len: usize, fill: impl FnOnce(&mut [u8])) -> Result<Self, String> {
        let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, len) }.map_err(|e| e.to_string())?;
        let me = Global { h, owned: true };
        let dst = unsafe { GlobalLock(h) } as *mut u8;
        if dst.is_null() {
            return Err(windows::core::Error::from_win32().to_string());
        }
        fill(unsafe { std::slice::from_raw_parts_mut(dst, len) });
        // 最後の錠を外すと「錠が無い」で Err が返る（成功の形）。中身は書き終えているので見ない
        let _ = unsafe { GlobalUnlock(h) };
        Ok(me)
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.h.0)
    }

    fn release(&mut self) {
        self.owned = false;
    }
}

impl Drop for Global {
    fn drop(&mut self) {
        if self.owned {
            let _ = unsafe { GlobalFree(Some(self.h)) };
        }
    }
}

/// 開いている間だけ生きる。どの道で抜けても閉じる
struct Opened;

impl Drop for Opened {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn open(owner: HWND) -> Result<Opened, String> {
    let mut last = String::new();
    for i in 0..OPEN_ATTEMPTS {
        match unsafe { OpenClipboard(Some(owner)) } {
            Ok(()) => return Ok(Opened),
            // ほかのアプリが開いているとき、OS は理由を返さないことがある（最後のエラーが 0 で「成功しました」になる）。
            // 記録と画面に「成功」と出さないよう、何が起きたかをこちらで言う（ゲート2）
            Err(e) => {
                last = format!(
                    "the clipboard is in use by another app (tried for {} ms): {e}",
                    OPEN_ATTEMPTS as u64 * OPEN_RETRY_MS
                )
            }
        }
        if i + 1 < OPEN_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(OPEN_RETRY_MS));
        }
    }
    Err(last)
}

fn png_bytes(img: &RgbaImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    // 圧縮は**速い側に固定する**。クリップボードを渡るだけの PNG なので縮める意味が薄い。image 0.25.10 の既定も
    // Fast だが、既定が変わったときに 45MP の待ちが黙って延びないよう名指しする
    image::codecs::png::PngEncoder::new_with_quality(
        &mut buf,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// `CF_DIBV5` の見出し
fn dibv5_header(w: usize, h: usize) -> BITMAPV5HEADER {
    BITMAPV5HEADER {
        bV5Size: std::mem::size_of::<BITMAPV5HEADER>() as u32,
        bV5Width: w as i32,
        bV5Height: h as i32,
        bV5Planes: 1,
        bV5BitCount: 32,
        bV5Compression: BI_BITFIELDS,
        bV5SizeImage: (4 * w * h) as u32,
        bV5RedMask: 0x00ff_0000,
        bV5GreenMask: 0x0000_ff00,
        bV5BlueMask: 0x0000_00ff,
        bV5AlphaMask: 0xff00_0000,
        bV5CSType: LCS_SRGB,
        bV5Intent: LCS_GM_IMAGES as u32,
        ..Default::default()
    }
}

/// `CF_DIBV5` の画素を `out` へ書く。BGRA の**下から上**——高さを負にすれば上からでよいが、Word とワードパッドは
/// それを貼れない（arboard の注記）
fn write_dib_pixels(img: &RgbaImage, out: &mut [u8]) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let src = img.as_raw();
    let row = 4 * w;
    for y in 0..h {
        let from = &src[y * row..(y + 1) * row];
        let to = &mut out[(h - 1 - y) * row..(h - y) * row];
        for (s, d) in from
            .as_chunks::<4>()
            .0
            .iter()
            .zip(to.as_chunks_mut::<4>().0)
        {
            *d = [s[2], s[1], s[0], s[3]];
        }
    }
}

/// 画像を PNG と `CF_DIBV5` で置く。**重い仕事は全部ここで先に済ませ**、開いている間は置くだけ
pub fn set_image(owner: HWND, img: &RgbaImage) -> Result<(), String> {
    let png_bytes = png_bytes(img)?;
    let mut png = Global::filled(png_bytes.len(), |out| out.copy_from_slice(&png_bytes))?;
    drop(png_bytes);
    let (w, h) = (img.width() as usize, img.height() as usize);
    let header = dibv5_header(w, h);
    let header_len = std::mem::size_of::<BITMAPV5HEADER>();
    let mut dib = Global::filled(header_len + 4 * w * h, |out| {
        let (head, pixels) = out.split_at_mut(header_len);
        head.copy_from_slice(unsafe {
            std::slice::from_raw_parts((&header as *const BITMAPV5HEADER).cast::<u8>(), header_len)
        });
        write_dib_pixels(img, pixels);
    })?;
    let png_format = unsafe { RegisterClipboardFormatW(windows::core::w!("PNG")) };
    if png_format == 0 {
        return Err(windows::core::Error::from_win32().to_string());
    }

    let _opened = open(owner)?;
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;
    unsafe { SetClipboardData(png_format, Some(png.handle())) }.map_err(|e| e.to_string())?;
    png.release();
    unsafe { SetClipboardData(CF_DIBV5, Some(dib.handle())) }.map_err(|e| e.to_string())?;
    dib.release();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_rows_are_bottom_up_bgra() {
        // 2×2: 上の行 赤・緑、下の行 青・半透明の白
        let img = RgbaImage::from_raw(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
            ],
        )
        .unwrap();
        let header = dibv5_header(2, 2);
        assert_eq!(
            (header.bV5Width, header.bV5Height, header.bV5SizeImage),
            (2, 2, 16)
        );
        let mut px = vec![0u8; 16];
        write_dib_pixels(&img, &mut px);
        // 先頭は下の行（青・白）、次が上の行（赤・緑）。並びは BGRA
        assert_eq!(
            px,
            vec![255, 0, 0, 255, 255, 255, 255, 128, 0, 0, 255, 255, 0, 255, 0, 255]
        );
    }
}
