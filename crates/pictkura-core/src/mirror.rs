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

/// 置くと決めた1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// 原本
    pub source: PathBuf,
    /// Google 用フォルダの中での場所（原本の、持ち主のルートからの相対パス）
    pub rel: PathBuf,
    /// 原本そのものではなく、**埋め込み JPEG を取り出して置く**（RAW だけのカット・
    /// [`RawOnly::EmbeddedJpeg`]）。取り出しはまだ無い（設計書の PR5）ので、[`place`] は
    /// [`PlaceReport::later`] に数えるだけで何も書かない
    pub embedded: bool,
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
/// `items` はこの回に取り込んだ原本（コピー先のパス）。返す順は `items` の順。
/// `photo_on_disk` は [`photos_on_disk`] を渡す（試験は偽物を渡す）。
pub fn plan(
    items: &[PathBuf],
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
    items: &[PathBuf],
    roots: &[PathBuf],
    cfg: &GoogleMirrorConfig,
    photo_on_disk: &mut dyn FnMut(&Path) -> bool,
    onedrive: &[PathBuf],
) -> Vec<Placement> {
    let kinds: Vec<MediaKind> = items.iter().map(|i| MediaKind::from_path(i)).collect();
    // 組の相方になれる「写真」の鍵。RAW だけのカットかどうかはこれで決まる
    let photo_keys: HashSet<(PathBuf, std::ffi::OsString)> = items
        .iter()
        .zip(&kinds)
        .filter(|(_, k)| **k == MediaKind::Photo)
        .map(|(i, _)| pair_key_folded(i))
        .collect();
    // 解決した場所と、綴りのままの場所の両方で比べる。**解決できない OneDrive を黙って
    // 落とさない**——落とすと判定ごと素通りして、既定で守るものを失う（ゲート2）
    let resolved: Vec<PathBuf> = onedrive.iter().filter_map(|d| disk_key(d).ok()).collect();
    let spelled: Vec<PathBuf> = onedrive.iter().map(|d| fold_path(d)).collect();
    // フォルダごとに1回だけ解決する（取り込みの1回は同じ日のフォルダに何百枚も来る。ゲート2）。
    // **解決できなければ OneDrive の中と見なす**——置く側へ倒すと、既定で守るはずの
    // 「オンラインのみにできる」を黙って失う（ゲート2）
    let mut parents: HashMap<PathBuf, bool> = HashMap::new();
    let mut in_onedrive = |path: &Path| -> bool {
        let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
        *parents
            .entry(parent)
            .or_insert_with_key(|parent| match disk_key(parent) {
                Ok(k) => resolved.iter().any(|d| k.starts_with(d)),
                Err(_) => true,
            })
    };

    let mut out = Vec::new();
    for (path, kind) in items.iter().zip(kinds) {
        // `Some(埋め込みか)`＝置く
        let wanted = match kind {
            MediaKind::Photo => Some(false),
            MediaKind::Video => (!cfg.exclude_video).then_some(false),
            MediaKind::Raw if !cfg.exclude_raw => Some(false),
            // 組を探すのは要るときだけ（既定の「上げない」ではフォルダを読まない。ゲート2）
            MediaKind::Raw => match cfg.raw_only {
                RawOnly::None => None,
                RawOnly::EmbeddedJpeg => {
                    let paired = photo_keys.contains(&pair_key_folded(path)) || photo_on_disk(path);
                    (!paired).then_some(true)
                }
            },
        };
        // OS の置き物の名前（`._IMG_1.JPG` は AppleDouble で写真ではない）は置かない
        let Some(embedded) = wanted.filter(|_| !path.file_name().is_some_and(is_os_litter)) else {
            continue;
        };
        // 安いほうを先に（どのルートにも入らないものはディスクを引かずに落とす。ゲート2）
        let Some(rel) = owning_root(path, roots) else {
            continue;
        };
        if !onedrive.is_empty()
            && (spelled.iter().any(|d| fold_path(path).starts_with(d)) || in_onedrive(path))
        {
            continue;
        }
        if is_plain_relative(&rel) {
            out.push(Placement {
                source: path.clone(),
                rel,
                embedded,
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
///
/// `extensions` は取り込みの設定の拡張子（[`crate::config::ImportConfig::extensions`]）
/// ——利用者が足した形式も相方に数える（ゲート2）。
pub fn photos_on_disk(extensions: &[String]) -> impl FnMut(&Path) -> bool {
    let extensions: HashSet<String> = extensions.iter().map(|e| e.to_ascii_lowercase()).collect();
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
                    .filter(|p| {
                        is_photo_extension(p, &extensions)
                            && !p.file_name().is_some_and(is_os_litter)
                    })
                    .map(|p| pair_key_folded(&p).1)
                    .collect()
            })
            .contains(&stem)
    }
}

fn is_photo_extension(path: &Path, extensions: &HashSet<String>) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    extensions.contains(&ext.to_ascii_lowercase()) && MediaKind::from_path(path) == MediaKind::Photo
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

/// `path` を持つルートからの残り。入れ子なら**いちばん外側**のルート——内側を選ぶと、
/// 外側のルートの直下の `IMG_0001.JPG` と内側の `IMG_0001.JPG` が同じ名前になる（ゲート2）。
///
/// フォルダの形は取り込み先と同じにする（`D:\photos\2026年\…` → `D:\pictkura-google\2026年\…`）。
/// 同じドライブの別のルートから同じ相対パスが来たら、2本目は [`place`] で
/// 「同じ名前が既にある」として失敗する（上書きはしない）。
fn owning_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .filter_map(|root| strip_root(path, root).map(|rest| (root, rest)))
        .filter(|(_, rest)| !rest.as_os_str().is_empty())
        .min_by_key(|(root, _)| root.components().count())
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
    /// リンクの実体の番号（原本と同じ。unix の inode、Windows のファイル番号）。
    /// **外すときにこれと照合する**——パスだけでは、フォルダを消して作り直したあとに
    /// 同じ名前で置かれた他人のファイルを、自分の置いたものと取り違える（ゲート1）。
    /// **ボリュームの番号は持たない**——macOS は外付けを挿し直すたびに `st_dev` を
    /// 振り直すので、照合に入れると外付けのリンクが一生外せなくなる
    pub index: u64,
}

/// 記録へ書こうとした結果（[`Ledger::claim`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// 新しく書いた
    New,
    /// 同じリンク・同じ原本の行が既に在った（前の回に置いたもの）。`index` は記録の番号
    Ours { index: u64 },
    /// 同じリンクの行が**別の原本で**在った——置かない。置くと、新しいリンクが古い原本の
    /// 行のまま残り、古い原本を消したときに外され、新しい原本からは外せなくなる（ゲート1）
    Other,
}

