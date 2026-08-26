//! サイドカー（`.xmp` 等）と、同じ名前の組（RAW+JPG）。
//!
//! **写真の隣に置かれた小さなファイルは、写真と一緒に動かないと意味を失う**。
//! `.xmp` には現像ソフト（Lightroom・darktable 等）が書いた現像設定・評価・
//! キーワードが入っていて、写真だけ取り込んで置き去りにすると、
//! 利用者から見れば**編集がぜんぶ消えたのと同じ**になる。ファイルが小さいので
//! 「一緒に運ぶ」以外の判断は要らない。
//!
//! ここに置くのは**名前の対応づけだけ**で、コピーも削除もしない。呼ぶ側
//! （取り込み・ゴミ箱）が実際の入出力を持つ。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 既定で一緒に運ぶ拡張子（2026-08-19に調べた）。
///
/// **入っているのは「利用者の作業そのもの」だけ**——置いていくと取り返しが
/// つかないものに絞ってある:
///
/// **名前の流儀は2つあり、ソフトごとに違う**（2026-08-20に各社の資料で確かめた）:
///
/// | 拡張子 | 書くソフト | 名前 |
/// |---|---|---|
/// | `xmp` | Adobe（Camera Raw / Lightroom / Bridge）・Photo Mechanic・digiKam | 置き換え型 `IMG_0001.xmp` |
/// | `xmp` | darktable | 足す型 `IMG_0001.CR3.xmp`（Adobe形式は読むが書かない） |
/// | `aae` | Apple 写真（iPhone / iPad の編集） | 置き換え型 `IMG_0001.AAE` |
/// | `dop` | DxO PhotoLab | 足す型 `IMG_0001.CR3.dop` |
/// | `pp3` | RawTherapee | 足す型 `IMG_0001.CR3.pp3` |
/// | `on1` | ON1 Photo RAW | 置き換え型 `IMG_0001.on1` |
///
/// 設定で足した拡張子にも同じ2つの流儀の判定がそのまま効く（形式で決まる話で、
/// 拡張子ごとの決め打ちはしていない）。
///
/// **入れなかったもの**と、その理由:
///
/// - `thm`（動画のサムネイル）・`lrv`（低解像度のプロキシ動画。GoPro・DJI）:
///   カメラが作り直せる派生物で、`lrv` は**大きい**
/// - `modd` / `moff`: **PlayMemories Home の管理ファイル**。このアプリは
///   そこから離れるための道具なので、運んでも使い道が無い
/// - `wav`（音声メモ。Olympus・Pentax）: 中身は利用者のものだが、
///   `song.jpg` と `song.wav` のような**同名の別物を巻き込む**。要る人は設定で足せる
/// - `cos`（Capture One）: 同階層ではなく `CaptureOne` フォルダの中に置かれるので、
///   同名の規則では拾えない
///
/// **画像の拡張子とは別の並び**にしてある。ここに足したものは一覧には出ず、
/// 写真の影として付いて回るだけ——`config.import.extensions` に足すと、
/// サイドカーが1枚の写真として一覧に並んでしまう。
pub const DEFAULT_SIDECAR_EXTENSIONS: &[&str] = &["xmp", "aae", "dop", "pp3", "on1"];

/// 同じ写真の組（RAW+JPG など）をまとめる鍵。
///
/// **フォルダと、拡張子を除いた名前**。`IMG_0001.CR3` と `IMG_0001.JPG` は
/// 同じ組になり、`dest_dir_for` の日付をそろえるのに使う——組が別々の日の
/// フォルダへ散ると、あとで RAW と JPG を突き合わせられなくなる。
///
/// 大文字小文字は**畳んで**比べる。カメラは `IMG_0001.CR3` と `img_0001.jpg` を
/// 混ぜて書くことがあり、Windowsではどのみち同じ名前として扱われる。
pub fn pair_key(path: &Path) -> (PathBuf, String) {
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    (dir, stem)
}

