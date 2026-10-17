//! 画像をクリップボードへ置く（Windows。pictkura-dev #47）。
//!
//! arboard の `set_image` は、**クリップボードを持ち主の窓なし（`OpenClipboard(NULL)`）で開き、空にしてから**
//! PNG への圧縮と DIB の組み立てをする。45MP の RAW では開いている間が約0.5秒（release、win の実測 10-17）で、
//! そのあいだにクリップボードをこまめに読むアプリが居ると、こちらの `SetClipboardData` が
//! 「スレッドはクリップボードを開いていません (os error 1418)」で落ちた（5回中5回）。Microsoft の説明でも、
//! 持ち主を NULL にして空にすると `SetClipboardData` は失敗しうる。
//!
//! だからここでは、**PNG と DIB を開く前に作り終え**、**自分の窓を持ち主にして**開き、開いている間は置くだけにする。
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
/// 開けなかったときに試し直す回数と間隔。arboard（Chromium と同じ）は 5回×5ms。こちらは開いている時間が短いぶん、
/// 他のアプリが開いている間を待てるようにもう少し待つ
const OPEN_ATTEMPTS: usize = 10;
const OPEN_RETRY_MS: u64 = 10;

/// 置く前の確保。置けたら持ち主はクリップボードに移るので、`release` で手放す。置けずに落ちたら自分で解放する
struct Global {
    h: HGLOBAL,
    owned: bool,
}

impl Global {
    fn from_bytes(parts: &[&[u8]]) -> Result<Self, String> {
        let len: usize = parts.iter().map(|p| p.len()).sum();
        let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, len) }.map_err(|e| e.to_string())?;
        let me = Global { h, owned: true };
        let dst = unsafe { GlobalLock(h) } as *mut u8;
        if dst.is_null() {
            return Err(windows::core::Error::from_win32().to_string());
        }
        let mut at = 0;
        for p in parts {
            unsafe { std::ptr::copy_nonoverlapping(p.as_ptr(), dst.add(at), p.len()) };
            at += p.len();
        }
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
            Err(e) => last = e.to_string(),
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
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// `CF_DIBV5` の中身（見出し＋画素）。画素は BGRA の**下から上**——高さを負にすれば上からでよいが、
/// Word とワードパッドはそれを貼れない（arboard の注記）
fn dibv5_parts(img: &RgbaImage) -> (BITMAPV5HEADER, Vec<u8>) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let header = BITMAPV5HEADER {
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
    };
    let src = img.as_raw();
    let row = 4 * w;
    let mut pixels = vec![0u8; row * h];
    for y in 0..h {
        let from = &src[y * row..(y + 1) * row];
        let to = &mut pixels[(h - 1 - y) * row..(h - y) * row];
        for (s, d) in from.chunks_exact(4).zip(to.chunks_exact_mut(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
    }
    (header, pixels)
}

/// 画像を PNG と `CF_DIBV5` で置く。**重い仕事は全部ここで先に済ませ**、開いている間は置くだけ
pub fn set_image(owner: HWND, img: &RgbaImage) -> Result<(), String> {
    let mut png = Global::from_bytes(&[&png_bytes(img)?])?;
    let (header, pixels) = dibv5_parts(img);
    let header_bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const BITMAPV5HEADER).cast::<u8>(),
            std::mem::size_of::<BITMAPV5HEADER>(),
        )
    };
    let mut dib = Global::from_bytes(&[header_bytes, &pixels])?;
    drop(pixels);
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
        let (header, px) = dibv5_parts(&img);
        assert_eq!(
            (header.bV5Width, header.bV5Height, header.bV5SizeImage),
            (2, 2, 16)
        );
        // 先頭は下の行（青・白）、次が上の行（赤・緑）。並びは BGRA
        assert_eq!(
            px,
            vec![255, 0, 0, 255, 255, 255, 255, 128, 0, 0, 255, 255, 0, 255, 0, 255]
        );
    }
}
