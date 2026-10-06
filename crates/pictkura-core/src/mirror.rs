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
/// 画面の呼び名「送り出し」に揃える（2026-10-06 利用者決定。`pictkura-google` から変えた——未公開のうちに。開発機で古い名前を使っていた台は、新しい名前のフォルダへ切り替わる）
pub const MIRROR_DIR_NAME: &str = "pictkura-outgoing";

/// 置くと決めた1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// 原本
    pub source: PathBuf,
    /// Google 用フォルダの中での場所（原本の、持ち主のルートからの相対パス）
    pub rel: PathBuf,
    /// 原本そのものではなく、**埋め込み JPEG を取り出して置く**（RAW だけのカット・
    /// [`RawOnly::EmbeddedJpeg`]）。置く名前は拡張子を `.jpg` にしたもの（[`place`]）
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
/// フォルダの形は取り込み先と同じにする（`D:\photos\2026年\…` → `D:\pictkura-outgoing\2026年\…`）。
/// 同じドライブの別のルートから同じ相対パスが来たら、2本目は [`place`] で
/// 「同じ名前が既にある」として失敗する（上書きはしない）。
fn owning_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    outer_root(path, roots).and_then(|root| strip_root(path, root))
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
    /// 原本へのリンクではなく、**取り出した埋め込み JPEG**（別の実体・名前はいつも1つ）。
    /// 外し方が違う（設計書 §4b）——pictkura で RAW をゴミ箱へ入れたら消す、外で消えたらゴミ箱へ
    pub extracted: bool,
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
    /// `link` の記録（原本・番号・取り出しか）。無ければ `None`
    fn holder(&mut self, link: &Path) -> io::Result<Option<(PathBuf, u64, bool)>>;
    /// `link` の記録を消す（取り出した JPEG を本物の JPEG に譲ったとき）
    fn forget(&mut self, link: &Path) -> io::Result<()>;
    /// `link` と**大文字小文字だけ違う**記録も含めて返す（記録のリンク・原本・番号・取り出しか）。
    /// 区別しない台（Windows・macOS）でだけ畳む。[`give_way`] が、`b.JPG` の前に `B.jpg` の
    /// 取り出しを見つけるために使う（ゲート1）
    fn holders_folded(&mut self, link: &Path) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>>;
    /// `link` が**この原本のものとして在ると確かめた**あとに呼ぶ。送り出しから外すと決まった印
    /// （外しきれずに残ったもの）を下ろす——送り直したのに、印のまま一覧に出ず、次の突き合わせで
    /// 外されないように（設計書 §3c）。**確かめる前に下ろさない**：在る名前が別の実体だったとき、
    /// 印が消えると古いリンクを外す道が無くなる（ゲート2）
    fn settle(&mut self, _link: &Path) -> io::Result<()> {
        Ok(())
    }
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
    /// この回に**新しく作った** Google 用フォルダ。Google フォトへの登録が要るので、画面で知らせる
    /// （取り込み先を別のドライブへ変えたあとの最初の取り込み等。黙って作ると、登録されずに上がらない）。
    /// 見逃しても、設定の「送り出し」に作ったフォルダの一覧が出る（2026-10-06 利用者決定）
    pub new_folders: Vec<PathBuf>,
    /// 失敗（原本と理由）。一時的なものも恒久的なものも入る
    pub failed: Vec<(PathBuf, String)>,
    /// **あとで置き直すもの**（一時的な失敗・クラウドのみ・フォルダを解決できなかったもの）。
    /// 原本の並び。
    /// 同じ名前が在る・別のドライブ・別の原本で記録済みのように、何度やっても同じものは入らない
    /// （2026-10-05 利用者決定。[`place_imported`] が保留として DB に残す）
    pub retry: Vec<PathBuf>,
}

impl PlaceReport {
    /// もう1つの結果を足し込む。**項目を足したらここも**——呼び出し側で1つずつ足すと漏れる（ゲート2）
    pub fn absorb(&mut self, other: PlaceReport) {
        let PlaceReport {
            placed,
            already,
            cloud_only,
            new_folders,
            failed,
            retry,
        } = other;
        // ドライブごとに作りうる（ルートが2つのドライブにまたがる回）。全部を残す（ゲート2）
        for f in new_folders {
            if !self.new_folders.contains(&f) {
                self.new_folders.push(f);
            }
        }
        self.placed += placed;
        self.already += already;
        self.cloud_only += cloud_only;
        self.failed.extend(failed);
        self.retry.extend(retry);
    }
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
    // 「無かった」と言うのは NotFound のときだけ（読めないだけの在るフォルダを「作った」と言わない。ゲート2）
    let existed = !is_missing(dir);
    std::fs::create_dir_all(dir)?;
    // 作った直後にもう一度見る（作る前は無かったので、リンクかどうかは作ってから分かる）
    if is_link(dir) {
        return Err(MirrorError::LinkInTheWay(dir.to_path_buf()));
    }

    let dir_volume = volume_of(dir)?;
    let mut report = PlaceReport {
        new_folders: if existed {
            Vec::new()
        } else {
            vec![dir.to_path_buf()]
        },
        ..PlaceReport::default()
    };
    // 置けなかった1件のために**この回に作った**フォルダ。最後に空なら畳む——Google が
    // 見ているフォルダに空のアルバムを残さない（ゲート2）
    let mut emptied: Vec<PathBuf> = Vec::new();
    // 埋め込み JPEG を書き切る作業場（要るときに作る。[`work_dir_for`]）
    let mut work: Option<PathBuf> = None;
    for p in placements {
        // `retry`: 一時的な失敗か（あとで置き直す）
        let fail = |report: &mut PlaceReport, why: String, retry: bool| {
            report.failed.push((p.source.clone(), why));
            if retry {
                report.retry.push(p.source.clone());
            }
        };
        if p.embedded {
            let mut created = Vec::new();
            let link = dir.join(p.rel.with_extension("jpg"));
            let placed = place_extracted(dir, p, ledger, &mut work, &mut created).and_then(|r| {
                ledger
                    .settle(&link)
                    .map(|()| r)
                    .map_err(|e| Skip::Failed(format!("記録の印を下ろせない: {e}"), true))
            });
            match placed {
                Ok(true) => report.placed += 1,
                Ok(false) => report.already += 1,
                Err(Skip::CloudOnly) => {
                    report.cloud_only += 1;
                    report.retry.push(p.source.clone());
                }
                Err(Skip::Failed(why, retry)) => {
                    fail(&mut report, why, retry);
                    emptied.extend(created);
                }
            }
            continue;
        }
        if !is_plain_relative(&p.rel) {
            fail(
                &mut report,
                format!("フォルダの外を指す: {}", p.rel.display()),
                false,
            );
            continue;
        }
        if is_link(&p.source) {
            // リンクそのものに張る台と先に張る台があり、どちらにしても Google は辿らない
            fail(&mut report, "原本がシンボリックリンク".into(), false);
            continue;
        }
        if crate::cloud::is_cloud_only_path(&p.source) {
            // 手元へ来れば置ける
            report.cloud_only += 1;
            report.retry.push(p.source.clone());
            continue;
        }
        let index = match file_id(&p.source) {
            Ok(id) => id.index,
            Err(e) => {
                fail(&mut report, e.to_string(), true);
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
                    false,
                );
                continue;
            }
            Err(e) => {
                fail(&mut report, e.to_string(), true);
                continue;
            }
        }
        if let Err(e) = give_way(dir, p, ledger) {
            fail(
                &mut report,
                format!("取り出した JPEG をどかせない: {e}"),
                true,
            );
            continue;
        }
        let placed = Placed {
            source: p.source.clone(),
            link: dir.join(&p.rel),
            index,
            extracted: false,
        };
        let mut created = Vec::new();
        let made = make_parent_dirs(dir, &p.rel, &mut created);
        if let Err(e) = made {
            // 途中にリンクかファイルが居座っている（`AlreadyExists`）のは、どかすまで同じ
            let retry = e.kind() != io::ErrorKind::AlreadyExists;
            fail(&mut report, e.to_string(), retry);
            emptied.extend(created);
            continue;
        }
        // `Err((理由, 一時的か))`
        let claim = match ledger.claim(&placed) {
            // 同じカットの取り出しの記録が、中身の消えたまま残っている（どかしたあと記録を消す前に
            // 落ちた）なら、記録を消して取り直す（PR の codex）
            Ok(Claim::Other) if stale_extract(&placed, ledger) => {
                match ledger
                    .forget(&placed.link)
                    .and_then(|()| ledger.claim(&placed))
                {
                    Ok(Claim::Other) => Err((
                        format!("同じ名前が別の原本で記録済み: {}", placed.link.display()),
                        false,
                    )),
                    Ok(claim) => Ok(claim),
                    Err(e) => Err((format!("記録できない: {e}"), true)),
                }
            }
            Ok(Claim::Other) => Err((
                format!("同じ名前が別の原本で記録済み: {}", placed.link.display()),
                false,
            )),
            Ok(claim) => Ok(claim),
            Err(e) => Err((format!("記録できない: {e}"), true)),
        };
        let claim = match claim {
            Ok(claim) => claim,
            Err((why, retry)) => {
                fail(&mut report, why, retry);
                emptied.extend(created);
                continue;
            }
        };
        // `Ok(true)` = この回に張った、`Ok(false)` = 同じ実体が既に在った、
        // `Err((理由, 一時的か))`
        let linked = match std::fs::hard_link(&p.source, &placed.link) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                let taken = || {
                    Err((
                        format!("同じ名前が既にある: {}", placed.link.display()),
                        false,
                    ))
                };
                if is_link(&placed.link) {
                    taken()
                } else {
                    match same_file(&p.source, &placed.link) {
                        Ok(true) => Ok(false),
                        Ok(false) => taken(),
                        // 確かめられなかった（一瞬の拒否・その間に消えた等）。名前の衝突と
                        // 決めつけると保留から外れる——次の回にもう一度見る（PR の codex）
                        Err(e) => Err((
                            format!("在る名前を確かめられない: {}: {e}", placed.link.display()),
                            true,
                        )),
                    }
                }
            }
            // 張れないドライブ（exFAT 等）・別のボリュームは何度やっても同じ。拒否（置き場の
            // フォルダのアクセス権など、直せば通る）とそのほかの入出力は一時的と見る（ゲート2）。
            // **使用中のファイルは拒否にならない**——NTFS は原本を書き込みで排他的に掴んだままでも
            // ハードリンクを張れた（2026-10-05 win の実機。#182 の W4）
            Err(e) => {
                let permanent = matches!(
                    e.kind(),
                    io::ErrorKind::Unsupported | io::ErrorKind::CrossesDevices
                );
                Err((e.to_string(), !permanent))
            }
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
                            Ok(()) => Err((format!("記録を書き直せない: {e}"), true)),
                            Err(u) => Err((
                                format!("記録を書き直せず、リンクも外せない: {e} / {u}"),
                                true,
                            )),
                        }
                    }
                }
            }
            other => other,
        };
        // 印が在りうるのは前からの行だけ。新しい行で呼ぶと、書けなかったときに下の失敗の道が記録を
        // 取り消し、張ったばかりのリンクが記録の無いまま残る（ゲート2）
        let linked = linked.and_then(|r| match claim {
            Claim::Ours { .. } => ledger
                .settle(&placed.link)
                .map(|()| r)
                .map_err(|e| (format!("記録の印を下ろせない: {e}"), true)),
            _ => Ok(r),
        });
        match linked {
            Ok(true) => report.placed += 1,
            Ok(false) => report.already += 1,
            Err((why, retry)) => {
                if claim == Claim::New {
                    ledger.release(&placed);
                }
                fail(&mut report, why, retry);
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
    // 作業場は空なら畳む（書き切ったものは名前を付けたあとで消しているので、普通は空）
    if let Some(w) = work {
        let _ = std::fs::remove_dir(w);
    }
    Ok(report)
}

/// 本物の写真を置く前に、**同じカットの RAW から取り出した JPEG** が同じ名前（`B.ARW` → `B.jpg`）を
/// 持っていればどかす（ゲート2）。RAW を先に取り込んでから JPEG を別の回に取り込むと、そのカットは
/// もう「RAW だけ」ではなく、名前は本物の JPEG のもの——どかさないと本物が永久に断られる
/// （大文字小文字を区別しない台では `B.JPG` と `B.jpg` は同じ名前）。どかすのは**消す**（RAW から
/// いつでも取り出し直せる。§4b の「pictkura が外す」と同じ）。Google に上がった分は残る。
fn give_way(dir: &Path, p: &Placement, ledger: &mut dyn Ledger) -> io::Result<()> {
    if MediaKind::from_path(&p.source) != MediaKind::Photo {
        return Ok(());
    }
    let spot = dir.join(p.rel.with_extension("jpg"));
    // その名前に何も無ければ、譲ってもらうものも無い。畳んだ引き方は表を全部なめるので、
    // 普通の取り込み（名前が空いている）では引かない（PR の codex）。大文字小文字を区別しない
    // 台では、`b.jpg` を訊いても `B.jpg` が答える
    if std::fs::symlink_metadata(&spot).is_err() {
        return Ok(());
    }
    for (link, raw, index, extracted) in ledger.holders_folded(&spot)? {
        if !extracted
            || raw == crate::paths::normalize(&p.source)
            || pair_key_folded(&raw) != pair_key_folded(&p.source)
        {
            continue;
        }
        // 途中にリンクを挟んでいれば、辿った先は Google 用フォルダの外——触らない（[`unplace`] と同じ門。
        // ゲート1）。そのときは記録も残し、本物の写真は名前がぶつかって置けないまま
        let inside = link
            .strip_prefix(dir)
            .is_ok_and(|rel| is_plain_relative(rel) && link_on_the_way(dir, rel).is_none());
        if !inside {
            continue;
        }
        // 記録の番号の実体だけを消す。違えば pictkura の置いたものではないので触らない（記録だけ消す）
        // 記録を忘れるのは、消せたか・もう無い・別物と確かめられたときだけ。確かめられなければ
        // 記録を残して誤りを返す——忘れると、残ったファイルを二度と外せない（ゲート2）
        match file_id(&link) {
            Ok(id) if id.index == index && !is_link(&link) => std::fs::remove_file(&link)?,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        ledger.forget(&link)?;
    }
    Ok(())
}

/// `link` が普通のファイルで中身が `tmp` と同じなら、その実体の番号。
fn same_bytes_index(link: &Path, tmp: &Path) -> Option<u64> {
    if is_link(link) {
        return None;
    }
    let id = file_id(link).ok()?;
    (std::fs::read(link).ok()? == std::fs::read(tmp).ok()?).then_some(id.index)
}

/// `placed.link` の記録が、同じカットの RAW からの取り出しで、名前にもう何も無いか。
fn stale_extract(placed: &Placed, ledger: &mut dyn Ledger) -> bool {
    matches!(
        ledger.holder(&placed.link),
        Ok(Some((raw, _, true))) if pair_key_folded(&raw) == pair_key_folded(&placed.source)
    ) && matches!(std::fs::symlink_metadata(&placed.link), Err(e) if e.kind() == io::ErrorKind::NotFound)
}

/// `path` が**無い**か（`NotFound` のときだけ真）。読めないだけ（権限・共有の一時的な誤り）や、壊れたリンクが
/// 居座っているのは「無い」ではない。新しく作るフォルダの判定（見積もり・置いた結果）はここだけを通す（ゲート2）
fn is_missing(path: &Path) -> bool {
    matches!(std::fs::symlink_metadata(path), Err(e) if e.kind() == io::ErrorKind::NotFound)
}

/// [`place_extracted`] が置かなかった理由。
enum Skip {
    /// 原本がクラウドのみ（手元へ来れば置ける）
    CloudOnly,
    /// 失敗（理由・一時的か）
    Failed(String, bool),
}

/// 埋め込み JPEG を書き切る作業場。**Google 用フォルダの隣**（同じボリュームなので、書き切った
/// ものをハードリンクで1回の操作として中へ入れられる）で、**中ではない**——Google は名前が付いた
/// 瞬間に拾うので、書きかけを中に置けない（S7b）。中身はすべて pictkura が書いたもの。
fn work_dir_for(dir: &Path) -> io::Result<PathBuf> {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
        return Err(io::Error::other("Google 用フォルダの隣に作業場を作れない"));
    };
    let mut work_name = std::ffi::OsString::from(".");
    work_name.push(name);
    work_name.push("-work");
    Ok(parent.join(work_name))
}

/// 作業場の名前の頭（これで始まるものだけを片付ける）
const WORK_PREFIX: &str = "pictkura-extract-";

/// 作業場を用意する。前の回が途中で落ちて残した書きかけは片付ける。
fn open_work_dir(dir: &Path) -> io::Result<PathBuf> {
    let work = work_dir_for(dir)?;
    // 隣が別のボリューム（Google 用フォルダがマウント先そのもの）なら、書き切ったものを
    // リンクで入れられない。そこに隠しフォルダを作る前に断る（ゲート2）
    if let Some(parent) = work.parent() {
        if volume_of(parent)? != volume_of(dir)? {
            return Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                "Google 用フォルダの隣が別のドライブなので、埋め込み JPEG の作業場を置けない",
            ));
        }
    }
    std::fs::create_dir_all(&work)?;
    if is_link(&work) {
        // 辿った先で書くと、どこを片付けるか分からなくなる
        return Err(io::Error::other(format!(
            "作業場がリンクになっている: {}",
            work.display()
        )));
    }
    // 消せない残り（削除の共有なしに掴まれている）があっても止めない——新しい名前は `create_new` で
    // 空いているものを取るので邪魔にならない。残りは突き合わせの片付けが拾う（ゲート2）
    let _ = clear_work_files(&work);
    Ok(work)
}