/// 試す綴り。**大文字小文字を区別しないOSでは1周だけ**。
///
/// Windowsでは `.xmp` と綴っても `IMG_0001.XMP` が開けるので、大文字の周回は
/// 同じファイルをもう一度statして [`std::fs::canonicalize`] で畳み直すだけの
/// 無駄になる——写真1枚あたりの stat が倍になり、クラウド同期フォルダの
/// ように1回が遅い場所では削除の押し始めがそのぶん止まる。
fn cased_candidates(ext: &str) -> Vec<String> {
    let lower = ext.to_lowercase();
    let upper = ext.to_uppercase();
    if cfg!(windows) || upper == lower {
        vec![lower]
    } else {
        vec![lower, upper]
    }
}

/// `path` に付いているサイドカーを、**実在するものだけ**返す。
///
/// 名前の流儀が2つあるので両方見る:
///
/// - `IMG_0001.xmp` … 拡張子を**置き換える**（Adobe・Apple の `.aae`）
/// - `IMG_0001.CR3.xmp` … 拡張子の後ろへ**足す**（darktable・digiKam）
///
/// 大文字小文字の候補も試す。macOS/Linux では `.XMP` を別物として持つ機械がある。
///
/// **同じ1つのファイルを2度返さない**のが要点。Windowsは大文字小文字を区別しないので、
/// `.xmp` と `.XMP` の**どちらの綴りでも同じファイルが実在**として返り、素朴に集めると
/// 同じサイドカーを2回コピーし（連番が付いて `IMG_0001-1.xmp` が生える）、
/// ゴミ箱へも2回送ることになる。実体で見分けるため [`std::fs::canonicalize`] を通す
/// ——OSが返す**本当の綴り**で畳めば、区別する機械でも正しく2つのままになる。
pub fn sidecars_of(path: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    // 写真そのものは候補から外す（`IMG_0001.xmp` を取り込むときの自分自身）
    let itself = std::fs::canonicalize(path).ok();
    for ext in extensions {
        let ext = ext.trim().trim_start_matches('.');
        if ext.is_empty() {
            continue;
        }
        for cased in cased_candidates(ext) {
            // 置き換え型: IMG_0001.CR3 → IMG_0001.xmp
            let replaced = path.with_extension(&cased);
            // 足す型: IMG_0001.CR3 → IMG_0001.CR3.xmp
            let mut appended = path.as_os_str().to_os_string();
            appended.push(".");
            appended.push(&cased);
            let appended = PathBuf::from(appended);
            for candidate in [replaced, appended] {
                if !candidate.is_file() {
                    continue;
                }
                // 実体で畳む。`canonicalize` が通らない環境では**拾わない**
                // （二重に運ぶより、運ばないほうが害が小さい……とは限らないが、
                // ここで綴り違いを見分けられない以上、同じものを2回運ぶ側は選べない）
                let Ok(canon) = std::fs::canonicalize(&candidate) else {
                    continue;
                };
                if Some(&canon) == itself.as_ref() || seen.contains(&canon) {
                    continue;
                }
                // **ディスク上の本当の綴りで返す**。Windowsでは `.aae` と綴っても
                // `IMG_1234.AAE` が開けてしまうので、こちらの候補の綴りをそのまま
                // 使うと**コピー先の名前が勝手に小文字になる**。現像ソフトや
                // 写真アプリは名前で結び付けるので、綴りは触らずに運ぶ
                let real = match (candidate.parent(), canon.file_name()) {
                    (Some(dir), Some(name)) => dir.join(name),
                    _ => candidate,
                };
                seen.push(canon);
                out.push(real);
            }
        }
    }
    out
}

/// そのサイドカーが**足す型**（`IMG_0001.CR3.xmp`）か。
///
/// 足す型は写真のフルネームを丸ごと含むので、**その1枚にしか付かない**。
/// 置き換え型（`IMG_0001.xmp`）は名前の芯しか持たないため、同じ芯の写真が
/// 複数あると**どれのものか決められない**——[`Companions`] がその差を使う。
fn is_appended_form(photo: &Path, sidecar: &Path) -> bool {
    let ext = sidecar
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = sidecar
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let photo_name = photo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.eq_ignore_ascii_case(&format!("{photo_name}.{ext}"))
}

