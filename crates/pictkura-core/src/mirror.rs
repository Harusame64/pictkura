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
        let Some((label, rest)) = owning_root(&item.path, roots, &labels) else {
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
///
/// **呼び出し側は窓口ごとに、その窓口へ来るルートだけを渡す**——別のドライブのルートが
/// 番号を食わないように。番号は並び順で決まるので、重なった片方を外すと残りの名前が
/// 変わり、窓口の中では貼り直しになる。**Google には二度上がらない**（中身で重複を弾く。
/// spike の S3・S11・S12）ので、損はローカルの貼り直しだけとして据え置く（ゲート2）。
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

/// `path` を持つルートのラベルと、ルートからの残り。入れ子なら**いちばん深い**ほう。
fn owning_root<'a>(
    path: &Path,
    roots: &[PathBuf],
    labels: &'a [String],
) -> Option<(&'a str, PathBuf)> {
    roots
        .iter()
        .zip(labels)
        .filter_map(|(root, label)| strip_root(path, root).map(|rest| (root, label, rest)))
        .filter(|(_, _, rest)| !rest.as_os_str().is_empty())
        .max_by_key(|(root, _, _)| root.components().count())
        .map(|(_, label, rest)| (label.as_str(), rest))
}

/// `path` が `root` の下なら残りを返す。**大文字小文字を区別しない台では畳んで比べる**
/// ——設定のルートが `D:\photos`、USN 経由で DB に入ったパスが `D:\Photos\…` のように
/// 綴りが割れることがある（`paths::normalize` が揃えるのはドライブ文字だけ。ゲート2の指摘）。
fn strip_root(path: &Path, root: &Path) -> Option<PathBuf> {
    let mut p = path.components();
    for r in root.components() {
        if fold(p.next()?.as_os_str()) != fold(r.as_os_str()) {
            return None;
        }
    }
    Some(p.as_path().to_path_buf())
}

/// 名前の比べ方。大文字小文字を区別しない台（Windows・macOS の既定）では小文字に畳む。
/// 区別する macOS のボリュームでは、違う2つを同じと見ることがある——窓口では
/// 「置かない・外す」側へ倒れるだけなので、それでよい。
fn fold(s: &std::ffi::OsStr) -> String {
    if cfg!(any(windows, target_os = "macos")) {
        s.to_string_lossy().to_lowercase()
    } else {
        s.to_string_lossy().into_owned()
    }
}

fn fold_path(p: &Path) -> PathBuf {
    p.components().map(|c| fold(c.as_os_str())).collect()
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
///
/// **綴りではなく実体で比べる**（[`disk_key`]）。`D:\Photos` と `d:\photos\pictkura-google`、
/// シンボリックリンク越しの別名、`..` を含む綴りは、文字列の前方一致では重ならないのに
/// ディスク上では重なる（ゲート1の指摘）。
pub fn check_location(mirror: &Path, roots: &[PathBuf]) -> Result<(), MirrorError> {
    let m = disk_key(mirror)?;
    for root in roots {
        let r = disk_key(root)?;
        if m.starts_with(&r) || r.starts_with(&m) {
            return Err(MirrorError::OverlapsRoot(root.clone()));
        }
    }
    for dir in sync_client_folders() {
        if disk_key(&dir).is_ok_and(|d| m.starts_with(d)) {
            return Err(MirrorError::InsideSyncFolder(dir));
        }
    }
    Ok(())
}

/// 比べるための、実体に寄せたパス。**在る**いちばん深い祖先を `canonicalize` で解決し
/// （リンク・`..`・短縮名が畳まれる）、まだ無い残りを足す。大文字小文字を区別しない
/// 台（Windows・macOS の既定）では小文字に畳む。残りに `..` があれば拒む。
fn disk_key(path: &Path) -> io::Result<PathBuf> {
    let existing = path
        .ancestors()
        .find(|a| !a.as_os_str().is_empty() && a.exists())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "在る祖先が無い"))?;
    let rest = path.strip_prefix(existing).unwrap_or(Path::new(""));
    if !rest.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "まだ無い部分に .. がある",
        ));
    }
    let resolved = std::fs::canonicalize(existing)?.join(rest);
    if cfg!(any(windows, target_os = "macos")) {
        Ok(PathBuf::from(resolved.to_string_lossy().to_lowercase()))
    } else {
        Ok(resolved)
    }
}

