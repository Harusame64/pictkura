//! 詳細画面の「抽出」——**いま画面に出ている絵を、そのまま取り出す**（`dev` の Issue #13）。
//!
//! RAWから絵だけ欲しい、という用途のための機能。**現像はしない**——カメラが
//! 埋め込んだ表示用JPEGがそのまま欲しい、というのが元の要求で、それは
//! ビューアが既に描いているものと同じ絵である。
//!
//! だから抽出は「新しく作る」ではなく「**配信しているものを取り直す**」。
//! 経路は [`crate::thumbs::display_jpeg`] と同じで、違うのは TIFF の丸め
//! （[`crate::thumbs::MAX_DISPLAY_EDGE`]）を外すことだけ——あれは画面へ
//! 送る都合であって、ファイルに入っている絵の限界ではない。
//!
//! **原寸そのものが小さいことはある。** 古い機種のRAWは埋め込みプレビューが
//! 160x120〜640x480 しかない（`dev` の `raw-samples.md`）。上限を外しても
//! それより大きくはならない——現像しない以上、ここが天井になる。

use std::path::Path;

/// 抽出で渡す絵の出どころ。
pub enum Source {
    /// **原本をそのまま渡してよい**（jpg・png・webp・avif・bmp・gif・svg）。
    /// ビューアが描いているのはこのファイルそのものなので、詰め直す理由が無い
    /// ——再エンコードを挟まなければ EXIF も ICC プロファイルも落ちない。
    Original,
    /// 詰め直したJPEG（RAW・HEIC・TIFF）。ビューアが描いているのもこれ。
    Jpeg(Vec<u8>),
}

/// 抽出する絵を用意する。**取り出す絵が無ければ `None`**。
///
/// `None` になるのは、プレビューを持たないRAW——Hasselblad の `.fff`、
/// Blackmagic の CinemaDNG、Panasonic の古い `.raw`（DMC-LX1 / DMC-FZ8）。
/// ビューアでも枠だけが出ている個体で、**取り出す絵はどこにも無い**。
pub fn source(path: &Path) -> Option<Source> {
    if !crate::thumbs::needs_display_transcode(path) {
        return Some(Source::Original);
    }
    crate::thumbs::display_jpeg_full(path).map(Source::Jpeg)
}

/// 抽出したJPEGに付ける拡張子。原本をそのまま渡す形式では `None`。
///
/// 保存先の名前を決めるのに使う（`IMG_0001.CR3` → `IMG_0001.jpg`）。
pub fn transcoded_extension(path: &Path) -> Option<&'static str> {
    crate::thumbs::needs_display_transcode(path).then_some("jpg")
}

/// 保存先が**原本そのもの**か。
///
/// **これを見ないと写真が消える。** `std::fs::copy` は行き先を先に空にするので、
/// 両方が同じファイルを指していると `Ok(0)` を返しながら**原本が0バイトになる**
/// （2026-08-31に実測）。詰め直す側も同じで、RAWの上にJPEGを書けば原本は消える。
///
/// **踏みやすい。** 保存先の名前は原本と同じものを提案していて、ライブラリの
/// 既定のルートは「ピクチャ」——初回のダイアログが**原本そのもの**を指すことがある。
/// OSは「置き換えますか」と聞くが、同じ写真なのだから押す人は押す。
///
/// 実体で比べる（`canonicalize`）——`../` や大小の違い、シンボリックリンクを
/// 綴りで比べると素通りする。**行き先がまだ無ければ別物**（存在しない
/// ファイルが既にある原本と同じことはない）。
pub fn dest_is_source(src: &Path, dest: &Path) -> bool {
    match (std::fs::canonicalize(src), std::fs::canonicalize(dest)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// 抽出した絵を `dest` へ書く。
///
/// 原本をそのまま渡せるものはコピーで済ませる（再エンコードしない＝EXIFも
/// ICCも落ちない）。詰め直しの要る形式だけJPEGを作って書く。
///
/// **更新日時を引き継ぐ**（`crate::export::copy_verified` と同じ理由）。
/// Unixの `fs::copy` は持って行かないので、そのままだと取り出した順に並ぶ
/// ——30枚まとめて取り出したフォルダが撮影順にならない。詰め直したJPEGにも
/// 引き継ぐ: カメラが埋め込んだ絵はEXIFを持たないことがあり、そのときは
/// 更新日時が唯一の手がかりになる。
pub fn write_to(src: &Path, dest: &Path) -> std::io::Result<()> {
    if dest_is_source(src, dest) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "保存先が元のファイルと同じ",
        ));
    }
    match source(src) {
        Some(Source::Original) => {
            std::fs::copy(src, dest)?;
        }
        Some(Source::Jpeg(bytes)) => std::fs::write(dest, bytes)?,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "取り出せる絵が無い",
            ))
        }
    }
    if let Ok(mtime) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(mtime));
    }
    Ok(())
}