/// 作業場に残った書きかけ・消し損ねた名前（[`WORK_PREFIX`] で始まるもの）を消す。
/// 消せなかったものがあれば、最初の誤りを返す（残りは試す）。
fn clear_work_files(work: &Path) -> io::Result<()> {
    let mut first = None;
    for e in std::fs::read_dir(work)?.flatten() {
        let ours = e.file_name().to_string_lossy().starts_with(WORK_PREFIX);
        if ours && e.file_type().is_ok_and(|t| t.is_file()) {
            if let Err(err) = std::fs::remove_file(e.path()) {
                first.get_or_insert(err);
            }
        }
    }
    first.map_or(Ok(()), Err)
}

/// 記録のある Google 用フォルダの作業場を片付けて畳む（突き合わせのたびに呼ぶ）。
/// 取り出した直後に作業場の名前を消せなかった（Windows で削除の共有なしに掴まれた）ものは、
/// 次の取り出しまで残り、名前が2つのまま中身がディスクに残る——RAW だけのカットを二度と
/// 取り込まない人のところでは残り続ける（win の W12b）。消せなかったものは `failed` に積む
fn tidy_work_dirs(dirs: &[PathBuf], report: &mut UnplaceReport) {
    for dir in dirs {
        let Ok(work) = work_dir_for(dir) else {
            continue;
        };
        // 作業場がリンクなら辿らない（[`open_work_dir`] と同じ）
        if !std::fs::symlink_metadata(&work).is_ok_and(|m| m.is_dir() && !is_link_meta(&m)) {
            continue;
        }
        if let Err(e) = clear_work_files(&work) {
            report
                .failed
                .push((work.clone(), format!("作業場を片付けられない: {e}")));
        }
        let _ = std::fs::remove_dir(&work);
    }
}

/// `bytes` を作業場に書き切って、その場所を返す。更新日時は原本に揃える（Google が日時を
/// EXIF から読めないときの手がかり。[`crate::extract::write_to`] と同じ理由）。
fn write_work_file(work: &Path, bytes: &[u8], source: &Path) -> io::Result<PathBuf> {
    use std::io::Write;
    let pid = std::process::id();
    for n in 0..64 {
        let tmp = work.join(format!("{WORK_PREFIX}{pid}-{n}.jpg"));
        let mut f = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let written = f.write_all(bytes).and_then(|()| f.sync_all());
        drop(f);
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Ok(m) = std::fs::metadata(source) {
            let _ =
                filetime::set_file_mtime(&tmp, filetime::FileTime::from_last_modification_time(&m));
        }
        return Ok(tmp);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "作業場の名前が空いていない",
    ))
}

/// RAW だけのカットの埋め込み JPEG を取り出して置く（設計書 §2・§4b。PR5）。
/// `Ok(true)` は新しく置いた、`Ok(false)` は前の回に置いたものが在った。
///
/// **作業場で書き切ってから、記録して、ハードリンクで名前を付ける**——[`place`] のリンクと
/// 同じ「記録してから置く」の順で、Google が見るのは書き終えた1回の操作だけ（S7c）。
/// 名前を付けたら作業場の名前は消す（中の実体は名前1つになる＝原本と分け合わない）。
/// 記録の番号は**取り出した JPEG 自身のもの**（原本の番号では照合で他人と見なされ、外せない）。
fn place_extracted(
    dir: &Path,
    p: &Placement,
    ledger: &mut dyn Ledger,
    work: &mut Option<PathBuf>,
    created: &mut Vec<PathBuf>,
) -> Result<bool, Skip> {
    let rel = p.rel.with_extension("jpg");
    if !is_plain_relative(&rel) {
        return Err(Skip::Failed(
            format!("フォルダの外を指す: {}", rel.display()),
            false,
        ));
    }
    if is_link(&p.source) {
        return Err(Skip::Failed("原本がシンボリックリンク".into(), false));
    }
    if crate::cloud::is_cloud_only_path(&p.source) {
        return Err(Skip::CloudOnly);
    }
    // 前の回に置いたものがそのまま在れば、RAW を読む前に済ませる（読むのは高い。ゲート2）
    let link = dir.join(&rel);
    if let Ok(Some((src, index, true))) = ledger.holder(&link) {
        // 記録の綴りは揃えてある（`paths::normalize`）ので、こちらも揃えて比べる（ゲート2）
        if src == crate::paths::normalize(&p.source)
            && file_id(&link).is_ok_and(|id| id.index == index)
        {
            return Ok(false);
        }
    }
    // 読めない（一瞬の拒否・外れた）ものは次に試す。読めて絵が無いものは何度やっても同じ
    if let Err(e) = std::fs::metadata(&p.source) {
        return Err(Skip::Failed(e.to_string(), true));
    }
    // 作業場を先に開く。開けない（隣に書けない等）のに RAW を読み切ると、1枚ごとに高い読みが無駄になる（ゲート2）
    let work_dir = match work {
        Some(w) => w.clone(),
        None => {
            let w = open_work_dir(dir).map_err(|e| {
                let retry = e.kind() != io::ErrorKind::CrossesDevices;
                Skip::Failed(e.to_string(), retry)
            })?;
            *work = Some(w.clone());
            w
        }
    };
    let bytes = match crate::embedded_jpeg::for_google(&p.source) {
        Ok(b) => b,
        Err(crate::embedded_jpeg::Missing::NoPreview) => {
            return Err(Skip::Failed("取り出せる埋め込み JPEG が無い".into(), false))
        }
        // 読み切れなかっただけ（共有ロック等）。保留に残して次に読み直す（ゲート1）
        Err(crate::embedded_jpeg::Missing::Unreadable) => {
            return Err(Skip::Failed("RAW を読み切れない".into(), true))
        }
    };
    let tmp = write_work_file(&work_dir, &bytes, &p.source)
        .map_err(|e| Skip::Failed(format!("作業場に書けない: {e}"), true))?;
    let result = link_extracted(dir, p, &rel, &tmp, ledger, created);
    // 名前を付けられてもそうでなくても、作業場の名前は消す
    let _ = std::fs::remove_file(&tmp);
    result
}