/// 置いたものの記録（DB の `google_placed`）。[`place`] が使う。
pub trait Ledger {
    /// リンクを張る**前に**書く。
    fn claim(&mut self, placed: &Placed) -> io::Result<Claim>;
    /// 張れなかった。[`Claim::New`] で書いた行だけを取り消すために呼ばれる
    /// （前から在った行は、前に置いたリンクのものなので残す）。
    fn release(&mut self, placed: &Placed);
    /// [`Claim::Ours`] の行に、いま張ってある実体の番号を書き直す（原本が差し替わって
    /// 張り直せたとき）。**失敗したら、張ったばかりのリンクを外す**——古い番号のまま
    /// 残すと、外すときに他人と見なされ、二度と外せないリンクになる（ゲート2）。
    fn renumber(&mut self, placed: &Placed) -> io::Result<()>;
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
    /// 埋め込み JPEG で置くと決めたが、取り出しがまだ無いので置かなかったもの（設計書の PR5）
    pub later: usize,
    /// 失敗（原本と理由）
    pub failed: Vec<(PathBuf, String)>,
}

/// `placements` を `dir`（Google 用フォルダ）へハードリンクで置く。
///
/// **記録してからリンクする**（[`Ledger::claim`]）。リンクだけが残って記録に無いと、
/// 二度と外せない（外すのは記録したものだけなので）。逆に、記録だけが残る（記録のあとで
/// 落ちた）のは害が無い: [`unplace`] は無いリンクを「もう無い」として記録から消す。
///
/// 在る名前は**上書きしない**。同じ実体なら済み（[`PlaceReport::already`]）、違えば失敗。
/// 原本がシンボリックリンクのもの・クラウドのみのもの・別の原本で記録済みの名前は置かない。
///
/// 1件ずつの失敗は [`PlaceReport::failed`] に積んで続ける。**同じフォルダに対して同時に
/// 2本走らせないこと**（呼び出し側が1本ずつ回す）。
pub fn place(
    dir: &Path,
    roots: &[PathBuf],
    placements: &[Placement],
    ledger: &mut dyn Ledger,
) -> Result<PlaceReport, MirrorError> {
    check_dir(dir, roots)?;
    std::fs::create_dir_all(dir)?;
    // 作った直後にもう一度見る（作る前は無かったので、リンクかどうかは作ってから分かる）
    if is_link(dir) {
        return Err(MirrorError::LinkInTheWay(dir.to_path_buf()));
    }

    let dir_volume = volume_of(dir)?;
    let mut report = PlaceReport::default();
    // 置けなかった1件のために**この回に作った**フォルダ。最後に空なら畳む——Google が
    // 見ているフォルダに空のアルバムを残さない（ゲート2）
    let mut emptied: Vec<PathBuf> = Vec::new();
    for p in placements {
        let fail = |report: &mut PlaceReport, why: String| {
            report.failed.push((p.source.clone(), why));
        };
        if p.embedded {
            report.later += 1;
            continue;
        }
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
        let index = match file_id(&p.source) {
            Ok(id) => id.index,
            Err(e) => {
                fail(&mut report, e.to_string());
                continue;
            }
        };
        // 別のボリュームの原本はリンクにならない。記録もフォルダも作る前に断る（ゲート2）
        match volume_of(&p.source).map(|v| v == dir_volume) {
            Ok(true) => {}
            Ok(false) => {
                fail(
                    &mut report,
                    "原本が Google 用フォルダと別のドライブにある".into(),
                );
                continue;
            }
            Err(e) => {
                fail(&mut report, e.to_string());
                continue;
            }
        }
        let placed = Placed {
            source: p.source.clone(),
            link: dir.join(&p.rel),
            index,
        };
        let mut created = Vec::new();
        let made = make_parent_dirs(dir, &p.rel, &mut created);
        if let Err(e) = made {
            fail(&mut report, e.to_string());
            emptied.extend(created);
            continue;
        }
        let claim = match ledger.claim(&placed) {
            Ok(Claim::Other) => Err(format!(
                "同じ名前が別の原本で記録済み: {}",
                placed.link.display()
            )),
            Ok(claim) => Ok(claim),
            Err(e) => Err(format!("記録できない: {e}")),
        };
        let claim = match claim {
            Ok(claim) => claim,
            Err(why) => {
                fail(&mut report, why);
                emptied.extend(created);
                continue;
            }
        };
        // `Ok(true)` = この回に張った、`Ok(false)` = 同じ実体が既に在った
        let linked = match std::fs::hard_link(&p.source, &placed.link) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if !is_link(&placed.link) && same_file(&p.source, &placed.link).unwrap_or(false) {
                    Ok(false)
                } else {
                    Err(format!("同じ名前が既にある: {}", placed.link.display()))
                }
            }
            Err(e) => Err(e.to_string()),
        };
        let linked = match linked {
            // 番号が記録と同じなら書かない。一時的に書けないだけで、正しい記録のリンクを
            // 外すことになる（ゲート2）
            Ok(created) if matches!(claim, Claim::Ours { index } if index != placed.index) => {
                match ledger.renumber(&placed) {
                    Ok(()) => Ok(created),
                    Err(e) => {
                        // 古い番号の記録のまま残すと二度と外せないので、リンクを外す。原本が在るので
                        // 名前の数は2以上——消しても実体は残る
                        match std::fs::remove_file(&placed.link) {
                            Ok(()) => Err(format!("記録を書き直せない: {e}")),
                            Err(u) => Err(format!("記録を書き直せず、リンクも外せない: {e} / {u}")),
                        }
                    }
                }
            }
            other => other,
        };
        match linked {
            Ok(true) => report.placed += 1,
            Ok(false) => report.already += 1,
            Err(why) => {
                if claim == Claim::New {
                    ledger.release(&placed);
                }
                fail(&mut report, why);
                emptied.extend(created);
            }
        }
    }
    // 深いほうから、作ったものだけを畳む（ほかの1件が中に置いたなら空ではないので残る）
    emptied.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    emptied.dedup();
    for d in &emptied {
        fold_one(dir, d);
    }
    Ok(report)
}

