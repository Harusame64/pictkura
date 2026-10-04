//! Google フォト用の**窓口フォルダ**（`dev/plan.google-photos-mirror.md`）。
//!
//! Google フォト Web の「フォルダをバックアップ」は、登録したフォルダを**丸ごと**上げる。
//! 形式で除く設定は無い。だから pictkura が、上げてよいものだけを並べたフォルダを
//! ドライブごとに1つ保ち、利用者は Google フォトに**そこだけ**を登録する。
//!
//! 中身は原本への**ハードリンク**——ディスクはほぼ使わない。pictkura は Google と
//! 1バイトも話さない。ここがするのは、ローカルのフォルダを揃えることだけ。
//!
//! 規則は win の実測（`dev/plan.google-photos-mirror.spike.md`、2026-10-04）から来ている:
//!
//! - **窓口の中で書かない。** Google は名前が付いた瞬間に拾い、拡張子でも絞らない。
//!   書きかけを拾うと失敗し、**ページを読み込み直すまで取り戻さない**（S6b/S7b）。
//!   リンクは1回の操作なので途中の名前が出ない。置き換えは窓口の外で作って改名で入れる
//! - **リンクの先を書き換えない。** 原本をその場で上書きすると、Google には別の写真が
//!   増える（S10）。pictkura 自身は原本を書き換えない
//! - **消しても Google からは消えない**（S8）。だから窓口から外すことは怖くない。
//!   怖いのは**窓口にしか残っていない実体**を消すこと——それは呼び出し側（ゴミ箱）へ渡す

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::config::{GoogleMirrorConfig, RawOnly};
use crate::search::MediaKind;

/// 窓口フォルダの名前（ドライブごとの既定の場所で使う）。
pub const MIRROR_DIR_NAME: &str = "pictkura-google";

/// 窓口に置く1件の材料（DB の1行）。
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub favorite: bool,
}

/// 窓口に置くと決めた1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// 原本
    pub source: PathBuf,
    /// 窓口の中での場所（ルートのラベル＋ルートからの相対パス）
    pub rel: PathBuf,
}

/// どれを窓口に置くかを決める（**ファイルには触らない**）。
///
/// - 動画は「動画を除く」なら置かない
/// - RAW は「RAW を除く」なら、同じフォルダ・同じ名前の写真（組＝[`crate::sidecar::pair_key`]）が
///   あれば置かない。RAW だけのカットは [`RawOnly`] に従う
/// - どのルートにも入らないものは置かない
///
/// 返す順は `items` の順。同じ `rel` が2つ出ることは無い（ルートのラベルを分けるため）。
pub fn plan(items: &[Item], roots: &[PathBuf], cfg: &GoogleMirrorConfig) -> Vec<Placement> {
    let labels = root_labels(roots);
    // 組の相方になれる「写真」の鍵。RAW だけのカットかどうかはこれで決まる
    let photo_keys: HashSet<(PathBuf, String)> = items
        .iter()
        .filter(|i| MediaKind::from_path(&i.path) == MediaKind::Photo)
        .map(|i| crate::sidecar::pair_key(&i.path))
        .collect();

    let mut out = Vec::new();
    for item in items {
        let wanted = match MediaKind::from_path(&item.path) {
            MediaKind::Photo => true,
            MediaKind::Video => !cfg.exclude_video,
            MediaKind::Raw if !cfg.exclude_raw => true,
            MediaKind::Raw => {
                let paired = photo_keys.contains(&crate::sidecar::pair_key(&item.path));
                !paired
                    && match cfg.raw_only {
                        RawOnly::None => false,
                        RawOnly::Starred => item.favorite,
                        RawOnly::All => true,
                    }
            }
        };
        if !wanted {
            continue;
        }
        let Some((root, label)) = owning_root(&item.path, roots, &labels) else {
            continue;
        };
        let Ok(rest) = item.path.strip_prefix(root) else {
            continue;
        };
        let rel = Path::new(label).join(rest);
        if is_plain_relative(&rel) {
            out.push(Placement {
                source: item.path.clone(),
                rel,
            });
        }
    }
    out
}

/// ルートごとの、窓口の中での名前。**フォルダ名が重なったら番号を足す**
/// （`D:\Photos` と `E:\old\Photos` が同じ窓口へ来ることは無いが、同じドライブの
/// `D:\a\Photos` と `D:\b\Photos` は来る）。ドライブそのもの（`D:\`）は名前を持たないので
/// ドライブ文字を使う。
fn root_labels(roots: &[PathBuf]) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    roots
        .iter()
        .map(|root| {
            let base = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| drive_label(root));
            let mut label = base.clone();
            let mut n = 2;
            while !used.insert(label.to_lowercase()) {
                label = format!("{base} ({n})");
                n += 1;
            }
            label
        })
        .collect()
}

