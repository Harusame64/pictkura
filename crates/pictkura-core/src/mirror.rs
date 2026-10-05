//! Google フォト用のフォルダへ、**取り込みのときに**原本のリンクを置く
//! （`dev/plan.google-photos-at-import.md`）。
//!
//! Google フォト Web の「フォルダをバックアップ」は、登録したフォルダを**丸ごと**上げる。
//! 形式で除く設定は無い。だから pictkura が、上げてよいものだけをドライブごとの
//! フォルダへ置き、利用者は Google フォトに**そこだけ**を登録する。
//!
//! 置くのは原本への**ハードリンク**——ディスクはほぼ使わない。pictkura は Google と
//! 1バイトも話さない。ここがするのは、ローカルのフォルダに名前を足すことと外すことだけ。
//!
//! **置いたものだけを記録し、記録したものだけを外す。** フォルダの中身を見て、
//! 持ち主や要不要を推し量ることはしない（#179 の窓口フォルダはそれをして、
//! 他人のフォルダを掃く道を何度も作った）。記録は呼び出し側（DB の `google_placed`）が持つ。
//!
//! 規則は実測から来ている（win: `dev/plan.google-photos-mirror.spike.md`、
//! mac: `dev/plan.google-photos-at-import.mac-spike.md`）:
//!
//! - **フォルダの中で書かない。** Google は名前が付いた瞬間に拾い、拡張子でも絞らない。
//!   書きかけを拾うと失敗し、**ページを読み込み直すまで取り戻さない**（S7b・M7）。
//!   ハードリンクは1回の操作なので途中の名前が出ない（S7c・M6）
//! - **リンクは辿らない。** シンボリックリンクもジャンクションも Google は辿らない
//!   （S4c・S5・M4・M5）。別のドライブの原本はリンクでは置けない
//! - **消しても Google からは消えない**（S8）。だから外すことは怖くない。
//!   怖いのは**そこにしか残っていない実体**を消すこと——それは呼び出し側（ゴミ箱）へ渡す

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::config::{GoogleMirrorConfig, RawOnly};
use crate::search::MediaKind;

/// Google 用フォルダの名前（ドライブごとの既定の場所で使う）。
pub const MIRROR_DIR_NAME: &str = "pictkura-google";

/// 置くかどうかを決める1件の材料（取り込んだ原本）。
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub favorite: bool,
}

/// 置くと決めた1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// 原本
    pub source: PathBuf,
    /// Google 用フォルダの中での場所（原本の、持ち主のルートからの相対パス）
    pub rel: PathBuf,
}

/// どれを置くかを決める。
///
/// - 動画は「動画を除く」なら置かない
/// - RAW は「RAW を除く」なら、同じフォルダ・同じ名前の写真（組＝[`crate::sidecar::pair_key`]）が
///   あれば置かない。RAW だけのカットは [`RawOnly`] に従う。**組は `items` の中と、
///   ディスクの上（`photo_on_disk`）の両方で探す**——RAW と JPEG を別の回に取り込んでも
///   組になるように（`items` はこの回の分だけ）
/// - OneDrive の中の原本は、設定で入れていなければ置かない（置くと OneDrive が
///   その原本を「オンラインのみ」にできなくなり、空き容量を増やせない）
/// - どのルートにも入らないものは置かない
///
/// 返す順は `items` の順。`photo_on_disk` は [`photos_on_disk`] を渡す（試験は偽物を渡す）。
pub fn plan(
    items: &[Item],
    roots: &[PathBuf],
    cfg: &GoogleMirrorConfig,
    photo_on_disk: &mut dyn FnMut(&Path) -> bool,
) -> Vec<Placement> {
    let onedrive = if cfg.include_onedrive {
        Vec::new()
    } else {
        onedrive_folders()
    };
    plan_with(items, roots, cfg, photo_on_disk, &onedrive)
}

fn plan_with(
    items: &[Item],
    roots: &[PathBuf],
    cfg: &GoogleMirrorConfig,
    photo_on_disk: &mut dyn FnMut(&Path) -> bool,
    onedrive: &[PathBuf],
) -> Vec<Placement> {
    let kinds: Vec<MediaKind> = items
        .iter()
        .map(|i| MediaKind::from_path(&i.path))
        .collect();
    // 組の相方になれる「写真」の鍵。RAW だけのカットかどうかはこれで決まる
    let photo_keys: HashSet<(PathBuf, std::ffi::OsString)> = items
        .iter()
        .zip(&kinds)
        .filter(|(_, k)| **k == MediaKind::Photo)
        .map(|(i, _)| pair_key_folded(&i.path))
        .collect();
    let onedrive: Vec<PathBuf> = onedrive.iter().filter_map(|d| disk_key(d).ok()).collect();

    let mut out = Vec::new();
    for (item, kind) in items.iter().zip(kinds) {
        let wanted = match kind {
            MediaKind::Photo => true,
            MediaKind::Video => !cfg.exclude_video,
            MediaKind::Raw if !cfg.exclude_raw => true,
            MediaKind::Raw => {
                let paired =
                    photo_keys.contains(&pair_key_folded(&item.path)) || photo_on_disk(&item.path);
                !paired
                    && match cfg.raw_only {
                        RawOnly::None => false,
                        RawOnly::Starred => item.favorite,
                        RawOnly::All => true,
                    }
            }
        };
        // OS の置き物の名前（`._IMG_1.JPG` は AppleDouble で写真ではない）は置かない
        if !wanted || item.path.file_name().is_some_and(is_os_litter) {
            continue;
        }
        if !onedrive.is_empty()
            && disk_key(&item.path).is_ok_and(|k| onedrive.iter().any(|d| k.starts_with(d)))
        {
            continue;
        }
        let Some(rel) = owning_root(&item.path, roots) else {
            continue;
        };
        if is_plain_relative(&rel) {
            out.push(Placement {
                source: item.path.clone(),
                rel,
            });
        }
    }
    out
}