/// 外すように渡す記録の1行（[`unplace`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub link: PathBuf,
    /// 置いたときの実体の番号（[`Placed::index`]）
    pub index: u64,
}

/// DB の `google_placed` を記録に使う [`Ledger`]。
pub struct DbLedger<'a> {
    pub db: &'a mut crate::db::Db,
    /// 置く先の Google 用フォルダ（記録の `dir`）
    pub dir: PathBuf,
}

impl Ledger for DbLedger<'_> {
    fn claim(&mut self, placed: &Placed) -> io::Result<Claim> {
        self.db
            .google_place_claim(placed, &self.dir)
            .map_err(io::Error::other)
    }

    fn release(&mut self, placed: &Placed) {
        // 消せなくても、リンクの無い記録が残るだけ（外すときに「もう無い」として消える）
        let _ = self
            .db
            .google_place_forget(std::slice::from_ref(&placed.link));
    }

    fn renumber(&mut self, placed: &Placed) -> io::Result<()> {
        self.db
            .google_place_renumber(placed)
            .map_err(io::Error::other)
    }
}

/// 取り込みのあとに置くか。**判定はここだけ**——呼び出し側が DB を開く前に聞くのにも使う。
pub fn is_on(config: &crate::Config) -> bool {
    config.google_mirror.enabled
}

