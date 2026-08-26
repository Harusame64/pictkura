//! OSのサムネイル機構から絵を借りる（第9部）。
//!
//! 動画の1コマを取り出すのに、**自前のデコーダを持たない**ための道。
//! エクスプローラが動画・PDF・その他あらゆる形式のサムネイルを出せるのは
//! このShellの仕組みで、同じ入口をアプリからも叩ける。
//!
//! 嬉しい副作用が2つある:
//!
//! - **コーデックを問わない**。Shellが絵を出せる形式はそのまま扱える。
//!   WebViewが再生できない .m2ts や .avi でも一覧には絵が出るはず
//!   （**未確認**——手元の該当ファイルがクラウドのみで確かめられていない。
//!   HEVCの.MOVでは実測で取得できている）
//! - **クラウドのみのファイルを本体ごと落とさずに済む**見込み。
//!   エクスプローラがOneDriveの未ダウンロードファイルのサムネイルを
//!   出せるのはこの経路（plan.md 第7部の積み残し）
//!
//! Windows以外では常に `None` を返す。macOSはQuickLook、Linuxは
//! ディストリのサムネイラという別の入口になるので、後続の課題に置く。

use std::path::Path;

/// このOSでOSのサムネイル機構が使えるか。
///
/// **macOSは動画だけ**（AVFoundation）。Windowsのように「Shellが出せる形式は
/// なんでも」ではないので、真でも [`thumbnail`] が `None` を返す相手はある。
pub fn available() -> bool {
    cfg!(windows) || cfg!(target_os = "macos")
}

/// OSにサムネイルを作らせる。長辺が `max_edge` 程度の絵が返る。
///
/// 取れなければ `None`。**アイコンでの代用はしない**
/// ——一覧に汎用のフィルムアイコンが並ぶより、枠のままの方がまだ良い。
pub fn thumbnail(path: &Path, max_edge: u32) -> Option<image::DynamicImage> {
    #[cfg(windows)]
    {
        windows_shell::thumbnail(path, max_edge)
    }
    #[cfg(target_os = "macos")]
    {
        macos_av::thumbnail(path, max_edge)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (path, max_edge);
        None
    }
}

/// macOSの AVFoundation から動画の1コマを借りる。
///
/// **QuickLookではなくこちらを選んだ理由**（2026-08-26に両方を実測して決めた）:
///
/// - **同期で呼べる**。`copyCGImageAtTime` はその場で返るので、
///   完了ハンドラを待つ仕掛け（block・セマフォ・タイムアウト）が要らない。
///   呼び出し元（[`crate::thumbs`]）はブロッキングのワーカーの中に居るので、
///   非同期を畳む側の事故——**詰まったらキュー全体が止まる**——を作らずに済む
/// - **QuickLookは実際に詰まった**。`qlmanage -t` に `.avi` を食わせると
///   45秒経っても返らず、強制終了するしかなかった。しかも `.avi` は
///   Spotlightも何も返さないので、**QuickLookで拾えるはずだった相手が
///   まさに固まる相手**だった
/// - **回転が付いてくる**。`appliesPreferredTrackTransform` を立てれば、
///   縦位置で撮った動画がそのまま縦で返る（HEIFの `irot` で踏んだ
///   「デコーダが回したかどうか」の見分けが、こちらでは要らない）
///
/// `.m2ts` / `.avi` のようにAVFoundationが開けない相手は `None`。
/// そこをQuickLookで拾うかは、**固まる問題を解いてから**判断する。
#[cfg(target_os = "macos")]
mod macos_av {
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use objc2_av_foundation::{AVAssetImageGenerator, AVURLAsset};
    use objc2_core_foundation::CGSize;
    use objc2_core_media::CMTime;
    use objc2_foundation::NSURL;