/// RAW の組の相方（同じフォルダ・同じ名前の写真）がディスクの上に在るかを答える。
/// フォルダごとに1回だけ読む（取り込みの1回で同じ日のフォルダを何百回も引くため）。
///
/// 相方と見なすのは**取り込みの対象になる写真の拡張子**のものだけ——`A.xmp` のような
/// 添え物を写真と見ると、RAW だけのカットが組に見えて置かれなくなる
/// （[`MediaKind::from_path`] は知らない拡張子を写真と答える）。
pub fn photos_on_disk() -> impl FnMut(&Path) -> bool {
    let mut seen: HashMap<PathBuf, HashSet<std::ffi::OsString>> = HashMap::new();
    move |raw: &Path| {
        let (dir, stem) = pair_key_folded(raw);
        let Some(parent) = raw.parent() else {
            return false;
        };
        seen.entry(dir)
            .or_insert_with(|| {
                std::fs::read_dir(parent)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                    .map(|e| e.path())
                    .filter(|p| is_photo_extension(p) && !p.file_name().is_some_and(is_os_litter))
                    .map(|p| pair_key_folded(&p).1)
                    .collect()
            })
            .contains(&stem)
    }
}

fn is_photo_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    crate::config::DEFAULT_EXTENSIONS.contains(&ext.as_str())
        && MediaKind::from_path(path) == MediaKind::Photo
}

/// 組の鍵。[`crate::sidecar::pair_key`] に**フォルダの大文字小文字の畳み**を足したもの
/// ——USN 経由で同じフォルダが別の綴りで DB に入ると、組の RAW が「RAW だけ」に見えて
/// 上がってしまう。
///
/// 語幹は `pair_key` と同じく小文字に畳むが、**UTF-8 として読めない語幹はバイト列のまま**
/// 持つ（Linux。文字列へ写すと別の語幹が置換文字で1つに潰れ、RAW が組に見える）。
fn pair_key_folded(path: &Path) -> (PathBuf, std::ffi::OsString) {
    let dir = fold_path(path.parent().unwrap_or(Path::new("")));
    let stem = path.file_stem().unwrap_or_default();
    let stem = match stem.to_str() {
        Some(s) => s.to_lowercase().into(),
        None => stem.to_os_string(),
    };
    (dir, stem)
}

/// `path` を持つルートからの残り。入れ子なら**いちばん深い**ルート。
///
/// フォルダの形は取り込み先と同じにする（`D:\photos\2026年\…` → `D:\pictkura-google\2026年\…`）。
/// 同じドライブの別のルートから同じ相対パスが来たら、2本目は [`place`] で
/// 「同じ名前が既にある」として失敗する（上書きはしない）。
fn owning_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .filter_map(|root| strip_root(path, root).map(|rest| (root, rest)))
        .filter(|(_, rest)| !rest.as_os_str().is_empty())
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, rest)| rest)
}

/// `path` が `root` の下なら残りを返す。**大文字小文字を区別しない台では畳んで比べる**
/// ——設定のルートが `D:\photos`、USN 経由で DB に入ったパスが `D:\Photos\…` のように
/// 綴りが割れることがある（`paths::normalize` が揃えるのはドライブ文字だけ）。
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
/// **区別する台ではバイト列のまま**比べる。Linux の名前は UTF-8 とは限らず、文字列へ
/// 写すと別の名前が置換文字で1つに潰れる。
fn fold(s: &std::ffi::OsStr) -> std::ffi::OsString {
    if cfg!(any(windows, target_os = "macos")) {
        s.to_string_lossy().to_lowercase().into()
    } else {
        s.to_os_string()
    }
}

fn fold_path(p: &Path) -> PathBuf {
    p.components().map(|c| fold(c.as_os_str())).collect()
}

/// フォルダの外を指さない相対パスか（`..`・絶対・ドライブ付きを拒む）。
/// **Google 用フォルダの外を決して触らない**ための門で、ここを通ったものだけを `dir.join` する。
fn is_plain_relative(rel: &Path) -> bool {
    !rel.as_os_str().is_empty() && rel.components().all(|c| matches!(c, Component::Normal(_)))
}

/// 置いた1件（記録の1行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// 原本
    pub source: PathBuf,
    /// Google 用フォルダの中のリンク
    pub link: PathBuf,
}

/// [`place`] の結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PlaceReport {
    /// 新しく置いたリンク
    pub placed: usize,
    /// 同じ実体が既にその名前で在った（記録は残す）
    pub already: usize,
    /// クラウドのみなので置かなかったもの（置くと Google が読みに行った瞬間に取り寄せが走る）
    pub cloud_only: usize,
    /// 失敗（原本と理由）
    pub failed: Vec<(PathBuf, String)>,
}