/// クリップボードへ載せる画素（RGBA8）。載せられなければ `None`。
///
/// **クリップボードだけは画素まで起こす**。OSのクリップボードが画像として
/// 受け取るのはビットマップで、JPEGのバイト列をそのまま置いても貼れない
/// （貼る側から見えるのは「ファイル」であって「画像」ではない）。
///
/// ベクタ（SVG）は `None`。ラスタライザを抱えていないので画素に起こせない
/// ——`crate::svg` の方針どおり、大きさはWebViewに描かせている。
///
/// **ここだけは丸めた側を使う**（[`source`] は丸めない）。理由は画素の量:
/// 1億画素のスキャンTIFFを起こすと RGBA だけで約400MB、OSへ渡すときに
/// もう一度写されるので、1回の 📋 で1GBに届く。**貼るための絵に原寸は要らない**
/// ——しかも画面に出ているのは丸めた側なので、こちらが「見えているものを渡す」
/// にもなっている。丸めているのはTIFFだけで、他の形式は元から等倍のまま
/// （WebViewが既にその画素を展開して描いている）。
pub fn rgba(path: &Path) -> Option<image::RgbaImage> {
    // 詰め直しの要る形式（RAW・HEIC・TIFF）は、**配信と同じ経路**で作った
    // JPEGを起こす。向きは [`crate::thumbs::display_jpeg`] が適用済み
    if crate::thumbs::needs_display_transcode(path) {
        let jpeg = crate::thumbs::display_jpeg(path)?;
        return Some(image::load_from_memory(&jpeg).ok()?.into_rgba8());
    }
    if crate::svg::is_svg_path(path) {
        return None;
    }
    // AVIFは**同梱のデコーダ**で展開する。`image` はavifの機能を切って
    // 積んである（`Cargo.toml`）ので、`image::open` はここを読めない。
    // 抽出はその1枚しか走っていないので全スレッドを使ってよい
    let img = if crate::heif::is_avif_path(path) {
        crate::av1::decode_file(path, None, crate::av1::Threads::All)?
    } else {
        image::open(path).ok()?
    };
    // 原本をデコードしただけでは**向きが寝ている**。`image` はEXIFの
    // Orientation を見ないので、WebViewが起こして描いていたぶんをここで補う
    // ——貼った先で横倒しになるのは、抽出としては失敗している。
    // 絵は要らない（向きだけ）ので `read_exif_meta` で足りる
    let orientation = crate::thumbs::read_exif_meta(path).orientation;
    Some(crate::thumbs::apply_orientation(img, orientation).into_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 長辺が上限を超えるTIFFを1枚置く。中身は問わない（見るのは寸法だけ）。
    fn wide_tiff(dir: &std::path::Path, w: u32, h: u32) -> std::path::PathBuf {
        let path = dir.join("大きい.tif");
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 96])
        });
        image::DynamicImage::ImageRgb8(img).save(&path).unwrap();
        path
    }

    fn edge(jpeg: &[u8]) -> u32 {
        let img = image::load_from_memory(jpeg).unwrap();
        img.width().max(img.height())
    }

    /// **抽出は丸めない**——これがこの機能の要点。配信の上限
    /// （[`crate::thumbs::MAX_DISPLAY_EDGE`]）はビューアの都合であって、
    /// ファイルに入っている絵の限界ではない。
    #[test]
    fn tiff_is_capped_for_the_viewer_but_not_for_the_extract() {
        let dir = tempfile::tempdir().unwrap();
        let over = crate::thumbs::MAX_DISPLAY_EDGE + 400;
        let path = wide_tiff(dir.path(), over, 64);

        let served = crate::thumbs::display_jpeg(&path).unwrap();
        assert_eq!(edge(&served), crate::thumbs::MAX_DISPLAY_EDGE);

        let Some(Source::Jpeg(extracted)) = source(&path) else {
            panic!("TIFFは詰め直して渡すはず");
        };
        assert_eq!(edge(&extracted), over);
    }

    /// 上限より小さいTIFFは、どちらの経路でも原寸のまま。
    /// **丸めが「常に縮める」に化けていない**ことの確認
    #[test]
    fn a_small_tiff_is_left_alone_on_both_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = wide_tiff(dir.path(), 800, 64);
        assert_eq!(edge(&crate::thumbs::display_jpeg(&path).unwrap()), 800);
        let Some(Source::Jpeg(extracted)) = source(&path) else {
            panic!("TIFFは詰め直して渡すはず");
        };
        assert_eq!(edge(&extracted), 800);
    }

    /// **普通の写真は詰め直さない**。原本をそのまま渡すので、EXIFもICCも残る
    #[test]
    fn an_ordinary_photo_is_handed_over_as_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("写真.jpg");
        let img = image::RgbImage::from_fn(32, 32, |_, _| image::Rgb([10, 20, 30]));
        image::DynamicImage::ImageRgb8(img).save(&path).unwrap();

        assert!(matches!(source(&path), Some(Source::Original)));
        assert_eq!(transcoded_extension(&path), None);
        // クリップボードへは画素で載せる（原本のままでは貼れない）
        let rgba = rgba(&path).unwrap();
        assert_eq!((rgba.width(), rgba.height()), (32, 32));
    }

    /// ベクタは画素に起こせない。**保存はできるがコピーはできない**、を守る
    #[test]
    fn a_vector_can_be_saved_but_not_put_on_the_clipboard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("図.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#,
        )
        .unwrap();
        assert!(matches!(source(&path), Some(Source::Original)));
        assert!(rgba(&path).is_none());
    }

    /// **原本の上には書かない。** これが無いと `fs::copy` が `Ok(0)` を返しながら
    /// 写真を0バイトにする——保存先の名前は原本と同じものを提案しているので、
    /// 「置き換えますか」に頷いた人がそのまま踏む
    #[test]
    fn saving_over_the_original_is_refused_and_the_original_survives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("写真.jpg");
        let img = image::RgbImage::from_fn(16, 16, |_, _| image::Rgb([7, 7, 7]));
        image::DynamicImage::ImageRgb8(img).save(&path).unwrap();
        let before = std::fs::metadata(&path).unwrap().len();
        assert!(before > 0);

        assert!(dest_is_source(&path, &path));
        assert!(write_to(&path, &path).is_err());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), before);

        // 綴りが違っても実体が同じなら同じ（`./` を挟んだ形）
        let same_by_another_spelling = dir.path().join(".").join("写真.jpg");
        assert!(dest_is_source(&path, &same_by_another_spelling));

        // まだ無い行き先は別物。ここを取り違えると保存が一切できなくなる
        let fresh = dir.path().join("取り出し.jpg");
        assert!(!dest_is_source(&path, &fresh));
        write_to(&path, &fresh).unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().len(), before);
    }

    /// **更新日時を引き継ぐ**（`export::copy_verified` と同じ理由）。
    /// まとめて取り出したフォルダが、撮影順ではなく取り出し順に並ばないように
    #[test]
    fn what_we_write_keeps_the_time_of_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("写真.jpg");
        let img = image::RgbImage::from_fn(16, 16, |_, _| image::Rgb([1, 2, 3]));
        image::DynamicImage::ImageRgb8(img).save(&src).unwrap();
        // 2020-01-02T03:04:05Z。「いま」と紛れない値にする
        let then = filetime::FileTime::from_unix_time(1_577_934_245, 0);
        filetime::set_file_mtime(&src, then).unwrap();

        let dest = dir.path().join("取り出し.jpg");
        write_to(&src, &dest).unwrap();
        let got =
            filetime::FileTime::from_last_modification_time(&std::fs::metadata(&dest).unwrap());
        assert_eq!(got.unix_seconds(), then.unix_seconds());
    }

    /// 詰め直す側でも引き継ぐ。カメラが埋め込んだ絵はEXIFを持たないことがあり、
    /// そのときは更新日時が唯一の手がかりになる
    #[test]
    fn a_transcoded_picture_keeps_the_time_too() {
        let dir = tempfile::tempdir().unwrap();
        let src = wide_tiff(dir.path(), 64, 64);
        let then = filetime::FileTime::from_unix_time(1_577_934_245, 0);
        filetime::set_file_mtime(&src, then).unwrap();

        let dest = dir.path().join("取り出し.jpg");
        write_to(&src, &dest).unwrap();
        let got =
            filetime::FileTime::from_last_modification_time(&std::fs::metadata(&dest).unwrap());
        assert_eq!(got.unix_seconds(), then.unix_seconds());
    }

    /// **クリップボードは丸めた側**（[`rgba`]）。1億画素のスキャンを起こすと
    /// RGBAだけで400MBになり、OSへ渡すときにもう一度写される
    #[test]
    fn the_clipboard_takes_the_rounded_picture_not_the_full_one() {
        let dir = tempfile::tempdir().unwrap();
        let over = crate::thumbs::MAX_DISPLAY_EDGE + 400;
        let path = wide_tiff(dir.path(), over, 64);

        let pixels = rgba(&path).unwrap();
        assert_eq!(pixels.width(), crate::thumbs::MAX_DISPLAY_EDGE);

        // 保存の側は丸めないまま（両者が別なのは意図）
        let Some(Source::Jpeg(saved)) = source(&path) else {
            panic!("TIFFは詰め直して渡すはず");
        };
        assert_eq!(edge(&saved), over);
    }

    /// 絵を持たないRAWは `None`。UIはこれを見てボタンを伏せる
    #[test]
    fn a_raw_without_a_preview_has_nothing_to_extract() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("枠だけ.fff");
        std::fs::write(&path, b"not really a Hasselblad file").unwrap();
        assert!(source(&path).is_none());
        assert!(rgba(&path).is_none());
    }
}