    pub fn thumbnail(path: &Path, max_edge: u32) -> Option<image::DynamicImage> {
        // 動画以外はAVFoundationの仕事ではない。開けない相手に問い合わせて
        // 待たされるより、拡張子で先に断る
        if !crate::video::is_video_path(path) {
            return None;
        }
        // **UTF-8を前提にしない。** `NSString` 経由でURLを組むと、名前が
        // UTF-8として読めないファイルを取りこぼす。APFSは非UTF-8の名前を
        // 作らせない（実測: `EILSEQ`）が、**カメラのカードはFAT32/exFATで、
        // そちらは通す**（実測: `\x82\xa0.mp4`＝Shift_JISの「あ」が作れた）。
        // ファイルシステム表現をそのまま渡せば、綴りを解釈せずに済む。
        //
        // このOSのtempdirはAPFSなので、**単体テストでは再現できない**
        // ——FAT32のボリュームをマウントしないと踏めない
        let raw = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let ptr = std::ptr::NonNull::new(raw.as_ptr().cast_mut())?;
        // SAFETY: `raw` はこの関数のあいだ生きている NUL 終端のバイト列。
        // `NSURL` は中身を複製するので、返った後に参照されない
        let url = unsafe {
            NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(ptr, false, None)
        };
        // SAFETY: options に None を渡すだけ
        let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
        // SAFETY: 生きている asset を渡す
        let generator = unsafe { AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset) };
        unsafe {
            // 縦位置で撮った動画をそのまま縦で返させる
            generator.setAppliesPreferredTrackTransform(true);
            // 長辺の上限。原寸のフレームを起こさせない
            let edge = f64::from(max_edge.max(1));
            generator.setMaximumSize(CGSize::new(edge, edge));
        }
        // 先頭のコマ。**尺の途中を選んでいない**——黒からのフェードインだと
        // 真っ黒が返る。改善は実物を見てから（`bench --video` で並べる）
        // SAFETY: 秒とタイムスケールを渡すだけ（副作用のない値の組み立て）
        let at = unsafe { CMTime::with_seconds(0.0, 600) };
        // `copyCGImageAtTime` は非推奨（Appleは非同期版へ寄せたい）。
        // **同期であること自体がここでの価値**なので、承知で使う。
        // 非同期版に替えるなら、上のdocに書いた仕掛けが丸ごと要る
        #[allow(deprecated)]
        // SAFETY: `actual_time` は要らないのでnullを渡す（宣言が許している）
        let image =
            unsafe { generator.copyCGImageAtTime_actualTime_error(at, std::ptr::null_mut()) }
                .ok()?;
        crate::macos_cg::to_image(&image)
    }
}

/// OSのプロパティから読めた素性。読めなかった項目は 0 / `None` のまま残る。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Meta {
    pub width: u32,
    pub height: u32,
    pub taken_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
}

impl Meta {
    /// 1項目も読めなかったか。呼び出し側が「DBを書き換える価値があるか」を決める用。
    pub fn is_empty(&self) -> bool {
        self.width == 0
            && self.height == 0
            && self.taken_at_ms.is_none()
            && self.duration_ms.is_none()
    }
}