/// `placements` を `dir`（Google 用フォルダ）へハードリンクで置く。
///
/// **記録してからリンクする。** `record` が `Ok` を返した1件だけを置く——リンクだけが
/// 残って記録に無いと、二度と外せない（外すのは記録したものだけなので）。逆に、記録だけが
/// 残る（記録のあとで落ちた）のは害が無い: [`unplace`] は無いリンクを「もう無い」として
/// 記録から消す。`record` は**新しく書いたら `true`**、既に在ったら `false` を返す。
/// 置けなかったときは、新しく書いた記録だけを `unrecord` で取り消す（前から在った記録は
/// 前に置いたリンクのものなので残す）。
///
/// 在る名前は**上書きしない**。同じ実体なら済み（[`PlaceReport::already`]）、違えば失敗。
/// 原本がシンボリックリンクのもの・クラウドのみのものは置かない。
///
/// 1件ずつの失敗は [`PlaceReport::failed`] に積んで続ける。**同じフォルダに対して同時に
/// 2本走らせないこと**（呼び出し側が1本ずつ回す）。
pub fn place(
    dir: &Path,
    roots: &[PathBuf],
    placements: &[Placement],
    record: &mut dyn FnMut(&Placed) -> io::Result<bool>,
    unrecord: &mut dyn FnMut(&Placed),
) -> Result<PlaceReport, MirrorError> {
    check_dir(dir, roots)?;
    std::fs::create_dir_all(dir)?;
    // 作った直後にもう一度見る（作る前は無かったので、リンクかどうかは作ってから分かる）
    if is_link(dir) {
        return Err(MirrorError::LinkInTheWay(dir.to_path_buf()));
    }

    let mut report = PlaceReport::default();
    for p in placements {
        let fail = |report: &mut PlaceReport, why: String| {
            report.failed.push((p.source.clone(), why));
        };
        if !is_plain_relative(&p.rel) {
            fail(
                &mut report,
                format!("フォルダの外を指す: {}", p.rel.display()),
            );
            continue;
        }
        if is_link(&p.source) {
            // リンクそのものに張る台と先に張る台があり、どちらにしても Google は辿らない
            fail(&mut report, "原本がシンボリックリンク".into());
            continue;
        }
        if crate::cloud::is_cloud_only_path(&p.source) {
            report.cloud_only += 1;
            continue;
        }
        let placed = Placed {
            source: p.source.clone(),
            link: dir.join(&p.rel),
        };
        if let Err(e) = make_parent_dirs(dir, &p.rel) {
            fail(&mut report, e.to_string());
            continue;
        }
        let inserted = match record(&placed) {
            Ok(inserted) => inserted,
            Err(e) => {
                fail(&mut report, format!("記録できない: {e}"));
                continue;
            }
        };
        match std::fs::hard_link(&p.source, &placed.link) {
            Ok(()) => report.placed += 1,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if !is_link(&placed.link) && same_file(&p.source, &placed.link).unwrap_or(false) {
                    report.already += 1;
                } else {
                    if inserted {
                        unrecord(&placed);
                    }
                    fail(
                        &mut report,
                        format!("同じ名前が既にある: {}", placed.link.display()),
                    );
                }
            }
            Err(e) => {
                if inserted {
                    unrecord(&placed);
                }
                fail(&mut report, e.to_string());
            }
        }
    }
    Ok(report)
}

/// [`unplace`] の結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnplaceReport {
    /// 外したリンク（実体は原本の側に残る）
    pub removed: usize,
    /// **そこにしか実体が無かった**ので `discard` へ渡したもの
    pub discarded: usize,
    /// もう無かった（または pictkura の置いた形ではなかったので触らなかった）
    pub gone: usize,
    /// 記録から消してよいリンク（上の3つの合計と同じ数）
    pub forget: Vec<PathBuf>,
    /// 失敗（リンクと理由）。記録は残す——次の回にまた試す
    pub failed: Vec<(PathBuf, String)>,
}

/// 記録にあるリンクを `dir`（Google 用フォルダ）から外す。Google フォトからは消えない。
///
/// - ほかに名前があれば（原本が在る）、名前を消すだけ
/// - **そこにしか実体が無ければ消さずに `discard` へ渡す**——原本を pictkura の外で
///   消し切った写真の、最後の1枚かもしれない
/// - もう無いもの・普通のファイルでないもの（リンク・フォルダ）は触らずに記録から消す
///   ——pictkura が置くのはハードリンク（普通のファイル）だけ
/// - `dir` の外を指す記録・途中にリンクを挟む記録は触らない（失敗として返す）
///
/// 外したあと、空になったフォルダを畳む（`dir` そのものは残す）。
pub fn unplace(
    dir: &Path,
    links: &[PathBuf],
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> UnplaceReport {
    let mut report = UnplaceReport::default();
    let mut parents: Vec<PathBuf> = Vec::new();
    for link in links {
        let rel = match link.strip_prefix(dir) {
            Ok(rel) if is_plain_relative(rel) => rel,
            _ => {
                report
                    .failed
                    .push((link.clone(), "Google 用フォルダの外を指す".into()));
                continue;
            }
        };
        if let Some(at) = link_on_the_way(dir, rel) {
            report.failed.push((
                link.clone(),
                format!("途中にリンクがある: {}", at.display()),
            ));
            continue;
        }
        let meta = match std::fs::symlink_metadata(link) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                report.gone += 1;
                report.forget.push(link.clone());
                continue;
            }
            Err(e) => {
                report.failed.push((link.clone(), e.to_string()));
                continue;
            }
        };
        if !meta.is_file() || is_link_meta(&meta) {
            report.gone += 1;
            report.forget.push(link.clone());
            continue;
        }
        match remove_or_discard(link, discard) {
            Ok(handed) => {
                if handed {
                    report.discarded += 1;
                } else {
                    report.removed += 1;
                }
                report.forget.push(link.clone());
                if let Some(p) = link.parent() {
                    parents.push(p.to_path_buf());
                }
            }
            Err(e) => report.failed.push((link.clone(), e.to_string())),
        }
    }
    fold_empty_dirs(dir, parents);
    report
}

/// `dir` の中で、`rel` の親までの途中にリンク（ジャンクション）があればその場所を返す。
/// 在って辿ると `dir` の外のファイルを消すことになる。
fn link_on_the_way(dir: &Path, rel: &Path) -> Option<PathBuf> {
    if is_link(dir) {
        return Some(dir.to_path_buf());
    }
    let mut at = dir.to_path_buf();
    for c in rel.parent()?.components() {
        at.push(c);
        if is_link(&at) {
            return Some(at);
        }
    }
    None
}