fn drive_label(root: &Path) -> String {
    let s: String = root
        .to_string_lossy()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    if s.is_empty() {
        "root".into()
    } else {
        s
    }
}

/// `path` を持つルート。入れ子なら**いちばん深い**ほう。
fn owning_root<'a>(
    path: &Path,
    roots: &'a [PathBuf],
    labels: &'a [String],
) -> Option<(&'a Path, &'a str)> {
    roots
        .iter()
        .zip(labels)
        .filter(|(root, _)| path.starts_with(root) && path != root.as_path())
        .max_by_key(|(root, _)| root.components().count())
        .map(|(root, label)| (root.as_path(), label.as_str()))
}

/// 窓口の外を指さない相対パスか（`..`・絶対・ドライブ付きを拒む）。
/// **窓口の外を決して触らない**ための門で、ここを通ったものだけを `mirror.join` する。
fn is_plain_relative(rel: &Path) -> bool {
    !rel.as_os_str().is_empty() && rel.components().all(|c| matches!(c, Component::Normal(_)))
}

/// 同期の結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// 新しく置いたリンク
    pub linked: usize,
    /// 原本が変わっていたので置き換えたもの
    pub replaced: usize,
    /// 窓口から外したもの（原本はほかに在る）
    pub removed: usize,
    /// 窓口から外したが、**そこにしか実体が無かった**ので `discard` へ渡したもの
    pub discarded: usize,
    /// クラウドのみなので置かなかったもの（置くと Google が読みに行った瞬間に取り寄せが走る）
    pub cloud_only: usize,
    /// 失敗（窓口の中の場所と理由）
    pub failed: Vec<(PathBuf, String)>,
}

/// 窓口が pictkura のものだという印にする作業場。**窓口の中には置かない**
/// （Google は拡張子で絞らず拾う。S6b）——隣に置く。置き換えのリンクもここで作る。
pub fn staging_dir(mirror: &Path) -> PathBuf {
    let name = mirror
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| MIRROR_DIR_NAME.into());
    mirror.with_file_name(format!(".{name}-staging"))
}