/// 取り込みでコピーしたものを Google 用フォルダへ置く（取り込みの最後に1回呼ぶ）。
///
/// - 設定で切っていれば何もしない（`None`。[`is_on`]）
/// - ライブラリの除外パターン（[`crate::config::LibraryConfig::exclude_patterns`]）に当たるものは
///   置かない——一覧に出ないので、pictkura から外す手段が無くなる（ゲート2）
/// - 埋め込み JPEG で置くと決めたものは数えるだけ（取り出しは設計書の PR5）。それしか無い回は
///   場所も決めず、フォルダも作らない（ゲート2）
/// - 置く先は取り込み先のドライブの Google 用フォルダ（[`location_for_root`]）
/// - `config` は**取り込み先をルートに足したあと**のもの（相対パスはルートから取る）
///
/// **同じフォルダに対して同時に2本走らせないこと**（呼び出し側が1本ずつ回す。[`place`]）。
pub fn place_imported(
    copied: &[PathBuf],
    dest: &Path,
    config: &crate::Config,
    db: &mut crate::db::Db,
) -> Result<Option<PlaceReport>, MirrorError> {
    let g = &config.google_mirror;
    if !is_on(config) {
        return Ok(None);
    }
    let roots = &config.library.roots;
    let patterns = &config.library.exclude_patterns;
    let copied: Vec<PathBuf> = copied
        .iter()
        .filter(|p| {
            // 取り込み先からの残りだけで見る（取り込み先より上の `.` で始まるフォルダに当てない）
            let rel = p.strip_prefix(dest).unwrap_or(p);
            !crate::scanner::is_excluded_path(rel, patterns)
        })
        .cloned()
        .collect();
    let (embedded, placements): (Vec<Placement>, Vec<Placement>) = plan(
        &copied,
        roots,
        g,
        &mut photos_on_disk(&config.import.extensions),
    )
    .into_iter()
    .partition(|p| p.embedded);
    if placements.is_empty() {
        return Ok(Some(PlaceReport {
            later: embedded.len(),
            ..PlaceReport::default()
        }));
    }
    let dir = location_for_root(dest, &g.locations, roots)?;
    let mut ledger = DbLedger {
        db,
        dir: dir.clone(),
    };
    let mut report = place(&dir, roots, &placements, &mut ledger)?;
    report.later += embedded.len();
    Ok(Some(report))
}