/// 空になったフォルダを深いほうから畳む。`dir` そのものと、その外には上がらない。
/// OS が置いたもの（`.DS_Store` 等）しか残っていなければ、それごと畳む——開いただけで
/// 書かれるので、残すと空のアルバムが残り続ける。
fn fold_empty_dirs(dir: &Path, starts: Vec<PathBuf>) {
    let mut todo: Vec<PathBuf> = Vec::new();
    for start in starts {
        let mut d = start;
        while d != dir && d.starts_with(dir) {
            if !todo.contains(&d) {
                todo.push(d.clone());
            }
            if !d.pop() {
                break;
            }
        }
    }
    todo.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for d in todo {
        if is_link(&d) {
            continue;
        }
        let only_litter = std::fs::read_dir(&d).is_ok_and(|rd| {
            rd.flatten()
                .all(|e| e.file_type().is_ok_and(|t| t.is_file()) && is_os_litter(&e.file_name()))
        });
        if only_litter {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        // 中身があれば失敗する＝それで正しい
        let _ = std::fs::remove_dir(&d);
    }
}

/// Google 用フォルダとして使ってよいか（[`place`] が毎回見る門）。
fn check_dir(dir: &Path, roots: &[PathBuf]) -> Result<(), MirrorError> {
    if dir.file_name().is_none() {
        return Err(MirrorError::NoName(dir.to_path_buf()));
    }
    // 記録した場所でも毎回見る——あとからルートを足すと Google 用フォルダがルートの中に入り、
    // リンクがライブラリに二重に載る
    check_location(dir, roots)?;
    if is_link(dir) {
        return Err(MirrorError::LinkInTheWay(dir.to_path_buf()));
    }
    Ok(())
}

/// Google 用フォルダの場所として使ってよいか。ルートの中・ルートを含む場所は、
/// ライブラリに二重に載るので拒む。同期フォルダの中も拒む。
///
/// **綴りではなく実体で比べる**（[`disk_key`]）。`D:\Photos` と `d:\photos\pictkura-google`、
/// シンボリックリンク越しの別名、`..` を含む綴りは、文字列の前方一致では重ならないのに
/// ディスク上では重なる。
pub fn check_location(dir: &Path, roots: &[PathBuf]) -> Result<(), MirrorError> {
    let m = disk_key(dir)?;
    for root in roots {
        // 解決できないルート（外れたドライブ）は、いまは重なりようが無い。1つのせいで
        // 全部を止めない。戻ってきたら次の確認で見る
        let Ok(r) = disk_key(root) else {
            continue;
        };
        if m.starts_with(&r) || r.starts_with(&m) {
            return Err(MirrorError::OverlapsRoot(root.clone()));
        }
    }
    for d in sync_client_folders() {
        if disk_key(&d).is_ok_and(|d| m.starts_with(d)) {
            return Err(MirrorError::InsideSyncFolder(d));
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
    let home_dirs = home_dir()
        .map(|h| {
            vec![
                h.join("Library").join("CloudStorage"),
                h.join("Google Drive"),
                // パソコン版 Google ドライブのミラーの既定と、Windows の Dropbox
                h.join("My Drive"),
                h.join("Dropbox"),
                // iCloud Drive。「デスクトップと書類」を入れていると、その2つも同期される
                // ——入っているかは外から分からないので、macOS では両方とも拒む
                h.join("Library").join("Mobile Documents"),
                // iCloud for Windows
                h.join("iCloudDrive"),
                h.join("Pictures").join("iCloud Photos"),
                #[cfg(target_os = "macos")]
                h.join("Desktop"),
                #[cfg(target_os = "macos")]
                h.join("Documents"),
            ]
        })
        .unwrap_or_default();
    onedrive_env_folders()
        .chain(home_dirs)
        .filter(|d| !d.as_os_str().is_empty())
        .collect()
}

fn onedrive_env_folders() -> impl Iterator<Item = PathBuf> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
}

/// OneDrive の同期フォルダ（Windows は環境変数、macOS は `~/Library/CloudStorage/OneDrive-*`）。
fn onedrive_folders() -> Vec<PathBuf> {
    let mac = home_dir()
        .map(|h| h.join("Library").join("CloudStorage"))
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("OneDrive"))
        });
    onedrive_env_folders()
        .chain(mac)
        .filter(|d| !d.as_os_str().is_empty())
        .collect()
}

use crate::paths::home_dir;

/// Google 用フォルダで起きる、続けられない誤り。
#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("Google 用フォルダがライブラリのフォルダ {0} と重なっている")]
    OverlapsRoot(PathBuf),
    #[error("Google 用フォルダが同期フォルダ {0} の中にある")]
    InsideSyncFolder(PathBuf),
    #[error("{0} のドライブはハードリンクを張れない（exFAT・FAT 等）: {1}")]
    NoHardLinks(PathBuf, io::Error),
    #[error("ライブラリのフォルダ {0} はドライブ丸ごとなので、同じドライブに Google 用フォルダを置く場所が無い")]
    RootIsWholeVolume(PathBuf),
    #[error("Google 用フォルダ {0} はフォルダの名前を持たない（ドライブそのもの等）")]
    NoName(PathBuf),
    #[error("Google 用フォルダ {0} がリンクになっている")]
    LinkInTheWay(PathBuf),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// `dir` にハードリンクを1本張って消す。張れなければ [`MirrorError::NoHardLinks`]。