/// 同期を始めてよい窓口か。**利用者のフォルダを窓口と取り違えると、写真を消しに行く**
/// ので、中身があって作業場が無い（＝pictkura が作ったものではない）フォルダは拒む。
pub fn check_ownership(mirror: &Path) -> Result<(), MirrorError> {
    let has_entries = match std::fs::read_dir(mirror) {
        Ok(mut rd) => rd.next().is_some(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(MirrorError::Io(e)),
    };
    if has_entries && !staging_dir(mirror).is_dir() {
        return Err(MirrorError::NotOurs(mirror.to_path_buf()));
    }
    Ok(())
}

/// 窓口の場所として使ってよいか。ルートの中・ルートを含む場所は、ライブラリに
/// 二重に載るので拒む。
pub fn check_location(mirror: &Path, roots: &[PathBuf]) -> Result<(), MirrorError> {
    for root in roots {
        if mirror.starts_with(root) || root.starts_with(mirror) {
            return Err(MirrorError::OverlapsRoot(root.clone()));
        }
    }
    if let Some(dir) = sync_client_folder(mirror) {
        return Err(MirrorError::InsideSyncFolder(dir));
    }
    Ok(())
}

/// 同期クライアントのフォルダの中か。OneDrive はハードリンクを扱えず、Google ドライブの
/// 同期フォルダの中だと二重にアップロードされる（公式に明記）。
fn sync_client_folder(path: &Path) -> Option<PathBuf> {
    let env_dirs = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from);
    let home_dirs = home_dir()
        .map(|h| {
            vec![
                h.join("Library").join("CloudStorage"),
                h.join("Google Drive"),
            ]
        })
        .unwrap_or_default();
    env_dirs
        .chain(home_dirs)
        .find(|d| !d.as_os_str().is_empty() && path.starts_with(d))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// 窓口の同期で起きる、続けられない誤り。
#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("窓口フォルダ {0} は pictkura が作ったものではない（中身がある）")]
    NotOurs(PathBuf),
    #[error("窓口フォルダがライブラリのフォルダ {0} と重なっている")]
    OverlapsRoot(PathBuf),
    #[error("窓口フォルダが同期フォルダ {0} の中にある")]
    InsideSyncFolder(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// 窓口を `placements` に揃える。
///
/// 1. 窓口を歩いて今ある集合を作る（リンク・ジャンクションは辿らない）
/// 2. **望まないものを先に外す**（大文字小文字だけ違う名前へ置き直すとき、先に空ける）。
///    そこにしか実体が無いファイル（リンク数1）は**消さずに** `discard` へ渡す——
///    原本を pictkura の外で消し切った写真の、最後の1枚かもしれない
/// 3. 置く。同じ実体を指していればそのまま、違えば作業場でリンクを作って改名で置き換える
/// 4. 空になったフォルダを畳む（窓口そのものは残す）
///
/// 1件ずつの失敗は [`SyncReport::failed`] に積んで続ける。全件の差を取るので、
/// 取りこぼしても次の同期で揃う。
pub fn sync(
    mirror: &Path,
    placements: &[Placement],
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> Result<SyncReport, MirrorError> {
    check_ownership(mirror)?;
    let staging = staging_dir(mirror);
    std::fs::create_dir_all(mirror)?;
    std::fs::create_dir_all(&staging)?;

    let mut report = SyncReport::default();
    let wanted: HashMap<&Path, &Placement> = placements
        .iter()
        .filter(|p| is_plain_relative(&p.rel))
        .map(|p| (p.rel.as_path(), p))
        .collect();

    // 1–2. 今ある集合と、外すもの
    let mut present: HashSet<PathBuf> = HashSet::new();
    for entry in walkdir::WalkDir::new(mirror)
        .follow_links(false)
        .min_depth(1)
    {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                report.failed.push((PathBuf::new(), e.to_string()));
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(mirror) else {
            continue;
        };
        if wanted.contains_key(rel) {
            present.insert(rel.to_path_buf());
            continue;
        }
        match remove_or_discard(entry.path(), discard) {
            Ok(true) => report.discarded += 1,
            Ok(false) => report.removed += 1,
            Err(e) => report.failed.push((rel.to_path_buf(), e.to_string())),
        }
    }

    // 3. 置く
    for (n, p) in placements.iter().enumerate() {
        if !wanted
            .get(p.rel.as_path())
            .is_some_and(|w| std::ptr::eq(*w, p))
        {
            continue;
        }
        let dest = mirror.join(&p.rel);
        let result = if crate::cloud::is_cloud_only_path(&p.source) {
            // 置かない。既に置いてあれば（前は手元にあった）外す
            report.cloud_only += 1;
            if present.contains(&p.rel) {
                remove_or_discard(&dest, discard).map(|_| ())
            } else {
                Ok(())
            }
        } else if present.contains(&p.rel) {
            match same_file(&p.source, &dest) {
                Ok(true) => Ok(()),
                Ok(false) => replace_link(&p.source, &dest, &staging, n, discard)
                    .map(|()| report.replaced += 1),
                Err(e) => Err(e),
            }
        } else {
            dest.parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::hard_link(&p.source, &dest))
                .map(|()| report.linked += 1)
        };
        if let Err(e) = result {
            report.failed.push((p.rel.clone(), e.to_string()));
        }
    }

    // 4. 空のフォルダを畳む（深いほうから）
    let mut dirs: Vec<PathBuf> = walkdir::WalkDir::new(mirror)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in dirs {
        // 中身があれば失敗する＝それで正しい
        let _ = std::fs::remove_dir(&d);
    }
    Ok(report)
}

/// 窓口のファイルを外す。ほかに名前があれば消すだけ（実体は原本の側に残る）。
/// **そこにしか実体が無ければ `discard` へ渡す**。渡したら `true`。
fn remove_or_discard(
    path: &Path,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<bool> {
    if file_id(path)?.links <= 1 {
        discard(path)?;
        Ok(true)
    } else {
        std::fs::remove_file(path)?;
        Ok(false)
    }
}

/// 作業場でリンクを作り、**1回の改名**で置き換える。窓口の中に途中の名前を出さない。
fn replace_link(
    source: &Path,
    dest: &Path,
    staging: &Path,
    n: usize,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    // 古いほうがそこにしか実体を持たないなら、上書きで消す前に渡す
    if file_id(dest)?.links <= 1 {
        discard(dest)?;
    }
    let tmp = staging.join(format!("{}-{n}.link", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::fs::hard_link(source, &tmp)?;
    let renamed = std::fs::rename(&tmp, dest);
    if renamed.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    renamed
}

/// 2つのパスが同じ実体か。
fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
    let (a, b) = (file_id(a)?, file_id(b)?);
    Ok(a.volume == b.volume && a.index == b.index)
}

/// 2つのパスが同じボリュームか（ハードリンクが張れるか）。どちらも在ること。
pub fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    Ok(file_id(a)?.volume == file_id(b)?.volume)
}

/// 実体の名札。ボリューム・ボリューム内の番号・名前の数。
struct FileId {
    volume: u64,
    index: u64,
    links: u64,
}

#[cfg(unix)]
fn file_id(path: &Path) -> io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::symlink_metadata(path)?;
    Ok(FileId {
        volume: m.dev(),
        index: m.ino(),
        links: m.nlink(),
    })
}

/// std の `MetadataExt::{volume_serial_number, file_index, number_of_links}` は
/// まだ不安定なので、同じ `GetFileInformationByHandle` を自分で呼ぶ。
/// 開くのは属性だけ（中身を読まない）。フォルダも開けるよう `BACKUP_SEMANTICS` を付ける。
#[cfg(windows)]
fn file_id(path: &Path) -> io::Result<FileId> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    };
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    // SAFETY: 全フィールドが整数の C 構造体なので 0 埋めは正しい初期値
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: ハンドルは `file` が生きている間有効で、`info` は書き込み先として正しい大きさ
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(FileId {
        volume: u64::from(info.dwVolumeSerialNumber),
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        links: u64::from(info.nNumberOfLinks),
    })
}