/// [`place_extracted`] の後半: 書き切った `tmp` を記録して `dir/rel` へ入れる。
fn link_extracted(
    dir: &Path,
    p: &Placement,
    rel: &Path,
    tmp: &Path,
    ledger: &mut dyn Ledger,
    created: &mut Vec<PathBuf>,
) -> Result<bool, Skip> {
    let index = file_id(tmp)
        .map_err(|e| Skip::Failed(e.to_string(), true))?
        .index;
    if let Err(e) = make_parent_dirs(dir, rel, created) {
        let retry = e.kind() != io::ErrorKind::AlreadyExists;
        return Err(Skip::Failed(e.to_string(), retry));
    }
    let placed = Placed {
        source: p.source.clone(),
        link: dir.join(rel),
        index,
        extracted: true,
    };
    let claim = match ledger.claim(&placed) {
        Ok(Claim::Other) => {
            return Err(Skip::Failed(
                format!("同じ名前が別の原本で記録済み: {}", placed.link.display()),
                false,
            ))
        }
        Ok(claim) => claim,
        Err(e) => return Err(Skip::Failed(format!("記録できない: {e}"), true)),
    };
    // 前の回に置いたものがそのまま在る（保留から置き直した等）。取り出し直さない
    if let Claim::Ours { index: old } = claim {
        if file_id(&placed.link).is_ok_and(|id| id.index == old) {
            return Ok(false);
        }
    }
    if let Err(e) = std::fs::hard_link(tmp, &placed.link) {
        // 前の回が張ったあと、番号を書く前に落ちた: 名前に在るのは自分の取り出しで、中身は今回と
        // 同じ（取り出しは同じ RAW から同じ絵を出す）。番号を書き直して済みとする（PR の codex）
        if e.kind() == io::ErrorKind::AlreadyExists && matches!(claim, Claim::Ours { .. }) {
            if let Some(index) = same_bytes_index(&placed.link, tmp) {
                let adopted = Placed { index, ..placed };
                return match ledger.renumber(&adopted) {
                    Ok(()) => Ok(false),
                    Err(e) => Err(Skip::Failed(format!("記録を書き直せない: {e}"), true)),
                };
            }
        }
        if claim == Claim::New {
            ledger.release(&placed);
        }
        return Err(if e.kind() == io::ErrorKind::AlreadyExists {
            Skip::Failed(
                format!("同じ名前が既にある: {}", placed.link.display()),
                false,
            )
        } else {
            let permanent = matches!(
                e.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::CrossesDevices
            );
            Skip::Failed(e.to_string(), !permanent)
        });
    }
    // 前の回の記録（中身は消えていた）なら、番号を新しい実体へ書き直す。書けなければ外す
    // ——古い番号のまま残すと二度と外せない（[`Ledger::renumber`]）
    if matches!(claim, Claim::Ours { .. }) {
        if let Err(e) = ledger.renumber(&placed) {
            let _ = std::fs::remove_file(&placed.link);
            return Err(Skip::Failed(format!("記録を書き直せない: {e}"), true));
        }
    }
    Ok(true)
}

/// 外すように渡す記録の1行（[`unplace`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub link: PathBuf,
    /// 置いたときの実体の番号（[`Placed::index`]）
    pub index: u64,
    /// 取り出した埋め込み JPEG か（[`Placed::extracted`]）
    pub extracted: bool,
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

    fn holder(&mut self, link: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
        self.db.google_place_holder(link).map_err(io::Error::other)
    }

    fn settle(&mut self, link: &Path) -> io::Result<()> {
        self.db.google_place_settle(link).map_err(io::Error::other)
    }

    fn forget(&mut self, link: &Path) -> io::Result<()> {
        self.db
            .google_place_forget(&[link.to_path_buf()])
            .map_err(io::Error::other)
    }

    fn holders_folded(&mut self, link: &Path) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
        self.db
            .google_place_holders_folded(link, cfg!(any(windows, target_os = "macos")))
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
/// - **置く前に `copied` を保留に書く**（DB の `google_pending`）。置けたもの・何度やっても同じもの・
///   置かないと決めたものは保留から外し、**一時的な失敗だけを残す**（[`PlaceReport::retry`]）。
///   途中で落ちても保留が残るので、次の [`retry_pending`] が拾う（2026-10-05 利用者決定）。
///   フォルダごと置けない誤り（場所が見えない等）のときは、全部を保留のまま返す
/// - ライブラリの除外パターン（[`crate::config::LibraryConfig::exclude_patterns`]）に当たるものは
///   置かない——一覧に出ないので、pictkura から外す手段が無くなる（ゲート2）
/// - RAW だけのカットで埋め込み JPEG と決めたものは、取り出して置く（[`place`]。PR5）
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
    if !is_on(config) {
        return Ok(None);
    }
    if copied.is_empty() {
        return Ok(Some(PlaceReport::default()));
    }
    db.google_pending_add(copied, dest)
        .map_err(io::Error::other)?;
    let report = place_now(copied, dest, config, db)?;
    // 一時的な失敗のほかは保留から外す
    let keep: HashSet<&PathBuf> = report.retry.iter().collect();
    let done: Vec<PathBuf> = copied
        .iter()
        .filter(|p| !keep.contains(p))
        .cloned()
        .collect();
    db.google_pending_remove(&done).map_err(io::Error::other)?;
    Ok(Some(report))
}

/// **明示して選んだもの**の置き方の規則（`dev/plan.google-photos-from-library.md` §5。2026-10-06 利用者決定）。
/// 動画も、RAW だけのカットも（埋め込み JPEG で）置く——設定は取り込みのときの自動の分にだけ効く。
/// ほかは取り込みと同じ: 組（RAW と JPEG）は JPEG だけ、OneDrive の中は設定どおり（空き容量の話で、
/// 選んだかどうかに関係しない）。設定ファイルだけにある `exclude_raw = false`（RAW そのものも置く）も
/// 取り込みと同じく効く——選んだときだけ RAW を落とすと、同じ写真が入口によって違う形で並ぶ（ゲート1）
fn chosen_rules(cfg: &GoogleMirrorConfig) -> GoogleMirrorConfig {
    GoogleMirrorConfig {
        exclude_video: false,
        raw_only: RawOnly::EmbeddedJpeg,
        ..cfg.clone()
    }
}

/// `path` を持つルートのうち、**いちばん外側**のもの。[`owning_root`] の選び方の本体——置き先の
/// ドライブ（[`place_chosen`]）と相対パスが別のルートから決まらないよう、1か所にだけ置く（ゲート2）
fn outer_root<'a>(path: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root| strip_root(path, root).is_some_and(|rest| !rest.as_os_str().is_empty()))
        .min_by_key(|root| root.components().count())
}

/// 選んだものを Google 用フォルダへ置く前の見積もり（確認に出す。設計 ② §1）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChosenSummary {
    /// 置く写真（原本へのリンク）
    pub photos: usize,
    /// 置く動画
    pub videos: usize,
    /// RAW だけのカット（埋め込み JPEG を取り出して置く）
    pub raw_only: usize,
    /// リンクで置くものの大きさの合計（Google へ上がる量の目安。取り出す JPEG は含まない）
    pub bytes: u64,
    /// 置き先の Google 用フォルダ（ドライブごと）。決まらないルートの写真は数えない
    pub folders: Vec<PathBuf>,
    /// `folders` のうち、まだ無い（送ると新しく作る）もの。Google フォトへの登録も要るので、
    /// 枚数に関係なく確認で知らせる（2026-10-06 利用者）
    pub new_folders: Vec<PathBuf>,
    /// 置き先が決まらない（ドライブ丸ごとのルート等）ので置けないもの
    pub unplaceable: usize,
    /// 選んだのに置かないもの（組の RAW だけを選んだ・OneDrive の中・ライブラリの外 等）。
    /// 組の相方がいっしょに選ばれて置かれる RAW は数えない（[`left_out`]）
    pub left_out: usize,
}

/// 選んだのに置かないものの数。黙って消えると「押したのに何も起きない」になる（ゲート2）。
/// 組の RAW で、相方の写真がこの回に置かれるものは数えない——組を選んだ人には「JPEG だけ置く」が答え
pub fn left_out(chosen: &[PathBuf], planned: &[Placement]) -> usize {
    let placed: HashSet<&PathBuf> = planned.iter().map(|p| &p.source).collect();
    // 相方と数えるのは**置く写真**だけ（動画・取り出しの JPEG・RAW は組の相方ではない。ゲート2）
    let keys: HashSet<(PathBuf, std::ffi::OsString)> = planned
        .iter()
        .filter(|p| !p.embedded && MediaKind::from_path(&p.source) == MediaKind::Photo)
        .map(|p| pair_key_folded(&p.source))
        .collect();
    let mut seen: HashSet<&PathBuf> = HashSet::new();
    chosen
        .iter()
        .filter(|p| seen.insert(p))
        .filter(|p| !placed.contains(p))
        .filter(|p| {
            !(MediaKind::from_path(p) == MediaKind::Raw && keys.contains(&pair_key_folded(p)))
        })
        .count()
}

/// 選んだもの（原本と大きさ）を、置くときと同じ規則で数える。ファイルは読まない（組の相方だけは
/// ディスクで探す——[`plan`] と同じ）。
pub fn summarize_chosen(items: &[(PathBuf, u64)], config: &crate::Config) -> ChosenSummary {
    let roots = &config.library.roots;
    let rules = chosen_rules(&config.google_mirror);
    let sources: Vec<PathBuf> = items.iter().map(|(p, _)| p.clone()).collect();
    let sizes: HashMap<&PathBuf, u64> = items.iter().map(|(p, n)| (p, *n)).collect();
    let planned = plan(
        &sources,
        roots,
        &rules,
        &mut photos_on_disk(&config.import.extensions),
    );
    let mut out = ChosenSummary {
        left_out: left_out(&sources, &planned),
        ..ChosenSummary::default()
    };
    let mut where_of: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();
    for p in &planned {
        let Some(root) = outer_root(&p.source, roots) else {
            continue;
        };
        let dir = where_of
            .entry(root.clone())
            .or_insert_with(|| location_for_root(root, &config.google_mirror.locations, roots).ok())
            .clone();
        let Some(dir) = dir else {
            out.unplaceable += 1;
            continue;
        };
        if !out.folders.contains(&dir) {
            // 「新しく作る」と言うのは、作れる（親が在る）ときだけ。外れたドライブの場所を
            // 「作ります・登録してください」と言わない（ゲート2）
            // 壊れたリンクが居座っているのも「無い」ではない（置くときに断られる。ゲート2）
            // 読めないだけ（権限・共有の一時的な誤り）は「無い」と言わない（PR の codex）
            if is_missing(&dir) && dir.parent().is_some_and(Path::is_dir) {
                out.new_folders.push(dir.clone());
            }
            out.folders.push(dir);
        }
        if p.embedded {
            out.raw_only += 1;
        } else {
            if MediaKind::from_path(&p.source) == MediaKind::Video {
                out.videos += 1;
            } else {
                out.photos += 1;
            }
            out.bytes += sizes.get(&p.source).copied().unwrap_or(0);
        }
    }
    out
}

/// [`place_chosen`] の結果。
#[derive(Debug, Default)]
pub struct ChosenReport {
    pub report: PlaceReport,
    /// 選んだのに置かないもの（[`left_out`]）
    pub left_out: usize,
    /// フォルダごと置けなかったルートの、最初の理由（場所が決まらない等）。その分は `report.failed` にも入る
    pub root_error: Option<MirrorError>,
}

/// **ライブラリから選んだもの**を Google 用フォルダへ置く（設計 ②）。ルートごとに、そのドライブの
/// Google 用フォルダ（[`location_for_root`]）へ置く。置き先が決まらないルートの分は失敗として返し、
/// 残りは続ける。
///
/// 一時的な失敗は**保留に残さない**——保留の置き直しは取り込みの規則（設定の動画・RAW だけ）で回るので、
/// 明示して選んだ動画を落としてしまう。結果に出し、利用者がもう一度送れば置ける。
///
/// 設定で切っていれば何もしない（`None`。入口も出さない）。
pub fn place_chosen(
    sources: &[PathBuf],
    config: &crate::Config,
    db: &mut crate::db::Db,
) -> Result<Option<ChosenReport>, MirrorError> {
    if !is_on(config) {
        return Ok(None);
    }
    let roots = &config.library.roots;
    let rules = chosen_rules(&config.google_mirror);
    let onedrive = if rules.include_onedrive {
        Vec::new()
    } else {
        onedrive_folders()
    };
    let mut by_root: Vec<(PathBuf, Vec<PathBuf>)> = Vec::new();
    for src in sources {
        let Some(root) = outer_root(src, roots) else {
            continue;
        };
        match by_root.iter_mut().find(|(r, _)| r == root) {
            Some((_, list)) => list.push(src.clone()),
            None => by_root.push((root.clone(), vec![src.clone()])),
        }
    }
    let mut total = ChosenReport::default();
    let mut planned_all: Vec<Placement> = Vec::new();
    for (root, items) in by_root {
        let placements = plan_with(
            &items,
            roots,
            &rules,
            &mut photos_on_disk(&config.import.extensions),
            &onedrive,
        );
        planned_all.extend(placements.iter().cloned());
        if placements.is_empty() {
            continue;
        }
        let placed =
            location_for_root(&root, &config.google_mirror.locations, roots).and_then(|dir| {
                let mut ledger = DbLedger {
                    db: &mut *db,
                    dir: dir.clone(),
                };
                place(&dir, roots, &placements, &mut ledger)
            });
        match placed {
            Ok(r) => total.report.absorb(r),
            // フォルダごと置けない（場所が決まらない・見えない）: その分を1件ずつの失敗にして続け、
            // 理由は画面に出せるよう持ち帰る（ゲート2）
            Err(e) => {
                let why = e.to_string();
                total
                    .report
                    .failed
                    .extend(placements.into_iter().map(|p| (p.source, why.clone())));
                total.root_error.get_or_insert(e);
            }
        }
    }
    total.left_out = left_out(sources, &planned_all);
    Ok(Some(total))
}

