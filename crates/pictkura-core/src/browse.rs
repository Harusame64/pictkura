//! 取り込み元（USB・SDカード等）の中身を覗く（第5部 段階E）。
//!
//! ライブラリのスキャナ（[`crate::scanner`]）との違いは**DBに一切触れない**こと。
//! 取り込み前のファイルは1件もDBに載せないまま、ツリー閲覧とサムネイル表示を
//! 成立させる。取り込み元は遅いUSBであることが多いので、
//!
//! - フォルダは**開いた1階層だけ**を読む（再帰しない）
//! - サムネイルは**EXIF埋め込みを最優先**（フルデコードを避ける）
//!
//! の2点で「刺さらないツリー」にする。

use std::path::{Path, PathBuf};

use crate::scanner::has_target_extension;
use crate::thumbs::{apply_orientation, read_exif};

/// 子フォルダの中身を数えるときに見るエントリ数の上限。
///
/// ツリーのバッジ（このフォルダに画像が何枚あるか）のためだけに、
/// 数万件のフォルダを最後まで数えて開くのを待たせない。
/// 上限に達したら [`SourceDir::count_capped`] を立てて「N+」と表示させる。
const PROBE_LIMIT: usize = 2000;

/// ツリーに出す子フォルダ1件。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceDir {
    pub name: String,
    pub path: PathBuf,
    /// さらに下の階層がある（ツリーの展開ボタンを出すか）
    pub has_subdirs: bool,
    /// このフォルダ直下の画像枚数（再帰しない）
    pub image_count: usize,
    /// 数え切る前に打ち切った（表示は「N+」にする）
    pub count_capped: bool,
}

/// グリッドに出す画像ファイル1件。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceFile {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// 更新日時（Unixエポックミリ秒）。サムネイルURLの版にも使う
    pub mtime_ms: i64,
    /// 実体がクラウド上にしか無い（OneDrive / iCloud Drive 等のプレースホルダ）。
    /// **開くとダウンロードが走る**ので、一覧のサムネイルは作らない
    pub offline: bool,
}

/// 実体がローカルに無いファイル（クラウドのプレースホルダ）か。
///
/// OneDrive・iCloud Drive・Google ドライブは「ドライブ」ではなく
/// Cドライブ上のただのフォルダなので、ドライブ種別では見分けられない。
/// 判定の中身は [`crate::cloud`] にある（ライブラリ側のサムネイル生成でも使う）。
use crate::cloud::is_cloud_only as is_offline;

/// 1フォルダ分の閲覧結果。
#[derive(Debug, Default, serde::Serialize)]
pub struct DirListing {
    pub dirs: Vec<SourceDir>,
    pub files: Vec<SourceFile>,
    /// フォルダ自体が読めなかった（取り外された・権限がない）
    pub unreadable: bool,
}

/// 取り込み元では見せないフォルダか。
///
/// ライブラリ側の `exclude_patterns` はここでは使わない。ユーザーの除外設定が
/// カメラのDCIMに誤マッチして「USBに写真が無い」ように見えるのは事故なので、
/// ゴミ箱・システム領域だけを固定で落とす。
fn is_hidden_dir(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("$Extend")
        // アプリが管理するパッケージ。中身は内部ファイルなので見せても選べない
        || crate::scanner::MANAGED_PACKAGE_PATTERNS
            .iter()
            .any(|p| crate::scanner::matches_pattern(name, p))
}

/// メタデータから更新日時（Unixエポックミリ秒）を取り出す。取れなければ0。
fn mtime_ms_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 名前順（大文字小文字を無視）。`100MSDCF` `101MSDCF` … が自然に並ぶ。
fn by_name_ci(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// 子フォルダの中身を軽く覗いて (下の階層があるか, 画像枚数, 打ち切ったか) を返す。
fn probe(dir: &Path, extensions: &[String]) -> (bool, usize, bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (false, 0, false);
    };
    let (mut has_subdirs, mut count, mut seen) = (false, 0usize, 0usize);
    for entry in entries.flatten() {
        seen += 1;
        if seen > PROBE_LIMIT {
            return (has_subdirs, count, true);
        }
        // file_type()はread_dirが返す情報から取れる（statが要らない＝USBでも速い）
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {
                if !is_hidden_dir(&entry.file_name().to_string_lossy()) {
                    has_subdirs = true;
                }
            }
            Ok(ft) if ft.is_file() && has_target_extension(&entry.path(), extensions) => {
                count += 1;
            }
            _ => {}
        }
    }
    (has_subdirs, count, false)
}