/// [`unplace`] の結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnplaceReport {
    /// 外したリンク（実体は原本の側に残る）
    pub removed: usize,
    /// **そこにしか実体が無かった**ので `discard` へ渡したもの
    pub discarded: usize,
    /// もう無かった、または pictkura の置いたものではなかった（触らなかった）
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
/// - もう無いもの・普通のファイルでないもの（リンク・フォルダ）・**実体の番号が記録と
///   違うもの**は触らずに記録から消す——pictkura が置いたのは、その番号のハードリンクだけ
/// - `dir` の外を指す記録・途中にリンクを挟む記録は触らない（失敗として返す）
///
/// 外したあと、空になったフォルダを畳む（`dir` そのものは残す）。
pub fn unplace(
    dir: &Path,
    records: &[Recorded],
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> UnplaceReport {
    let mut report = UnplaceReport::default();
    // フォルダごと見えない（外付けが外れている等）なら、どのリンクも「もう無い」とは言えない。
    // 記録を消すと、挿し直したあとで二度と外せない（ゲート1）。全部を失敗として残す
    if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !is_link_meta(&m)) {
        for Recorded { link, .. } in records {
            report
                .failed
                .push((link.clone(), "Google 用フォルダが見えない".into()));
        }
        return report;
    }
    let mut parents: Vec<PathBuf> = Vec::new();
    for Recorded { link, index } in records {
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
        // 番号と名前の数は**同じ1回の問い合わせ**で読む（ゲート2: 2回引くと、間に差し替わった
        // 別物の名前の数で決めることになる）
        let id = if meta.is_file() && !is_link_meta(&meta) {
            match file_id(link) {
                Ok(id) => Some(id),
                Err(e) => {
                    report.failed.push((link.clone(), e.to_string()));
                    continue;
                }
            }
        } else {
            None
        };
        let Some(id) = id.filter(|id| id.index == *index) else {
            report.gone += 1;
            report.forget.push(link.clone());
            continue;
        };
        match remove_or_discard(link, id.links, discard) {
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
        fold_one(dir, &d);
    }
}

/// `d` が空か OS の置き物しか持たなければ畳む。
fn fold_one(dir: &Path, d: &Path) {
    // **`dir` から `d` までのどこにもリンクが無いこと**。`d` だけ見ると、途中のリンクを
    // 辿って `dir` の外の空のアルバムを畳む（ゲート1: 置けなかった1件の後片付けが踏んだ）
    if d.strip_prefix(dir)
        .map_or(true, |rel| link_on_the_way(dir, &rel.join("_")).is_some())
        || is_link(d)
    {
        return;
    }
    let only_litter = std::fs::read_dir(d).is_ok_and(|rd| {
        rd.flatten()
            .all(|e| e.file_type().is_ok_and(|t| t.is_file()) && is_os_litter(&e.file_name()))
    });
    if only_litter {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    // 中身があれば失敗する＝それで正しい
    let _ = std::fs::remove_dir(d);
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

/// OneDrive の同期フォルダ（Windows は環境変数、macOS は `~/Library/CloudStorage/OneDrive-*`
/// と、古い版の `~/OneDrive`）。
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
    let old_mac = home_dir()
        .filter(|_| cfg!(target_os = "macos"))
        .map(|h| h.join("OneDrive"))
        .filter(|d| d.is_dir());
    onedrive_env_folders()
        .chain(mac)
        .chain(old_mac)
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
///
/// 作ったフォルダを `created` に積む（浅いほうから）。置けなかったときに、**この回に作った
/// ものだけ**を畳むため——利用者が置いた空のフォルダや `desktop.ini` だけのフォルダは
/// 触らない（ゲート2）。
fn make_parent_dirs(dir: &Path, rel: &Path, created: &mut Vec<PathBuf>) -> io::Result<()> {
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
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir(&at)?;
                created.push(at.clone());
            }
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
    links: u64,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> io::Result<bool> {
    if links <= 1 {
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

    fn item(p: &str) -> PathBuf {
        PathBuf::from(p)
    }

    /// 埋め込み JPEG で置くものには `*` を付ける
    fn rels(ps: &[Placement]) -> Vec<String> {
        ps.iter()
            .map(|p| {
                let r = p.rel.to_string_lossy().replace('\\', "/");
                if p.embedded {
                    format!("{r}*")
                } else {
                    r
                }
            })
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

    fn plan_of(items: &[PathBuf], roots: &[PathBuf], c: &GoogleMirrorConfig) -> Vec<Placement> {
        plan_with(items, roots, c, &mut nothing_on_disk(), &[])
    }

    const ROOT: &str = "/lib/Photos";

    fn shoot() -> Vec<PathBuf> {
        vec![
            item("/lib/Photos/d/A.ARW"),
            item("/lib/Photos/d/a.jpg"), // 組（大文字小文字を畳む）
            item("/lib/Photos/d/B.ARW"), // RAW だけ
            item("/lib/Photos/d/C.CR3"), // RAW だけ
            item("/lib/Photos/d/clip.mp4"),
        ]
    }

    #[test]
    fn the_default_sends_photos_and_videos_but_no_raw() {
        let got = plan_of(&shoot(), &[ROOT.into()], &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["d/a.jpg", "d/clip.mp4"]);
    }

    #[test]
    fn raw_only_shots_follow_the_two_way_setting() {
        let roots = [PathBuf::from(ROOT)];
        let none = plan_of(&shoot(), &roots, &cfg(RawOnly::None));
        assert_eq!(rels(&none), ["d/a.jpg", "d/clip.mp4"]);
        let embedded = plan_of(&shoot(), &roots, &cfg(RawOnly::EmbeddedJpeg));
        assert_eq!(
            rels(&embedded),
            ["d/a.jpg", "d/B.ARW*", "d/C.CR3*", "d/clip.mp4"]
        );
    }

    #[test]
    fn a_paired_raw_stays_home_even_when_embedded_jpegs_are_chosen() {
        let items = [item("/lib/Photos/d/A.ARW"), item("/lib/Photos/d/A.JPG")];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::EmbeddedJpeg));
        assert_eq!(rels(&got), ["d/A.JPG"]);
    }

    #[test]
    fn a_partner_already_on_disk_pairs_a_raw_imported_later() {
        // JPEG は前の回に取り込み済み。この回は RAW だけ
        let items = [item("/lib/Photos/d/A.ARW")];
        let mut on_disk = |p: &Path| p.file_stem() == Some(std::ffi::OsStr::new("A"));
        let got = plan_with(
            &items,
            &[ROOT.into()],
            &cfg(RawOnly::EmbeddedJpeg),
            &mut on_disk,
            &[],
        );
        assert!(got.is_empty());
    }

    #[test]
    fn a_video_is_not_a_partner_for_a_raw() {
        let items = [item("/lib/Photos/d/A.ARW"), item("/lib/Photos/d/A.MOV")];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::EmbeddedJpeg));
        assert_eq!(rels(&got), ["d/A.ARW*", "d/A.MOV"]);
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
    fn a_file_belongs_to_the_outermost_root_and_strays_are_skipped() {
        let roots = [PathBuf::from("/lib"), PathBuf::from("/lib/Photos")];
        let items = [
            item("/lib/Photos/1.jpg"),
            item("/lib/x.jpg"),
            item("/elsewhere/2.jpg"),
        ];
        let got = plan_of(&items, &roots, &GoogleMirrorConfig::default());
        assert_eq!(rels(&got), ["Photos/1.jpg", "x.jpg"]);
    }

    #[test]
    fn a_root_spelled_in_another_case_still_owns_its_files() {
        let items = [item("/lib/Photos/d/1.jpg")];
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
            item("/lib/Photos/Trip/A.ARW"),
            item("/lib/Photos/trip/A.JPG"),
        ];
        let got = plan_of(&items, &[ROOT.into()], &cfg(RawOnly::EmbeddedJpeg));
        assert_eq!(rels(&got), ["trip/A.JPG"]);
    }

    #[test]
    fn os_litter_is_never_planned() {
        let items = [item("/lib/Photos/d/._a.jpg")];
        assert!(plan_of(&items, &[ROOT.into()], &GoogleMirrorConfig::default()).is_empty());
    }

    #[test]
    fn originals_inside_onedrive_stay_home_unless_included() {
        let f = fixture();
        let od = f.lib.join("OneDrive");
        put(&od.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/b.jpg"), b"photo");
        let items = [od.join("d/a.jpg"), f.lib.join("d/b.jpg")];
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
    fn an_original_whose_folder_cannot_be_resolved_counts_as_inside_onedrive() {
        // 在る祖先が1つも無い綴り（解決できない）
        let items = [item("no-such-root/d/a.jpg")];
        let roots = [PathBuf::from("no-such-root")];
        let c = GoogleMirrorConfig::default();
        let od = [std::env::temp_dir()];
        assert!(plan_with(&items, &roots, &c, &mut nothing_on_disk(), &od).is_empty());
        assert_eq!(
            plan_with(&items, &roots, &c, &mut nothing_on_disk(), &[]).len(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_original_on_another_volume_is_refused_before_anything_is_written() {
        let f = fixture();
        // /dev は別のボリューム（devfs・devtmpfs）
        let src = PathBuf::from("/dev/null");
        let p = Placement {
            source: src,
            rel: PathBuf::from("d/null"),
            embedded: false,
        };
        /// 記録に手を付けたら落ちる
        struct Untouched;
        impl Ledger for Untouched {
            fn claim(&mut self, p: &Placed) -> io::Result<Claim> {
                panic!("claimed {}", p.link.display())
            }
            fn release(&mut self, _: &Placed) {}
            fn renumber(&mut self, _: &Placed) -> io::Result<()> {
                Ok(())
            }
        }
        let r = place(
            &f.google,
            std::slice::from_ref(&f.lib),
            &[p],
            &mut Untouched,
        )
        .unwrap();
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert!(!f.google.join("d").exists());
    }

    #[test]
    fn only_photo_extensions_on_disk_count_as_a_partner() {
        let f = fixture();
        put(&f.lib.join("d/A.ARW"), b"raw");
        put(&f.lib.join("d/A.xmp"), b"sidecar");
        put(&f.lib.join("d/B.ARW"), b"raw");
        put(&f.lib.join("d/b.JPG"), b"photo");
        let exts: Vec<String> = crate::config::DEFAULT_EXTENSIONS
            .iter()
            .map(|e| e.to_string())
            .collect();
        let mut on_disk = photos_on_disk(&exts);
        assert!(!on_disk(&f.lib.join("d/A.ARW")));
        assert!(on_disk(&f.lib.join("d/B.ARW")));
        // 利用者が足した形式も相方に数える
        put(&f.lib.join("e/C.ARW"), b"raw");
        put(&f.lib.join("e/C.jxl"), b"photo");
        assert!(!photos_on_disk(&exts)(&f.lib.join("e/C.ARW")));
        let mut more = exts.clone();
        more.push("JXL".into());
        let mut on_disk = photos_on_disk(&more);
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
            embedded: false,
        }
    }

    /// 記録の代わり（DB の `google_placed`）
    #[derive(Default)]
    struct Book(Vec<Placed>);

    impl Ledger for Book {
        fn claim(&mut self, p: &Placed) -> io::Result<Claim> {
            match self.0.iter().find(|r| r.link == p.link) {
                Some(r) if r.source == p.source => Ok(Claim::Ours { index: r.index }),
                Some(_) => Ok(Claim::Other),
                None => {
                    self.0.push(p.clone());
                    Ok(Claim::New)
                }
            }
        }
        fn release(&mut self, p: &Placed) {
            self.0.retain(|r| r.link != p.link);
        }
        fn renumber(&mut self, p: &Placed) -> io::Result<()> {
            for r in self.0.iter_mut().filter(|r| r.link == p.link) {
                r.index = p.index;
            }
            Ok(())
        }
    }

    impl Book {
        fn place(&mut self, f: &Fixture, ps: &[Placement]) -> PlaceReport {
            place(&f.google, std::slice::from_ref(&f.lib), ps, self).unwrap()
        }

        fn links(&self) -> Vec<PathBuf> {
            self.0.iter().map(|p| p.link.clone()).collect()
        }

        fn records(&self) -> Vec<Recorded> {
            self.0
                .iter()
                .map(|p| Recorded {
                    link: p.link.clone(),
                    index: p.index,
                })
                .collect()
        }
    }

    fn rec(link: PathBuf) -> Recorded {
        Recorded { link, index: 0 }
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
            index: 0,
        });
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert_eq!(book.links(), [link]);
    }

    #[test]
    fn a_name_recorded_for_another_source_is_not_linked() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        // 別の原本の記録が同じリンクで残っている（リンクは外で消された）
        let link = f.google.join("d/a.jpg");
        book.0.push(Placed {
            source: f.lib.join("old/a.jpg"),
            link: link.clone(),
            index: 0,
        });
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert!(!link.exists());
        assert_eq!(book.0[0].source, f.lib.join("old/a.jpg"));
        // 置けなかった1件のために作ったフォルダは畳む
        assert!(!f.google.join("d").exists());
    }

    #[test]
    fn a_relinked_replacement_is_renumbered() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        // 外で原本を差し替え、古いリンクも消した
        std::fs::remove_file(f.google.join("d/a.jpg")).unwrap();
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        put(&f.lib.join("d/a.jpg"), b"new photo");
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!(r.placed, 1);
        // 番号が新しい実体に合っていれば、外せる
        let u = unplace(&f.google, &book.records(), &mut no_discard());
        assert_eq!(u.removed, 1);
    }

    #[test]
    fn a_renumber_that_fails_takes_the_new_link_back() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        std::fs::remove_file(f.google.join("d/a.jpg")).unwrap();
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        put(&f.lib.join("d/a.jpg"), b"new photo");
        /// 書き直しだけが落ちる記録
        struct Stuck(Book);
        impl Ledger for Stuck {
            fn claim(&mut self, p: &Placed) -> io::Result<Claim> {
                self.0.claim(p)
            }
            fn release(&mut self, p: &Placed) {
                self.0.release(p)
            }
            fn renumber(&mut self, _: &Placed) -> io::Result<()> {
                Err(io::Error::other("database is locked"))
            }
        }
        let mut stuck = Stuck(book);
        let r = place(
            &f.google,
            std::slice::from_ref(&f.lib),
            &[placement(&f.lib, "d/a.jpg")],
            &mut stuck,
        )
        .unwrap();
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        // 古い番号の記録が指す名前に、新しい実体を残さない
        assert!(!f.google.join("d/a.jpg").exists());
        assert!(f.lib.join("d/a.jpg").exists());
    }

    #[test]
    fn an_unchanged_record_is_not_rewritten_so_a_busy_database_cannot_cost_a_link() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        struct Busy(Book);
        impl Ledger for Busy {
            fn claim(&mut self, p: &Placed) -> io::Result<Claim> {
                self.0.claim(p)
            }
            fn release(&mut self, p: &Placed) {
                self.0.release(p)
            }
            fn renumber(&mut self, _: &Placed) -> io::Result<()> {
                Err(io::Error::other("database is locked"))
            }
        }
        let r = place(
            &f.google,
            std::slice::from_ref(&f.lib),
            &[placement(&f.lib, "d/a.jpg")],
            &mut Busy(book),
        )
        .unwrap();
        assert_eq!((r.already, r.failed.len()), (1, 0));
        assert!(f.google.join("d/a.jpg").exists());
    }

    #[test]
    fn a_failed_placement_leaves_folders_it_did_not_make() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        // 利用者が置いた、アイコンの設定だけのフォルダ
        put(&f.google.join("d/desktop.ini"), b"[.ShellClassInfo]");
        // その中の名前は別の原本で記録済み（置けない）
        let mut book = Book::default();
        book.0.push(Placed {
            source: f.lib.join("old/a.jpg"),
            link: f.google.join("d/a.jpg"),
            index: 0,
        });
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!(r.failed.len(), 1);
        assert!(f.google.join("d/desktop.ini").exists());
    }

    #[test]
    fn a_stranger_at_a_recorded_name_is_left_alone() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        // フォルダを消して作り直し、同じ名前に他人のファイルが来た
        std::fs::remove_dir_all(&f.google).unwrap();
        put(&f.google.join("d/a.jpg"), b"someone else's only copy");
        let r = unplace(&f.google, &book.records(), &mut no_discard());
        assert_eq!((r.removed, r.discarded, r.gone), (0, 0, 1));
        assert_eq!(
            std::fs::read(f.google.join("d/a.jpg")).unwrap(),
            b"someone else's only copy"
        );
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
    fn a_failed_placement_behind_a_link_does_not_fold_outside() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let outside = f.lib.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(outside.join("album")).unwrap();
        put(&outside.join("album/.DS_Store"), b"finder");
        std::fs::create_dir_all(&f.google).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, f.google.join("alias")).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&outside, f.google.join("alias")).is_err() {
            return;
        }
        let p = Placement {
            source: f.lib.join("d/a.jpg"),
            rel: PathBuf::from("alias/album/a.jpg"),
            embedded: false,
        };
        let r = Book::default().place(&f, &[p]);
        assert_eq!(r.failed.len(), 1);
        assert!(outside.join("album/.DS_Store").exists());
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
        let err = place(&f.google, &wider, &[], &mut Book::default()).unwrap_err();
        assert!(matches!(err, MirrorError::OverlapsRoot(_)));
    }

    #[test]
    fn unplacing_removes_only_the_name_and_folds_empty_albums() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut book = Book::default();
        book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        put(&f.google.join("d/.DS_Store"), b"finder");
        let r = unplace(&f.google, &book.records(), &mut no_discard());
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
        let r = unplace(&f.google, &book.records(), &mut move_into(&trash));
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
        let r = unplace(&f.google, &book.records(), &mut |_: &Path| Ok(()));
        assert_eq!(r.failed.len(), 1);
        assert!(r.forget.is_empty());
        assert!(f.google.join("d/a.jpg").exists());
    }

    #[test]
    fn a_missing_link_or_a_non_file_is_only_forgotten() {
        let f = fixture();
        std::fs::create_dir_all(&f.google).unwrap();
        std::fs::create_dir_all(f.google.join("d/dir.jpg")).unwrap();
        let links = [
            rec(f.google.join("d/gone.jpg")),
            rec(f.google.join("d/dir.jpg")),
        ];
        let r = unplace(&f.google, &links, &mut no_discard());
        assert_eq!((r.gone, r.forget.len()), (2, 2));
        assert!(f.google.join("d/dir.jpg").is_dir());
    }

    #[test]
    fn records_are_kept_while_the_folder_is_out_of_reach() {
        let f = fixture();
        // 外付けが外れている: フォルダごと無い
        let r = unplace(
            &f.google,
            &[rec(f.google.join("d/a.jpg"))],
            &mut no_discard(),
        );
        assert_eq!((r.gone, r.forget.len(), r.failed.len()), (0, 0, 1));
    }

    #[test]
    fn records_pointing_outside_the_folder_are_not_touched() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let links = [
            rec(f.lib.join("d/a.jpg")),
            rec(f.google.join("../lib/d/a.jpg")),
        ];
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
        let r = unplace(
            &f.google,
            &[rec(f.google.join("d/a.jpg"))],
            &mut no_discard(),
        );
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
    fn an_embedded_jpeg_placement_is_counted_for_later_and_writes_nothing() {
        let f = fixture();
        put(&f.lib.join("d/B.ARW"), b"raw");
        let p = Placement {
            embedded: true,
            ..placement(&f.lib, "d/B.ARW")
        };
        let mut book = Book::default();
        let r = book.place(&f, &[p]);
        assert_eq!((r.later, r.placed, r.failed.len()), (1, 0, 0));
        assert!(book.0.is_empty());
        assert!(!f.google.join("d").exists());
    }

    #[test]
    fn placing_what_was_imported_records_it_in_the_database() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/B.ARW"), b"raw");
        let mut config = crate::Config::default();
        config.library.roots = vec![f.lib.clone()];
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let copied = [f.lib.join("d/a.jpg"), f.lib.join("d/B.ARW")];
        // 切のあいだは何もしない
        assert_eq!(
            place_imported(&copied, &f.lib, &config, &mut db).unwrap(),
            None
        );
        config.google_mirror.enabled = true;
        config.google_mirror.locations = vec![f.google.clone()];
        let r = place_imported(&copied, &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        // RAW だけは既定で上げない
        assert_eq!((r.placed, r.failed.len()), (1, 0));
        assert!(same_file(&f.lib.join("d/a.jpg"), &f.google.join("d/a.jpg")).unwrap());
        let rows = db
            .google_placed_for_sources(&[f.lib.join("d/a.jpg")])
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, f.google);
        // 記録どおりに外せる
        let u = unplace(&f.google, &[rows[0].1.clone()], &mut no_discard());
        assert_eq!(u.removed, 1);
    }

    #[test]
    fn imported_files_under_an_exclude_pattern_are_not_placed() {
        let f = fixture();
        put(&f.lib.join("private/a.jpg"), b"photo");
        let mut config = crate::Config::default();
        config.library.roots = vec![f.lib.clone()];
        config.library.exclude_patterns = vec!["private".into()];
        config.google_mirror.enabled = true;
        config.google_mirror.locations = vec![f.google.clone()];
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let r = place_imported(&[f.lib.join("private/a.jpg")], &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!(r, PlaceReport::default());
        assert!(!f.google.exists());
    }

    #[test]
    fn a_run_of_only_embedded_placements_makes_no_folder() {
        let f = fixture();
        put(&f.lib.join("d/B.ARW"), b"raw");
        let mut config = crate::Config::default();
        config.library.roots = vec![f.lib.clone()];
        config.google_mirror.enabled = true;
        config.google_mirror.raw_only = RawOnly::EmbeddedJpeg;
        config.google_mirror.locations = vec![f.google.clone()];
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let r = place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!((r.later, r.placed), (1, 0));
        assert!(!f.google.exists());
    }

    #[test]
    fn the_default_never_reads_folders_for_raw_partners() {
        let items = [item("/lib/Photos/d/B.ARW")];
        let mut asked = |_: &Path| -> bool { panic!("read a folder for nothing") };
        let got = plan_with(
            &items,
            &[ROOT.into()],
            &GoogleMirrorConfig::default(),
            &mut asked,
            &[],
        );
        assert!(got.is_empty());
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