/// 保留を置き直す（次の取り込みの前・起動のあとに呼ぶ）。設定で切っていれば何もしない。
/// 原本がもう無いものは保留から外す。取り込み先ごとに [`place_imported`] を通し、
/// フォルダごと置けない取り込み先は保留のまま次へ進む（最初の誤りだけを返す）。
pub fn retry_pending(
    config: &crate::Config,
    db: &mut crate::db::Db,
) -> Result<Option<PlaceReport>, MirrorError> {
    if !is_on(config) {
        return Ok(None);
    }
    let pending = db.google_pending_all().map_err(io::Error::other)?;
    let mut total = PlaceReport::default();
    let mut first_err = None;
    for (dest, sources) in pending {
        // 取り込み先が見えない（外付けが外れている等）なら、この組は触らない。ここで
        // 「原本が無い」と読むと保留を捨て、挿し直しても二度と置けない（ゲート1）
        if !dest.is_dir() {
            continue;
        }
        // 消えたと言えるのは、ファイルが `NotFound` で、**ライブラリの走査もその行を消した**ときだけ。
        // 見え方（フォルダが在るか）では決めない——Unix ではボリュームを外すとマウント先が残り、
        // その下に同じ並びが在ることもある（PR の codex が3周続けて別の形で指摘した）。
        // 走査は、走査できたルートの中でしか行を消さないので、外れたボリュームの原本は行が残る。
        // マウント先の下の並びが見えてルートが走査できた場合は、pictkura 自身がその原本を
        // 消えたと扱う（一覧からも消える）ので、判断が食い違わない。
        // 読めない（権限・入出力）ものは保留のまま、この回は飛ばす
        let mut here = Vec::new();
        let mut gone = Vec::new();
        for p in sources {
            match std::fs::symlink_metadata(&p) {
                Ok(_) => here.push(p),
                // 一度もライブラリに載っていない原本は「走査が消した」と読めない（コピーの直後・走査の
                // 前にボリュームが外れた等）。載ったことがあって、いま行が無いときだけ外す（PR4）
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let was = db.google_pending_was_indexed(&p);
                    match (db.get_meta_by_path(&p), was) {
                        (Ok(None), Ok(true)) => gone.push(p),
                        (Ok(_), Ok(_)) => {}
                        (Err(e), _) | (_, Err(e)) => {
                            first_err.get_or_insert(MirrorError::Io(io::Error::other(e)));
                        }
                    }
                }
                Err(_) => {}
            }
        }
        if let Err(e) = db.google_pending_remove(&gone) {
            first_err.get_or_insert(MirrorError::Io(io::Error::other(e)));
            continue;
        }
        match place_imported(&here, &dest, config, db) {
            Ok(Some(r)) => total.absorb(r),
            Ok(None) => {}
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    match first_err {
        Some(e) => Err(e),
        None => Ok(Some(total)),
    }
}

impl UnplaceReport {
    /// もう1つの結果を足し込む。**項目を足したらここも**
    pub fn absorb(&mut self, other: UnplaceReport) {
        let UnplaceReport {
            removed,
            discarded,
            deleted,
            gone,
            forget,
            failed,
        } = other;
        self.removed += removed;
        self.discarded += discarded;
        self.deleted += deleted;
        self.gone += gone;
        self.forget.extend(forget);
        self.failed.extend(failed);
    }
}

/// フォルダごとに外して、外し終えた記録を消す。DB の誤りは `failed` に積んで次へ進む
/// ——ファイルはもう動いているので、結果を捨てると何をゴミ箱へ渡したかが残らない（ゲート2）。
fn unplace_grouped(
    db: &mut crate::db::Db,
    by_dir: Vec<(PathBuf, Vec<Recorded>)>,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> UnplaceReport {
    let mut total = UnplaceReport::default();
    for (dir, recs) in by_dir {
        let r = unplace(&dir, &recs, discard);
        if let Err(e) = db.google_place_forget(&r.forget) {
            total
                .failed
                .push((dir.clone(), format!("記録から消せない: {e}")));
        }
        total.absorb(r);
    }
    total
}

fn group_by_dir(
    rows: impl IntoIterator<Item = (PathBuf, Recorded)>,
) -> Vec<(PathBuf, Vec<Recorded>)> {
    let mut map: HashMap<PathBuf, Vec<Recorded>> = HashMap::new();
    for (dir, rec) in rows {
        map.entry(dir).or_default().push(rec);
    }
    let mut by_dir: Vec<(PathBuf, Vec<Recorded>)> = map.into_iter().collect();
    by_dir.sort_by(|a, b| a.0.cmp(&b.0));
    by_dir
}

/// **pictkura でゴミ箱へ入れた**原本のリンクを外す（設計書 §4。PR4）。原本はゴミ箱の中で実体を
/// リンクと分け合っている（名前の数 2）ので、リンクの名前を消すだけ。保留からも外す。
///
/// 何を入れたかを知っているのはゴミ箱へ入れた側だけなので、そこから原本の並びを渡す
/// ——走査のあとの突き合わせ（[`sweep_orphans`]）は、移動とゴミ箱を見分けられない。
pub fn unplace_sources(
    db: &mut crate::db::Db,
    sources: &[PathBuf],
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> Result<UnplaceReport, MirrorError> {
    // **先に印を付ける**。外すのに一時的に失敗しても、次の突き合わせが名前の数に関係なく外す
    // ——原本はゴミ箱の中で名前を持ち続けるので、印が無いと「移っただけ」と見分けられない（ゲート1）
    db.google_place_doom(sources).map_err(io::Error::other)?;
    let rows = db
        .google_placed_for_sources(sources)
        .map_err(io::Error::other)?;
    // 取り出した JPEG は**消す**（ゴミ箱へ入れない）——RAW はゴミ箱にあって戻せ、戻せばいつでも
    // 取り出し直せる（設計書 §4b。2026-10-05 利用者決定）
    // 作業場を片付けるフォルダは**記録を消す前に**控える——最後の1件の記録が消えると、突き合わせの
    // 片付け（[`tidy_work_dirs`]）は作業場を見つけられない（PR の codex）
    let mut dirs: Vec<PathBuf> = rows.iter().map(|(d, _)| d.clone()).collect();
    dirs.sort();
    dirs.dedup();
    let (extracted, links): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(_, r)| r.extracted);
    let mut report = unplace_grouped(db, group_by_dir(links), discard);
    report.absorb(delete_extracted_grouped(db, group_by_dir(extracted)));
    tidy_work_dirs(&dirs, &mut report);
    if let Err(e) = db.google_pending_remove(sources) {
        report
            .failed
            .push((PathBuf::new(), format!("保留から外せない: {e}")));
    }
    Ok(report)
}

/// **書き出しで移した**原本の記録を消す（リンクは残す。設計書 §4「移してもリンクはそのまま」）。
/// 追い続けると、別のドライブへ移した原本は OS のゴミ箱を経て消え、残ったリンクが「最後の1枚」に
/// 見えてゴミ箱へ渡される——写真は移した先で生きているのに（ゲート2）。
pub fn forget_moved(db: &mut crate::db::Db, sources: &[PathBuf]) -> Result<(), MirrorError> {
    let rows = db
        .google_placed_for_sources(sources)
        .map_err(io::Error::other)?;
    let links: Vec<PathBuf> = rows.into_iter().map(|(_, r)| r.link).collect();
    db.google_place_forget(&links).map_err(io::Error::other)?;
    db.google_pending_remove(sources)
        .map_err(io::Error::other)?;
    Ok(())
}

/// 原本が pictkura の外で消えて、**Google 用フォルダのリンクが最後の1枚になった**ものを
/// ゴミ箱へ渡す（設計書 §4。PR4）。走査・監視のあとに呼ぶ。
///
/// - 見るのは、原本の行がライブラリに無く、原本のファイルも無い記録
/// - **名前の数が 2 以上なら触らない**——原本はどこかへ移った・名前が変わった（Finder でフォルダの
///   名前を変えた、書き出しで移した）か、OS のゴミ箱の中にある。外すと、Google がまだ上げて
///   いない写真が上がらずじまいになる（ゲート2）。OS のゴミ箱を空にした時点で名前の数が 1 になり、
///   次の回に拾う
/// - 名前の数が 1 のものは消さずに `discard`（ゴミ箱）へ渡す——[`unplace`] の規則のまま
///
/// 設定で切っていても回す——前に置いたリンクを、原本を消したあとまで残さないため。
pub fn sweep_orphans(
    db: &mut crate::db::Db,
    discard: &mut dyn FnMut(&Path) -> io::Result<()>,
) -> Result<UnplaceReport, MirrorError> {
    let dirs = db.google_placed_dirs().map_err(io::Error::other)?;
    let orphans = db.google_placed_orphans().map_err(io::Error::other)?;
    // 渡すのは「原本のファイルが無い」記録のうち、**リンクに別の名前が残っているもの以外**。
    // リンクがもう無い・番号が違う記録も渡す——[`unplace`] が「もう無い」として記録から消す。
    // 飛ばすと、表に残って毎回見直すことになる（ゲート2）
    // 外すと決まった印のある行（pictkura がゴミ箱へ入れた原本の、外し損ねたリンク）は無条件に渡す
    // 取り出した JPEG は名前の数を見ない——名前はいつも1つの約束で、作業場の名前を消し損ねて
    // 2つになっていても「移っただけ」ではない（ゲート2）
    let last_copies = orphans.into_iter().filter(|(_, rec, source, doomed)| {
        *doomed
            || (matches!(std::fs::symlink_metadata(source), Err(e) if e.kind() == io::ErrorKind::NotFound)
                && (rec.extracted
                    || !file_id(&rec.link).is_ok_and(|id| id.links >= 2 && id.index == rec.index)))
    });
    // 取り出した JPEG のうち、pictkura が RAW をゴミ箱へ入れた印のある行は消す（[`unplace_sources`] で
    // 外し損ねたもの）。外で RAW が消えたものはゴミ箱へ——手元に残る最後の1枚かもしれない（§4b）。
    // 取り出したものは名前がいつも1つなので、名前の数では「移っただけ」と見分けられない。
    // RAW を外で移した・名前を変えたときもゴミ箱へ渡る（戻せる。まだ上がっていなかった分は
    // 上がらずじまいになる——据え置き、設計書 §7d）
    let (deleted, rest): (Vec<_>, Vec<_>) =
        last_copies.partition(|(_, rec, _, doomed)| rec.extracted && *doomed);
    let mut report = unplace_grouped(
        db,
        group_by_dir(rest.into_iter().map(|(dir, rec, _, _)| (dir, rec))),
        discard,
    );
    report.absorb(delete_extracted_grouped(
        db,
        group_by_dir(deleted.into_iter().map(|(dir, rec, _, _)| (dir, rec))),
    ));
    tidy_work_dirs(&dirs, &mut report);
    Ok(report)
}

/// 取り出した JPEG を**消して**外す（ゴミ箱を通さない。設計書 §4b）。[`unplace`] は名前が
/// 最後の1つのものを `discard` へ渡すので、そこで消し、数は `deleted` へ付け替える
/// ——記録に「ゴミ箱へ」と書かない（ゲート2）
fn delete_extracted_grouped(
    db: &mut crate::db::Db,
    by_dir: Vec<(PathBuf, Vec<Recorded>)>,
) -> UnplaceReport {
    let mut r = unplace_grouped(db, by_dir, &mut |path: &Path| std::fs::remove_file(path));
    r.deleted += std::mem::take(&mut r.discarded);
    r
}

/// [`place_imported`] の中身（保留の出し入れを除く）。
fn place_now(
    copied: &[PathBuf],
    dest: &Path,
    config: &crate::Config,
    db: &mut crate::db::Db,
) -> Result<PlaceReport, MirrorError> {
    let onedrive = if config.google_mirror.include_onedrive {
        Vec::new()
    } else {
        onedrive_folders()
    };
    place_now_with(copied, dest, config, db, &onedrive)
}

/// 連写の鎖に使う、1枚の撮影の印（機体と、秒未満まで分かる撮影時刻のミリ秒）。
/// 一覧の重ね（`ui/src/stacks.ts` の `burstTime`）と同じ条件: 機体が分かり、秒未満まで分かること。
/// どちらかが欠けたら `None`（束ねない——誤って間引くより、全部置く）
fn capture_of(path: &Path) -> Option<(String, i64)> {
    let exif = crate::thumbs::read_exif_capture(path)?;
    let camera = exif.camera.filter(|c| !c.trim().is_empty())?;
    if !exif.taken_subsec {
        return None;
    }
    let ms = exif.taken_at_ms?;
    let serial = exif.body_serial.unwrap_or_default();
    Some((format!("{camera}\u{1}{}", serial.trim()), ms))
}

/// 取り込みの回の連写を、**最初のコマだけ**にする（設計 §3b。2026-10-05 利用者: 連写の JPEG が全部並ぶと
/// 家族のアルバムが埋まる）。束ね方は一覧の重ね（`ui/src/stacks.ts`）と同じ: 同じ機体のコマを撮影時刻の順に
/// 並べ、直前のコマとの間隔が `gap_ms` 以下なら鎖でつなぐ。日をまたいだら切る。2コマ以上の鎖が連写。
///
/// - 表紙は**撮り始めのコマ**（取り込んだばかりで ⚑ はまだ無い。あとで ⚑ を付けたコマは、
///   [`place_chosen`] で足す——設計 §3b の案B）
/// - 動画は鎖に入れない。機体か秒未満が分からないコマも入れない（そのまま置く）
/// - 撮影の印を読めないものは束ねない側へ倒す
fn thin_bursts(
    placements: Vec<Placement>,
    gap_ms: i64,
    capture: &mut dyn FnMut(&Path) -> Option<(String, i64)>,
) -> Vec<Placement> {
    use chrono::TimeZone;
    let day_of = |ms: i64| {
        chrono::Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|t| t.date_naive())
    };
    // **コマは組で数える**（同じフォルダ・同じ名前の RAW と JPEG は1コマ）。設定ファイルの `exclude_raw = false`
    // では組の両方を置くので、ファイルで数えると連写でない1組の片方を落とす（ゲート1）
    // 組にするのは一覧（`shotsOfDay`）と同じ条件だけ: 同じフォルダ・同じ名前の **RAW と RAW 以外**で、
    // 撮影時刻が**同じ秒**のもの。名前が同じだけ（`IMG_1.JPG` と `IMG_1.HEIC`、名前を使い回した別の撮影）は
    // 別のコマ（PR の codex）
    let stamps: Vec<Option<(String, i64)>> = placements
        .iter()
        .map(|p| {
            if MediaKind::from_path(&p.source) == MediaKind::Video {
                None
            } else {
                capture(&p.source)
            }
        })
        .collect();
    let is_raw = |i: usize| MediaKind::from_path(&placements[i].source) == MediaKind::Raw;
    let mut shots: Vec<((PathBuf, std::ffi::OsString), Vec<usize>)> = Vec::new();
    for (i, p) in placements.iter().enumerate() {
        if MediaKind::from_path(&p.source) == MediaKind::Video {
            continue;
        }
        let key = pair_key_folded(&p.source);
        let partner = shots.iter_mut().find(|(k, files)| {
            *k == key
                && files.len() == 1
                && is_raw(files[0]) != is_raw(i)
                && matches!(
                    (&stamps[files[0]], &stamps[i]),
                    (Some((_, a)), Some((_, b))) if a.div_euclid(1000) == b.div_euclid(1000)
                )
        });
        match partner {
            Some((_, files)) => files.push(i),
            None => shots.push((key, vec![i])),
        }
    }
    // 機体ごとに、(撮影時刻, コマの番号)。コマの時刻は読めた最初のファイルのもの
    let mut by_body: HashMap<String, Vec<(i64, usize)>> = HashMap::new();
    for (n, (_, files)) in shots.iter().enumerate() {
        if let Some((body, ms)) = files.iter().find_map(|&i| stamps[i].clone()) {
            by_body.entry(body).or_default().push((ms, n));
        }
    }
    let mut drop: HashSet<usize> = HashSet::new();
    for list in by_body.values_mut() {
        list.sort();
        let mut run_start = 0;
        for k in 1..=list.len() {
            let breaks = k == list.len()
                || list[k].0 - list[k - 1].0 > gap_ms
                || day_of(list[k].0) != day_of(list[k - 1].0);
            if breaks {
                // 鎖 [run_start, k) が2コマ以上なら、最初の1コマ以外のファイルを落とす
                if k - run_start >= 2 {
                    for &(_, n) in &list[run_start + 1..k] {
                        drop.extend(shots[n].1.iter().copied());
                    }
                }
                run_start = k;
            }
        }
    }
    placements
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, p)| p)
        .collect()
}