/// フォルダを1階層だけ読む（再帰しない）。
pub fn list_dir(dir: &Path, extensions: &[String]) -> DirListing {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return DirListing {
            unreadable: true,
            ..DirListing::default()
        };
    };

    let mut listing = DirListing::default();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {
                if is_hidden_dir(&name) {
                    continue;
                }
                let (has_subdirs, image_count, count_capped) = probe(&path, extensions);
                listing.dirs.push(SourceDir {
                    name,
                    path,
                    has_subdirs,
                    image_count,
                    count_capped,
                });
            }
            Ok(ft) if ft.is_file() => {
                if !has_target_extension(&path, extensions) {
                    continue;
                }
                // ここで初めてstatする（対象拡張子のファイルにだけコストを払う）
                let meta = entry.metadata().ok();
                listing.files.push(SourceFile {
                    name,
                    path,
                    size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                    mtime_ms: meta.as_ref().map(mtime_ms_of).unwrap_or(0),
                    offline: meta.as_ref().is_some_and(is_offline),
                });
            }
            _ => {}
        }
    }

    listing.dirs.sort_by(|a, b| by_name_ci(&a.name, &b.name));
    listing.files.sort_by(|a, b| by_name_ci(&a.name, &b.name));
    listing
}

/// フォルダの下を**丸ごと**さらった結果（第5部 段階E-5）。
#[derive(Debug, Default, serde::Serialize)]
pub struct TreeListing {
    pub files: Vec<SourceFile>,
    /// 上限に達して打ち切った（先頭 `limit` 件だけ返している）
    pub truncated: bool,
    /// 走査中に読めないフォルダがあった（取りこぼしの可能性）
    pub incomplete: bool,
}

/// フォルダの下の階層まで含めて画像を集める。
///
/// 「カードのどこに写真が入っているか分からない」人のための経路。
/// ツリーを辿らなくても、メディアを選んだだけで中の写真が全部並ぶようにする。
///
/// **`limit` は走査そのものを止める**（結果を切り詰めるだけではない）。
/// 見えているツリーからは想像しにくい規模のフォルダ（システムドライブ直下など）を
/// 選ばれても、上限に達した時点でそれ以上歩かない。
///
/// 除外は取り込み本体（[`crate::import::import_from`]）と同じくドットフォルダと
/// システム領域のみ（ライブラリの `exclude_patterns` を持ち込むと静かに取りこぼす）。
/// 1件ごとのメタデータは**walkdirが列挙時に持っている情報**から取る
/// （Windowsではディレクトリ列挙の結果を再利用するのでファイルを開かない。
/// クラウドのプレースホルダを開かせない、という意味でも重要）。
pub fn list_tree(dir: &Path, extensions: &[String], limit: usize) -> TreeListing {
    let mut listing = TreeListing::default();
    if !dir.is_dir() {
        listing.incomplete = true;
        return listing;
    }

    let walker = walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            // ルート自身は必ず通す。それ以外の隠し・システムフォルダは降りない
            entry.depth() == 0
                || !entry.file_type().is_dir()
                || !is_hidden_dir(&entry.file_name().to_string_lossy())
        });

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            // 読めないフォルダがあっても止めない。「全部見えた」とは言わないだけ
            Err(_) => {
                listing.incomplete = true;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !has_target_extension(path, extensions) {
            continue;
        }
        if listing.files.len() >= limit {
            // ここで歩くのをやめる（数え上げのために全部舐めない）
            listing.truncated = true;
            break;
        }
        let meta = entry.metadata().ok();
        listing.files.push(SourceFile {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_path_buf(),
            size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            mtime_ms: meta.as_ref().map(mtime_ms_of).unwrap_or(0),
            offline: meta.as_ref().is_some_and(is_offline),
        });
    }

    // パス順＝フォルダごとにまとまり、その中は名前順（撮った順に近い）
    listing.files.sort_by(|a, b| a.path.cmp(&b.path));
    listing
}

/// 取り込み前プレビューのバイナリ。
pub struct Preview {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

fn encode_jpeg(img: image::DynamicImage) -> Option<Preview> {
    // JPEGは透過を持てない。捨てるのではなく白へ重ねる（一覧の背景が白）
    let rgb = crate::resize::flatten_onto_white(&img);
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82);
    rgb.write_with_encoder(encoder).ok()?;
    Some(Preview {
        bytes,
        mime: "image/jpeg",
    })
}