/// 同期クライアントのフォルダ。OneDrive はハードリンクを扱えず、Google ドライブの
/// 同期フォルダの中だと二重にアップロードされる（公式に明記）。
fn sync_client_folders() -> Vec<PathBuf> {
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
        .filter(|d| !d.as_os_str().is_empty())
        .collect()
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
    #[error("窓口フォルダ {0} がリンクになっている")]
    LinkInTheWay(PathBuf),
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
///
/// **費用は窓口の件数に比例する**（置いてある1件ごとに属性を2回引く。Windows では
/// ハンドルを2回開く）。10万件で何秒かかるかは**まだ測っていない**——配線する PR で
/// win の実機で測り、重ければ歩くときの属性を使い回す（ゲート2の指摘を据え置いた）。
pub fn sync(
    mirror: &Path,
    placements: &[Placement],
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> Result<SyncReport, MirrorError> {
    check_ownership(mirror)?;
    if is_link(mirror) {
        return Err(MirrorError::LinkInTheWay(mirror.to_path_buf()));
    }
    let staging = staging_dir(mirror);
    std::fs::create_dir_all(mirror)?;
    std::fs::create_dir_all(&staging)?;

    let mut report = SyncReport::default();

    // 0. 前の回が途中で落ちて作業場に残したリンクを片付ける。残すと原本の
    //    「名前の数」を1つ多く見せ、窓口にしか無い実体を見落とす（ゲート2）
    for entry in std::fs::read_dir(&staging)?.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            if let Err(e) = remove_or_discard(&entry.path(), discard) {
                report.failed.push((entry.path(), e.to_string()));
            }
        }
    }

    // 窓口の中の名前は**畳んで**突き合わせる。大文字小文字だけ違う改名
    // （`Trip` → `trip`）を別物と見ると、毎回フォルダごと外して貼り直す（ゲート2）。
    // 置いてあれば、ディスクの綴りのまま使い続ける
    let wanted: HashMap<PathBuf, &Placement> = placements
        .iter()
        .filter(|p| is_plain_relative(&p.rel))
        .map(|p| (fold_path(&p.rel), p))
        .collect();

    // 1–2. 今ある集合と、外すもの（畳んだ名前 → ディスクの綴り）
    let mut present: HashMap<PathBuf, PathBuf> = HashMap::new();
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
        let key = fold_path(rel);
        if wanted.contains_key(&key) && !present.contains_key(&key) {
            present.insert(key, rel.to_path_buf());
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
        let key = fold_path(&p.rel);
        if !wanted.get(&key).is_some_and(|w| std::ptr::eq(*w, p)) {
            continue;
        }
        let on_disk = present.get(&key);
        let dest = mirror.join(on_disk.unwrap_or(&p.rel));
        let result = if crate::cloud::is_cloud_only_path(&p.source) {
            // 置かない。既に置いてあれば（前は手元にあった）外す
            report.cloud_only += 1;
            if on_disk.is_some() {
                remove_or_discard(&dest, discard).map(|handed| {
                    if handed {
                        report.discarded += 1;
                    } else {
                        report.removed += 1;
                    }
                })
            } else {
                Ok(())
            }
        } else if on_disk.is_some() {
            match same_file(&p.source, &dest) {
                Ok(true) => Ok(()),
                Ok(false) => replace_link(&p.source, &dest, &staging, n, discard)
                    .map(|()| report.replaced += 1),
                Err(e) => Err(e),
            }
        } else {
            make_parent_dirs(mirror, &p.rel)
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

/// `rel` の親フォルダを窓口の中に作る。**途中にリンク・ジャンクションがあれば止める**
/// ——`create_dir_all` と `hard_link` はそれを辿るので、窓口の中に外を指すリンクが
/// 1つあるだけで窓口の外へ書いてしまう（ゲート1の指摘。歩くときに辿らないだけでは足りない）。
fn make_parent_dirs(mirror: &Path, rel: &Path) -> io::Result<()> {
    let mut dir = mirror.to_path_buf();
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    for c in parent.components() {
        dir.push(c);
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() && !is_link_meta(&m) => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("フォルダの場所にリンクかファイルがある: {}", dir.display()),
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&dir)?,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| is_link_meta(&m))
}

/// シンボリックリンクか、Windows のジャンクション（std はどちらも `is_symlink` で返す）。
fn is_link_meta(m: &std::fs::Metadata) -> bool {
    m.file_type().is_symlink()
}

/// 窓口のファイルを外す。ほかに名前があれば消すだけ（実体は原本の側に残る）。
/// **そこにしか実体が無ければ `discard` へ渡す**。渡したら `true`。
fn remove_or_discard(
    path: &Path,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<bool> {
    if file_id(path)?.links <= 1 {
        hand_over(path, discard)?;
        Ok(true)
    } else {
        std::fs::remove_file(path)?;
        Ok(false)
    }
}

/// **`discard` の約束**: `Ok` を返すなら、ファイルを `path` から**どかしてある**こと
/// （ゴミ箱へ移すなど）。印を付けただけで `Ok` を返されると、続く改名や後片付けが
/// 最後の1枚を上書きしてしまう（ゲート2）。だから返ったあとに確かめる。
fn hand_over(path: &Path, discard: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    discard(path)?;
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(io::Error::other(format!(
            "渡したファイルがまだ在る: {}",
            path.display()
        )));
    }
    Ok(())
}

/// 作業場でリンクを作り、**1回の改名**で置き換える。窓口の中に途中の名前を出さない。
///
/// **新しいリンクを先に作る**。古いほうを渡したあとで作れないと分かると、窓口の項目が
/// 消えたまま残る（ゲート2）。
fn replace_link(
    source: &Path,
    dest: &Path,
    staging: &Path,
    n: usize,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let tmp = staging.join(format!("{}-{n}.link", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    std::fs::hard_link(source, &tmp)?;
    let result = (|| {
        // 古いほうがそこにしか実体を持たないなら、上書きで消す前に渡す
        if file_id(dest)?.links <= 1 {
            hand_over(dest, discard)?;
        }
        std::fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
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
///
/// **返す前に [`check_location`] を通す**——ルートがドライブ丸ごと（`D:\`）やホームそのもの
/// だと、既定の場所はルートの中に落ちる（ゲート2）。そのときは誤りを返し、利用者に
/// 場所を選んでもらう。
pub fn location_for_root(
    root: &Path,
    locations: &[PathBuf],
    roots: &[PathBuf],
) -> Result<PathBuf, MirrorError> {
    let chosen = locations.iter().find(|loc| {
        // 窓口そのものか、その親が在ること。外付けが外れていると祖先は別の
        // ボリューム（macOS の `/Volumes`）に落ちるので、それより上では比べない（ゲート2）
        let probe = if loc.exists() {
            Some(loc.as_path())
        } else {
            loc.parent()
        };
        probe.is_some_and(|p| p.is_dir() && same_volume(root, p).unwrap_or(false))
    });
    let loc = match chosen {
        Some(loc) => loc.clone(),
        None => match home_dir() {
            Some(home) if same_volume(root, &home)? => home.join(MIRROR_DIR_NAME),
            _ => volume_top(root)?.join(MIRROR_DIR_NAME),
        },
    };
    check_location(&loc, roots)?;
    Ok(loc)
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
    fn a_root_spelled_in_another_case_still_owns_its_files() {
        let items = [item("/lib/Photos/d/1.jpg", false)];
        let got = plan(
            &items,
            &["/lib/photos".into()],
            &GoogleMirrorConfig::default(),
        );
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(rels(&got), ["photos/d/1.jpg"]);
        } else {
            assert!(got.is_empty());
        }
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

    /// ゴミ箱の代わり: `dir` へ移す
    fn move_into(dir: &Path) -> impl FnMut(&Path) -> io::Result<()> + '_ {
        move |p: &Path| {
            std::fs::create_dir_all(dir)?;
            std::fs::rename(p, dir.join(p.file_name().unwrap()))
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

        let bin = f.lib.join("bin");
        let r = sync(&f.mirror, &[], &mut move_into(&bin)).unwrap();
        assert_eq!((r.removed, r.discarded), (0, 1));
        assert!(!f.mirror.join("lib/d/a.jpg").exists());
        assert_eq!(std::fs::read(bin.join("a.jpg")).unwrap(), b"photo");
    }

    #[test]
    fn a_discard_that_only_takes_note_does_not_lose_the_last_copy() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"v1");
        let ps = [placement(&f.lib, "d/a.jpg")];
        sync(&f.mirror, &ps, &mut no_discard()).unwrap();
        // 原本の実体が替わった＝窓口の v1 はそこにしか無い
        let tmp = f.lib.join("d/a.tmp");
        put(&tmp, b"v2");
        std::fs::rename(&tmp, &src).unwrap();

        let r = sync(&f.mirror, &ps, &mut |_: &Path| Ok(())).unwrap();
        assert_eq!((r.replaced, r.failed.len()), (0, 1));
        assert_eq!(std::fs::read(f.mirror.join("lib/d/a.jpg")).unwrap(), b"v1");
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

        let bin = f.lib.join("bin");
        let r = sync(&f.mirror, &ps, &mut move_into(&bin)).unwrap();
        assert_eq!(r.replaced, 1);
        let dest = f.mirror.join("lib/d/a.jpg");
        assert_eq!(std::fs::read(&dest).unwrap(), b"v2");
        assert!(same_file(&src, &dest).unwrap());
        // 古い版は窓口にしか無かったので、上書きの前に渡した
        assert_eq!(std::fs::read(bin.join("a.jpg")).unwrap(), b"v1");
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
    fn a_link_inside_the_mirror_does_not_carry_new_files_outside() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        sync(&f.mirror, &[], &mut no_discard()).unwrap();
        let outside = f.lib.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, f.mirror.join("lib")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&outside, f.mirror.join("lib")).is_err() {
            return;
        }
        let r = sync(
            &f.mirror,
            &[placement(&f.lib, "d/a.jpg")],
            &mut no_discard(),
        )
        .unwrap();
        assert_eq!((r.linked, r.failed.len()), (0, 1));
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }

    #[test]
    fn the_mirror_may_not_overlap_a_root() {
        let f = fixture();
        let roots = [f.lib.clone()];
        assert!(check_location(&f.lib.join("g"), &roots).is_err());
        assert!(check_location(f.lib.parent().unwrap(), &roots).is_err());
        assert!(check_location(&f.mirror, &roots).is_ok());
        // `..` を含む綴りも、実体で比べれば中にある
        assert!(check_location(&f.lib.join("d/../g"), &roots).is_err());
    }

    #[test]
    fn an_alias_of_a_root_is_seen_through() {
        let f = fixture();
        let alias = f.mirror.with_file_name("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&f.lib, &alias).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&f.lib, &alias).is_err() {
            return;
        }
        let roots = [f.lib.clone()];
        assert!(check_location(&alias.join("pictkura-google"), &roots).is_err());
        // 大文字小文字を区別しない台では、綴りの違いも同じ場所。在る部分は canonicalize が
        // ディスクの綴りに直すので、効くのは**まだ無い部分**——そこを違えて比べる
        if cfg!(any(windows, target_os = "macos")) {
            let upper = PathBuf::from(f.lib.to_string_lossy().to_uppercase()).join("g");
            assert!(check_location(&upper, &roots).is_err());
            let not_yet = [f.lib.join("New")];
            assert!(check_location(&f.lib.join("NEW").join("g"), &not_yet).is_err());
        }
    }

    #[test]
    fn the_default_location_for_a_root_on_the_home_volume_is_under_home() {
        let Some(home) = home_dir() else { return };
        let f = fixture();
        if !same_volume(&f.lib, &home).unwrap() {
            return; // 一時フォルダが別ボリュームの台では測れない
        }
        let roots = [f.lib.clone()];
        assert_eq!(
            location_for_root(&f.lib, &[], &roots).unwrap(),
            home.join(MIRROR_DIR_NAME)
        );
        // 選んだ窓口が同じボリュームなら、まだ作っていなくてもそちら
        let chosen = f.lib.parent().unwrap().join("chosen");
        assert_eq!(
            location_for_root(&f.lib, std::slice::from_ref(&chosen), &roots).unwrap(),
            chosen
        );
        // 親ごと無い窓口（外れた外付け）は比べずに飛ばし、既定へ落ちる
        let unplugged = f.lib.parent().unwrap().join("gone").join(MIRROR_DIR_NAME);
        assert_eq!(
            location_for_root(&f.lib, std::slice::from_ref(&unplugged), &roots).unwrap(),
            home.join(MIRROR_DIR_NAME)
        );
        // ルートの中に落ちる場所は返さない
        let inside = f.lib.join(MIRROR_DIR_NAME);
        assert!(matches!(
            location_for_root(&f.lib, std::slice::from_ref(&inside), &roots),
            Err(MirrorError::OverlapsRoot(_))
        ));
    }

    #[test]
    fn a_case_only_rename_is_not_relinked_every_time() {
        if !cfg!(any(windows, target_os = "macos")) {
            return;
        }
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let old = Placement {
            source: f.lib.join("d/a.jpg"),
            rel: PathBuf::from("lib/Trip/a.jpg"),
        };
        sync(&f.mirror, std::slice::from_ref(&old), &mut no_discard()).unwrap();
        let renamed = Placement {
            rel: PathBuf::from("lib/trip/a.jpg"),
            ..old
        };
        let ps = [renamed];
        assert_eq!(
            sync(&f.mirror, &ps, &mut no_discard()).unwrap(),
            SyncReport::default()
        );
        assert_eq!(
            sync(&f.mirror, &ps, &mut no_discard()).unwrap(),
            SyncReport::default()
        );
    }

    #[test]
    fn leftovers_in_the_staging_dir_are_cleared_first() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"photo");
        sync(&f.mirror, &[], &mut no_discard()).unwrap();
        // 前の回が改名の手前で落ちた
        std::fs::hard_link(&src, staging_dir(&f.mirror).join("1-0.link")).unwrap();
        sync(&f.mirror, &[], &mut no_discard()).unwrap();
        assert_eq!(
            std::fs::read_dir(staging_dir(&f.mirror)).unwrap().count(),
            0
        );
        assert!(src.exists());
    }
}