/// [`place_now`] の OneDrive の場所を外から渡す形（試験はこちらを呼ぶ。台の OneDrive に左右されない）。
fn place_now_with(
    copied: &[PathBuf],
    dest: &Path,
    config: &crate::Config,
    db: &mut crate::db::Db,
    onedrive: &[PathBuf],
) -> Result<PlaceReport, MirrorError> {
    let g = &config.google_mirror;
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
    let planned = plan_with(
        &copied,
        roots,
        g,
        &mut photos_on_disk(&config.import.extensions),
        onedrive,
    );
    // フォルダを解決できなかったものは、OneDrive の判定で「中」と見なされて計画から落ちる
    // （置く側へ倒さないため）。**計画から落ちたものに限り、保留に残す**——一時的に見えなかった
    // だけなら次で置ける（ゲート2）。計画に入ったものまで残すと、置けても保留が消えない
    let unsure: Vec<PathBuf> = if onedrive.is_empty() {
        Vec::new()
    } else {
        let in_plan: HashSet<&PathBuf> = planned.iter().map(|p| &p.source).collect();
        let mut seen: HashMap<PathBuf, bool> = HashMap::new();
        copied
            .iter()
            .filter(|p| !in_plan.contains(p))
            .filter(|p| {
                let parent = p.parent().unwrap_or(Path::new("")).to_path_buf();
                !*seen
                    .entry(parent)
                    .or_insert_with_key(|d| disk_key(d).is_ok())
            })
            .cloned()
            .collect()
    };
    // 連写は表紙の1コマだけ（設計 §3b。一覧で連写を重ねているときだけ——重ねない人には連写が見えない）
    let placements = if config.grid.stack_bursts {
        thin_bursts(
            planned,
            i64::from(config.grid.burst_gap_ms),
            &mut capture_of,
        )
    } else {
        planned
    };
    let deferred = PlaceReport {
        retry: unsure,
        ..PlaceReport::default()
    };
    if placements.is_empty() {
        return Ok(deferred);
    }
    let dir = location_for_root(dest, &g.locations, roots)?;
    let mut ledger = DbLedger {
        db,
        dir: dir.clone(),
    };
    let mut report = place(&dir, roots, &placements, &mut ledger)?;
    report.absorb(deferred);
    Ok(report)
}