/// `root` に置く窓口の場所。`locations`（利用者が選んだ窓口）に同じボリュームのものが
/// あればそれ、無ければ既定——ホームと同じボリュームならホームの直下、
/// そうでなければそのボリュームのいちばん上。**AppData の下は使わない**
/// （Chromium のフォルダ選択が弾く）。
pub fn location_for_root(root: &Path, locations: &[PathBuf]) -> io::Result<PathBuf> {
    for loc in locations {
        // まだ作っていない窓口は、在る親で比べる
        let probe = loc.ancestors().find(|a| a.exists()).unwrap_or(loc);
        if same_volume(root, probe)? {
            return Ok(loc.clone());
        }
    }
    if let Some(home) = home_dir() {
        if same_volume(root, &home)? {
            return Ok(home.join(MIRROR_DIR_NAME));
        }
    }
    Ok(volume_top(root)?.join(MIRROR_DIR_NAME))
}

/// `path` と同じボリュームの、いちばん上のフォルダ（Windows ならドライブ、macOS なら
/// `/Volumes/名前`）。親へ上がってボリュームが変わる手前で止める。
fn volume_top(path: &Path) -> io::Result<PathBuf> {
    let vol = file_id(path)?.volume;
    let mut top = path.to_path_buf();
    for a in path.ancestors().skip(1) {
        match file_id(a) {
            Ok(id) if id.volume == vol => top = a.to_path_buf(),
            _ => break,
        }
    }
    Ok(top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GoogleMirrorConfig;

    fn item(p: &str, favorite: bool) -> Item {
        Item {
            path: PathBuf::from(p),
            favorite,
        }
    }

    fn rels(ps: &[Placement]) -> Vec<String> {
        ps.iter()
            .map(|p| p.rel.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    fn cfg(raw_only: RawOnly) -> GoogleMirrorConfig {
        GoogleMirrorConfig {
            raw_only,
            ..GoogleMirrorConfig::default()
        }
    }

    const ROOT: &str = "/lib/Photos";

    fn shoot() -> Vec<Item> {
        vec![
            item("/lib/Photos/d/A.ARW", false),
            item("/lib/Photos/d/a.jpg", false), // 組（大文字小文字を畳む）
            item("/lib/Photos/d/B.ARW", false), // RAW だけ・★なし
            item("/lib/Photos/d/C.CR3", true),  // RAW だけ・★あり
            item("/lib/Photos/d/clip.mp4", false),
        ]
    }

    #[test]
    fn the_default_sends_photos_and_videos_but_no_raw() {
        let got = plan(&shoot(), &[ROOT.into()], &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["Photos/d/a.jpg", "Photos/d/clip.mp4"]);
    }

    #[test]
    fn raw_only_shots_follow_the_three_way_setting() {
        let roots = [PathBuf::from(ROOT)];
        let starred = plan(&shoot(), &roots, &cfg(RawOnly::Starred));
        assert_eq!(
            rels(&starred),
            ["Photos/d/a.jpg", "Photos/d/C.CR3", "Photos/d/clip.mp4"]
        );
        let all = plan(&shoot(), &roots, &cfg(RawOnly::All));
        assert_eq!(
            rels(&all),
            [
                "Photos/d/a.jpg",
                "Photos/d/B.ARW",
                "Photos/d/C.CR3",
                "Photos/d/clip.mp4"
            ]
        );
    }

    #[test]
    fn a_paired_raw_stays_home_even_when_starred_and_all_is_chosen() {
        let items = [
            item("/lib/Photos/d/A.ARW", true),
            item("/lib/Photos/d/A.JPG", false),
        ];
        let got = plan(&items, &[ROOT.into()], &cfg(RawOnly::All));
        assert_eq!(rels(&got), ["Photos/d/A.JPG"]);
    }

    #[test]
    fn a_video_is_not_a_partner_for_a_raw() {
        let items = [
            item("/lib/Photos/d/A.ARW", false),
            item("/lib/Photos/d/A.MOV", false),
        ];
        let got = plan(&items, &[ROOT.into()], &cfg(RawOnly::All));
        assert_eq!(rels(&got), ["Photos/d/A.ARW", "Photos/d/A.MOV"]);
    }

    #[test]
    fn excluding_video_and_including_raw_are_both_honoured() {
        let c = GoogleMirrorConfig {
            exclude_raw: false,
            exclude_video: true,
            ..GoogleMirrorConfig::default()
        };
        let got = plan(&shoot(), &[ROOT.into()], &c);
        assert_eq!(
            rels(&got),
            [
                "Photos/d/A.ARW",
                "Photos/d/a.jpg",
                "Photos/d/B.ARW",
                "Photos/d/C.CR3"
            ]
        );
    }

    #[test]
    fn roots_with_the_same_folder_name_get_separate_labels() {
        let roots = [PathBuf::from("/a/Photos"), PathBuf::from("/b/photos")];
        let items = [
            item("/a/Photos/1.jpg", false),
            item("/b/photos/1.jpg", false),
        ];
        let got = plan(&items, &roots, &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["Photos/1.jpg", "photos (2)/1.jpg"]);
    }

    #[test]
    fn a_file_belongs_to_the_deepest_root_and_strays_are_skipped() {
        let roots = [PathBuf::from("/lib"), PathBuf::from("/lib/Photos")];
        let items = [
            item("/lib/Photos/1.jpg", false),
            item("/lib/x.jpg", false),
            item("/elsewhere/2.jpg", false),
        ];
        let got = plan(&items, &roots, &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["Photos/1.jpg", "lib/x.jpg"]);
    }

    #[test]
    fn only_plain_relative_paths_pass_the_gate() {
        assert!(is_plain_relative(Path::new("a/b.jpg")));
        assert!(!is_plain_relative(Path::new("../b.jpg")));
        assert!(!is_plain_relative(Path::new("a/../../b.jpg")));
        assert!(!is_plain_relative(Path::new("/b.jpg")));
        assert!(!is_plain_relative(Path::new("")));
    }

    // ---- 実ファイル ----

    struct Fixture {
        _dir: tempfile::TempDir,
        lib: PathBuf,
        mirror: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        std::fs::create_dir_all(lib.join("d")).unwrap();
        let mirror = dir.path().join(MIRROR_DIR_NAME);
        Fixture {
            lib,
            mirror,
            _dir: dir,
        }
    }

    fn put(p: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn placement(lib: &Path, rel: &str) -> Placement {
        Placement {
            source: lib.join(rel),
            rel: Path::new("lib").join(rel),
        }
    }

    fn no_discard() -> impl FnMut(&Path) -> io::Result<()> {
        |p: &Path| panic!("discard was not expected: {}", p.display())
    }

    #[test]
    fn sync_links_without_copying_and_a_second_run_changes_nothing() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let ps = [placement(&f.lib, "d/a.jpg")];

        let first = sync(&f.mirror, &ps, &mut no_discard()).unwrap();
        assert_eq!(first.linked, 1);
        let dest = f.mirror.join("lib/d/a.jpg");
        assert!(same_file(&f.lib.join("d/a.jpg"), &dest).unwrap());
        assert_eq!(file_id(&dest).unwrap().links, 2);

        let second = sync(&f.mirror, &ps, &mut no_discard()).unwrap();
        assert_eq!(second, SyncReport::default());
    }

    #[test]
    fn a_dropped_placement_is_unlinked_and_the_original_survives() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"photo");
        sync(
            &f.mirror,
            &[placement(&f.lib, "d/a.jpg")],
            &mut no_discard(),
        )
        .unwrap();

        let r = sync(&f.mirror, &[], &mut no_discard()).unwrap();
        assert_eq!(r.removed, 1);
        assert_eq!(std::fs::read(&src).unwrap(), b"photo");
        // 空になったフォルダは畳む。窓口そのものは残す
        assert!(!f.mirror.join("lib").exists());
        assert!(f.mirror.is_dir());
    }

    #[test]
    fn the_last_copy_goes_to_discard_instead_of_being_deleted() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"photo");
        sync(
            &f.mirror,
            &[placement(&f.lib, "d/a.jpg")],
            &mut no_discard(),
        )
        .unwrap();
        // 原本を pictkura の外で消し切った
        std::fs::remove_file(&src).unwrap();

        let mut handed = Vec::new();
        let r = sync(&f.mirror, &[], &mut |p: &Path| {
            handed.push(p.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!((r.removed, r.discarded), (0, 1));
        assert_eq!(handed, [f.mirror.join("lib/d/a.jpg")]);
        // discard が何もしなかったので、ファイルはまだ在る（消すのは呼び出し側の仕事）
        assert!(f.mirror.join("lib/d/a.jpg").exists());
    }

    #[test]
    fn a_replaced_original_is_relinked_through_the_staging_dir() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"v1");
        let ps = [placement(&f.lib, "d/a.jpg")];
        sync(&f.mirror, &ps, &mut no_discard()).unwrap();

        // 編集アプリが「別名で書いて改名」で保存した＝原本の実体が替わった
        let tmp = f.lib.join("d/a.tmp");
        put(&tmp, b"v2");
        std::fs::rename(&tmp, &src).unwrap();

        let mut handed = Vec::new();
        let r = sync(&f.mirror, &ps, &mut |p: &Path| {
            handed.push(p.to_path_buf());
            Ok(())
        })
        .unwrap();
        assert_eq!(r.replaced, 1);
        let dest = f.mirror.join("lib/d/a.jpg");
        assert_eq!(std::fs::read(&dest).unwrap(), b"v2");
        assert!(same_file(&src, &dest).unwrap());
        // 古い版は窓口にしか無かったので、上書きの前に渡した
        assert_eq!(handed, [dest]);
        // 作業場に残骸を残さない
        assert_eq!(
            std::fs::read_dir(staging_dir(&f.mirror)).unwrap().count(),
            0
        );
    }

    #[test]
    fn a_folder_with_someone_elses_files_is_refused() {
        let f = fixture();
        put(&f.mirror.join("mine.jpg"), b"not pictkura's");
        let err = sync(&f.mirror, &[], &mut no_discard()).unwrap_err();
        assert!(matches!(err, MirrorError::NotOurs(_)));
        assert!(f.mirror.join("mine.jpg").exists());
    }

    #[test]
    fn links_inside_the_mirror_are_not_followed() {
        let f = fixture();
        sync(&f.mirror, &[], &mut no_discard()).unwrap();
        let outside = f.lib.join("d");
        put(&outside.join("keep.jpg"), b"keep");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, f.mirror.join("link")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&outside, f.mirror.join("link")).is_err() {
            return; // 権限が無ければ作れない（spike の S4）
        }
        sync(&f.mirror, &[], &mut no_discard()).unwrap();
        assert!(outside.join("keep.jpg").exists());
    }

    #[test]
    fn the_mirror_may_not_overlap_a_root() {
        let roots = [PathBuf::from("/lib/Photos")];
        assert!(check_location(Path::new("/lib/Photos/g"), &roots).is_err());
        assert!(check_location(Path::new("/lib"), &roots).is_err());
        assert!(check_location(Path::new("/lib/pictkura-google"), &roots).is_ok());
    }

    #[test]
    fn the_default_location_for_a_root_on_the_home_volume_is_under_home() {
        let Some(home) = home_dir() else { return };
        let f = fixture();
        if !same_volume(&f.lib, &home).unwrap() {
            return; // 一時フォルダが別ボリュームの台では測れない
        }
        assert_eq!(
            location_for_root(&f.lib, &[]).unwrap(),
            home.join(MIRROR_DIR_NAME)
        );
        // 選んだ窓口が同じボリュームなら、まだ作っていなくてもそちら
        let chosen = f.lib.parent().unwrap().join("chosen");
        assert_eq!(
            location_for_root(&f.lib, std::slice::from_ref(&chosen)).unwrap(),
            chosen
        );
    }
}