/// 設定画面で場所を選んだときに、**Google 用フォルダの親**に対して呼ぶ
/// ——フォルダの中で試すと、Google がその一瞬のファイルを拾って失敗を出す（S7b）。
///
/// **自分で作れたものだけを消す**。同じ名前が先に在れば別の番号を試す——在るものへ
/// 書くと、それがリンクなら外の原本を空にしてしまう。
pub fn probe_hard_links(dir: &Path) -> Result<(), MirrorError> {
    let pid = std::process::id();
    for n in 0..16 {
        let a = dir.join(format!("{PROBE_PREFIX}{pid}-{n}-a"));
        let b = dir.join(format!("{PROBE_PREFIX}{pid}-{n}-b"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&a)
        {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
        let linked = std::fs::hard_link(&a, &b);
        let _ = std::fs::remove_file(&a);
        return match linked {
            Ok(()) => {
                let _ = std::fs::remove_file(&b);
                Ok(())
            }
            // 落ちた回の残りが居た。張れたかは分からないので、次の番号で試し直す
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => Err(MirrorError::NoHardLinks(dir.to_path_buf(), e)),
        };
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists, "試しの名前が空いていない").into())
}

/// ハードリンクが張れるかの試しに付ける名前の頭。
const PROBE_PREFIX: &str = "pictkura-probe-";

/// OS が利用者の知らないうちに置くファイルか。
fn is_os_litter(name: &std::ffi::OsStr) -> bool {
    let n = name.to_string_lossy();
    n.starts_with("._")
        || [".DS_Store", "desktop.ini", "Thumbs.db"]
            .iter()
            .any(|l| n.eq_ignore_ascii_case(l))
}

/// `rel` の親フォルダを `dir` の中に作る。**途中にリンク・ジャンクションがあれば止める**
/// ——`create_dir_all` と `hard_link` はそれを辿るので、中に外を指すリンクが
/// 1つあるだけで `dir` の外へ書いてしまう。
fn make_parent_dirs(dir: &Path, rel: &Path) -> io::Result<()> {
    let mut at = dir.to_path_buf();
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    for c in parent.components() {
        at.push(c);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.is_dir() && !is_link_meta(&m) => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("フォルダの場所にリンクかファイルがある: {}", at.display()),
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&at)?,
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

/// リンクを外す。ほかに名前があれば消すだけ（実体は原本の側に残る）。
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
/// （ゴミ箱へ移すなど）。印を付けただけで `Ok` を返されると、記録から消えて
/// 最後の1枚が誰の手にも残らない。だから返ったあとに確かめる。
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

/// 2つのパスが同じ実体か。
fn same_file(a: &Path, b: &Path) -> io::Result<bool> {
    let (a, b) = (file_id(a)?, file_id(b)?);
    Ok(a.volume == b.volume && a.index == b.index)
}

/// 2つのパスが同じボリュームか（ハードリンクが張れるか）。どちらも在ること。
pub fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    Ok(volume_of(a)? == volume_of(b)?)
}

/// 実体の名札。ボリューム・ボリューム内の番号・名前の数。
struct FileId {
    volume: u64,
    index: u64,
    links: u64,
}

/// どのボリュームか。**リンクは辿る**——ホームの中のリンクが外付けを指すルートは、
/// 外付けの側に Google 用フォルダが要る。消す判断に使う [`file_id`] は辿らない。
fn volume_of(path: &Path) -> io::Result<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(std::fs::metadata(path)?.dev())
    }
    #[cfg(windows)]
    {
        Ok(file_id_with(path, true)?.volume)
    }
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
    file_id_with(path, false)
}

/// `follow` が偽ならリンク（ジャンクション）そのものを開く。
#[cfg(windows)]
fn file_id_with(path: &Path, follow: bool) -> io::Result<FileId> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
    };
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(if follow {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
        })
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

/// `root` の Google 用フォルダの場所。`locations`（利用者が選んだ場所）に同じボリュームの
/// ものがあればそれ、無ければ既定——ホームと同じボリュームならホームの直下、
/// そうでなければそのボリュームのいちばん上。**AppData の下は使わない**
/// （Chromium のフォルダ選択が弾く）。
///
/// **返す前に [`check_location`] を通す**——ルートがドライブ丸ごと（`D:\`）やホームそのもの
/// だと、既定の場所はルートの中に落ちる。そのときは誤りを返し、利用者に場所を選んでもらう。
pub fn location_for_root(
    root: &Path,
    locations: &[PathBuf],
    roots: &[PathBuf],
) -> Result<PathBuf, MirrorError> {
    let chosen = locations.iter().find(|loc| {
        // その場所か、その親が在ること。外付けが外れていると祖先は別の
        // ボリューム（macOS の `/Volumes`）に落ちるので、それより上では比べない
        let probe = if loc.exists() {
            Some(loc.as_path())
        } else {
            loc.parent()
        };
        probe.is_some_and(|p| p.is_dir() && same_volume(root, p).unwrap_or(false))
    });
    // ドライブ丸ごとのルート（SD カードの `E:\` 等）は、同じドライブのどこに置いても
    // ルートの中に入り、別のドライブではリンクが張れない
    if volume_top(root).is_ok_and(|top| {
        // 両側とも `\\?\` を外して比べる（`volume_top` は外して返す）
        std::fs::canonicalize(root)
            .is_ok_and(|r| fold_path(&without_verbatim(r)) == fold_path(&top))
    }) {
        return Err(MirrorError::RootIsWholeVolume(root.to_path_buf()));
    }
    let loc = match chosen {
        Some(loc) => loc.clone(),
        None => match home_dir() {
            // ホームが読めない（移動プロファイルがまだ来ていない等）なら、ボリュームの頭へ
            Some(home) if same_volume(root, &home).unwrap_or(false) => home.join(MIRROR_DIR_NAME),
            _ => volume_top(root)?.join(MIRROR_DIR_NAME),
        },
    };
    check_location(&loc, roots)?;
    Ok(loc)
}