/// OSに「このファイルの素性」を聞く（寸法・撮影日時・長さ）。
///
/// 自前のコンテナ読み（[`crate::video`] / [`crate::heif`] / EXIF）が使えない場面の保険で、
/// 塞げる穴が2つある:
///
/// - **実体がクラウドにしか無いファイル**。自前で読むにはファイルを開くしかなく、
///   開けば実体のダウンロードが始まる。Shellは同期クライアントが持っている
///   メタだけを返せるので、**1バイトも落とさずに寸法と撮影日時が埋まる**。
///   これが無いと一覧の並びが mtime に落ちる（実測: 動画239本のうち146本、
///   HEIC 3322枚のうち94%がクラウドのみ）
/// - **自前で読めないコンテナ**。`.m2ts` は MPEG-TS、`.avi` は RIFF で、
///   どちらもISO-BMFFではないので `video::read_info` が何も返せない。
///   Shellは長さも寸法も返す
///
/// **ハイドレートしないと言える根拠**（実測 2026-08-13）: クラウドのみのファイルでは
/// **長さだけが返らない**。長さはコンテナを解釈するプロパティハンドラしか出せないので、
/// これは「ハンドラが動いていない＝ファイルが開かれていない」ことを意味する。
/// 返ってくるのは同期クライアントが置いた分だけで、問い合わせの前後で
/// ファイル属性も変わらなかった（`bench --shell-meta` で確認できる）。
/// なお `GPS_FASTPROPERTIESONLY` は**ディスクを一切読まない**指定で、
/// 実測ではどのファイルでも何も返さなかった。安全側に倒すつもりで指定すると
/// 機能ごと消えるので使わない。
///
/// **撮影日時の時差について**: Shellが返すのはUTCの `FILETIME` で、
/// EXIFのように時差を持たない日時は**読んだ機械の時間帯**で換算されている。
/// pictkura が自前でEXIFを読むときと同じ解釈なので食い違わないが、
/// 時間帯をまたいで持ち歩くと値が動きうる。**自前で読めるなら自前を優先する**。
///
/// 取れなければ `None`。Windows以外では常に `None`。
pub fn metadata(path: &Path) -> Option<Meta> {
    #[cfg(windows)]
    {
        windows_shell::metadata(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// プロパティを片端から列挙して `(正式名, 値)` で返す（調査用）。
///
/// 「Shellは持っているのに pictkura が取れていない」を切り分けるための道具。
/// どのキーに入っているかは提供元次第で、実体のあるファイルと
/// クラウドのみのファイルで**別のキーに入る**ことが実際にあった。
pub fn dump_properties(path: &Path) -> Vec<(String, String)> {
    #[cfg(windows)]
    {
        windows_shell::dump_properties(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Vec::new()
    }
}

/// WindowsのShell（`IShellItemImageFactory` と `IPropertyStore`）から借りる。
#[cfg(windows)]
mod windows_shell {
    use std::path::Path;

    use windows::core::{GUID, HSTRING};
    use windows::Win32::Foundation::{FILETIME, PROPERTYKEY, SIZE};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    };
    use windows::Win32::System::Com::StructuredStorage::{
        PropVariantClear, PropVariantToFileTime, PropVariantToStringAlloc, PropVariantToUInt32,
        PropVariantToUInt64,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
    use windows::Win32::System::Variant::PSTF_UTC;
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IPropertyStore, PSGetNameFromPropertyKey, SHGetPropertyStoreFromParsingName, GPS_DEFAULT,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY,
    };

    /// `canonicalize` が付ける拡張長プレフィックス（`\\?\`）。
    /// エスケープを重ねると読めなくなるので定数に逃がす
    const VERBATIM: &str = r"\\?\";
    /// ネットワークパス版（`\\?\UNC\`）
    const VERBATIM_UNC: &str = r"\\?\UNC\";

    thread_local! {
        /// COMはスレッドごとに初期化が要る。サムネイルワーカーは複数走るので
        /// スレッドローカルに1回だけ済ませる（heif側のWICと同じ作り）
        static COM_READY: bool = init_com();
    }

    fn init_com() -> bool {
        // 既に別のモード（STA）で初期化済みなら RPC_E_CHANGED_MODE が返るが、
        // COM自体は使えるので成功扱いにする
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() }
    }

    /// Shellに渡せる形の絶対パスへ直す。
    ///
    /// 相対パスだと `SHCreateItemFromParsingName` が現在のディレクトリを見るので、
    /// ワーカースレッドの作業ディレクトリに依存させないため絶対パスへ直す。ただし
    /// **canonicalize が付ける拡張長プレフィックスは Shell が解釈できない**
    /// （名前空間の指定と見なされ、0x80070057 で弾かれる）ので剥がす。
    ///
    /// `canonicalize` はハンドルを開くが**データを読まない**ので、実体がクラウドに
    /// しか無いファイルでもダウンロードは起きない（ハイドレートの引き金は
    /// データへの読み書きであって、ハンドルを開くことではない）。
    fn shell_path(path: &Path) -> Option<String> {
        let full = std::fs::canonicalize(path).ok()?;
        let text = full.as_os_str().to_string_lossy();
        Some(match text.strip_prefix(VERBATIM_UNC) {
            // ネットワークパスは UNC 形式（先頭が二重の区切り）へ戻す
            Some(rest) => format!(r"\\{rest}"),
            None => text.strip_prefix(VERBATIM).unwrap_or(&text).to_string(),
        })
    }

    pub fn thumbnail(path: &Path, max_edge: u32) -> Option<image::DynamicImage> {
        COM_READY.with(|_| ());
        let text = shell_path(path)?;
        let edge = i32::try_from(max_edge.clamp(1, 4096)).ok()?;

        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(text.as_str()), None).ok()?;
            // THUMBNAILONLY: 絵が無いときに汎用アイコンで代用させない
            let bitmap = factory
                .GetImage(SIZE { cx: edge, cy: edge }, SIIGBF_THUMBNAILONLY)
                .ok()?;
            let image = bitmap_to_image(bitmap);
            // GetImage が返した HBITMAP は呼び出し側が解放する
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            image
        }
    }

    /// `HBITMAP` の画素を取り出してRGB画像にする。
    ///
    /// Shellが返すのは32bitのBGRAで、**透過が入っていることがある**
    /// （動画のコマは不透明だが、形式によっては縁が抜けている）。
    /// 一覧の背景は白なので白へ重ねる。
    unsafe fn bitmap_to_image(bitmap: HBITMAP) -> Option<image::DynamicImage> {
        let mut info = BITMAP::default();
        let written = GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(std::ptr::from_mut(&mut info).cast()),
        );
        if written == 0 || info.bmWidth <= 0 || info.bmHeight <= 0 {
            return None;
        }
        let width = u32::try_from(info.bmWidth).ok()?;
        let height = u32::try_from(info.bmHeight).ok()?;
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        let mut buf = vec![0u8; len];

        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: info.bmWidth,
                // 負の高さ＝上から下へ並べる。正のままだと絵が上下逆さまになる
                biHeight: -info.bmHeight,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let rows = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(buf.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        if rows == 0 {
            return None;
        }

        // BGRA → RGB。
        //
        // **罠1**: Shellのサムネイル提供元によっては、alphaを**一切書かない**
        // （GDIで描いただけのビットマップは alpha が0のまま残る）。それを
        // 素直に信じると全画素が「完全に透明」扱いになり、白へ重ねた結果
        // **サムネイルが真っ白になる**。しかもこれは高品質（state=2）として
        // 保存されるので、一度出ると戻らない。
        // alphaが全部0なら「書いていない」と見なして不透明として扱う。
        //
        // **罠2**: `GetImage` が返すのは**alphaを掛け済み**のビットマップ。
        // 掛け直すと半透明の縁だけ二重に薄くなるので、白を足すだけにする
        let opaque = buf.chunks_exact(4).all(|px| px[3] == 0);
        let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
        for px in buf.chunks_exact(4) {
            if opaque {
                rgb.extend_from_slice(&[px[2], px[1], px[0]]);
                continue;
            }
            let a = u32::from(px[3]);
            // 掛け済みなので、残りの割合ぶんの白を足すだけ
            let over = |c: u8| -> u8 { (u32::from(c) + (255 - a)).min(255) as u8 };
            rgb.push(over(px[2]));
            rgb.push(over(px[1]));
            rgb.push(over(px[0]));
        }
        image::RgbImage::from_raw(width, height, rgb).map(image::DynamicImage::ImageRgb8)
    }

    /// `PKEY_*` の定数は windows クレートに入っていないので自前で書く。
    ///
    /// 綴りを1文字間違えても**黙って何も取れなくなるだけ**で気付けないので、
    /// `PSGetPropertyKeyFromName` と突き合わせるテストを下に置いた。
    const fn pkey(fmtid: u128, pid: u32) -> PROPERTYKEY {
        PROPERTYKEY {
            fmtid: GUID::from_u128(fmtid),
            pid,
        }
    }

    /// `System.Media.Duration`（100ナノ秒単位）
    const PKEY_MEDIA_DURATION: PROPERTYKEY = pkey(0x6444_0490_4c8b_11d1_8b70_0800_36b1_1a03, 3);
    /// `System.Media.DateEncoded`（UTC。動画の「撮影日時」はこちらに入る）
    const PKEY_MEDIA_DATE_ENCODED: PROPERTYKEY =
        pkey(0x2e4b_640d_5019_46d8_8881_5541_4cc5_caa0, 100);
    /// `System.Video.FrameWidth`
    const PKEY_VIDEO_FRAME_WIDTH: PROPERTYKEY = pkey(0x6444_0491_4c8b_11d1_8b70_0800_36b1_1a03, 3);
    /// `System.Video.FrameHeight`
    const PKEY_VIDEO_FRAME_HEIGHT: PROPERTYKEY = pkey(0x6444_0491_4c8b_11d1_8b70_0800_36b1_1a03, 4);
    /// `System.Image.HorizontalSize`（写真の幅）
    const PKEY_IMAGE_HORIZONTAL_SIZE: PROPERTYKEY =
        pkey(0x6444_048f_4c8b_11d1_8b70_0800_36b1_1a03, 3);
    /// `System.Image.VerticalSize`（写真の高さ）
    const PKEY_IMAGE_VERTICAL_SIZE: PROPERTYKEY =
        pkey(0x6444_048f_4c8b_11d1_8b70_0800_36b1_1a03, 4);
    /// `System.Photo.DateTaken`（UTC）
    const PKEY_PHOTO_DATE_TAKEN: PROPERTYKEY =
        pkey(0x14b8_1da1_0135_4d31_96d9_6cbf_c967_1a99, 36867);

    /// 寸法として受け入れる上限。桁の壊れた値でグリッドを壊さないための番人
    const MAX_EDGE: u32 = 1_000_000;
    /// 1601-01-01 から 1970-01-01 までのミリ秒
    const FILETIME_EPOCH_TO_UNIX_MS: i64 = 11_644_473_600_000;

    pub fn metadata(path: &Path) -> Option<super::Meta> {
        COM_READY.with(|_| ());
        let text = shell_path(path)?;
        unsafe {
            let store: IPropertyStore =
                SHGetPropertyStoreFromParsingName(&HSTRING::from(text.as_str()), None, GPS_DEFAULT)
                    .ok()?;

            // 寸法のキーは動画と写真で別。持っていない方は空で返るので順に試す。
            //
            // **`System.Video.Frame*` は回転を当てる前の格納寸法**なので、
            // 縦持ちの動画では幅と高さが入れ替わっている。ここが使われるのは
            // 自前のコンテナ読みが失敗したとき（`.m2ts` / `.avi`）だけで、
            // そのコンテナには回転情報自体が無いため実害は出ない。
            // クラウドのみのファイルでは `System.Image.*` の側が返り、
            // そちらは表示どおりの向きで入っている（実測: 1080x1920）
            let width = read_u32(&store, &PKEY_VIDEO_FRAME_WIDTH)
                .or_else(|| read_u32(&store, &PKEY_IMAGE_HORIZONTAL_SIZE));
            let height = read_u32(&store, &PKEY_VIDEO_FRAME_HEIGHT)
                .or_else(|| read_u32(&store, &PKEY_IMAGE_VERTICAL_SIZE));
            // 片方だけでは一覧の枠を作れないので、両方揃ったときだけ採る
            let (width, height) = match (width, height) {
                (Some(w), Some(h))
                    if (1..=MAX_EDGE).contains(&w) && (1..=MAX_EDGE).contains(&h) =>
                {
                    (w, h)
                }
                _ => (0, 0),
            };

            // 写真は DateTaken、動画は DateEncoded。どちらもUTCで入っている
            let taken_at_ms = read_time(&store, &PKEY_PHOTO_DATE_TAKEN)
                .or_else(|| read_time(&store, &PKEY_MEDIA_DATE_ENCODED));

            // 長さは100ナノ秒単位。0は「書いていない」ことを表すので採らない
            let duration_ms = read_u64(&store, &PKEY_MEDIA_DURATION)
                .and_then(|v| i64::try_from(v / 10_000).ok())
                .filter(|ms| *ms > 0);

            let meta = super::Meta {
                width,
                height,
                taken_at_ms,
                duration_ms,
            };
            (!meta.is_empty()).then_some(meta)
        }
    }

    /// 32ビット整数のプロパティを1つ読む。無ければ `None`。
    ///
    /// **`PROPVARIANT` は windows クレートでは Drop を持たない生の構造体**なので、
    /// 読み終えたら自分で `PropVariantClear` する。いま扱う型（整数・FILETIME）は
    /// 中身がヒープに出ないため実害は無いが、キーを増やしたときに漏れる形を残さない。
    unsafe fn read_u32(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<u32> {
        let mut value = store.GetValue(key).ok()?;
        // **罠**: 持っていないキーでも `GetValue` は成功し、`VT_EMPTY` が返る。
        // `PropVariantToUInt32` はそれを **0 に変換して成功**させるので、
        // 素直に受け取ると「取れた（0）」になり、次のキーを試す `or_else` へ
        // 進まなくなる（実測: クラウドのみの動画で寸法が落ちた）。
        // 幅も高さも0は意味を成さないので、ここで無かったことにする
        let read = PropVariantToUInt32(&value).ok().filter(|v| *v > 0);
        let _ = PropVariantClear(&mut value);
        read
    }

    /// 64ビット整数のプロパティを1つ読む。
    unsafe fn read_u64(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<u64> {
        let mut value = store.GetValue(key).ok()?;
        // [`read_u32`] と同じ理由で0を弾く（長さ0の動画も意味を成さない）
        let read = PropVariantToUInt64(&value).ok().filter(|v| *v > 0);
        let _ = PropVariantClear(&mut value);
        read
    }

    /// 日時のプロパティを1つ読み、エポックミリ秒で返す。
    unsafe fn read_time(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<i64> {
        let mut value = store.GetValue(key).ok()?;
        // PSTF_UTC: 現地時刻へ直さずUTCのまま受け取る。
        // ここで現地へ直すと、後段でもう一度時間帯を掛けて二重にずれる
        let read = PropVariantToFileTime(&value, PSTF_UTC).ok();
        let _ = PropVariantClear(&mut value);
        filetime_to_ms(read?)
    }

    /// `FILETIME`（1601年からの100ナノ秒・UTC）をエポックミリ秒へ。
    fn filetime_to_ms(ft: FILETIME) -> Option<i64> {
        let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
        let ms = i64::try_from(ticks / 10_000).ok()? - FILETIME_EPOCH_TO_UNIX_MS;
        // 1970年より前は「書いていない」とみなす（video.rs の mvhd と同じ線引き）
        (ms > 0).then_some(ms)
    }

    pub fn dump_properties(path: &Path) -> Vec<(String, String)> {
        COM_READY.with(|_| ());
        let Some(text) = shell_path(path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        unsafe {
            let Ok(store) = SHGetPropertyStoreFromParsingName::<_, Option<&IBindCtx>, IPropertyStore>(
                &HSTRING::from(text.as_str()),
                None,
                GPS_DEFAULT,
            ) else {
                return out;
            };
            let count = store.GetCount().unwrap_or(0);
            for i in 0..count {
                let mut key = PROPERTYKEY::default();
                if store.GetAt(i, &mut key).is_err() {
                    continue;
                }
                let name = match PSGetNameFromPropertyKey(&key) {
                    Ok(p) => {
                        let s = p.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(p.0.cast()));
                        s
                    }
                    Err(_) => format!("{:?}/{}", key.fmtid, key.pid),
                };
                let Ok(mut value) = store.GetValue(&key) else {
                    continue;
                };
                let text = match PropVariantToStringAlloc(&value) {
                    Ok(p) => {
                        let s = p.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(p.0.cast()));
                        s
                    }
                    Err(_) => "(文字にできない型)".to_string(),
                };
                let _ = PropVariantClear(&mut value);
                out.push((name, text));
            }
        }
        out
    }

    /// `PKEY_*` を自前で書き写した以上、綴りが合っているかは機械に確かめさせる。
    ///
    /// 間違えても実行時は「そのプロパティが無い」ように見えるだけで、
    /// **失敗が失敗として現れない**（クラウドのみのファイルや対応外のコンテナと
    /// 見分けが付かない）。OSの名前解決と突き合わせて固定する。
    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::UI::Shell::PropertiesSystem::PSGetPropertyKeyFromName;

        fn key_of(name: &str) -> PROPERTYKEY {
            // 名前解決もCOM経由。初期化前に呼ぶと 0x800401F0 で落ちる
            COM_READY.with(|_| ());
            let mut key = PROPERTYKEY::default();
            unsafe { PSGetPropertyKeyFromName(&HSTRING::from(name), &mut key) }
                .unwrap_or_else(|e| panic!("{name} を解決できない: {e}"));
            key
        }

        #[test]
        fn pkeys_match_the_names_windows_resolves() {
            for (name, ours) in [
                ("System.Media.Duration", PKEY_MEDIA_DURATION),
                ("System.Media.DateEncoded", PKEY_MEDIA_DATE_ENCODED),
                ("System.Video.FrameWidth", PKEY_VIDEO_FRAME_WIDTH),
                ("System.Video.FrameHeight", PKEY_VIDEO_FRAME_HEIGHT),
                ("System.Image.HorizontalSize", PKEY_IMAGE_HORIZONTAL_SIZE),
                ("System.Image.VerticalSize", PKEY_IMAGE_VERTICAL_SIZE),
                ("System.Photo.DateTaken", PKEY_PHOTO_DATE_TAKEN),
            ] {
                let theirs = key_of(name);
                assert_eq!(ours.fmtid, theirs.fmtid, "{name} のGUIDが違う");
                assert_eq!(ours.pid, theirs.pid, "{name} のpidが違う");
            }
        }

        /// エポックの基準がずれていないか（1970-01-01T00:00:00Z のFILETIME）
        #[test]
        fn converts_filetime_at_the_unix_epoch() {
            let ticks: u64 = 11_644_473_600 * 10_000_000;
            let at_epoch = FILETIME {
                dwLowDateTime: ticks as u32,
                dwHighDateTime: (ticks >> 32) as u32,
            };
            // ちょうどエポックは「書いていない」扱い（>0 の線引き）
            assert_eq!(filetime_to_ms(at_epoch), None);

            let ticks = ticks + 1_500 * 10_000;
            let after = FILETIME {
                dwLowDateTime: ticks as u32,
                dwHighDateTime: (ticks >> 32) as u32,
            };
            assert_eq!(filetime_to_ms(after), Some(1_500));
        }

        /// 1601年（FILETIMEの原点）は1970年より前なので採らない
        #[test]
        fn rejects_times_before_the_unix_epoch() {
            assert_eq!(filetime_to_ms(FILETIME::default()), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows と macOS では使える、それ以外では使えないと申告する。
    ///
    /// **macOSは動画だけ**（AVFoundation）で、Windowsのように何でも出せる
    /// わけではない——真でも [`thumbnail`] が `None` を返す相手はある
    #[test]
    fn reports_availability_per_os() {
        assert_eq!(
            available(),
            cfg!(windows) || cfg!(target_os = "macos"),
            "macOSは動画のサムネイルを出せる（第9部・AVFoundation）"
        );
    }

    /// 存在しないファイルでも落ちない
    #[test]
    fn returns_none_for_missing_file() {
        assert!(thumbnail(Path::new("存在しないファイル.mp4"), 256).is_none());
    }

    /// 中身が動画でないファイルでも落ちない
    #[test]
    fn returns_none_for_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("fake.mp4");
        std::fs::write(&p, b"not a video").unwrap();
        assert!(thumbnail(&p, 256).is_none());
    }
}