/// 取り込み元ファイルのサムネイルを作る（DBもキャッシュファイルも使わない）。
///
/// EXIF埋め込みサムネイルがあれば**再エンコードせずそのまま返す**。
/// USBからの読み出しはヘッダ数十KBで済み、フルデコード（数百ms）を丸ごと省ける。
/// 埋め込みが無いPNG等だけデコード＋縮小する。
pub fn preview(path: &Path, max_edge: u32) -> Option<Preview> {
    // ベクタはそのまま返す。ブラウザが描けるので縮小もラスタライズも要らない
    if crate::svg::is_svg_path(path) {
        return std::fs::read(path).ok().map(|bytes| Preview {
            bytes,
            mime: "image/svg+xml",
        });
    }

    // AVIFは**同梱のデコーダ**で展開する（第7部 段階G-6）。OSの拡張機能に頼らないので
    // 環境を選ばない。展開に失敗しても、ブラウザがAVIFを描けるので原本を返せばよい
    if crate::heif::is_avif_path(path) {
        // 取り込みウィザードの一覧なので、1枚に全コアを割り当てない
        if let Some(img) = crate::av1::decode_file(path, Some(max_edge), crate::av1::Threads::One) {
            return encode_jpeg(crate::resize::shrink_to_fit(&img, max_edge));
        }
        return std::fs::read(path).ok().map(|bytes| Preview {
            bytes,
            mime: "image/avif",
        });
    }

    if crate::heif::is_heif_path(path) {
        let img = crate::heif::decode_thumbnail(path).or_else(|| crate::heif::decode(path))?;
        // 向きは heif 側で適用済みなので、ここでは回転させない
        return encode_jpeg(crate::resize::shrink_to_fit(&img, max_edge));
    }

    // RAWは現像しない。read_exifが埋め込みプレビューJPEGを拾ってくれる
    // （CR3のようにフルサイズが入っていることもあるので、大きければ縮める）
    let exif = read_exif(path);
    if let Some(embedded) = &exif.thumbnail {
        // 一覧に出すには大きすぎる埋め込み（RAWのフルサイズプレビュー等）は縮める。
        // 回転が要るときも同じ経路（デコード→回転→再エンコード）
        let oversized = embedded.len() > 512 * 1024;
        if exif.orientation == 1 && !oversized {
            return Some(Preview {
                bytes: embedded.clone(),
                mime: "image/jpeg",
            });
        }
        // 埋め込みは原寸のJPEGのことがある。間引きながら起こす（第8部）
        let decoded = crate::jpeg::decode_scaled_mem(embedded, max_edge)
            .or_else(|| image::load_from_memory(embedded).ok());
        if let Some(img) = decoded {
            let small = crate::resize::shrink_to_fit(&img, max_edge);
            return encode_jpeg(apply_orientation(small, exif.orientation));
        }
    }
    // JPEGは間引きながら展開する。USB/SDカードの一覧はここの待ち時間がそのまま体感になる。
    // 拡張子で先に振り分ける: PNG等をまず mozjpeg に読ませると、
    // ヘッダで弾かれるまでの読み込みが丸ごと無駄になる
    let img = if crate::jpeg::is_jpeg_path(path) {
        crate::jpeg::decode_scaled(path, max_edge).map_or_else(|| image::open(path).ok(), Some)?
    } else {
        image::open(path).ok()?
    };
    let small = crate::resize::shrink_to_fit(&img, max_edge);
    encode_jpeg(apply_orientation(small, exif.orientation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn exts() -> Vec<String> {
        vec!["jpg".into(), "png".into()]
    }

    #[test]
    fn 一階層だけ読みフォルダと画像を分けて返す() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("DCIM/100MSDCF")).unwrap();
        fs::write(root.join("a.jpg"), b"x").unwrap();
        fs::write(root.join("readme.txt"), b"x").unwrap();
        fs::write(root.join("DCIM/100MSDCF/b.jpg"), b"x").unwrap();

        let listing = list_dir(root, &exts());
        assert!(!listing.unreadable);
        // 直下のjpgだけ（txtは対象外、下の階層へは再帰しない）
        assert_eq!(listing.files.len(), 1);
        assert_eq!(listing.files[0].name, "a.jpg");
        assert_eq!(listing.dirs.len(), 1);
        assert_eq!(listing.dirs[0].name, "DCIM");
        // DCIM直下に画像は無いが、下に階層はある
        assert_eq!(listing.dirs[0].image_count, 0);
        assert!(listing.dirs[0].has_subdirs);

        let sub = list_dir(&root.join("DCIM"), &exts());
        assert_eq!(sub.dirs.len(), 1);
        assert_eq!(sub.dirs[0].image_count, 1, "直下の枚数を数える");
        assert!(!sub.dirs[0].has_subdirs);
    }

    #[test]
    fn 隠しフォルダとシステム領域は出さない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            ".Trashes",
            "$RECYCLE.BIN",
            "System Volume Information",
            "写真",
        ] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let listing = list_dir(root, &exts());
        let names: Vec<_> = listing.dirs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["写真"]);
    }

    /// アプリが管理するパッケージは取り込み元でも見せない。
    /// 中身はUUID名の内部ファイルで、選んでも意味が無い
    #[test]
    fn 写真ライブラリのパッケージは取り込み元に出さない() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "写真ライブラリ.photoslibrary",
            "Photos Library.photoslibrary",
            "photoslibrary", // 拡張子ではない同名フォルダは出す
            "DCIM",
        ] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let listing = list_dir(root, &exts());
        let mut names: Vec<_> = listing.dirs.iter().map(|d| d.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["DCIM", "photoslibrary"]);
    }

    #[test]
    fn 手元にあるファイルはofflineにならない() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.jpg"), b"x").unwrap();
        let listing = list_dir(dir.path(), &exts());
        assert!(!listing.files[0].offline);
        assert!(!list_tree(dir.path(), &exts(), 10).files[0].offline);
    }

    #[test]
    fn 名前順に並ぶ() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in ["b.jpg", "A.jpg", "c.jpg"] {
            fs::write(root.join(name), b"x").unwrap();
        }
        let listing = list_dir(root, &exts());
        let names: Vec<_> = listing.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["A.jpg", "b.jpg", "c.jpg"]);
    }

    #[test]
    fn 読めないフォルダはunreadableで返す() {
        let dir = tempfile::tempdir().unwrap();
        let listing = list_dir(&dir.path().join("取り外し済み"), &exts());
        assert!(listing.unreadable);
        assert!(listing.dirs.is_empty() && listing.files.is_empty());
    }

    #[test]
    fn 枚数の上限に達したら打ち切る() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("many");
        fs::create_dir_all(&sub).unwrap();
        for i in 0..(PROBE_LIMIT + 10) {
            fs::write(sub.join(format!("{i}.jpg")), b"x").unwrap();
        }
        let listing = list_dir(dir.path(), &exts());
        assert!(listing.dirs[0].count_capped, "打ち切りを申告する");
        assert!(listing.dirs[0].image_count <= PROBE_LIMIT);
    }

    #[test]
    fn 下の階層まで集めて返す() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("DCIM/100CANON")).unwrap();
        fs::create_dir_all(root.join("DCIM/101CANON")).unwrap();
        fs::create_dir_all(root.join(".Trashes")).unwrap();
        fs::write(root.join("top.jpg"), b"x").unwrap();
        fs::write(root.join("DCIM/100CANON/a.jpg"), b"x").unwrap();
        fs::write(root.join("DCIM/101CANON/b.jpg"), b"x").unwrap();
        fs::write(root.join("DCIM/101CANON/memo.txt"), b"x").unwrap();
        fs::write(root.join(".Trashes/deleted.jpg"), b"x").unwrap();

        let listing = list_tree(root, &exts(), 1000);
        let names: Vec<_> = listing.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["a.jpg", "b.jpg", "top.jpg"], "パス順に並ぶ");
        assert!(!listing.truncated && !listing.incomplete);
    }

    #[test]
    fn 上限を超えたら打ち切りを申告する() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            fs::write(dir.path().join(format!("{i}.jpg")), b"x").unwrap();
        }
        let listing = list_tree(dir.path(), &exts(), 3);
        assert_eq!(listing.files.len(), 3);
        assert!(listing.truncated);
    }

    #[test]
    fn 上限に達したら走査を止める() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a/b/c");
        fs::create_dir_all(&deep).unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("top{i}.jpg")), b"x").unwrap();
            fs::write(deep.join(format!("deep{i}.jpg")), b"x").unwrap();
        }
        let listing = list_tree(dir.path(), &exts(), 5);
        assert_eq!(listing.files.len(), 5, "上限を超えて集めない");
        assert!(listing.truncated);
    }

    #[test]
    fn 読めないフォルダはincompleteで申告する() {
        let dir = tempfile::tempdir().unwrap();
        let listing = list_tree(&dir.path().join("取り外し済み"), &exts(), 100);
        assert!(listing.incomplete, "空ではなく読めなかったと伝える");
        assert!(listing.files.is_empty());
    }

    #[test]
    fn プレビューは画像でないファイルでnoneを返す() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("broken.jpg");
        fs::write(&f, b"not an image").unwrap();
        assert!(preview(&f, 240).is_none());
    }

    #[test]
    fn プレビューは縮小したjpegを返す() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("big.png");
        image::RgbImage::new(800, 600).save(&f).unwrap();
        let p = preview(&f, 240).expect("プレビューが作れる");
        assert_eq!(p.mime, "image/jpeg");
        let decoded = image::load_from_memory(&p.bytes).unwrap();
        assert_eq!(decoded.width().max(decoded.height()), 240);
    }
}