/// 大文字小文字を畳んだパス（Windowsのみ。区別するOSでは綴りのまま）。
fn folded(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

/// **残るほうの相方から `.xmp` を奪わない**ための門（ゴミ箱送り・移動で使う）。
///
/// 置き換え型のサイドカーは**名前でしか写真に結び付いていない**。
/// `IMG_0001.CR3` と `IMG_0001.JPG` が並ぶフォルダの `IMG_0001.xmp` は、
/// 実際には**RAWのもの**であることが多い——Adobe（Camera Raw / Lightroom）は
/// RAWにだけ `.xmp` を書く（**JPEG・TIFFは写真の中に埋め込む**ので書かない。
/// DNGも既定は中。2026-08-20に確認）。
///
/// **逆向きの取りこぼしは承知のうえ**。RAWだけを捨ててJPGを残すときも、
/// 同じ名前の写真が残る以上「どちらのものか」は決められないので `.xmp` は
/// 置いていく——迷子が1つ残るのと、残った写真から現像設定が消えるのとでは、
/// 取り返しのつかなさが違う。
///
/// ここで JPG だけをゴミ箱へ送ると、素朴に集める実装は `IMG_0001.xmp` も
/// 道連れにする。**写真（CR3）は残るので利用者は気付かず**、次に現像ソフトを
/// 開いたときに現像設定・評価・キーワードが消えている——置き去りを防ぐはずの
/// 仕掛けが、逆向きに同じ事故を起こす。
///
/// そこで、**同じ名前の写真がフォルダに残るなら置き換え型は連れていかない**。
/// 足す型（`IMG_0001.CR3.xmp`）はその1枚に固有なので、いつでも連れていく。
/// 組の全員を一緒に消す／動かすときは `leaving` に入っているので、
/// そのときは連れていく（取り残しにならない）。
///
/// フォルダの中身は**1回だけ読んで覚える**。500枚の削除で500回読み直すと、
/// クラウド同期フォルダでは押した直後の待ちがそのぶん伸びる。
pub struct Companions {
    /// この操作で居なくなる写真（畳んだフルパス）
    leaving: HashSet<String>,
    /// フォルダ → その中のファイル名（読めなかったフォルダは空）
    dirs: HashMap<PathBuf, Vec<String>>,
}

impl Companions {
    /// `leaving` はこの操作で**一緒に居なくなる**ファイル（写真自身も含めてよい）。
    pub fn new(leaving: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            leaving: leaving.into_iter().map(|p| folded(&p)).collect(),
            dirs: HashMap::new(),
        }
    }

    /// 連れていってよいサイドカーだけを返す（[`sidecars_of`] の絞り込み版）。
    pub fn sidecars_of(&mut self, photo: &Path, extensions: &[String]) -> Vec<PathBuf> {
        let found = sidecars_of(photo, extensions);
        if found.is_empty() {
            return found;
        }
        // フォルダを読むのは**置き換え型が実際にあったときだけ**（遅延）
        let mut stays: Option<bool> = None;
        let mut out = Vec::with_capacity(found.len());
        for sidecar in found {
            if !is_appended_form(photo, &sidecar) {
                if stays.is_none() {
                    stays = Some(self.other_photo_stays(photo, extensions));
                }
                if stays == Some(true) {
                    continue;
                }
            }
            out.push(sidecar);
        }
        out
    }

    /// 同じ名前（拡張子違い）の別のファイルが、この操作のあとも**残る**か。
    fn other_photo_stays(&mut self, photo: &Path, extensions: &[String]) -> bool {
        let (dir, stem) = pair_key(photo);
        // `leaving` と `dirs` を別々に借りる（片方の借用でもう片方が使えなくなる）
        let Self { leaving, dirs } = self;
        let names = dirs.entry(dir.clone()).or_insert_with(|| {
            std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default()
        });
        let myself = photo
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        for name in names.iter() {
            if name.to_lowercase() == myself {
                continue;
            }
            let as_path = Path::new(name);
            if as_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default()
                != stem
            {
                continue;
            }
            // **サイドカー同士は数えない**（`IMG_0001.xmp` の隣の `IMG_0001.dop`）。
            // 影が影を守り合うと、組の全員を消しても `.xmp` だけが残る
            if let Some(ext) = as_path.extension() {
                let ext = ext.to_string_lossy();
                if extensions
                    .iter()
                    .any(|e| e.trim().trim_start_matches('.').eq_ignore_ascii_case(&ext))
                {
                    continue;
                }
            }
            let full = dir.join(name);
            // 一緒に居なくなるものは「残る」に数えない
            if leaving.contains(&folded(&full)) {
                continue;
            }
            if full.is_file() {
                return true;
            }
        }
        false
    }
}