/// `path` と同じボリュームの、いちばん上のフォルダ（Windows ならドライブ、macOS なら
/// `/Volumes/名前`）。親へ上がってボリュームが変わる手前で止める。
fn volume_top(path: &Path) -> io::Result<PathBuf> {
    // リンクの先のボリュームで上がる。リンクの綴りのまま上がると、ホームのボリュームへ戻る
    let real = std::fs::canonicalize(path)?;
    let vol = volume_of(&real)?;
    let mut top = real.clone();
    for a in real.ancestors().skip(1) {
        match volume_of(a) {
            Ok(v) if v == vol => top = a.to_path_buf(),
            _ => break,
        }
    }
    Ok(without_verbatim(top))
}

/// Windows の `canonicalize` が付ける `\\?\` を外す。利用者に見せ、設定に書き、
/// Google フォトのフォルダ選択で選んでもらう場所なので、ふだんの綴りに戻す。
fn without_verbatim(p: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return p;
    }
    let s = p.to_string_lossy();
    if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
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

    /// ディスクに相方が居ない
    fn nothing_on_disk() -> impl FnMut(&Path) -> bool {
        |_: &Path| false
    }

    fn plan_of(items: &[Item], roots: &[PathBuf], c: &GoogleMirrorConfig) -> Vec<Placement> {
        plan_with(items, roots, c, &mut nothing_on_disk(), &[])
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
        let got = plan_of(&shoot(), &[ROOT.into()], &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["d/a.jpg", "d/clip.mp4"]);
    }

    #[test]
    fn raw_only_shots_follow_the_three_way_setting() {
        let roots = [PathBuf::from(ROOT)];
        let starred = plan_of(&shoot(), &roots, &cfg(RawOnly::Starred));
        assert_eq!(rels(&starred), ["d/a.jpg", "d/C.CR3", "d/clip.mp4"]);
        let all = plan_of(&shoot(), &roots, &cfg(RawOnly::All));
        assert_eq!(rels(&all), ["d/a.jpg", "d/B.ARW", "d/C.CR3", "d/clip.mp4"]);
    }

    #[test]
    fn a_paired_raw_stays_home_even_when_starred_and_all_is_chosen() {
        let items = [
            item("/lib/Photos/d/A.ARW", true),
            item("/lib/Photos/d/A.JPG", false),
        ];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::All));
        assert_eq!(rels(&got), ["d/A.JPG"]);
    }

    #[test]
    fn a_partner_already_on_disk_pairs_a_raw_imported_later() {
        // JPEG は前の回に取り込み済み。この回は RAW だけ
        let items = [item("/lib/Photos/d/A.ARW", true)];
        let mut on_disk = |p: &Path| p.file_stem() == Some(std::ffi::OsStr::new("A"));
        let got = plan_with(
            &items,
            &[ROOT.into()],
            &cfg(RawOnly::All),
            &mut on_disk,
            &[],
        );
        assert!(got.is_empty());
    }

    #[test]
    fn a_video_is_not_a_partner_for_a_raw() {
        let items = [
            item("/lib/Photos/d/A.ARW", false),
            item("/lib/Photos/d/A.MOV", false),
        ];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::All));
        assert_eq!(rels(&got), ["d/A.ARW", "d/A.MOV"]);
    }

    #[test]
    fn excluding_video_and_including_raw_are_both_honoured() {
        let c = GoogleMirrorConfig {
            exclude_raw: false,
            exclude_video: true,
            ..GoogleMirrorConfig::default()
        };
        let got = plan_of(&shoot(), &[ROOT.into()], &c);
        assert_eq!(rels(&got), ["d/A.ARW", "d/a.jpg", "d/B.ARW", "d/C.CR3"]);
    }

    #[test]
    fn a_file_belongs_to_the_deepest_root_and_strays_are_skipped() {
        let roots = [PathBuf::from("/lib"), PathBuf::from("/lib/Photos")];
        let items = [
            item("/lib/Photos/1.jpg", false),
            item("/lib/x.jpg", false),
            item("/elsewhere/2.jpg", false),
        ];
        let got = plan_of(&items, &roots, &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["1.jpg", "x.jpg"]);
    }

    #[test]
    fn a_root_spelled_in_another_case_still_owns_its_files() {
        let items = [item("/lib/Photos/d/1.jpg", false)];
        let got = plan_of(
            &items,
            &["/lib/photos".into()],
            &GoogleMirrorConfig::default(),
        );
        if cfg!(any(windows, target_os = "macos")) {
            assert_eq!(rels(&got), ["d/1.jpg"]);
        } else {
            assert!(got.is_empty());
        }
    }

    #[test]
    fn a_pair_split_by_folder_case_is_still_a_pair() {
        if !cfg!(any(windows, target_os = "macos")) {
            return;
        }
        let items = [
            item("/lib/Photos/Trip/A.ARW", true),
            item("/lib/Photos/trip/A.JPG", false),
        ];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::All));
        assert_eq!(rels(&got), ["trip/A.JPG"]);
    }

    #[test]
    fn os_litter_is_never_planned() {
        let items = [item("/lib/Photos/d/._a.jpg", false)];
        assert!(plan_of(&items, &[ROOT.into()], &GoogleMirrorConfig::default()).is_empty());
    }

    #[test]
    fn originals_inside_onedrive_stay_home_unless_included() {
        let f = fixture();
        let od = f.lib.join("OneDrive");
        put(&od.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/b.jpg"), b"photo");
        let items = [
            item(od.join("d/a.jpg").to_str().unwrap(), false),
            item(f.lib.join("d/b.jpg").to_str().unwrap(), false),
        ];
        let roots = [f.lib.clone()];
        let c = GoogleMirrorConfig::default();
        let got = plan_with(
            &items,
            &roots,
            &c,
            &mut nothing_on_disk(),
            std::slice::from_ref(&od),
        );
        assert_eq!(rels(&got), ["d/b.jpg"]);
        // 入れていれば、OneDrive の場所を渡さない（`plan` がそうする）
        let got = plan_with(&items, &roots, &c, &mut nothing_on_disk(), &[]);
        assert_eq!(rels(&got), ["OneDrive/d/a.jpg", "d/b.jpg"]);
    }

    #[test]
    fn only_photo_extensions_on_disk_count_as_a_partner() {
        let f = fixture();
        put(&f.lib.join("d/A.ARW"), b"raw");
        put(&f.lib.join("d/A.xmp"), b"sidecar");
        put(&f.lib.join("d/B.ARW"), b"raw");
        put(&f.lib.join("d/b.JPG"), b"photo");
        let mut on_disk = photos_on_disk();
        assert!(!on_disk(&f.lib.join("d/A.ARW")));
        assert!(on_disk(&f.lib.join("d/B.ARW")));
    }

    #[test]
    fn only_plain_relative_paths_pass_the_gate() {
        assert!(is_plain_relative(Path::new("a/b.jpg")));
        assert!(!is_plain_relative(Path::new("../b.jpg")));
        assert!(!is_plain_relative(Path::new("a/../../b.jpg")));
        assert!(!is_plain_relative(Path::new("/b.jpg")));
        assert!(!is_plain_relative(Path::new("")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_stems_keep_distinct_pair_keys() {
        use std::os::unix::ffi::OsStrExt;
        let a = Path::new("/d").join(std::ffi::OsStr::from_bytes(b"\xff.ARW"));
        let b = Path::new("/d").join(std::ffi::OsStr::from_bytes(b"\xfe.JPG"));
        assert_ne!(pair_key_folded(&a), pair_key_folded(&b));
    }

    // ---- 実ファイル ----

    struct Fixture {
        _dir: tempfile::TempDir,
        lib: PathBuf,
        google: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        std::fs::create_dir_all(lib.join("d")).unwrap();
        let google = dir.path().join(MIRROR_DIR_NAME);
        Fixture {
            lib,
            google,
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
            rel: PathBuf::from(rel),
        }
    }

    /// 記録の代わり（DB の `google_placed`）
    #[derive(Default)]
    struct Book(Vec<Placed>);

    impl Book {
        fn place(&mut self, f: &Fixture, ps: &[Placement]) -> PlaceReport {
            let rows = std::cell::RefCell::new(std::mem::take(&mut self.0));
            let r = place(
                &f.google,
                std::slice::from_ref(&f.lib),
                ps,
                &mut |p: &Placed| {
                    let mut rows = rows.borrow_mut();
                    if rows.iter().any(|r| r.link == p.link) {
                        return Ok(false);
                    }
                    rows.push(p.clone());
                    Ok(true)
                },
                &mut |p: &Placed| rows.borrow_mut().retain(|r| r.link != p.link),
            )
            .unwrap();
            self.0 = rows.into_inner();
            r
        }

        fn links(&self) -> Vec<PathBuf> {
            self.0.iter().map(|p| p.link.clone()).collect()
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
    fn placing_links_without_copying_and_records_first() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.already, r.failed.len()), (1, 0, 0));
        let link = f.google.join("d/a.jpg");
        assert!(same_file(&f.lib.join("d/a.jpg"), &link).unwrap());
        assert_eq!(book.links(), [link]);
        // 2回目は同じ実体なので済み。記録は1行のまま
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.already), (0, 1));
        assert_eq!(book.0.len(), 1);
    }

    #[test]
    fn a_different_file_at_the_name_is_not_overwritten_and_not_recorded() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.google.join("d/a.jpg"), b"someone else's");
        let mut book = Book::default();
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert_eq!(
            std::fs::read(f.google.join("d/a.jpg")).unwrap(),
            b"someone else's"
        );
        assert!(book.0.is_empty());
    }

    #[test]
    fn a_failed_link_keeps_a_record_that_was_there_before() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        // 外で原本を差し替えた: 同じ名前の別の実体
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        put(&f.lib.join("d/a.jpg"), b"new photo");
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!(r.failed.len(), 1);
        // 前に置いたリンクの記録は残る（残さないと二度と外せない）
        assert_eq!(book.links(), [f.google.join("d/a.jpg")]);
    }

    #[test]
    fn a_link_that_cannot_be_made_keeps_an_older_record() {
        let f = fixture();
        let mut book = Book::default();
        // 前の回の記録だけが残っている（リンクは外で消された）。原本ももう無い
        let link = f.google.join("d/a.jpg");
        book.0.push(Placed {
            source: f.lib.join("d/a.jpg"),
            link: link.clone(),
        });
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert_eq!(book.links(), [link]);
    }

    #[test]
    fn a_link_inside_the_folder_does_not_carry_new_files_outside() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let outside = f.lib.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&f.google).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, f.google.join("d")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&outside, f.google.join("d")).is_err() {
            return;
        }
        let mut book = Book::default();
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        assert!(book.0.is_empty());
    }

    #[test]
    fn a_symlinked_source_is_not_placed() {
        let f = fixture();
        put(&f.lib.join("d/real.jpg"), b"photo");
        #[cfg(unix)]
        std::os::unix::fs::symlink(f.lib.join("d/real.jpg"), f.lib.join("d/a.jpg")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(f.lib.join("d/real.jpg"), f.lib.join("d/a.jpg"))
            .is_err()
        {
            return;
        }
        let mut book = Book::default();
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert!(book.0.is_empty());
    }

    #[test]
    fn placing_refuses_a_folder_that_a_root_now_contains() {
        let f = fixture();
        let wider = [f.google.parent().unwrap().to_path_buf()];
        let err = place(&f.google, &wider, &[], &mut |_| Ok(true), &mut |_| {}).unwrap_err();
        assert!(matches!(err, MirrorError::OverlapsRoot(_)));
    }

    #[test]
    fn unplacing_removes_only_the_name_and_folds_empty_albums() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        put(&f.google.join("d/.DS_Store"), b"finder");
        let r = unplace(&f.google, &book.links(), &mut no_discard());
        assert_eq!((r.removed, r.discarded, r.gone), (1, 0, 0));
        assert_eq!(r.forget, book.links());
        assert!(f.lib.join("d/a.jpg").exists());
        assert!(!f.google.join("d").exists());
        assert!(f.google.exists());
    }

    #[test]
    fn the_last_copy_goes_to_discard_instead_of_being_deleted() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        // pictkura の外で原本を消した
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        let trash = f.lib.parent().unwrap().join("trash");
        let r = unplace(&f.google, &book.links(), &mut move_into(&trash));
        assert_eq!((r.removed, r.discarded), (0, 1));
        assert_eq!(std::fs::read(trash.join("a.jpg")).unwrap(), b"photo");
    }

    #[test]
    fn a_discard_that_only_takes_note_does_not_lose_the_last_copy() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        let r = unplace(&f.google, &book.links(), &mut |_: &Path| Ok(()));
        assert_eq!(r.failed.len(), 1);
        assert!(r.forget.is_empty());
        assert!(f.google.join("d/a.jpg").exists());
    }

    #[test]
    fn a_missing_link_or_a_non_file_is_only_forgotten() {
        let f = fixture();
        std::fs::create_dir_all(f.google.join("d/dir.jpg")).unwrap();
        let links = [f.google.join("d/gone.jpg"), f.google.join("d/dir.jpg")];
        let r = unplace(&f.google, &links, &mut no_discard());
        assert_eq!((r.gone, r.forget.len()), (2, 2));
        assert!(f.google.join("d/dir.jpg").is_dir());
    }

    #[test]
    fn records_pointing_outside_the_folder_are_not_touched() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let links = [f.lib.join("d/a.jpg"), f.google.join("../lib/d/a.jpg")];
        let r = unplace(&f.google, &links, &mut no_discard());
        assert_eq!((r.failed.len(), r.forget.len()), (2, 0));
        assert!(f.lib.join("d/a.jpg").exists());
    }

    #[test]
    fn a_link_on_the_way_is_not_followed_when_unplacing() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        std::fs::create_dir_all(&f.google).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(f.lib.join("d"), f.google.join("d")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(f.lib.join("d"), f.google.join("d")).is_err() {
            return;
        }
        let r = unplace(&f.google, &[f.google.join("d/a.jpg")], &mut no_discard());
        assert_eq!((r.failed.len(), r.forget.len()), (1, 0));
        assert!(f.lib.join("d/a.jpg").exists());
    }

    #[test]
    fn the_probe_never_writes_into_a_name_it_did_not_create() {
        let f = fixture();
        let src = f.lib.join("d/a.jpg");
        put(&src, b"photo");
        let pid = std::process::id();
        let squatter = f.lib.join(format!("{PROBE_PREFIX}{pid}-0-a"));
        std::fs::hard_link(&src, &squatter).unwrap();
        probe_hard_links(&f.lib).unwrap();
        assert_eq!(std::fs::read(&src).unwrap(), b"photo");
        assert!(squatter.exists());
    }

    #[test]
    fn the_folder_may_not_overlap_a_root() {
        let f = fixture();
        let roots = [f.lib.clone()];
        assert!(check_location(&f.lib.join("g"), &roots).is_err());
        assert!(check_location(f.lib.parent().unwrap(), &roots).is_err());
        assert!(check_location(&f.google, &roots).is_ok());
        // `..` を含む綴りも、実体で比べれば中にある
        assert!(check_location(&f.lib.join("d/../g"), &roots).is_err());
    }

    #[test]
    fn an_alias_of_a_root_is_seen_through() {
        let f = fixture();
        let alias = f.google.with_file_name("alias");
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
        // 選んだ場所が同じボリュームなら、まだ作っていなくてもそちら
        let chosen = f.lib.parent().unwrap().join("chosen");
        assert_eq!(
            location_for_root(&f.lib, std::slice::from_ref(&chosen), &roots).unwrap(),
            chosen
        );
        // 親ごと無い場所（外れた外付け）は比べずに飛ばし、既定へ落ちる
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
    fn the_verbatim_prefix_is_dropped_on_windows() {
        let p = PathBuf::from(r"\\?\D:\pictkura-google");
        let q = PathBuf::from(r"\\?\UNC\nas\share\pictkura-google");
        if cfg!(windows) {
            assert_eq!(without_verbatim(p), PathBuf::from(r"D:\pictkura-google"));
            assert_eq!(
                without_verbatim(q),
                PathBuf::from(r"\\nas\share\pictkura-google")
            );
        } else {
            assert_eq!(without_verbatim(p.clone()), p);
        }
    }

    #[test]
    fn an_unplugged_root_does_not_block_the_others() {
        let f = fixture();
        let roots = [f.lib.clone(), PathBuf::from("/no/such/volume/Photos")];
        if cfg!(windows) {
            // Windows では外れたドライブの祖先が1つも無い
            let roots = [f.lib.clone(), PathBuf::from("Q:\\Photos")];
            assert!(check_location(&f.google, &roots).is_ok());
        }
        assert!(check_location(&f.google, &roots).is_ok());
    }

    #[test]
    fn the_volume_of_a_linked_root_is_where_the_link_points() {
        let f = fixture();
        let alias = f.google.with_file_name("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&f.lib, &alias).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&f.lib, &alias).is_err() {
            return;
        }
        assert_eq!(volume_of(&alias).unwrap(), volume_of(&f.lib).unwrap());
        assert_eq!(volume_top(&alias).unwrap(), volume_top(&f.lib).unwrap());
    }

    #[test]
    fn a_whole_volume_root_has_nowhere_to_put_a_folder() {
        let root = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        assert!(matches!(
            location_for_root(&root, &[], std::slice::from_ref(&root)),
            Err(MirrorError::RootIsWholeVolume(_))
        ));
    }
}