/// [`unplace`] の結果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnplaceReport {
    /// 外したリンク（実体は原本の側に残る）
    pub removed: usize,
    /// **そこにしか実体が無かった**ので `discard` へ渡したもの
    pub discarded: usize,
    /// 取り出した JPEG で、ゴミ箱を通さずに消したもの（pictkura が RAW をゴミ箱へ入れた。§4b）
    pub deleted: usize,
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
    for Recorded {
        link,
        index,
        extracted,
    } in records
    {
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
        // 取り出した JPEG は名前の数を見ない——2つ目の名前は作業場に消し損ねたもので、次の取り出しが
        // 片付ける。名前を外すだけにすると、片付けのときに中身ごと消える（PR の codex）
        let links = if *extracted { 1 } else { id.links };
        match remove_or_discard(link, links, discard) {
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

/// Google 用フォルダとして使ってよいか（[`place`] が毎回見る門）。設定画面で場所を決めるときも
/// これを通す——ここより緩い確かめで保存すると、入れたのに置くたびに断られる（ゲート1）。
pub fn check_dir(dir: &Path, roots: &[PathBuf]) -> Result<(), MirrorError> {
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
/// **綴りではなく実体で比べる**（[`disk_key`]）。`D:\Photos` と `d:\photos\pictkura-outgoing`、
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
            fn holder(&mut self, _: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
                Ok(None)
            }
            fn forget(&mut self, _: &Path) -> io::Result<()> {
                Ok(())
            }
            fn holders_folded(
                &mut self,
                _: &Path,
            ) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
                Ok(Vec::new())
            }
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
        fn holder(&mut self, link: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
            Ok(self
                .0
                .iter()
                .find(|r| r.link == link)
                .map(|r| (r.source.clone(), r.index, r.extracted)))
        }
        fn forget(&mut self, link: &Path) -> io::Result<()> {
            self.0.retain(|r| r.link != link);
            Ok(())
        }
        fn holders_folded(
            &mut self,
            link: &Path,
        ) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
            Ok(self
                .0
                .iter()
                .filter(|r| fold_path(&r.link) == fold_path(link))
                .map(|r| (r.link.clone(), r.source.clone(), r.index, r.extracted))
                .collect())
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
                    extracted: p.extracted,
                })
                .collect()
        }
    }

    fn rec(link: PathBuf) -> Recorded {
        Recorded {
            link,
            index: 0,
            extracted: false,
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
    fn a_folder_made_by_this_run_is_reported_once() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/b.jpg"), b"photo b");
        let mut book = Book::default();
        let r = book.place(&f, &[placement(&f.lib, "d/a.jpg")]);
        assert_eq!(r.new_folders, std::slice::from_ref(&f.google));
        // 在るフォルダへ置いた回は知らせない
        let r = book.place(&f, &[placement(&f.lib, "d/b.jpg")]);
        assert!(r.new_folders.is_empty());
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
            extracted: false,
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
            extracted: false,
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
            fn holder(&mut self, _: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
                Ok(None)
            }
            fn forget(&mut self, _: &Path) -> io::Result<()> {
                Ok(())
            }
            fn holders_folded(
                &mut self,
                _: &Path,
            ) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
                Ok(Vec::new())
            }
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
            fn holder(&mut self, _: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
                Ok(None)
            }
            fn forget(&mut self, _: &Path) -> io::Result<()> {
                Ok(())
            }
            fn holders_folded(
                &mut self,
                _: &Path,
            ) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
                Ok(Vec::new())
            }
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
            extracted: false,
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

    /// 埋め込み JPEG を持つ RAW（TIFF の形。IFD0 に JPEG の申告と向き、ExifIFD に撮影日時）。
    /// `preview` が `None` なら絵を持たない
    fn raw_with_preview(path: &Path, orientation: u16, preview: Option<&[u8]>) {
        let jpeg = preview.unwrap_or(&[]);
        let mut buf: Vec<u8> = b"II".to_vec();
        buf.extend_from_slice(&42u16.to_le_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes());
        // IFD0（8〜62）: Orientation・JPEGInterchangeFormat・その長さ・ExifIFD への指し
        // ExifIFD（62〜80）: DateTimeOriginal、文字列は 80〜100、絵は 100 から
        let mut entries: Vec<(u16, u16, u32)> = vec![(274, 3, orientation as u32)];
        if preview.is_some() {
            entries.push((513, 4, 100));
            entries.push((514, 4, jpeg.len() as u32));
        }
        entries.push((0x8769, 4, 0));
        let exif_at = (8 + 2 + entries.len() * 12 + 4) as u32;
        entries.last_mut().unwrap().2 = exif_at;
        let dto_at = exif_at + 2 + 12 + 4;
        let jpeg_at = dto_at + 20;
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, kind, value) in entries {
            let value = if tag == 513 { jpeg_at } else { value };
            buf.extend_from_slice(&tag.to_le_bytes());
            buf.extend_from_slice(&kind.to_le_bytes());
            buf.extend_from_slice(&1u32.to_le_bytes());
            if kind == 3 {
                buf.extend_from_slice(&(value as u16).to_le_bytes());
                buf.extend_from_slice(&[0, 0]);
            } else {
                buf.extend_from_slice(&value.to_le_bytes());
            }
        }
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&0x9003u16.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&20u32.to_le_bytes());
        buf.extend_from_slice(&dto_at.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(b"2026:10:05 09:30:15\0");
        buf.extend_from_slice(jpeg);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &buf).unwrap();
    }

    fn preview_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    fn dto_of(path: &Path) -> Option<String> {
        let file = std::fs::File::open(path).ok()?;
        let exif = exif::Reader::new()
            .read_from_container(&mut std::io::BufReader::new(file))
            .ok()?;
        let f = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
        match &f.value {
            exif::Value::Ascii(v) => Some(String::from_utf8_lossy(v.first()?).into_owned()),
            _ => None,
        }
    }

    fn embedded(lib: &Path, rel: &str) -> Placement {
        Placement {
            embedded: true,
            ..placement(lib, rel)
        }
    }

    #[test]
    fn an_embedded_jpeg_is_extracted_dated_and_recorded_as_its_own_file() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1600, 1200)));
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.placed, r.failed.len()), (1, 0), "{:?}", r.failed);
        let out = f.google.join("d/B.jpg");
        let img = image::open(&out).expect("取り出したものが絵として読めない");
        assert_eq!((img.width(), img.height()), (1600, 1200));
        assert_eq!(dto_of(&out).as_deref(), Some("2026:10:05 09:30:15"));
        // 原本とは別の実体で、名前は1つ（作業場の名前は消えている）
        let id = file_id(&out).unwrap();
        assert_eq!(id.links, 1);
        assert_ne!(id.index, file_id(&f.lib.join("d/B.ARW")).unwrap().index);
        assert_eq!(
            book.0,
            [Placed {
                source: f.lib.join("d/B.ARW"),
                link: out.clone(),
                index: id.index,
                extracted: true,
            }]
        );
        // 作業場は Google 用フォルダの外で、終われば畳まれる
        assert!(!work_dir_for(&f.google).unwrap().exists());
        // もう一度置いても、前の回のものが在れば取り出し直さない
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.placed, r.already), (0, 1));
        assert_eq!(file_id(&out).unwrap().index, id.index);
    }

    #[test]
    fn a_turned_preview_is_turned_upright_and_still_dated() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 6, Some(&preview_jpeg(1600, 1200)));
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!(r.placed, 1, "{:?}", r.failed);
        let out = f.google.join("d/B.jpg");
        let img = image::open(&out).unwrap();
        assert_eq!((img.width(), img.height()), (1200, 1600));
        assert_eq!(dto_of(&out).as_deref(), Some("2026:10:05 09:30:15"));
    }

    #[test]
    fn a_raw_without_a_preview_fails_for_good_and_leaves_nothing() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, None);
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.placed, r.failed.len(), r.retry.len()), (0, 1, 0));
        assert!(book.0.is_empty());
        assert!(!f.google.join("d").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_raw_that_cannot_be_read_now_is_kept_for_a_retry() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let raw = f.lib.join("d/B.ARW");
        raw_with_preview(&raw, 1, Some(&preview_jpeg(1200, 900)));
        // 名前は見えるが中身を開けない（Windows の共有ロックの代わり）
        std::fs::set_permissions(&raw, std::fs::Permissions::from_mode(0o000)).unwrap();
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        std::fs::set_permissions(&raw, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!((r.placed, r.failed.len()), (0, 1));
        assert_eq!(
            r.retry,
            std::slice::from_ref(&raw),
            "読めなかっただけなら保留に残す"
        );
        // 読めるようになれば置ける
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!(r.placed, 1, "{:?}", r.failed);
    }

    #[test]
    fn a_thumbnail_sized_preview_is_not_sent_as_the_photo() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(160, 120)));
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.placed, r.failed.len(), r.retry.len()), (0, 1, 0));
        assert!(!f.google.join("d/B.jpg").exists());
    }

    #[test]
    fn the_real_jpeg_imported_later_takes_its_name_back_from_the_extract() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let mut book = Book::default();
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/B.ARW")]).placed, 1);
        // 同じカットの JPEG が別の回に来た
        put(&f.lib.join("d/B.jpg"), b"camera jpeg");
        let r = book.place(&f, &[placement(&f.lib, "d/B.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (1, 0), "{:?}", r.failed);
        let out = f.google.join("d/B.jpg");
        assert!(same_file(&f.lib.join("d/B.jpg"), &out).unwrap());
        assert_eq!(book.0.len(), 1);
        assert!(!book.0[0].extracted && book.0[0].source == f.lib.join("d/B.jpg"));
        // 別のカットの取り出しには触らない
        raw_with_preview(&f.lib.join("d/C.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/C.ARW")]).placed, 1);
        put(&f.lib.join("e/C.jpg"), b"elsewhere");
        let _ = book.place(&f, &[placement(&f.lib, "e/C.jpg")]);
        assert!(f.google.join("d/C.jpg").exists());
    }

    /// 大文字小文字を区別しない台でだけ、名前が同じになる
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_real_jpeg_spelled_in_another_case_still_takes_the_name_back() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let mut book = Book::default();
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/B.ARW")]).placed, 1);
        put(&f.lib.join("d/b.JPG"), b"camera jpeg");
        let r = book.place(&f, &[placement(&f.lib, "d/b.JPG")]);
        assert_eq!((r.placed, r.failed.len()), (1, 0), "{:?}", r.failed);
        assert!(same_file(&f.lib.join("d/b.JPG"), &f.google.join("d/b.JPG")).unwrap());
        assert!(book.0.iter().all(|r| !r.extracted));
    }

    #[cfg(unix)]
    #[test]
    fn an_extract_behind_a_linked_folder_is_not_touched() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let mut book = Book::default();
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/B.ARW")]).placed, 1);
        // アルバムを外へ移して、同じ名前のリンクに差し替えた
        let outside = f.lib.parent().unwrap().join("outside");
        std::fs::rename(f.google.join("d"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, f.google.join("d")).unwrap();
        put(&f.lib.join("d/B.jpg"), b"camera jpeg");
        let _ = book.place(&f, &[placement(&f.lib, "d/B.jpg")]);
        assert!(outside.join("B.jpg").is_file(), "外のファイルを消した");
    }

    #[test]
    fn an_extract_with_a_stray_second_name_still_goes_when_its_raw_is_gone() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db).unwrap();
        // 作業場の名前を消し損ねた（Windows で一瞬掴まれた等）
        let stray = f.lib.parent().unwrap().join("stray.jpg");
        std::fs::hard_link(f.google.join("d/B.jpg"), &stray).unwrap();
        std::fs::remove_file(f.lib.join("d/B.ARW")).unwrap();
        let trash = f.lib.parent().unwrap().join("trash");
        let r = sweep_orphans(&mut db, &mut move_into(&trash)).unwrap();
        // 名前を外すだけでは、作業場の名前が片付けられたときに中身ごと消える。ゴミ箱へ渡す
        assert_eq!((r.discarded, r.removed, r.failed.len()), (1, 0, 0));
        assert!(trash.join("B.jpg").is_file());
        assert!(!f.google.join("d/B.jpg").exists());
    }

    #[test]
    fn an_extract_linked_before_its_number_was_written_is_taken_back_on_retry() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let mut book = Book::default();
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/B.ARW")]).placed, 1);
        // 張ったが番号を書く前に落ちた形: 記録は古い番号のまま、名前には新しい実体
        let out = f.google.join("d/B.jpg");
        let bytes = std::fs::read(&out).unwrap();
        std::fs::remove_file(&out).unwrap();
        std::fs::write(&out, &bytes).unwrap();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.already, r.failed.len()), (1, 0), "{:?}", r.failed);
        assert_eq!(book.0[0].index, file_id(&out).unwrap().index);
        // 中身の違う他人のファイルは取らない
        std::fs::remove_file(&out).unwrap();
        std::fs::write(&out, b"someone else").unwrap();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.already, r.placed, r.failed.len()), (0, 0, 1));
        assert_eq!(std::fs::read(&out).unwrap(), b"someone else");
    }

    #[test]
    fn a_work_name_left_behind_is_cleared_by_the_next_sweep() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db).unwrap();
        // 作業場の名前を消し損ねた形（win の W12b）
        let work = work_dir_for(&f.google).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let left = work.join(format!("{WORK_PREFIX}1-0.jpg"));
        std::fs::hard_link(f.google.join("d/B.jpg"), &left).unwrap();
        // 利用者の置いたものには触らない
        let theirs = work.join("notes.txt");
        put(&theirs, b"mine");
        let r = sweep_orphans(&mut db, &mut no_discard()).unwrap();
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert!(!left.exists());
        assert!(theirs.exists());
        assert_eq!(file_id(&f.google.join("d/B.jpg")).unwrap().links, 1);
    }

    #[test]
    fn trashing_the_last_extract_also_clears_a_work_name_left_behind() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db).unwrap();
        let work = work_dir_for(&f.google).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        let left = work.join(format!("{WORK_PREFIX}1-0.jpg"));
        std::fs::hard_link(f.google.join("d/B.jpg"), &left).unwrap();
        let bin = f.lib.parent().unwrap().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::rename(f.lib.join("d/B.ARW"), bin.join("B.ARW")).unwrap();
        let r = unplace_sources(&mut db, &[f.lib.join("d/B.ARW")], &mut no_discard()).unwrap();
        assert_eq!((r.deleted, r.failed.len()), (1, 0), "{:?}", r.failed);
        // 記録はもう無いが、作業場の名前も消えている（中身がディスクに残らない）
        assert!(!left.exists());
        assert!(!work.exists());
    }

    #[test]
    fn a_stale_extract_record_left_by_a_crash_gives_way_on_retry() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let mut book = Book::default();
        assert_eq!(book.place(&f, &[embedded(&f.lib, "d/B.ARW")]).placed, 1);
        // 取り出しを消したが、記録を消す前に落ちた形
        std::fs::remove_file(f.google.join("d/B.jpg")).unwrap();
        put(&f.lib.join("d/B.jpg"), b"camera jpeg");
        let r = book.place(&f, &[placement(&f.lib, "d/B.jpg")]);
        assert_eq!((r.placed, r.failed.len()), (1, 0), "{:?}", r.failed);
        assert!(book.0.iter().all(|r| !r.extracted));
    }

    /// 外しきれずに印が残った行は、送り直して**置けたと確かめたら**印を下ろす（設計書 §3c）。
    /// 在る名前が別の実体（差し替わった原本の古いリンク）なら下ろさない——下ろすと古いリンクを
    /// 外す道が無くなる（ゲート2）。取り出した JPEG の「前の回のまま在る」近道でも下ろす
    #[test]
    fn re_sending_settles_the_mark_only_once_the_link_is_confirmed() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo a");
        put(&f.lib.join("d/c.jpg"), b"photo c");
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let all = ["d/a.jpg", "d/c.jpg", "d/B.ARW"].map(|r| f.lib.join(r));
        let r = place_imported(&all, &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!(r.placed, 3, "{:?}", r.failed);
        db.google_place_doom(&all).unwrap();
        let doomed = |db: &crate::db::Db| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = db
                .google_placed_orphans()
                .unwrap()
                .into_iter()
                .filter(|(_, _, _, d)| *d)
                .map(|(_, rec, _, _)| rec.link)
                .collect();
            v.sort();
            v
        };
        assert_eq!(doomed(&db).len(), 3);

        // c.jpg の原本を差し替える（新しい実体）。送り出しの c.jpg は古い実体のまま
        std::fs::remove_file(f.lib.join("d/c.jpg")).unwrap();
        put(&f.lib.join("d/c.jpg"), b"photo c, new");

        let c = place_chosen(&all, &config, &mut db).unwrap().unwrap();
        assert_eq!(c.report.already, 2, "{:?}", c.report.failed);
        assert_eq!(c.report.failed.len(), 1, "c.jpg は名前が塞がっている");
        assert_eq!(
            doomed(&db),
            [crate::paths::normalize(&f.google.join("d/c.jpg"))],
            "置けたと確かめた a.jpg と B.jpg（取り出し）だけ下りる"
        );
    }

    /// 設定は「動画は置かない・RAW だけは上げない」——明示して選んだものはそれでも置く（設計 ② §5）
    fn strict_on(f: &Fixture) -> crate::Config {
        let mut config = on(f);
        config.google_mirror.exclude_video = true;
        config.google_mirror.raw_only = RawOnly::None;
        config
    }

    fn chosen_shoot(f: &Fixture) -> Vec<PathBuf> {
        put(&f.lib.join("2010/a.jpg"), b"photo a");
        put(&f.lib.join("2010/P.ARW"), b"raw of a pair");
        put(&f.lib.join("2010/P.JPG"), b"jpeg of a pair");
        put(&f.lib.join("2010/v.mp4"), b"video");
        raw_with_preview(&f.lib.join("2010/R.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        ["a.jpg", "P.ARW", "P.JPG", "v.mp4", "R.ARW"]
            .iter()
            .map(|n| f.lib.join("2010").join(n))
            .collect()
    }

    #[test]
    fn chosen_videos_and_raw_only_cuts_are_placed_whatever_the_settings() {
        let f = fixture();
        let chosen = chosen_shoot(&f);
        let config = strict_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let c = place_chosen(&chosen, &config, &mut db).unwrap().unwrap();
        // 組の RAW は「置かないもの」に数えない（相方の JPEG が置かれる）
        assert_eq!(c.left_out, 0);
        let r = c.report;
        assert_eq!((r.placed, r.failed.len()), (4, 0), "{:?}", r.failed);
        let g = f.google.join("2010");
        assert!(same_file(&f.lib.join("2010/a.jpg"), &g.join("a.jpg")).unwrap());
        assert!(same_file(&f.lib.join("2010/v.mp4"), &g.join("v.mp4")).unwrap());
        assert!(same_file(&f.lib.join("2010/P.JPG"), &g.join("P.JPG")).unwrap());
        // 組の RAW は置かない。RAW だけのカットは埋め込み JPEG で
        assert!(!g.join("P.ARW").exists());
        assert!(g.join("R.jpg").is_file());
        // 保留には残さない（置き直しは取り込みの規則で回るので、選んだ動画を落とす）
        assert!(db.google_pending_all().unwrap().is_empty());
        // もう一度送っても重ならない
        let r = place_chosen(&chosen, &config, &mut db)
            .unwrap()
            .unwrap()
            .report;
        assert_eq!((r.placed, r.already), (0, 4));
    }

    #[test]
    fn the_summary_counts_what_sending_would_place() {
        let f = fixture();
        let chosen = chosen_shoot(&f);
        let config = strict_on(&f);
        let items: Vec<(PathBuf, u64)> = chosen
            .iter()
            .map(|p| (p.clone(), std::fs::metadata(p).unwrap().len()))
            .collect();
        let s = summarize_chosen(&items, &config);
        let bytes = ["a.jpg", "P.JPG", "v.mp4"]
            .iter()
            .map(|n| std::fs::metadata(f.lib.join("2010").join(n)).unwrap().len())
            .sum::<u64>();
        assert_eq!(
            s,
            ChosenSummary {
                photos: 2,
                videos: 1,
                raw_only: 1,
                bytes,
                folders: vec![f.google.clone()],
                // まだ置いたことが無いので、送ると作る
                new_folders: vec![f.google.clone()],
                unplaceable: 0,
                left_out: 0,
            }
        );
        // 数えるだけで、何も作らない
        assert!(!f.google.exists());
        // 在るフォルダは「新しく作る」に入れない
        std::fs::create_dir_all(&f.google).unwrap();
        let s = summarize_chosen(&items, &config);
        assert_eq!((s.folders.len(), s.new_folders.len()), (1, 0));
    }

    #[test]
    fn nothing_is_sent_while_the_switch_is_off_or_outside_the_library() {
        let f = fixture();
        let chosen = chosen_shoot(&f);
        let mut config = strict_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        config.google_mirror.enabled = false;
        assert!(place_chosen(&chosen, &config, &mut db).unwrap().is_none());
        config.google_mirror.enabled = true;
        let outside = f.lib.parent().unwrap().join("elsewhere/x.jpg");
        put(&outside, b"not in the library");
        let c = place_chosen(&[outside], &config, &mut db).unwrap().unwrap();
        assert_eq!(c.report, PlaceReport::default());
        // 黙って消さない: 置かないものとして数える
        assert_eq!(c.left_out, 1);
        assert!(!f.google.exists());
    }

    #[test]
    fn a_raw_chosen_without_its_jpeg_is_counted_as_left_out() {
        let f = fixture();
        chosen_shoot(&f);
        let config = strict_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // 組の RAW だけを選んだ（重ねずに見ている一覧で）
        let c = place_chosen(&[f.lib.join("2010/P.ARW")], &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!((c.report.placed, c.left_out), (0, 1));
    }

    #[test]
    fn a_video_of_the_same_name_does_not_cover_a_raw_left_out() {
        let f = fixture();
        chosen_shoot(&f);
        put(&f.lib.join("2010/P.MOV"), b"live photo");
        let config = strict_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // 組の RAW と、同じ名前の動画だけを選んだ（JPEG は選んでいない）
        let c = place_chosen(
            &[f.lib.join("2010/P.ARW"), f.lib.join("2010/P.MOV")],
            &config,
            &mut db,
        )
        .unwrap()
        .unwrap();
        assert_eq!((c.report.placed, c.left_out), (1, 1));
    }

    #[test]
    fn a_root_that_has_no_place_brings_its_reason_back() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let mut config = strict_on(&f);
        // 置き場の候補がライブラリの中（重なる）
        config.google_mirror.locations = vec![f.lib.join("inside")];
        std::fs::create_dir_all(f.lib.join("inside")).unwrap();
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let c = place_chosen(&[f.lib.join("d/a.jpg")], &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!(c.report.failed.len(), 1);
        assert!(
            matches!(c.root_error, Some(MirrorError::OverlapsRoot(_))),
            "{:?}",
            c.root_error
        );
    }

    #[test]
    fn a_burst_at_import_keeps_only_its_first_frame() {
        let lib = PathBuf::from("/lib");
        let p = |n: &str| placement(&lib, n);
        let placements = vec![
            p("d/B1.JPG"),
            p("d/B2.JPG"),
            p("d/B3.JPG"),
            p("d/LONE.JPG"),
            p("d/X1.JPG"),
            p("d/NOSUB.JPG"),
            p("d/CLIP.MP4"),
        ];
        // 機体 A で 0.4 秒おきの3コマ、2.5 秒あけて1枚。機体 B の1枚は A と同じ時刻でも別の鎖。
        // 秒未満の無いもの・動画は束ねない
        let mut capture = |path: &Path| -> Option<(String, i64)> {
            let name = path.file_name()?.to_str()?;
            let base = 1_700_000_000_000;
            match name {
                "B1.JPG" => Some(("A".into(), base)),
                "B2.JPG" => Some(("A".into(), base + 400)),
                "B3.JPG" => Some(("A".into(), base + 800)),
                "LONE.JPG" => Some(("A".into(), base + 3_300)),
                "X1.JPG" => Some(("B".into(), base + 400)),
                "CLIP.MP4" => Some(("A".into(), base + 900)),
                _ => None,
            }
        };
        let kept: Vec<String> = thin_bursts(placements, 1000, &mut capture)
            .into_iter()
            .map(|p| p.rel.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            kept,
            [
                "d/B1.JPG",
                "d/LONE.JPG",
                "d/X1.JPG",
                "d/NOSUB.JPG",
                "d/CLIP.MP4"
            ]
        );
    }

    #[test]
    fn a_raw_jpeg_pair_counts_as_one_frame_when_both_are_placed() {
        let lib = PathBuf::from("/lib");
        let p = |n: &str| placement(&lib, n);
        // exclude_raw = false で組の両方を置く形。連写でない1組と、2組の連写
        let placements = vec![
            p("d/SOLO.ARW"),
            p("d/SOLO.JPG"),
            p("d/C1.ARW"),
            p("d/C1.JPG"),
            p("d/C2.ARW"),
            p("d/C2.JPG"),
        ];
        let base = 1_700_000_000_000;
        let mut capture = |path: &Path| -> Option<(String, i64)> {
            let stem = path.file_stem()?.to_str()?;
            match stem {
                "SOLO" => Some(("A".into(), base)),
                "C1" => Some(("A".into(), base + 5_000)),
                "C2" => Some(("A".into(), base + 5_300)),
                _ => None,
            }
        };
        let kept: Vec<String> = thin_bursts(placements, 1000, &mut capture)
            .into_iter()
            .map(|p| p.rel.to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept, ["d/SOLO.ARW", "d/SOLO.JPG", "d/C1.ARW", "d/C1.JPG"]);
    }

    #[test]
    fn files_sharing_a_name_are_one_frame_only_as_a_raw_jpeg_pair_of_the_same_second() {
        let lib = PathBuf::from("/lib");
        let p = |n: &str| placement(&lib, n);
        let base = 1_700_000_000_000;
        // JPG と HEIC は名前が同じでも別のコマ。0.3 秒おきなので連写になり、HEIC は落ちる
        // （組と取り違えると、表紙と一緒に残ってしまう）
        let placements = vec![p("d/IMG_1.JPG"), p("d/IMG_1.HEIC")];
        let mut capture = |path: &Path| -> Option<(String, i64)> {
            match path.extension()?.to_str()? {
                "JPG" => Some(("A".into(), base)),
                "HEIC" => Some(("A".into(), base + 300)),
                _ => None,
            }
        };
        let kept: Vec<String> = thin_bursts(placements, 1000, &mut capture)
            .into_iter()
            .map(|p| p.rel.to_string_lossy().into_owned())
            .collect();
        assert_eq!(kept, ["d/IMG_1.JPG"]);
    }

    #[test]
    fn a_name_already_taken_is_not_overwritten_by_an_extract() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        put(&f.google.join("d/B.jpg"), b"someone else");
        let mut book = Book::default();
        let r = book.place(&f, &[embedded(&f.lib, "d/B.ARW")]);
        assert_eq!((r.placed, r.failed.len(), r.retry.len()), (0, 1, 0));
        assert!(book.0.is_empty(), "置けなかった行は取り消す");
        assert_eq!(
            std::fs::read(f.google.join("d/B.jpg")).unwrap(),
            b"someone else"
        );
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

    fn on(f: &Fixture) -> crate::Config {
        let mut config = crate::Config::default();
        config.library.roots = vec![f.lib.clone()];
        config.google_mirror.enabled = true;
        config.google_mirror.locations = vec![f.google.clone()];
        config
    }

    #[test]
    fn a_folder_out_of_reach_keeps_everything_pending_until_the_next_try() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // Google 用フォルダの場所をリンクがふさいでいる（フォルダごと置けない）
        let elsewhere = f.lib.parent().unwrap().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, &f.google).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(&elsewhere, &f.google).is_err() {
            return;
        }
        let copied = [f.lib.join("d/a.jpg")];
        assert!(place_imported(&copied, &f.lib, &config, &mut db).is_err());
        assert_eq!(db.google_pending_all().unwrap().len(), 1);
        // どかしたら、次の試みで置ける（Windows のフォルダのリンクは `remove_dir` で消す）
        #[cfg(unix)]
        std::fs::remove_file(&f.google).unwrap();
        #[cfg(windows)]
        std::fs::remove_dir(&f.google).unwrap();
        let r = retry_pending(&config, &mut db).unwrap().unwrap();
        assert_eq!(r.placed, 1);
        assert!(db.google_pending_all().unwrap().is_empty());
    }

    #[test]
    fn a_transient_failure_stays_pending_and_the_rest_leave() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // 1枚は読めない（コピーのあとで外された外付け等）＝一時的な失敗
        let copied = [f.lib.join("d/a.jpg"), f.lib.join("d/unreadable.jpg")];
        let r = place_imported(&copied, &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!((r.placed, r.retry.len()), (1, 1));
        assert_eq!(
            db.google_pending_all().unwrap(),
            [(f.lib.clone(), vec![f.lib.join("d/unreadable.jpg")])]
        );
    }

    #[test]
    fn pending_on_an_unplugged_destination_is_kept() {
        let f = fixture();
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // 取り込み先ごと見えない
        let unplugged = f.lib.parent().unwrap().join("unplugged");
        db.google_pending_add(&[unplugged.join("d/a.jpg")], &unplugged)
            .unwrap();
        retry_pending(&config, &mut db).unwrap();
        assert_eq!(db.google_pending_all().unwrap().len(), 1);
    }

    #[test]
    fn an_original_whose_folder_cannot_be_resolved_is_kept_for_retry() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let od = [f.lib.parent().unwrap().join("OneDrive")];
        // 在る祖先が1つも無い綴り（解決できない）は計画から落ち、保留に残す
        let lost = PathBuf::from("no-such-root/d/b.jpg");
        let r = place_now_with(
            &[f.lib.join("d/a.jpg"), lost.clone()],
            &f.lib,
            &config,
            &mut db,
            &od,
        )
        .unwrap();
        assert_eq!(r.placed, 1);
        // 置けたものは入らない（計画に入ったものまで残すと、置けても保留が消えない）
        assert_eq!(r.retry, [lost]);
    }

    #[test]
    fn pending_under_an_unmounted_volume_is_kept_until_the_library_drops_it() {
        let f = fixture();
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // ボリュームを外したあと: マウント先のフォルダは残り、その下に同じ並びまで在る。
        // ライブラリの走査はそのルートを走査できていないので、原本の行は残っている
        let mount = f.lib.parent().unwrap().join("mnt");
        std::fs::create_dir_all(mount.join("2026/2026-10-05")).unwrap();
        let a = mount.join("2026/2026-10-05/a.jpg");
        let b = mount.join("b.jpg");
        let row = |p: &Path| crate::scanner::ScannedFile {
            path: p.to_path_buf(),
            size: 1,
            mtime_ms: 1,
        };
        db.upsert_files(&[row(&a), row(&b)]).unwrap();
        db.google_pending_add(&[a.clone(), b.clone()], &mount)
            .unwrap();
        retry_pending(&config, &mut db).unwrap();
        assert_eq!(db.google_pending_all().unwrap()[0].1.len(), 2);
        // 走査が行を消したら（pictkura も消えたと扱う）、保留からも外す
        db.remove_paths(&[a]).unwrap();
        retry_pending(&config, &mut db).unwrap();
        assert_eq!(db.google_pending_all().unwrap()[0].1, [b]);
    }

    #[test]
    fn a_pending_original_never_indexed_is_not_read_as_deleted() {
        let f = fixture();
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        // コピーの直後・走査の前にボリュームが外れた: 行は一度も出来ていない
        db.google_pending_add(&[f.lib.join("d/a.jpg")], &f.lib)
            .unwrap();
        retry_pending(&config, &mut db).unwrap();
        assert_eq!(db.google_pending_all().unwrap().len(), 1);
    }

    #[test]
    fn only_a_last_copy_is_swept_and_a_moved_original_keeps_its_link() {
        let f = fixture();
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let row = |p: PathBuf| crate::scanner::ScannedFile {
            path: p,
            size: 1,
            mtime_ms: 1,
        };
        let names = ["kept", "moved", "deleted", "unlisted"];
        let paths: Vec<PathBuf> = names
            .iter()
            .map(|n| f.lib.join(format!("d/{n}.jpg")))
            .collect();
        for p in &paths {
            put(p, b"photo");
        }
        db.upsert_files(&paths.iter().cloned().map(row).collect::<Vec<_>>())
            .unwrap();
        place_imported(&paths, &f.lib, &config, &mut db).unwrap();
        // Finder でフォルダの名前を変えた・OS のゴミ箱へ入れた（実体は別の名前で生きている）
        std::fs::create_dir_all(f.lib.join("Trip")).unwrap();
        std::fs::rename(&paths[1], f.lib.join("Trip/moved.jpg")).unwrap();
        // 外で消し切った（リンクが最後の1枚）
        std::fs::remove_file(&paths[2]).unwrap();
        // 3つとも行が消えたが、unlisted はファイルが在る（ライブラリから外しただけ）
        db.remove_paths(&paths[1..]).unwrap();
        let trash = f.lib.parent().unwrap().join("trash");
        let r = sweep_orphans(&mut db, &mut move_into(&trash)).unwrap();
        assert_eq!((r.removed, r.discarded), (0, 1));
        assert!(f.google.join("d/kept.jpg").exists());
        assert!(f.google.join("d/moved.jpg").exists());
        assert!(f.google.join("d/unlisted.jpg").exists());
        assert_eq!(std::fs::read(trash.join("deleted.jpg")).unwrap(), b"photo");
        // 移った先が消し切られたら、次の回に拾う
        std::fs::remove_file(f.lib.join("Trip/moved.jpg")).unwrap();
        let r = sweep_orphans(&mut db, &mut move_into(&trash)).unwrap();
        assert_eq!(r.discarded, 1);
        // 2回目は何もしない
        let again = sweep_orphans(&mut db, &mut no_discard()).unwrap();
        assert_eq!(again, UnplaceReport::default());
        // リンクを手で消してから原本も消した記録は「もう無い」として消え、表に残らない
        std::fs::remove_file(f.google.join("d/kept.jpg")).unwrap();
        std::fs::remove_file(&paths[0]).unwrap();
        db.remove_paths(&paths[..1]).unwrap();
        let r = sweep_orphans(&mut db, &mut no_discard()).unwrap();
        assert_eq!(r.gone, 1);
        let left = db.google_placed_orphans().unwrap();
        assert!(left
            .iter()
            .all(|(_, rec, _, _)| rec.link != f.google.join("d/kept.jpg")));
    }

    #[cfg(unix)]
    #[test]
    fn a_trashed_originals_link_that_could_not_be_removed_is_removed_later() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/a.jpg")], &f.lib, &config, &mut db).unwrap();
        let bin = f.lib.parent().unwrap().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::rename(f.lib.join("d/a.jpg"), bin.join("a.jpg")).unwrap();
        // 外すときだけフォルダが書けない（一時的な失敗）
        let album = f.google.join("d");
        std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o555)).unwrap();
        let r = unplace_sources(&mut db, &[f.lib.join("d/a.jpg")], &mut no_discard()).unwrap();
        std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!((r.removed, r.failed.len()), (0, 1));
        // 原本はゴミ箱の中で名前を持ち続ける（名前の数 2）が、印があるので次の突き合わせが外す
        let r = sweep_orphans(&mut db, &mut no_discard()).unwrap();
        assert_eq!(r.removed, 1);
        assert!(!album.join("a.jpg").exists());
        assert_eq!(std::fs::read(bin.join("a.jpg")).unwrap(), b"photo");
    }

    #[test]
    fn a_moved_out_original_is_no_longer_followed_and_its_link_stays() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/a.jpg")], &f.lib, &config, &mut db).unwrap();
        forget_moved(&mut db, &[f.lib.join("d/a.jpg")]).unwrap();
        // 移した先が別のドライブで、原本が OS のゴミ箱を経て消えても、リンクは触られない
        std::fs::remove_file(f.lib.join("d/a.jpg")).unwrap();
        let r = sweep_orphans(&mut db, &mut no_discard()).unwrap();
        assert_eq!(r, UnplaceReport::default());
        assert!(f.google.join("d/a.jpg").exists());
    }

    #[test]
    fn trashing_in_pictkura_takes_only_the_name_back() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/a.jpg")], &f.lib, &config, &mut db).unwrap();
        // pictkura がゴミ箱へ入れた（実体はゴミ箱の中に残る）
        let bin = f.lib.parent().unwrap().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::rename(f.lib.join("d/a.jpg"), bin.join("a.jpg")).unwrap();
        let r = unplace_sources(&mut db, &[f.lib.join("d/a.jpg")], &mut no_discard()).unwrap();
        assert_eq!((r.removed, r.discarded), (1, 0));
        assert!(!f.google.join("d/a.jpg").exists());
        assert_eq!(std::fs::read(bin.join("a.jpg")).unwrap(), b"photo");
        assert!(db
            .google_placed_for_sources(&[f.lib.join("d/a.jpg")])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_permanent_failure_is_reported_but_not_kept_pending() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.google.join("d/a.jpg"), b"someone else's");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let r = place_imported(&[f.lib.join("d/a.jpg")], &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!((r.failed.len(), r.retry.len()), (1, 0));
        assert!(db.google_pending_all().unwrap().is_empty());
    }

    #[test]
    fn a_run_that_died_midway_is_finished_by_the_next_try() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/gone.jpg"), b"photo");
        let config = on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let row = |p: PathBuf| crate::scanner::ScannedFile {
            path: p,
            size: 1,
            mtime_ms: 1,
        };
        // 2枚ともライブラリに載ったあと、保留を書いたところで落ちた
        db.upsert_files(&[row(f.lib.join("d/a.jpg")), row(f.lib.join("d/gone.jpg"))])
            .unwrap();
        db.google_pending_add(&[f.lib.join("d/a.jpg"), f.lib.join("d/gone.jpg")], &f.lib)
            .unwrap();
        // その間に1枚は消され、走査も行を消した
        std::fs::remove_file(f.lib.join("d/gone.jpg")).unwrap();
        db.remove_paths(&[f.lib.join("d/gone.jpg")]).unwrap();
        let r = retry_pending(&config, &mut db).unwrap().unwrap();
        assert_eq!((r.placed, r.failed.len()), (1, 0));
        assert!(db.google_pending_all().unwrap().is_empty());
        // 切っているあいだは何もしない（保留も触らない）
        db.google_pending_add(&[f.lib.join("d/a.jpg")], &f.lib)
            .unwrap();
        let mut off = config.clone();
        off.google_mirror.enabled = false;
        assert_eq!(retry_pending(&off, &mut db).unwrap(), None);
        assert_eq!(db.google_pending_all().unwrap().len(), 1);
    }

    #[test]
    fn a_record_that_cannot_be_written_is_retried_but_a_taken_name_is_not() {
        let f = fixture();
        put(&f.lib.join("d/a.jpg"), b"photo");
        put(&f.lib.join("d/b.jpg"), b"photo");
        struct Flaky;
        impl Ledger for Flaky {
            fn holder(&mut self, _: &Path) -> io::Result<Option<(PathBuf, u64, bool)>> {
                Ok(None)
            }
            fn forget(&mut self, _: &Path) -> io::Result<()> {
                Ok(())
            }
            fn holders_folded(
                &mut self,
                _: &Path,
            ) -> io::Result<Vec<(PathBuf, PathBuf, u64, bool)>> {
                Ok(Vec::new())
            }
            fn claim(&mut self, p: &Placed) -> io::Result<Claim> {
                if p.link.ends_with("a.jpg") {
                    Err(io::Error::other("database is locked"))
                } else {
                    Ok(Claim::Other)
                }
            }
            fn release(&mut self, _: &Placed) {}
            fn renumber(&mut self, _: &Placed) -> io::Result<()> {
                Ok(())
            }
        }
        let r = place(
            &f.google,
            std::slice::from_ref(&f.lib),
            &[placement(&f.lib, "d/a.jpg"), placement(&f.lib, "d/b.jpg")],
            &mut Flaky,
        )
        .unwrap();
        assert_eq!(r.failed.len(), 2);
        assert_eq!(r.retry, [f.lib.join("d/a.jpg")]);
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

    fn embedded_on(f: &Fixture) -> crate::Config {
        let mut config = on(f);
        config.google_mirror.raw_only = RawOnly::EmbeddedJpeg;
        config
    }

    #[test]
    fn a_raw_only_cut_is_placed_as_its_embedded_jpeg_at_import() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        let r = place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db)
            .unwrap()
            .unwrap();
        assert_eq!((r.placed, r.failed.len()), (1, 0), "{:?}", r.failed);
        assert!(f.google.join("d/B.jpg").is_file());
        assert!(!f.google.join("d/B.ARW").exists());
        assert!(db.google_pending_all().unwrap().is_empty());
        let rows = db
            .google_placed_for_sources(&[f.lib.join("d/B.ARW")])
            .unwrap();
        assert!(rows.len() == 1 && rows[0].1.extracted);
    }

    #[test]
    fn trashing_the_raw_in_pictkura_deletes_its_extract_outright() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db).unwrap();
        let bin = f.lib.parent().unwrap().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::rename(f.lib.join("d/B.ARW"), bin.join("B.ARW")).unwrap();
        // 取り出したものの名前は1つだが、ゴミ箱へは渡さずに消す（RAW から取り出し直せる）
        let r = unplace_sources(&mut db, &[f.lib.join("d/B.ARW")], &mut no_discard()).unwrap();
        // 消した数は「ゴミ箱へ」と数えない
        assert_eq!((r.deleted, r.discarded, r.failed.len()), (1, 0, 0));
        assert!(!f.google.join("d/B.jpg").exists());
        assert!(db
            .google_placed_for_sources(&[f.lib.join("d/B.ARW")])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_raw_gone_outside_pictkura_sends_its_extract_to_the_trash() {
        let f = fixture();
        raw_with_preview(&f.lib.join("d/B.ARW"), 1, Some(&preview_jpeg(1200, 900)));
        let config = embedded_on(&f);
        let mut db = crate::db::Db::open_in_memory().unwrap();
        place_imported(&[f.lib.join("d/B.ARW")], &f.lib, &config, &mut db).unwrap();
        std::fs::remove_file(f.lib.join("d/B.ARW")).unwrap();
        let trash = f.lib.parent().unwrap().join("trash");
        let r = sweep_orphans(&mut db, &mut move_into(&trash)).unwrap();
        assert_eq!((r.discarded, r.failed.len()), (1, 0));
        assert!(trash.join("B.jpg").is_file());
        assert!(!f.google.join("d/B.jpg").exists());
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
        assert!(check_location(&alias.join("pictkura-outgoing"), &roots).is_err());
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
        let p = PathBuf::from(r"\\?\D:\pictkura-outgoing");
        let q = PathBuf::from(r"\\?\UNC\nas\share\pictkura-outgoing");
        if cfg!(windows) {
            assert_eq!(without_verbatim(p), PathBuf::from(r"D:\pictkura-outgoing"));
            assert_eq!(
                without_verbatim(q),
                PathBuf::from(r"\\nas\share\pictkura-outgoing")
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