/// コピー先での、そのサイドカーの名前を決める。
///
/// **写真の付いた先の名前に合わせる**のが要点。取り込みは同名衝突を
/// `IMG_0001-1.CR3` のように連番で避けるので、サイドカーだけ元の名前で置くと
/// **別の写真の設定として読まれる**（現像ソフトは名前で結びつける）。
///
/// - 置き換え型（`IMG_0001.xmp`）→ 付いた先の名前の拡張子を `.xmp` にしたもの
/// - 足す型（`IMG_0001.CR3.xmp`）→ 付いた先の名前に `.xmp` を足したもの
pub fn sidecar_dest_name(source_photo: &Path, sidecar: &Path, dest_photo_name: &str) -> String {
    let ext = sidecar
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    if is_appended_form(source_photo, sidecar) {
        // 足す型
        format!("{dest_photo_name}.{ext}")
    } else {
        // 置き換え型
        let stem = Path::new(dest_photo_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| dest_photo_name.to_string());
        format!("{stem}.{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テストは既定の並びで引く（設定から来るのと同じ形）
    fn exts() -> Vec<String> {
        DEFAULT_SIDECAR_EXTENSIONS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn the_pair_key_drops_the_extension_and_folds_case() {
        let a = pair_key(Path::new("D:/photo/IMG_0001.CR3"));
        let b = pair_key(Path::new("D:/photo/img_0001.jpg"));
        assert_eq!(a, b, "RAWとJPGは同じ組");

        let other_dir = pair_key(Path::new("D:/別/IMG_0001.JPG"));
        assert_ne!(a, other_dir, "フォルダが違えば別の組");

        let other_name = pair_key(Path::new("D:/photo/IMG_0002.JPG"));
        assert_ne!(a, other_name);
    }

    #[test]
    fn both_naming_styles_are_picked_up_but_only_where_the_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_0001.CR3");
        std::fs::write(&photo, b"raw").unwrap();
        // まだサイドカーが無い
        assert!(sidecars_of(&photo, &exts()).is_empty());

        // 置き換え型
        let replaced = dir.path().join("IMG_0001.xmp");
        std::fs::write(&replaced, b"<x/>").unwrap();
        assert_eq!(sidecars_of(&photo, &exts()), vec![replaced.clone()]);

        // 足す型も同時に置く
        let appended = dir.path().join("IMG_0001.CR3.xmp");
        std::fs::write(&appended, b"<x/>").unwrap();
        let found = sidecars_of(&photo, &exts());
        assert_eq!(found.len(), 2, "両方の流儀を拾う: {found:?}");
        assert!(found.contains(&replaced) && found.contains(&appended));
    }

    #[test]
    fn the_iphone_aae_is_picked_up_too() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_1234.HEIC");
        std::fs::write(&photo, b"heic").unwrap();
        let aae = dir.path().join("IMG_1234.AAE");
        std::fs::write(&aae, b"plist").unwrap();
        assert_eq!(sidecars_of(&photo, &exts()), vec![aae]);
    }

    /// `.xmp` 自身を取り込むときに、自分を自分のサイドカーとして拾わないこと。
    #[test]
    fn the_file_itself_is_not_picked_up() {
        let dir = tempfile::tempdir().unwrap();
        let x = dir.path().join("IMG_0001.xmp");
        std::fs::write(&x, b"<x/>").unwrap();
        assert!(sidecars_of(&x, &exts()).is_empty());
    }

    /// **P1（ゲート1）**: RAW+JPGのうち片方だけ消すと、素朴な実装は
    /// 共有の `IMG_0001.xmp` を道連れにする。写真は残るので気付けない。
    #[test]
    fn a_replacing_sidecar_stays_when_its_partner_stays() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("IMG_0001.CR3");
        let jpg = dir.path().join("IMG_0001.JPG");
        let xmp = dir.path().join("IMG_0001.xmp");
        for p in [&raw, &jpg, &xmp] {
            std::fs::write(p, b"x").unwrap();
        }

        // JPGだけをゴミ箱へ: `.xmp` は**RAWのもの**なので置いていく
        let mut only_jpg = Companions::new([jpg.clone()]);
        assert!(
            only_jpg.sidecars_of(&jpg, &exts()).is_empty(),
            "残るRAWから現像設定を奪ってはいけない"
        );

        // 組の全員を消すなら連れていく（取り残しにしない）
        let mut both = Companions::new([raw.clone(), jpg.clone()]);
        assert_eq!(both.sidecars_of(&jpg, &exts()), vec![xmp.clone()]);
        // 2枚目からも同じ答え（フォルダを読み直さない経路も同じ）
        assert_eq!(both.sidecars_of(&raw, &exts()), vec![xmp.clone()]);

        // 相方が居なければ、1枚だけでも連れていく
        std::fs::remove_file(&jpg).unwrap();
        let mut alone = Companions::new([raw.clone()]);
        assert_eq!(alone.sidecars_of(&raw, &exts()), vec![xmp]);
    }

    /// 足す型（`IMG_0001.CR3.xmp`）は**その1枚にしか付かない**ので、
    /// 相方が残っていても連れていく。
    #[test]
    fn an_adding_sidecar_travels_even_when_its_partner_stays() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("IMG_0001.CR3");
        let jpg = dir.path().join("IMG_0001.JPG");
        let appended = dir.path().join("IMG_0001.CR3.xmp");
        for p in [&raw, &jpg, &appended] {
            std::fs::write(p, b"x").unwrap();
        }
        let mut only_raw = Companions::new([raw.clone()]);
        assert_eq!(only_raw.sidecars_of(&raw, &exts()), vec![appended]);
    }

    /// サイドカー同士は「残る相方」に数えない——数えると、組を全部消しても
    /// `.xmp` と `.dop` がお互いを盾にして両方残る。
    #[test]
    fn sidecars_do_not_count_as_each_other_s_partner() {
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("IMG_0001.CR3");
        let xmp = dir.path().join("IMG_0001.xmp");
        let dop = dir.path().join("IMG_0001.dop");
        for p in [&raw, &xmp, &dop] {
            std::fs::write(p, b"x").unwrap();
        }
        let mut only_raw = Companions::new([raw.clone()]);
        let found = only_raw.sidecars_of(&raw, &exts());
        assert_eq!(found.len(), 2, "両方連れていく: {found:?}");
    }

    #[test]
    fn the_copied_name_follows_where_the_photo_landed() {
        let photo = Path::new("E:/DCIM/IMG_0001.CR3");
        // 置き換え型: 連番が付いたら、サイドカーも同じ連番になる
        let replaced = Path::new("E:/DCIM/IMG_0001.xmp");
        assert_eq!(
            sidecar_dest_name(photo, replaced, "IMG_0001-1.CR3"),
            "IMG_0001-1.xmp"
        );
        // 足す型: 付いた先の名前まるごとの後ろに足す
        let appended = Path::new("E:/DCIM/IMG_0001.CR3.xmp");
        assert_eq!(
            sidecar_dest_name(photo, appended, "IMG_0001-1.CR3"),
            "IMG_0001-1.CR3.xmp"
        );
        // 衝突が無ければ名前は変わらない
        assert_eq!(
            sidecar_dest_name(photo, replaced, "IMG_0001.CR3"),
            "IMG_0001.xmp"
        );
    }
}
