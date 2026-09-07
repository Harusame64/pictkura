//! USB等からの取り込み（コピー）処理。
//!
//! - 取り込み元をスキャンし、`[routing]` の設定（コピー先ルート＋日付フォルダパターン）に
//!   従ってコピーする
//! - 日付はEXIF撮影日時を優先、なければファイルのmtime
//! - 同名・同サイズのファイルが既にあればスキップ（再取り込みの重複防止）
//! - 同名・別サイズなら `名前-1.jpg` 形式で衝突回避
//! - コピー後にサイズ比較で検証する（`verify_after_copy`）

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, TimeZone};

use crate::config::Config;
use crate::scanner;
use crate::thumbs::read_exif_meta;

pub use scanner::is_managed_package_path;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("コピー先フォルダが設定されていません")]
    NoDestination,
    #[error("取り込み元フォルダが読めません: {0}")]
    SourceUnreadable(PathBuf),
    /// 写真.appのライブラリのような、アプリが管理するパッケージの中を
    /// 取り込み元に指定した。中身はUUID名の内部ファイルなので、
    /// 取り込むと派生画像を数千枚コピーすることになる
    #[error(
        "アプリが管理するライブラリの中は取り込めません（中身は内部ファイルです）。\
         中の写真を取り出したいときは、フォルダ名から拡張子（.photoslibrary など）を\
         外してから選び直してください: {0}"
    )]
    SourceIsManagedPackage(PathBuf),
}

/// 取り込み結果の件数サマリ。
#[derive(Debug, Default, PartialEq)]
pub struct ImportStats {
    /// コピーしたファイル数
    pub copied: usize,
    /// 既に存在していてスキップした数
    pub skipped: usize,
    /// コピーまたは検証に失敗した数
    pub failed: usize,
    /// 取り込み元の走査中にエラーがあった（**取りこぼしの可能性あり**）。
    /// trueの場合、UIは「すべて取り込めた」と表示してはならない
    pub scan_incomplete: bool,
}

/// 撮影日（またはmtime）の年月日。フォルダパターンの置換に使う。
#[derive(Debug, Clone, Copy)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// エポックミリ秒からローカルタイムゾーンの年月日を取り出す。
/// EXIF撮影日時（ローカル壁時計としてエポック化）とmtimeの両方に使う。
fn date_from_local_ms(ms: i64) -> Option<CivilDate> {
    let dt = Local.timestamp_millis_opt(ms).single()?;
    Some(CivilDate {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
    })
}

/// 今日（取り込み実行日）のローカル年月日。日付が全く取れないファイルのフォールバック。
fn today_local() -> CivilDate {
    let now = Local::now();
    CivilDate {
        year: now.year(),
        month: now.month(),
        day: now.day(),
    }
}

/// コピー先のフォルダ構成の候補。
///
/// Lightroom Classic が「日付で整理」に用意しているのと同じ発想で、
/// 自由記述に頼らず**選ぶだけ**で決められるようにする。既定（先頭）の
/// `{year}/{year}-{month}-{day}` は、年で束ねつつ日フォルダが名前順＝時系列に
/// 並ぶため、写真管理で最も広く薦められている形。
pub const FOLDER_PATTERN_PRESETS: &[&str] = &[
    "{year}/{year}-{month}-{day}",         // 2026/2026-08-12
    "{year}/{month}/{day}",                // 2026/08/12
    "{year}/{month}-{day}",                // 2026/08-12
    "{year}-{month}/{year}-{month}-{day}", // 2026-08/2026-08-12
    "{year}-{month}-{day}",                // 2026-08-12（年の階層は作らない）
    "{month}/{year}-{month}-{day}",        // 08/2026-08-12
    "{year}/{month}",                      // 2026/08（月単位）
    "{year}",                              // 2026（年単位）
    "",                                    // 振り分けない（コピー先直下）
];

/// フォルダパターンの `{year}` `{month}` `{day}` を置換する（月日はゼロ埋め2桁）。
///
/// 置換後は**必ず** [`sanitize_relative`] を通す。パターンは設定ファイルを
/// 直接編集して壊せてしまうため、コピー先の外へ書き出さないことをここで担保する。
pub fn render_folder_pattern(pattern: &str, date: CivilDate) -> String {
    let rendered = pattern
        .replace("{year}", &format!("{:04}", date.year))
        .replace("{month}", &format!("{:02}", date.month))
        .replace("{day}", &format!("{:02}", date.day));
    sanitize_relative(&rendered)
}

/// パターンから作った相対パスを安全な形へ正規化する。
///
/// - `..` と絶対パス（`/foo` `C:\foo`）を落として**コピー先の外へ出さない**
/// - 各階層からファイル名に使えない文字（`:*?"<>|`）を除く
/// - 空になった階層は詰める
///
/// 危険な入力は「エラーにする」のではなく「無害化する」。取り込みの最中に
/// 設定不備で止まるより、コピー先直下へでも確実に取り込めた方が写真は失われない。
fn sanitize_relative(rendered: &str) -> String {
    /// `C:` のようなWindowsのドライブ指定か（そのまま残すと "C" フォルダになる）。
    fn is_drive_spec(part: &str) -> bool {
        let mut chars = part.chars();
        matches!((chars.next(), chars.next(), chars.next()),
            (Some(c), Some(':'), None) if c.is_ascii_alphabetic())
    }
    rendered
        .split(['/', '\\'])
        .map(|part| part.trim())
        // "." と ".." は階層移動、"C:" はドライブ指定。いずれも相対パスには残さない
        .filter(|part| !part.is_empty() && *part != "." && *part != ".." && !is_drive_spec(part))
        .map(|part| {
            part.chars()
                .filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                .collect::<String>()
        })
        // 末尾のドット・空白はWindowsで開けないフォルダ名になるため落とす
        .map(|part| part.trim_end_matches(['.', ' ']).to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// **この操作でもう使った行き先**の覚え書き。
///
/// **畳み方は自分で決めず、ファイルシステムに訊く。** 「同じファイルを指すか」の規則は
/// OS とファイルシステムが持っていて、こちらで当てにいくと必ずどこかで外れる——
/// macOS（APFS）も Windows も**大文字小文字を畳む**うえ、**APFS は Unicode の正規化
/// （NFC と NFD）も畳む**。`Path` を素で持つと、`DSC00001.ARW` を書いたあとの
/// `dsc00001.arw` が**覚え書きに当たらない**——当たらないと、そのあと
/// 「同じ大きさ・同じ時刻だから取り込み済み」と読まれて、**2枚目が黙って消える**
/// （2026-09-06・ゲート1とゲート2）。カードには大小が混ざるし、
/// 取り込み元はカードとは限らない。
///
/// `canonicalize` は**OSが見ている姿**を返すので、畳むかどうかもそちらの規則に従う
/// ——case-sensitive な Linux では畳まれず、そこでも正しい。
///
/// **OneDrive のクラウドのみファイルは実体化しない**（2026-09-06・win の実機で実測。
/// 実物のクラウドのみ3本を `canonicalize` と `metadata` で触って、属性 `0x401620` が
/// 前後で変わらず、10秒後も `0x400000` が立ったまま。`Get-Item` の属性だけで見ている
/// ——`fsutil` は道具自体がハイドレートする）。**理屈ではなく測った値。**
/// ただし**1回ずつなので幅は取っていない。**
/// **`canonicalize` が通らなかったときのために、小文字に畳んだ綴りも一緒に持つ**
/// ——素の `Path` を控えても、**綴りが違えば当たらないので控えにならない**。
/// 畳んだほうは正規化までは面倒を見ないが、**何も無いよりはよい**
/// （当たらなかった側の害は、写真が消えることである）。
#[derive(Default, Debug)]
pub(crate) struct TakenPaths {
    /// OS が見ている姿（`canonicalize` が通ったもの）。**通っている限り、これが答え。**
    real: HashSet<PathBuf>,
    /// 通らなかった行き先の、**小文字に畳んだ綴り**。素の `Path` を控えても
    /// **綴りが違えば当たらないので控えにならない**ので、畳んで持つ
    folded: HashSet<String>,
    /// **`canonicalize` が通らなかった行き先が1つでもあるか。**
    ///
    /// 立っていないときは `folded` を**見ない**——見ると、畳まないファイルシステム
    /// （Linux）で `DSC00001.ARW` と `dsc00001.arw` を**同じ行き先と読んで、
    /// 空いている名前があるのに連番を付ける**。
    /// 立っているときだけ畳んだほうへ落ちる——**要らない連番1つ**と
    /// **写真が1枚消えること**なら、前者を取る。
    degraded: bool,
}

impl TakenPaths {
    fn fold(path: &Path) -> String {
        path.to_string_lossy().to_lowercase()
    }

    pub(crate) fn insert(&mut self, path: &Path) {
        match std::fs::canonicalize(path) {
            Ok(real) => {
                self.real.insert(real);
            }
            Err(_) => self.degraded = true,
        }
        // **畳んだ綴りは、通ったときも控える。** 入れるときに通って、**引くときに
        // 通らない**ことがある（書いた直後の RAW を、ウイルス対策や OneDrive が
        // 掴んでいる）。そのとき `Err` の枝が空の控えを引いて偽を返し、
        // **自分がいま書いたファイルを「取り込み済み」と読む**——直したはずの穴に戻る
        self.folded.insert(Self::fold(path));
    }

    pub(crate) fn contains(&self, path: &Path) -> bool {
        // **空なら何も測らない。** `resolve_dest_path` は常に空を渡すので、
        // ウィザードの「済」バッジ（1行ごと）とサイドカー（1本ごと）が
        // **結果の出ようがない `canonicalize` を1回ずつ払う**ことになる
        if self.real.is_empty() && self.folded.is_empty() {
            return false;
        }
        // **在らないパスは、書いた覚えがあるはずがない**（入れるのはコピーに成功した
        // ときだけ）。ここで先に切ると、衝突していない大多数で `canonicalize` を呼ばない。
        // 畳むファイルシステムでは、綴りが違っても `exists()` は当たる
        if !path.exists() {
            return false;
        }
        // **クラウドにしか実体が無いものは開かない。**
        // `canonicalize` は**ハンドルを開く**ので、`RECALL_ON_OPEN`（`0x40000`。
        // `cloud.rs` が見ている2つのうちの片方）が立っていると**開いた時点で取り寄せが走る**。
        // 2026-09-06 に win の実機で「実体化しない」と測ったのは
        // **`RECALL_ON_DATA_ACCESS`（`0x400000`）のほうだけ**で、
        // **開くのが引き金になる側は測っていない**。**測っていないものを前提にしない。**
        //
        // 取り込み先が Files On-Demand の中にあると、**古い写真は寝ている**。
        // ここで畳んだ綴りへ落とすと、正規化の違いまでは見分けられなくなるが、
        // **寝ている写真を起こすよりはよい**（起こすと、利用者の回線と容量を黙って使う）。
        if crate::cloud::is_cloud_only_path(path) {
            return self.folded.contains(&Self::fold(path));
        }
        match std::fs::canonicalize(path) {
            Ok(real) => {
                self.real.contains(&real)
                    || (self.degraded && self.folded.contains(&Self::fold(path)))
            }
            Err(_) => self.folded.contains(&Self::fold(path)),
        }
    }
}

/// コピー先パスの決定結果。
pub(crate) enum DestResolution {
    /// このパスへコピーする
    CopyTo(PathBuf),
    /// 同名・同サイズのファイルが既にある（取り込み済み）
    AlreadyImported,
    /// 連番の衝突回避が尽きた（コピーできない＝失敗として扱う）
    Exhausted,
}

/// **行き先に在るものを「同じ写真」と読んでよいか、どこまで確かめて決めるか。**
///
/// 既定は [`Confirm::ByLooks`]——名前・大きさ・更新時刻まで見て、**中身は読まない**。
/// **カードを挿し直すたびに全ファイルを読む**のは、このリポジトリが払わないと
/// 決めている費用である（`looks_same` に書いてある）。
///
/// [`Confirm::ByBytes`] は、**その費用を、払う価値のある所だけで払う**。
/// **同じ名前がこの取り込みの中に2つ以上あるとき**だけ渡す。そこは
/// 「行き先に在る1本が、2つのうちどちらの写真なのか」を名前と大きさでは決められない所で、
/// 決めずに畳むと**2枚目が黙って消える**。**普通のカードでは名前が重ならないので、
/// 挿し直しでここへは来ない。**
#[derive(Clone, Copy)]
pub(crate) enum Confirm<'a> {
    /// 名前・大きさ・更新時刻まで。**中身は読まない。**
    ByLooks,
    /// **中身まで読んで確かめる。**
    ///
    /// **読めなかったときは [`Confirm::ByLooks`] と同じ答えに落ちる**——
    /// 読めないことを「別物」に倒すと、**行き先がクラウドにしか実体を持たない
    /// ライブラリ**（OneDrive の Files On-Demand）では、**挿し直すたびに
    /// カード1枚ぶんが2枚に増える**。**取りこぼしより、増えるほうが害が大きい。**
    ByBytes(&'a Path),
}

/// コピー先のフルパスを決める。同名・別内容の場合は `-1`, `-2` … で衝突回避。
pub(crate) fn resolve_dest_path(
    dest_dir: &Path,
    file_name: &str,
    src_size: u64,
    src_mtime_ms: i64,
) -> DestResolution {
    resolve_dest_path_avoiding(
        dest_dir,
        file_name,
        src_size,
        src_mtime_ms,
        &TakenPaths::default(),
        Confirm::ByLooks,
    )
}

/// 「同じもの」と見なしてよいか。**サイズと更新時刻だけで決める**
/// （このリポジトリの差分検知の原則。ハッシュは取らない——衝突のたびに
/// 両方を丸ごと読み直すことになり、USBへの再書き出しで全ファイルに効いてしまう）。
///
/// 更新時刻は**2秒の幅**を持たせる。FAT32 は2秒刻みでしか保持できないので、
/// USBメモリへ書き出したものを比べると、厳密一致では必ず別物になる。
///
/// **ちょうど1時間のずれも同じものとみなす**。FAT32 は更新時刻をローカル時刻で
/// 持つため、夏時間の切り替えをまたぐと同じファイルが1時間ずれて見える
/// （robocopy の `/DST` と同じ話。日本では起きないが、英語でも配っている）。
/// これを見落とすと、**カード1枚が丸ごと二重に取り込まれる**。
///
/// **ここには、この判定だけでは閉じられない穴がある。** 「名前も大きさも更新時刻も同じで、
/// 中身が別」を**この判定では見分けられない**——見分けるには両方を読むしかなく、
/// それは**カードを挿し直すたびに全ファイルを読む**ということなので、ここでは取らない。
/// 同じ周の中で来た場合は、書いた行き先を覚えているので消えない
/// （[`resolve_dest_path_avoiding`] の `taken`）。
///
/// **周をまたぐぶんは、呼ぶ側が名前のぶつかりを知っているときだけ塞げる**
/// （[`Confirm::ByBytes`]）。**同じ名前がその取り込みの中に2つ以上ある**なら、
/// 行き先に在る1本はどちらか一方のもので、**もう一方はまだ入っていない**。
/// 読む相手は**ぶつかった名前だけ**なので、名前が重ならない普通のカードでは1件も読まない。
///
/// **それでも残るのは2つ**——**2枚が別々のカードから来る場合**（`100MSDCF/DSC00001.ARW`
/// を入れたあと、**別のカードの** `DSC00001.ARW` が同じ日のフォルダへ来て、撮影が2秒以内）と、
/// **行き先が読めない場合**（クラウドにしか実体が無いライブラリ）。
/// **どちらも2枚目は取り込まれない。**
/// **誤って同じとみなす害より、取りこぼしを疑わせる害のほうが小さい**という判断で、
/// ちょうど1時間ずれ（夏時間）も同じ扱いにしてある。
fn looks_same(existing: &Path, src_size: u64, src_mtime_ms: i64) -> bool {
    let Ok(meta) = existing.metadata() else {
        return false;
    };
    let Ok(mtime) = meta.modified() else {
        return false;
    };
    let dest_ms = filetime::FileTime::from_system_time(mtime).unix_seconds() * 1000;
    looks_alike((meta.len(), dest_ms), (src_size, src_mtime_ms))
}

/// 「大きさと更新時刻」の2つ組が、[`looks_same`] の目で同じに見えるか。
///
/// **[`looks_same`] と [`contested_names`] は同じ目でなければならない。**
/// 数える側が厳しいと、**見分けられない2枚を「ぶつかっていない」と数える**
/// ——そこは中身を読まない枝なので、**2枚目が黙って消える**。だから規則はここ1つ。
///
/// ちょうど1時間ずれ（夏時間）を同じ扱いにしているのは [`looks_same`] の説明のとおり。
fn looks_alike(a: (u64, i64), b: (u64, i64)) -> bool {
    if a.0 != b.0 {
        return false;
    }
    let diff = (a.1 - b.1).abs();
    diff <= MTIME_TOLERANCE_MS || (diff - 3_600_000).abs() <= MTIME_TOLERANCE_MS
}

/// `IMG_0001.CR3` の `i` 番目の別名（`IMG_0001-1.CR3`）。
///
/// **連番の付け方は1箇所に置く。** 行き先を決める側と、
/// 「もう書いたか」を探す側で**別々に組み立てると、ずれたときに気づけない。**
fn numbered_name(file_name: &str, i: usize) -> String {
    let path = Path::new(file_name);
    match (
        path.file_stem().and_then(|s| s.to_str()),
        path.extension().and_then(|e| e.to_str()),
    ) {
        (Some(stem), Some(ext)) => format!("{stem}-{i}.{ext}"),
        _ => format!("{file_name}-{i}"),
    }
}

/// **この周で自分が書いた行き先の中に、中身まで同じものがあるか。**
///
/// 素の名前だけでは足りない——**素の名前が別のファイルで埋まっていると、
/// 1枚目自体が `-1` に付く**ので、2枚目は素の名前では当たらず `-2` へ回る。
/// **行き先を決めるのと同じ順で辿る**（素 → `-1` → `-2` …）。
fn already_written_same_photo(
    dest_dir: &Path,
    file_name: &str,
    src: &Path,
    src_size: u64,
    src_mtime_ms: i64,
    written: &TakenPaths,
) -> bool {
    // **範囲は行き先を決める側と同じにする**（素の名前と `-1`..`-999`）。
    // ずれていると、`-999` に書いたものが探せず、同じ写真が増える
    for i in 0..1000 {
        let candidate = if i == 0 {
            dest_dir.join(file_name)
        } else {
            dest_dir.join(numbered_name(file_name, i))
        };
        if written.contains(&candidate)
            && looks_same(&candidate, src_size, src_mtime_ms)
            && same_bytes(src, &candidate)
        {
            return true;
        }
        // **行き先を決める側が止まる所で、こちらも止まる**
        // （空いている名前より先には、書いたものがあるはずがない）。
        //
        // 後ろの条件はいまは**必ず真**——`TakenPaths::contains` は在らないパスに
        // `false` を返すので、`!exists()` なら `!contains()` でもある。
        // **残してあるのは、その結び付きが変わったときに黙って壊れないため**
        // （「名前だけ押さえて、まだ書いていない」を持てるようにしたら、
        // ここは在るかどうかだけでは止まれなくなる）
        if !candidate.exists() && !written.contains(&candidate) {
            return false;
        }
    }
    false
}

/// 途中まで書けたコピーを片付ける。**自分が作った物だけ消す。**
///
/// **素通しで消してはいけない。** 行き先が**壊れたシンボリックリンク**だと、
/// `Path::exists()` は**リンクを辿って**「空いている」と読む（辿った先が無いので）。
/// そのあと `fs::copy` が転ぶと、ここで消えるのは**こちらが作った物ではなく、
/// 元から在ったリンクそのもの**になる。
///
/// `symlink_metadata` は**辿らない**ので、これで普通のファイルかどうかを見る。
/// 別の処理が割り込んで同じ名前を作った場合までは防げない——防ぐには
/// 一時名で作って付け替える形にする必要があり、それはこの直しの範囲ではない。
fn remove_partial_copy(path: &Path) {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        let _ = std::fs::remove_file(path);
    }
}

/// 中身を突き合わせた答え。**「読めなかった」を「別物」と混ぜない。**
///
/// 混ぜると、**どちらへ倒すかを呼ぶ側が選べなくなる**。
/// 「読めない＝別物」でよいのは**連番が1つ増えるだけ**の所
/// （[`already_written_same_photo`]）で、[`Confirm::ByBytes`] は逆である
/// ——**行き先が読めないというだけで、カード1枚ぶんが2枚に増えてよい所は無い。**
///
/// **読めなかった側で分ける。** 倒す先が逆だからである——**行き先**が読めないなら
/// 「同じ」へ倒してよい（**倒さないと、挿し直すたびにライブラリが2倍になる**）が、
/// **取り込み元**が読めないなら倒してはいけない。**倒すと「取り込み済み」と報告する**
/// ことになり、**カードを抜かれた・I/O が転んだだけの1枚が、入ったことになる。**
#[derive(Debug, PartialEq, Eq)]
enum Bytes {
    /// 最後まで一致した
    Same,
    /// 違うところがあった（長さの違いを含む）
    Differ,
    /// **読まなかった、または行き先が読めなかった。**
    /// クラウドにしか実体が無い・開けない・途中で失敗した
    Unknown,
    /// **取り込み元のほうが読めなかった。** 抜かれた・一時的な I/O の失敗・
    /// クラウドにしか実体が無くて取り寄せに失敗した。
    SourceUnreadable,
}

/// 2つのファイルの中身が同じか。**呼ぶのは名前がぶつかったときだけ。**
///
/// **クラウドにしか実体が無いなら読まない——ただし、それは行き先の側だけ**
/// （[`Bytes::Unknown`]）。読むと**静かに取り寄せが走る**ので、同じ写真だと分かって
/// 飛ばすためだけに数GBを落とすのは割に合わない（`export.rs` と `thumbs.rs` も同じ線）。
///
/// **取り込み元は免除しない**（2026-09-07・ゲート1の P1）。ウィザードは
/// クラウドにしか実体が無い行も選ばせるので、**免除すると「読めなかった」→「同じ」と
/// 倒れて、別の写真が黙って入らない**。そして**取り寄せは、どのみち走る**
/// ——飛ばさない限り `fs::copy` が実体化させるので、**読まずに済むのは飛ばすときだけ**で、
/// **その「飛ばしてよいか」を決めるために読んでいる**。払うのは
/// **ぶつかっている名前で、行き先に見た目の合う物があるとき**だけである。
///
/// **長さを先に見る。** 片方がもう片方の頭だけ、というときに
/// 「同じ」と読まないため——`looks_same` の大きさは**走査した時点の値**なので、
/// 走査から複写までの間に元が短くなっていると、そこだけでは守れない。
///
/// 丸ごとメモリへ載せない（RAWは1枚で数百MBになる）。
///
/// **費用の目安**: ぶつかった1組につき、両方を1回ずつ読む。ふつうのカードでは
/// ほとんど呼ばれないが、**カードの中に自分の控えが丸ごとある**ときは**全ファイルが
/// ぶつかる**ので、**カード1枚ぶんを読み直す**ことになる。
/// **それでも、同じ写真が2枚に増えるよりはよい。**
fn compare_bytes(a: &Path, b: &Path) -> Bytes {
    // **免除されるのは行き先だけ。** 取り込み元（`a`）はここで見ない
    if crate::cloud::is_cloud_only_path(b) {
        return Bytes::Unknown;
    }
    // **転んだ側で分ける**（[`Bytes::SourceUnreadable`]）。取り込み元の失敗を
    // 「読めなかった」に混ぜると、呼ぶ側が「同じ」へ倒して**入っていない1枚を
    // 「取り込み済み」と報告する**
    let (ma, mb) = match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => (ma, mb),
        (Err(_), _) => return Bytes::SourceUnreadable,
        _ => return Bytes::Unknown,
    };
    if ma.len() != mb.len() {
        return Bytes::Differ;
    }

    use std::io::Read;
    let (fa, fb) = match (std::fs::File::open(a), std::fs::File::open(b)) {
        (Ok(fa), Ok(fb)) => (fa, fb),
        (Err(_), _) => return Bytes::SourceUnreadable,
        _ => return Bytes::Unknown,
    };
    let mut ra = std::io::BufReader::new(fa);
    let mut rb = std::io::BufReader::new(fb);
    let mut ba = [0u8; 64 * 1024];
    let mut bb = [0u8; 64 * 1024];
    loop {
        // `ra` は取り込み元。**途中で転んだのも「元が読めない」である**
        let Ok(na) = ra.read(&mut ba) else {
            return Bytes::SourceUnreadable;
        };
        if na == 0 {
            // **こちらが尽きただけでは足りない。** 相手も尽きていることまで見る
            return match rb.read(&mut bb) {
                Ok(0) => Bytes::Same,
                Ok(_) => Bytes::Differ,
                Err(_) => Bytes::Unknown,
            };
        }
        // **同じ長さだけ読ませる。** 短く返ってきた側に合わせないと、
        // 位置がずれて同じ中身を「違う」と読む
        let mut nb = 0;
        while nb < na {
            match rb.read(&mut bb[nb..na]) {
                Ok(0) => break,
                Ok(n) => nb += n,
                Err(_) => return Bytes::Unknown,
            }
        }
        if na != nb || ba[..na] != bb[..nb] {
            return Bytes::Differ;
        }
    }
}

/// [`compare_bytes`] を「同じと言い切れたか」だけに畳む。
/// **読めなかったものは「別物」に倒す**——連番が1つ増えるだけで、写真は消えない。
fn same_bytes(a: &Path, b: &Path) -> bool {
    compare_bytes(a, b) == Bytes::Same
}

/// 更新時刻の許容差（FAT32 の2秒刻み）。
const MTIME_TOLERANCE_MS: i64 = 2_000;

/// 上と同じだが、**この操作で自分が書いたばかりのパス**（`taken`）は
/// 「同じもの」と見なさずに連番へ回す。
///
/// 「同名・同サイズなら同じもの」は取り込みでは成り立つ（日付でフォルダが分かれるので、
/// 別の日の同名ファイルが同じフォルダへ来ない）。**平置きの書き出しでは成り立たない**
/// ——カメラの連番が一周すると別の日の `DSC00001.ARW` が同じフォルダへ落ちるし、
/// 非圧縮RAWは中身が違ってもサイズが同じになる。**選んだ写真が黙って1枚欠ける**ので、
/// 自分が書いたものとの衝突は必ず連番で避ける。
///
/// `confirm` は**「既にある」と読んでよい根拠**（[`Confirm`]）。
/// [`Confirm::ByBytes`] を渡すと、**名前と大きさが合っただけでは畳まず、中身まで読む**。
pub(crate) fn resolve_dest_path_avoiding(
    dest_dir: &Path,
    file_name: &str,
    src_size: u64,
    src_mtime_ms: i64,
    taken: &TakenPaths,
    confirm: Confirm<'_>,
) -> DestResolution {
    // **在るものが「この写真」かどうか。** `looks_same` を通ったものだけ中身を読む
    // ——読む相手を、名前も大きさも時刻も合ったものだけに絞るため
    let is_this_photo = |existing: &Path| {
        if !looks_same(existing, src_size, src_mtime_ms) {
            return false;
        }
        match confirm {
            Confirm::ByLooks => true,
            // **読めなかったら畳む側へ倒す**（[`Bytes::Unknown`]）。
            // 行き先がクラウドにしか実体を持たないとき、ここを「別物」に倒すと
            // **挿し直すたびにライブラリが2倍になる**。
            //
            // **ただし、取り込み元が読めなかったぶんは倒さない**
            // （[`Bytes::SourceUnreadable`]）——倒すと**「取り込み済み」と報告する**
            // ことになる。倒さなければ連番へ回り、**コピーがそこで転んで
            // `failed` に数えられる**。**入っていないものを、入ったことにしない。**
            Confirm::ByBytes(src) => !matches!(
                compare_bytes(src, existing),
                Bytes::Differ | Bytes::SourceUnreadable
            ),
        }
    };

    let candidate = dest_dir.join(file_name);
    if !candidate.exists() && !taken.contains(&candidate) {
        return DestResolution::CopyTo(candidate);
    }
    if !taken.contains(&candidate) && is_this_photo(&candidate) {
        return DestResolution::AlreadyImported;
    }
    for i in 1..1000 {
        let alt = dest_dir.join(numbered_name(file_name, i));
        if taken.contains(&alt) {
            continue;
        }
        if !alt.exists() {
            return DestResolution::CopyTo(alt);
        }
        if is_this_photo(&alt) {
            return DestResolution::AlreadyImported;
        }
    }
    DestResolution::Exhausted
}

/// **この取り込みの中で、2つ以上のファイルが名乗っている名前。**
///
/// ここに載っている名前だけ、行き先の「同じもの」を**中身まで確かめる**
/// （[`Confirm::ByBytes`]）。**普通のカードでは空になる**ので、費用は0である。
/// 中身が入るのは `DCIM` が分かれているカード（連番を戻した機種・繰り上がったカード・
/// 2台で使ったカード）と、**控えのフォルダを抱えたカード**。
///
/// **大文字小文字は畳んで数える。** `DSC00001.ARW` と `dsc00001.arw` は
/// macOS と Windows では**同じ行き先**になるので、ぶつかっているのに
/// ぶつかっていないと読むと、2枚目が消える。Linux では実際にはぶつからないが、
/// **中身を1回読むだけで答えは変わらない**（行き先の名前が別なので `looks_same` に来ない）。
///
/// **正規化までは畳まない。** APFS は `café.jpg`（NFC）と `cafe\u{301}.jpg`（NFD）も
/// 1本と見るが、ここは**名前しか持っていない**ので、`TakenPaths` のように
/// **ファイルシステムへ訊く**（`canonicalize`）ことができない——**行き先はまだ無い**し、
/// 取り込み元の2本は**別のフォルダに居る**ので、互いの綴りを試しても当たらない。
/// **std に分解表がなく、そのために依存を1つ増やすかは別の判断**なので、
/// **いまは畳まない**（2026-09-06・ゲート2の指摘。**退行ではない**——
/// この工事の前から同じだった）。**効く範囲**: 綴りだけが違う双子が
/// **別々の周で来たとき、2枚目は「済」と出る**。同じ周の中なら `TakenPaths` が
/// 訊いてくれるので消えない（`the_same_name_in_a_different_normalisation_still_arrives`）。
///
/// **名前だけでは数えない。大きさと更新時刻まで見る**（2026-09-07・ゲート2）。
/// ぶつかっているのは「行き先に在る1本が、どちらのものか決められない」ときだけで、
/// **決められないのは、2枚が [`looks_alike`] のときに限る**——大きさか時刻が違えば、
/// 行き先の1本はどちらか一方にしか似ない。名前だけで数えると、
/// **`DCIM` が一周したカードや2台で使ったカード**（同じ名前・別の写真・別の日）で
/// **入っているものが毎回「分からない」に戻り**、`?` が並び、隠されず、既定で選び直され、
/// **押すと全件ぶんの中身比べが走って `copied` は 0** になる。
///
/// **絞りすぎない**ことのほうが大事なので、判定は[`looks_alike`]と同じ目にしてある
/// （[`looks_alike`] の説明）。**名前の単位で返す**ので、同じ名前に
/// 「似ている2枚」と「似ていない1枚」が混ざると、その1枚も巻き込んで
/// 「分からない」になる——**安全な側へ倒れる**ぶんは受け入れる。
///
/// **公開しているのは、「済」バッジと取り込みが同じ材料で数えるため。**
/// [`is_already_imported`] と [`import_files`] は同じ集合を受け取る——
/// **別々に数えると、片方が「まだ」と言い、もう片方が「もう入っている」と言う。**
pub fn contested_names<'a>(entries: impl Iterator<Item = (&'a str, u64, i64)>) -> HashSet<String> {
    // **同じ名前・同じ大きさ**の組にだけ絞ってから、時刻を突き合わせる。
    // [`looks_alike`] は大きさが違えば必ず偽なので、**違う大きさ同士は比べる必要が無い**
    // ——1万件が同じ名前でも、比べるのはこの中だけになる
    let mut buckets: HashMap<(String, u64), Vec<i64>> = HashMap::new();
    for (name, size, mtime_ms) in entries {
        buckets
            .entry((fold_name(name), size))
            .or_default()
            .push(mtime_ms);
    }
    let mut contested: HashSet<String> = HashSet::new();
    for ((name, size), mtimes) in buckets {
        if contested.contains(&name) {
            continue;
        }
        if mtimes.iter().enumerate().any(|(i, a)| {
            mtimes[i + 1..]
                .iter()
                .any(|b| looks_alike((size, *a), (size, *b)))
        }) {
            contested.insert(name);
        }
    }
    contested
}

/// 名前の畳み方。**ここ以外に置かない。**
///
/// [`contested_names`] は畳んだ綴りで数え、[`is_contested`] は畳んだ綴りで引く。
/// **片方だけ変えると、当たらなくなったことに誰も気づけない**——
/// バッジは「まだ入っていない」と出し続け、取り込みは中身を読まなくなる。
fn fold_name(name: &str) -> String {
    name.to_lowercase()
}

/// 既に「ぶつかっている」と分かっている名前から、[`is_already_imported`] が
/// そのまま受け取れる集合を作る。
///
/// **数えるのは呼ぶ側ではない**——[`contested_flags`] が一覧全体で数えた答えを、
/// **その一部だけ持ち回るため**にある。畳み方を呼ぶ側へ出さないのが目的で、
/// **ここで数え直しはしない。**
pub fn contested_set<'a>(names: impl Iterator<Item = &'a str>) -> HashSet<String> {
    names.map(fold_name).collect()
}

/// 一覧の各行が「ぶつかっている」か（[`contested_names`] と同じ数え方・同じ並び）。
///
/// 名前の集合をそのまま返すと、**呼ぶ側が畳み方を知らないと自分の行と突き合わせられない**
/// ——知らせると畳む規則が2か所になる。**行ごとの真偽なら、並びだけで突き合う。**
pub fn contested_flags(entries: &[(String, u64, i64)]) -> Vec<bool> {
    let contested = contested_names(entries.iter().map(|(n, s, m)| (n.as_str(), *s, *m)));
    entries
        .iter()
        .map(|(n, _, _)| is_contested(&contested, n))
        .collect()
}

/// その名前が [`contested_names`] に載っているか。
fn is_contested(contested: &HashSet<String>, file_name: &str) -> bool {
    !contested.is_empty() && contested.contains(&fold_name(file_name))
}

/// 取り込み元フォルダをスキャンし、設定に従ってコピーする。
///
/// `on_progress(処理済み件数, 総件数, いま処理したファイル)` は1件ごとに呼ばれる。
/// 進捗表示に「今どれを入れているか」を出せるよう、パスも渡す。
pub fn import_from(
    source: &Path,
    config: &Config,
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ImportStats, ImportError> {
    let dest_root = config
        .routing
        .destination
        .as_ref()
        .ok_or(ImportError::NoDestination)?;
    if !source.is_dir() {
        return Err(ImportError::SourceUnreadable(source.to_path_buf()));
    }
    // **取り込み元がパッケージの中**のときは、下の除外では止まらない
    // （`scan_roots` はルート自身を除外判定しない）。ここで断る——
    // 黙って0件を返すと「USBに写真が無い」と同じ見え方になり、
    // 何が起きたのか分からないため
    if is_managed_package_path(source) {
        return Err(ImportError::SourceIsManagedPackage(source.to_path_buf()));
    }

    // 取り込み元の除外はドットフォルダ（.Trashes等）と、アプリが管理する
    // パッケージ（写真.app等）だけ。**固定の一覧**で、ライブラリ用の
    // exclude_patterns はここに適用しない——ユーザーの除外設定が
    // DCIMフォルダに誤マッチして写真を静かに取りこぼす恐れがあるため。
    // パッケージを落とすのは、中身が内部ファイルだから: 外付けHDDに
    // 写真ライブラリがあると、派生JPEGを数千枚**コピーしてしまう**
    let source_buf = source.to_path_buf();
    let exclude: Vec<String> = std::iter::once(".*")
        .chain(scanner::MANAGED_PACKAGE_PATTERNS.iter().copied())
        .map(|p| p.to_string())
        .collect();
    let outcome = scanner::scan_roots(
        std::slice::from_ref(&source_buf),
        &config.import.extensions,
        &exclude,
    );

    let total = outcome.files.len();
    let mut stats = ImportStats {
        // 走査中にエラーがあった（ok_rootsに入らなかった）＝取りこぼしの可能性
        scan_incomplete: !outcome.ok_roots.contains(&source_buf),
        ..ImportStats::default()
    };

    // 同じ名前の相方を探すためのフォルダの覚え書き（読むのは1フォルダ1回）
    let mut pairs = PairIndex::default();
    // **この操作で自分が書いた行き先**。同じ名前・同じ大きさの2枚目を
    // 「取り込み済み」と読ませないために持ち回る（`import_one_with` の説明）
    let mut written = TakenPaths::default();
    // **カードの中で名前がぶつかっているぶん**。ここだけ中身まで確かめる
    // （`contested_names`）。**前の周で片方だけ入っている**と、名前と大きさでは
    // 「もう入っている」と読めてしまい、2枚目が黙って消える
    let contested = contested_names(outcome.files.iter().filter_map(|f| {
        f.path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| (n, f.size as u64, f.mtime_ms))
    }));

    for (i, file) in outcome.files.iter().enumerate() {
        let name_contested = file
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| is_contested(&contested, n));
        let result = import_one_with(
            Incoming {
                path: &file.path,
                size: file.size as u64,
                mtime_ms: file.mtime_ms,
                taken_at_ms: taken_at_paired(&file.path, config, &mut pairs),
                contested: name_contested,
            },
            dest_root,
            config,
            &mut written,
        );
        match result {
            ImportOneResult::Copied => stats.copied += 1,
            ImportOneResult::Skipped => stats.skipped += 1,
            ImportOneResult::Failed => stats.failed += 1,
        }
        on_progress(i + 1, total, &file.path);
    }
    Ok(stats)
}

/// 選んだファイルだけを取り込む（第5部 段階E: 取り込みウィザード）。
///
/// [`import_from`] と違い**走査しない**。ウィザードが既に一覧を持っており、
/// ユーザーがチェックを外した分を取り込まないことが本質なので、
/// 渡されたパスをそのまま順に処理する。
///
/// `contested` は**一覧に出ていた全部**から作る（[`contested_names`]）。
/// **選んだぶんから作ってはいけない。**「済」バッジ（[`is_already_imported`]）は
/// カード全体で数えるので、**選ばれた1枚だけで数え直すと根拠が変わる**
/// ——ウィザードが「まだ入っていない」と出して選ばせた写真を、
/// **取り込みが「もう入っている」と読んで飛ばす**。
/// **同じ答えを2度出すには、同じ材料で数えるしかない。**
pub fn import_files(
    files: &[PathBuf],
    contested: &HashSet<String>,
    config: &Config,
    on_progress: impl Fn(usize, usize, &Path),
) -> Result<ImportStats, ImportError> {
    let dest_root = config
        .routing
        .destination
        .as_ref()
        .ok_or(ImportError::NoDestination)?;
    let total = files.len();
    let mut stats = ImportStats::default();
    // 組は**ディスクの中身**で決める（選んだ枚数では決めない）。片方だけ選んだ
    // ときも相方の日付に合わせる——ウィザードの「済」バッジと行き先が同じ規則で
    // 決まっていないと、済と出ないまま**同じ写真をもう一度コピー**することになる
    let mut pairs = PairIndex::default();
    // 丸ごと取り込みと同じ理由で持ち回る（`import_one_with` の説明）——
    // ウィザードで**同じ名前の2枚を両方選ぶ**のは、カードの `DCIM` が分かれていれば普通に起きる
    let mut written = TakenPaths::default();
    for (i, path) in files.iter().enumerate() {
        let name_contested = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| is_contested(contested, n));
        // ウィザードで一覧を出した後にファイルが消えている可能性があるので
        // ここで改めてstatする（読めなければ失敗として数え、他は続行する）。
        //
        // アプリが管理するパッケージの中身も同じ扱いで**1件ずつ落とす**。
        // 一覧の経路（`list_source_dir` / `list_source_tree`）が既に断っているので
        // ここへ来ることは無いはずだが、入口ごとに守る。ただし**一括中止はしない**
        // ——紛れ込んだ1件で1000件の取り込みが丸ごと消えるのは、
        // 読めないファイルを1件ずつ数えるこの関数の作法に合わない
        let result = if is_managed_package_path(path) {
            ImportOneResult::Failed
        } else {
            match std::fs::metadata(path) {
                Ok(meta) => import_one_with(
                    Incoming {
                        path,
                        size: meta.len(),
                        mtime_ms: mtime_ms_of(&meta),
                        taken_at_ms: taken_at_paired(path, config, &mut pairs),
                        contested: name_contested,
                    },
                    dest_root,
                    config,
                    &mut written,
                ),
                Err(_) => ImportOneResult::Failed,
            }
        };
        match result {
            ImportOneResult::Copied => stats.copied += 1,
            ImportOneResult::Skipped => stats.skipped += 1,
            ImportOneResult::Failed => stats.failed += 1,
        }
        on_progress(i + 1, total, path);
    }
    Ok(stats)
}

/// メタデータからmtime（Unixエポックミリ秒）を取り出す。取れなければ0。
fn mtime_ms_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// ウィザードの1行が、コピー先に対してどう見えるか（[`is_already_imported`]）。
///
/// **`Unsure` は「分からない」であって「入っている」ではない。**
/// **「済」に丸めると、まだ入っていない写真が既定で選択から外れ、既定で画面からも消える**
/// ——**利用者には何も起きていないように見えたまま、その写真は二度と来ない。**
/// **「未取り込み」に丸めると**、控えのフォルダを抱えたカードで**入っているものが
/// 毎回まるごと未取り込みに見える**。**どちらでもない、と言えるようにしてある。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportState {
    /// 行き先に同じものがある
    Imported,
    /// 行き先に無い
    NotImported,
    /// **同じ名前がこの取り込みの中に2つ以上あり、行き先に名前も大きさも時刻も合うものがある。**
    /// **在る1本はどちらか一方のもの**で、どちらかは**中身を読まないと分からない**
    /// ——読むのは取り込みのとき（[`Confirm::ByBytes`]）。
    Unsure,
}

/// そのファイルが既にコピー先へ取り込まれているか（コピーはしない）。
///
/// ウィザードで「済」バッジを出し、**未取り込みだけを初期選択する**ために使う。
/// 判定は取り込み本体と同じ経路（同じ日付決定＋同じ衝突回避）を通るので、
/// 「済と出たのにもう一度コピーされた」というズレが起きない——組で日付を
/// そろえる（0.2）ぶんも [`taken_at_paired`] で同じように通す。
///
/// `contested` は**カードに出ている名前全部**から作る（[`contested_names`]）。
/// **同じ名前が2つ以上あるなら、行き先に在る1本はどちらか一方のもの**で、
/// **名前と大きさではどちらかを決められない**。そこは [`ImportState::Unsure`] を返す。
///
/// **ここでは中身を読まない。** 読めば決まるが、**この関数はウィザードが開くたび・
/// フォルダを変えるたびに、一覧の全件に対して走る**。控えのフォルダを抱えたカードでは
/// **全部の名前がぶつかる**ので、**開くだけでカード1枚ぶんを読む**ことになる
/// （2026-09-06・ゲート2）。**読むのは取り込みのときだけ**にして、
/// ここでは**分からないと言う**——[`import_files`] が同じ `contested` を受け取って、
/// そちらで中身まで確かめる。
///
/// **[`import_files`] へ渡すのと同じ集合を渡すこと。**
pub fn is_already_imported(
    path: &Path,
    config: &Config,
    contested: &HashSet<String>,
) -> ImportState {
    let Some(dest_root) = config.routing.destination.as_ref() else {
        return ImportState::NotImported;
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return ImportState::NotImported;
    };
    let mtime_ms = mtime_ms_of(&meta);
    let mut pairs = PairIndex::default();
    let dest_dir = dest_dir_for(
        taken_at_paired(path, config, &mut pairs),
        mtime_ms,
        dest_root,
        config,
    );
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return ImportState::NotImported;
    };
    match resolve_dest_path_avoiding(
        &dest_dir,
        file_name,
        meta.len(),
        mtime_ms,
        &TakenPaths::default(),
        Confirm::ByLooks,
    ) {
        // **名前も大きさも時刻も合った。** それが「この写真」かどうかは、
        // 名前がぶつかっていなければ言い切ってよく、ぶつかっていれば言い切れない
        DestResolution::AlreadyImported if is_contested(contested, file_name) => {
            ImportState::Unsure
        }
        DestResolution::AlreadyImported => ImportState::Imported,
        // `Exhausted` は連番が尽きた（1000本埋まっている）。**入っていない**ので、
        // 選ばせて取り込みに失敗させる。ここで「済」と出すと、入っていないものが消える
        DestResolution::CopyTo(_) | DestResolution::Exhausted => ImportState::NotImported,
    }
}

enum ImportOneResult {
    Copied,
    Skipped,
    Failed,
}

/// このファイルのコピー先フォルダ（作成はしない）。
///
/// 日付決定: 撮影日時 → mtime（正の値のみ。0はメタデータ欠損とみなす）→ 取り込み実行日。
/// mtime=0を有効値として扱うと1970年のフォルダが生まれてしまう。
///
/// 撮影日時の出どころは形式で変わる。**動画はEXIFを持たない**のでコンテナ
///（`moov`）を読む——ここでmtimeへ落とすと、フォルダはカードからコピーした日、
/// 一覧は撮影日になり、**同じファイルの置き場所と表示日がずれる**（第9部）。
/// その1枚の撮影日時（EXIF・動画のメタ・ファイル名の順）。無ければ None。
fn taken_at_of(path: &Path) -> Option<i64> {
    let from_meta = if crate::video::is_video_path(path) {
        crate::video::read_info(path).and_then(|i| i.taken_at_ms)
    } else {
        read_exif_meta(path).taken_at_ms
    };
    // 撮影日時を持たないファイルはファイル名に聞く（段階H-2）
    from_meta.or_else(|| crate::namedate::guess_taken_at(path))
}

/// 同じフォルダの、**同じ名前（拡張子違い）の写真**を覚えておく箱。
///
/// 相方を探すのにフォルダを読むが、**1つのフォルダは1回しか読まない**。
/// 1000枚のカードで1枚ごとに読み直すと、そのぶん取り込みの頭が止まる。
#[derive(Default)]
struct PairIndex {
    dirs: std::collections::HashMap<PathBuf, Vec<PathBuf>>,
}

impl PairIndex {
    /// `path` と同じ名前の相方（自分は含まない）。**パス順**で返す。
    ///
    /// 相方かどうかは**ディスクの中身**で決める——選んだ枚数では決めない。
    /// ウィザードの「済」バッジ（[`is_already_imported`]）と実際の行き先が
    /// 同じ規則で決まっていないと、**済と出ていないのに二重コピー**になる。
    fn mates(&mut self, path: &Path, config: &Config) -> Vec<PathBuf> {
        let (dir, stem) = crate::sidecar::pair_key(path);
        let listing = self.dirs.entry(dir.clone()).or_insert_with(|| {
            let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            crate::scanner::has_target_extension(p, &config.import.extensions)
                        })
                        .collect()
                })
                .unwrap_or_default();
            // 並びを固定する。どちらから取り込んでも同じ答えになるように
            found.sort();
            found
        });
        let myself = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        listing
            .iter()
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default()
                    != myself
                    && crate::sidecar::pair_key(p).1 == stem
            })
            .cloned()
            .collect()
    }
}

/// そのファイルの撮影日時。無ければ**同じ名前の相方**に聞く（0.2・`dev/loadmap.md` 1.1）。
///
/// `IMG_0001.CR3` と `IMG_0001.JPG` は1回の撮影の表と裏で、別々の日のフォルダへ
/// 散ると**あとで突き合わせられなくなる**。片方だけEXIFを持たない（RAWの
/// メーカー独自タグが読めない・JPGだけ編集して日時が消えた等）ときに起きる。
///
/// **自分の撮影日時が最優先**。組の日付で上書きすると、たまたま同じ名前の
/// 無関係な2枚（2019年の `note.jpg` と 2024年の `note.png`、連番が一周した
/// `DSC00001`）が、拡張子の並び順という**恣意的な理由**で片方の年へ引きずられる。
/// 相方を読むのは**自分の日付が取れなかったときだけ**なので、
/// ふつうの取り込みで読む回数は今までと変わらない。
fn taken_at_paired(path: &Path, config: &Config, pairs: &mut PairIndex) -> Option<i64> {
    if let Some(taken) = taken_at_of(path) {
        return Some(taken);
    }
    pairs
        .mates(path, config)
        .iter()
        .find_map(|mate| taken_at_of(mate))
}

/// このファイルのコピー先フォルダ（撮影日時は決まっているものを渡す）。
fn dest_dir_for(
    taken_at_ms: Option<i64>,
    mtime_ms: i64,
    dest_root: &Path,
    config: &Config,
) -> PathBuf {
    let date = taken_at_ms
        // 撮影日時が無いものを mtime へ落とすと、コピー先フォルダが
        // 「取り込んだ日」になり、一覧の表示日（同じ順で名前を見る）とずれる
        .and_then(date_from_local_ms)
        .or_else(|| {
            (mtime_ms > 0)
                .then(|| date_from_local_ms(mtime_ms))
                .flatten()
        })
        .unwrap_or_else(today_local);
    dest_root.join(render_folder_pattern(&config.routing.folder_pattern, date))
}

/// 1枚ぶんの、**もう読んである事実**。[`import_one_with`] はここから読み直さない
/// ——同じファイルの EXIF を2回開かないため（`taken_at_ms` は [`taken_at_paired`] の答え）。
struct Incoming<'a> {
    path: &'a Path,
    size: u64,
    mtime_ms: i64,
    /// 決まった撮影日時。行き先のフォルダはこれで決まる
    taken_at_ms: Option<i64>,
    /// **この取り込みの中に、同じ名前がもう1つあるか**（[`contested_names`]）
    contested: bool,
}

/// 1枚をコピー先へ運ぶ。
///
/// `taken_at_ms` は**決まった撮影日時**（[`taken_at_paired`] が組まで見て出したもの）。
/// ここでは読み直さない——同じファイルのEXIFを2回開かないため。
/// `written` は**この操作で自分が書いたばかりの行き先**。
///
/// **渡さないと、同じ名前・同じ大きさの2枚目が「取り込み済み」に化けて黙って消える。**
/// 起きるのは**カードの中で `DCIM` のフォルダが分かれているとき**——連番を戻した機種、
/// 2台のカメラで使ったカード、9999枚で繰り上がったカードでは、
/// `100MSDCF/DSC00001.ARW` と `101MSDCF/DSC00001.ARW` が**同じ日のフォルダへ来る**。
/// **非圧縮RAWは中身が違ってもサイズが同じ**なので、`looks_same` は「同じもの」と読む。
/// `export.rs` は最初からこれを渡していた（[`resolve_dest_path_avoiding`] の説明）。
///
/// `contested` は**この取り込みの中に同じ名前がもう1つある**か（[`contested_names`]）。
/// 立っているときは、**行き先に在るものを「同じ写真」と読む前に中身まで読む**
/// （[`Confirm::ByBytes`]）——**前の周で片方だけ入っている**カードで、
/// 2枚目が「取り込み済み」に化けるのを止めるのはここである。
fn import_one_with(
    file: Incoming<'_>,
    dest_root: &Path,
    config: &Config,
    written: &mut TakenPaths,
) -> ImportOneResult {
    let Incoming {
        path,
        size,
        mtime_ms,
        taken_at_ms,
        contested,
    } = file;
    let dest_dir = dest_dir_for(taken_at_ms, mtime_ms, dest_root, config);
    if std::fs::create_dir_all(&dest_dir).is_err() {
        return ImportOneResult::Failed;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return ImportOneResult::Failed;
    };
    // **自分がこの周で書いた行き先とぶつかったとき、中身まで同じなら同じ写真である。**
    //
    // 名前と大きさと時刻だけでは、**同じ写真の写し**と**別の写真**が区別できない。
    // 区別するには読むしかないが、**読むのはここだけでよい**——ぶつかったときにしか
    // 来ないので、**カードを挿し直すたびに全件読む**ことにはならない
    // （挿し直しは `written` が空のまま進むので、ここへ来ない）。
    // これが無いと、**カードの中に控えのフォルダがある**だけで
    // （利用者が作った backup、ディスクイメージから戻したカード）
    // **同じ写真が2枚に増える**。
    if already_written_same_photo(&dest_dir, file_name, path, size, mtime_ms, written) {
        // **飛ばした側のサイドカーは運ばない。** 中身の違う `.xmp` が付いていても、である。
        // **この工事の前からそうだった**（同じ入力で前後とも `copied=1 / skipped=1`、
        // 残る `.xmp` は先に来たほうだけ、と実測して確かめた）ので、ここでは変えない。
        // 運ぶとしたら連番の名前になるが、**写真は `-1` に付いていない**ので、
        // **どの写真のものでもない `.xmp` が1本残る**——現像ソフトは名前で結び付けるので、
        // 置き去りより始末が悪い。**変えるなら、飛ばす条件をサイドカーごと見る形に
        // 組み替える話**で、この直しの範囲ではない。
        return ImportOneResult::Skipped;
    }

    let confirm = if contested {
        Confirm::ByBytes(path)
    } else {
        Confirm::ByLooks
    };
    let dest_path =
        match resolve_dest_path_avoiding(&dest_dir, file_name, size, mtime_ms, written, confirm) {
            DestResolution::CopyTo(p) => p,
            DestResolution::AlreadyImported => return ImportOneResult::Skipped,
            // コピーしていないのにSkippedと報告すると「取り込み済み」と誤認される
            DestResolution::Exhausted => return ImportOneResult::Failed,
        };

    match std::fs::copy(path, &dest_path) {
        Ok(copied_bytes) => {
            // Unixのfs::copyはmtimeを保持しないため明示的に引き継ぐ
            // （日付フォルダの振り分けとグリッドの日付グルーピングを一致させる）
            let src_meta = std::fs::metadata(path);
            if let Ok(meta) = &src_meta {
                if let Ok(mtime) = meta.modified() {
                    let _ = filetime::set_file_mtime(
                        &dest_path,
                        filetime::FileTime::from_system_time(mtime),
                    );
                }
            }
            if config.import.verify_after_copy {
                // スキャン時点ではなく「今の」ソースサイズと比較する
                // （スキャン後にファイルが変化したケースで正しいコピーを消さない）
                let src_now = src_meta.map(|m| m.len()).unwrap_or(copied_bytes);
                let dest_len = std::fs::metadata(&dest_path).map(|m| m.len()).ok();
                if dest_len != Some(src_now) {
                    // 検証失敗: 中途半端なファイルを残さない
                    remove_partial_copy(&dest_path);
                    return ImportOneResult::Failed;
                }
            }
            // **写真が付いたら、その影も連れていく**（0.2・`dev/loadmap.md` 1.1）。
            // `.xmp` には現像設定・評価・キーワードが入っていて、置き去りにすると
            // 利用者から見れば編集がぜんぶ消えたのと同じになる。
            //
            // **写真の成否には影響させない**。サイドカーが運べなかったからといって
            // 写真を「失敗」に数えると、取り込みそのものが止まったように見える
            copy_sidecars(path, &dest_path, config);
            // **書けたものだけを「使った」に数える。** 失敗した行き先を数えると、
            // 次の1枚が要らない連番へ回される（`export.rs` の
            // `a_name_whose_export_failed_is_not_marked_as_taken` と同じ扱い）
            written.insert(&dest_path);
            ImportOneResult::Copied
        }
        Err(_) => {
            // **中途半端なファイルを残さない**（検証に失敗した枝と同じ扱い。
            // `export.rs` の `copy_verified` もそうしている）。残すと**連番の1枠を
            // 永久に埋める**うえ、大きさと時刻がたまたま合えば、あとから来た写真が
            // それを見て「取り込み済み」に化ける
            remove_partial_copy(&dest_path);
            ImportOneResult::Failed
        }
    }
}

/// 写真に付いているサイドカーを、写真の隣（付いた先）へコピーする。
///
/// 名前は**付いた先の写真に合わせる**（連番が付いたら同じ連番にする）。
/// 現像ソフトは名前で結び付けるので、ここがずれると別の写真の設定として読まれる。
///
/// 既にあるものは**上書きしない**——同名衝突は写真と同じ連番の規則で避ける。
fn copy_sidecars(source_photo: &Path, dest_photo: &Path, config: &Config) {
    let exts = &config.import.sidecar_extensions;
    if exts.is_empty() {
        return;
    }
    let (Some(dest_dir), Some(dest_name)) = (
        dest_photo.parent(),
        dest_photo.file_name().and_then(|n| n.to_str()),
    ) else {
        return;
    };
    for sidecar in crate::sidecar::sidecars_of(source_photo, exts) {
        let Ok(meta) = std::fs::metadata(&sidecar) else {
            continue;
        };
        let name = crate::sidecar::sidecar_dest_name(source_photo, &sidecar, dest_name);
        let target = match resolve_dest_path(dest_dir, &name, meta.len(), mtime_ms_of(&meta)) {
            DestResolution::CopyTo(p) => p,
            // 同じものが既にある／連番が尽きた。どちらも**黙って諦める**
            // （写真は運べているので、取り込み全体を失敗にはしない）
            DestResolution::AlreadyImported | DestResolution::Exhausted => continue,
        };
        if std::fs::copy(&sidecar, &target).is_ok() {
            if let Ok(mtime) = meta.modified() {
                let _ =
                    filetime::set_file_mtime(&target, filetime::FileTime::from_system_time(mtime));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config(dest: &Path) -> Config {
        let mut config = Config::default();
        config.routing.destination = Some(dest.to_path_buf());
        config
    }

    /// **この作り物には同じ名前が2つ無い**ので、ぶつかっている名前も無い。
    ///
    /// `HashSet::new()` と書いても同じだが、**名前を付けて事実として書く**
    /// ——空を渡すのは手抜きではなく、`contested_names` がこの作り物に対して
    /// 実際に返す値である。**名前が重なる作り物では、これを渡してはいけない。**
    fn no_repeated_names() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn pattern_substitution_is_zero_padded() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 5,
        };
        assert_eq!(
            render_folder_pattern("{year}/{year}-{month}-{day}", date),
            "2026/2026-08-05"
        );
        assert_eq!(render_folder_pattern("{year}/{month}", date), "2026/08");
    }

    #[test]
    fn same_name_and_size_with_a_different_mtime_is_not_counted_as_imported() {
        // 差分検知の原則（サイズと更新時刻だけを見る）を取り込みの重複判定にも
        // 効かせている。**別の日の同名ファイル**（連番が一周したRAW等）を
        // 「もう取り込んだ」と誤判定して落とさないため
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("dest");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("DSC00001.ARW"), b"1111").unwrap();
        filetime::set_file_mtime(
            dest_dir.join("DSC00001.ARW"),
            filetime::FileTime::from_unix_time(1_000_000_000, 0),
        )
        .unwrap();

        // 同じ名前・同じサイズ・別の時刻 → 連番へ回る
        assert!(matches!(
            resolve_dest_path(&dest_dir, "DSC00001.ARW", 4, 1_700_000_000_000),
            DestResolution::CopyTo(_)
        ));
        // 時刻も合っていれば「もうある」
        assert!(matches!(
            resolve_dest_path(&dest_dir, "DSC00001.ARW", 4, 1_000_000_000_000),
            DestResolution::AlreadyImported
        ));
    }

    #[test]
    fn import_copies_into_the_date_folder() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();
        fs::write(src.join("b.jpg"), b"bbbb").unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 2);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.failed, 0);

        // mtimeは今日 → 今日の日付フォルダに入っている
        let copied: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();
        assert_eq!(copied.len(), 2);
        // パターン {year}/{year}-{month}-{day} の2階層下にある
        for p in &copied {
            let rel = p.strip_prefix(&dest).unwrap();
            assert_eq!(rel.components().count(), 3);
        }
    }

    /// **取り込み元そのもの**にパッケージを指定したら断る。
    ///
    /// 取り込み元はネイティブのフォルダ選択ダイアログで選ぶので、一覧から
    /// 隠しても名指しで選べる。`scan_roots` はルート自身を除外判定しないため、
    /// ここで止めないと内部の派生画像を全部コピーする
    #[test]
    fn a_source_that_is_itself_a_package_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("写真ライブラリ.photoslibrary");
        let dest = dir.path().join("photos");
        let inner = src.join("resources/derivatives");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("derived.jpg"), b"xxx").unwrap();

        let config = test_config(&dest);
        let err = import_from(&src, &config, |_, _, _| {}).unwrap_err();
        assert!(
            matches!(err, ImportError::SourceIsManagedPackage(_)),
            "err={err}"
        );
        // 1枚もコピーしていない
        assert!(!dest.exists());

        // **中のフォルダを名指しされても断る**。ネイティブのダイアログは
        // パッケージの中へ入って選べるので、葉の名前だけでは守れない
        let err = import_from(&inner, &config, |_, _, _| {}).unwrap_err();
        assert!(
            matches!(err, ImportError::SourceIsManagedPackage(_)),
            "err={err}"
        );
        assert!(!dest.exists());
    }

    #[test]
    fn pointing_inside_a_package_is_recognised_too() {
        assert!(is_managed_package_path(Path::new(
            "/x/写真ライブラリ.photoslibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            r"E:\Photos Library.photoslibrary"
        )));
        // **中を名指しした場合**。葉の名前だけの判定ではここが抜ける
        // （ネイティブのダイアログはパッケージの中へ入って選べる）
        assert!(is_managed_package_path(Path::new(
            "/x/写真ライブラリ.photoslibrary/originals/0"
        )));

        // 写真.appの系譜はまとめて落とす（どれも `~/Pictures` に住む）
        assert!(is_managed_package_path(Path::new(
            "/x/iPhoto Library.photolibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            "/x/iPhoto Library.migratedphotolibrary"
        )));
        assert!(is_managed_package_path(Path::new(
            "/x/Aperture Library.aplibrary"
        )));

        assert!(!is_managed_package_path(Path::new("/x/DCIM")));
        assert!(!is_managed_package_path(Path::new("/x/photoslibrary/a")));
    }

    /// アプリが管理するパッケージの中身は取り込まない。
    ///
    /// ここは**ファイルをコピーする**経路なので、漏らすと索引より重い
    /// ——外付けHDDに写真ライブラリがあると、内部の派生JPEGを数千枚
    /// コピー先へ書いてしまう
    #[test]
    fn a_photo_library_package_is_not_imported() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        let pkg = src.join("写真ライブラリ.photoslibrary/resources/derivatives");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("derived.jpg"), b"xxx").unwrap();
        fs::create_dir_all(src.join("DCIM")).unwrap();
        fs::write(src.join("DCIM/a.jpg"), b"aaa").unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 1, "DCIMの1枚だけ");

        let copied: Vec<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(copied, vec!["a.jpg"]);
    }

    #[test]
    fn same_name_and_size_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"aaa").unwrap();

        let config = test_config(&dest);
        let first = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(first.copied, 1);

        // 再取り込み → スキップ
        let second = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(second.copied, 0);
        assert_eq!(second.skipped, 1);
    }

    /// **`.xmp` を置き去りにしない**（0.2・`dev/loadmap.md` 1.1）。
    /// 置き去りにすると、利用者から見れば現像の作業がぜんぶ消えたのと同じになる。
    #[test]
    fn sidecars_travel_with_the_photo() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        // 置き換え型（Adobe）と足す型（darktable）の両方
        fs::write(src.join("IMG_0001.xmp"), b"<x>adobe</x>").unwrap();
        fs::write(src.join("IMG_0001.jpg.xmp"), b"<x>darktable</x>").unwrap();

        let config = test_config(&dest);
        assert_eq!(import_from(&src, &config, |_, _, _| {}).unwrap().copied, 1);

        let names: Vec<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"IMG_0001.jpg".to_string()), "{names:?}");
        assert!(names.contains(&"IMG_0001.xmp".to_string()), "{names:?}");
        assert!(names.contains(&"IMG_0001.jpg.xmp".to_string()), "{names:?}");
        // **写真1枚に対してサイドカーは2つだけ**（綴り違いで二重に運ばない）
        assert_eq!(names.len(), 3, "{names:?}");
    }

    /// サイドカーは**写真が付いた先の名前**に合わせる。連番が付いたのに
    /// サイドカーだけ元の名前で置くと、別の写真の設定として読まれる。
    #[test]
    fn a_numbered_copy_gives_its_sidecar_the_same_number() {
        let dir = tempfile::tempdir().unwrap();
        let src1 = dir.path().join("usb1");
        let src2 = dir.path().join("usb2");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src1).unwrap();
        fs::create_dir_all(&src2).unwrap();
        fs::write(src1.join("DSC_0001.jpg"), b"first").unwrap();
        fs::write(src1.join("DSC_0001.xmp"), b"<x>1</x>").unwrap();
        fs::write(src2.join("DSC_0001.jpg"), b"second-longer").unwrap();
        fs::write(src2.join("DSC_0001.xmp"), b"<x>2</x>").unwrap();

        let config = test_config(&dest);
        import_from(&src1, &config, |_, _, _| {}).unwrap();
        import_from(&src2, &config, |_, _, _| {}).unwrap();

        let mut names: Vec<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "DSC_0001-1.jpg",
                "DSC_0001-1.xmp",
                "DSC_0001.jpg",
                "DSC_0001.xmp"
            ],
            "2枚目の写真とサイドカーが同じ連番で並ぶこと"
        );
    }

    /// 設定を空にすれば**一切運ばない**（要らない人の逃げ道）。
    #[test]
    fn an_empty_sidecar_setting_carries_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("IMG_0001.jpg"), b"photo").unwrap();
        fs::write(src.join("IMG_0001.xmp"), b"<x/>").unwrap();

        let mut config = test_config(&dest);
        config.import.sidecar_extensions.clear();
        import_from(&src, &config, |_, _, _| {}).unwrap();

        let names: Vec<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["IMG_0001.jpg"], "{names:?}");
    }

    /// EXIFに**撮影日時だけ**を持つ、最小のJPEG。
    ///
    /// 組の日付をめぐるテストには「同じ名前で、違う撮影日時」の2枚が要る。
    /// 名前から日付を読む道（段階H-2の `namedate`）は名前が同じなら同じ答えを
    /// 返すので、ここだけはEXIFを実際に埋める。絵は要らない——読むのは日時だけ。
    fn jpeg_with_taken_at(date: &str) -> Vec<u8> {
        assert_eq!(date.len(), 19, "YYYY:MM:DD HH:MM:SS の形で渡す");
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes());
        // IFD0: ExifIFDへのポインタ1本だけ（26バイト目から）
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x8769u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&26u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        // ExifIFD: DateTimeOriginal（文字列は44バイト目から20バイト）
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&44u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(date.as_bytes());
        tiff.push(0);

        let mut out: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE1];
        // APP1の長さだけはビッグエンディアン（TIFFの中身とは別の決まり）
        out.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
        out.extend_from_slice(b"Exif\0\0");
        out.extend_from_slice(&tiff);
        out.extend_from_slice(&[0xFF, 0xD9]);
        out
    }

    /// 上の細工が本当にEXIFとして読めることを、先に固定しておく
    /// （読めていないのに「日付が取れなかった」経路で通ってしまうと、
    /// 下の2つのテストが何も確かめていないことになる）。
    #[test]
    fn the_test_exif_reads() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("IMG_0001.jpg");
        fs::write(&photo, jpeg_with_taken_at("2019:08:11 12:00:00")).unwrap();
        assert!(taken_at_of(&photo).is_some(), "EXIFの日時が読めていない");
    }

    /// **自分の撮影日時は、組に上書きされない**（ゲート1のP2）。
    ///
    /// たまたま同じ名前の無関係な2枚（別々に保存した `note.jpg` と `note.png`）が、
    /// 拡張子の並び順という恣意的な理由で片方の年へ引きずられてはいけない。
    #[test]
    fn its_own_capture_date_outranks_the_pair() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("note.jpg"),
            jpeg_with_taken_at("2019:08:11 12:00:00"),
        )
        .unwrap();
        fs::write(
            src.join("note.png"),
            jpeg_with_taken_at("2024:03:05 09:00:00"),
        )
        .unwrap();

        let config = test_config(&dest);
        import_from(&src, &config, |_, _, _| {}).unwrap();

        let dirs: std::collections::HashSet<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.path().parent().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            dirs.len(),
            2,
            "自分の日付を持つ2枚が同じ年へ寄せられた: {dirs:?}"
        );
    }

    /// **読めなかった行き先は「別の写真」ではない。**
    ///
    /// `Confirm::ByBytes` は `Bytes::Unknown` を**畳む側へ倒す**。
    /// 「別物」に倒すと、**行き先がクラウドにしか実体を持たないライブラリ**
    /// （OneDrive の Files On-Demand）で、名前がぶつかっているカードを
    /// 挿し直すたびに**中身が2倍になる**。
    ///
    /// ここでは `Unknown` を**権限に頼らずに**作る——**行き先をディレクトリにする**。
    /// **クラウドにしか実体が無いファイルを真似ているのではない**。作りたいのは
    /// 「**在って、名前も大きさも時刻も合うのに、中身が読めない**」という形で、
    /// ディレクトリはどの OS でもそれになる（開けないか、開けても読めない）。
    /// **在らないパスでは作れない**——それは**取り込み元**の側が読めない形で、
    /// 畳んではいけないほう（`a_source_that_cannot_be_read_is_not_reported_as_imported`）。
    /// 権限で作る側（`chmod 000`）は `tests/card_import.rs` にあるが、
    /// **root では効かない**ので、**どの uid でも必ず走るのはこちら**である。
    #[test]
    fn a_destination_that_cannot_be_read_is_not_a_different_photo() {
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("2020-09-13");
        fs::create_dir_all(&dest_dir).unwrap();
        let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        let mtime_ms = 1_600_000_000_000;
        let existing = dest_dir.join("DSC00001.ARW");
        fs::write(&existing, b"aaaa").unwrap();
        filetime::set_file_mtime(&existing, stamp).unwrap();

        // 読めない行き先 → `Unknown` → **畳む**。
        // **大きさと時刻はその場で訊く**——ディレクトリの `len()` は
        // プラットフォームで違う（NTFS は 0、ext4 は 4096）ので、書くと外れる
        let opaque = dest_dir.join("DSC00002.ARW");
        fs::create_dir(&opaque).unwrap();
        let opaque_meta = fs::metadata(&opaque).unwrap();
        let opaque_size = opaque_meta.len();
        let opaque_mtime_ms = mtime_ms_of(&opaque_meta);
        // 取り込み元は**読める**（同じ大きさ）。読めないのは行き先だけ、という形にする
        let readable_src = dir.path().join("DSC00002.ARW");
        fs::write(&readable_src, vec![0u8; opaque_size as usize]).unwrap();
        assert!(
            matches!(
                resolve_dest_path_avoiding(
                    &dest_dir,
                    "DSC00002.ARW",
                    opaque_size,
                    opaque_mtime_ms,
                    &TakenPaths::default(),
                    Confirm::ByBytes(&readable_src),
                ),
                DestResolution::AlreadyImported
            ),
            "行き先が読めないというだけで別物に倒している（挿し直すたびに増える）"
        );

        // **対照**: 読めて、中身が違う → 連番へ回す
        let different = dir.path().join("DSC00001.ARW");
        fs::write(&different, b"bbbb").unwrap();
        filetime::set_file_mtime(&different, stamp).unwrap();
        match resolve_dest_path_avoiding(
            &dest_dir,
            "DSC00001.ARW",
            4,
            mtime_ms,
            &TakenPaths::default(),
            Confirm::ByBytes(&different),
        ) {
            DestResolution::CopyTo(p) => {
                assert_eq!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("DSC00001-1.ARW")
                )
            }
            _ => panic!("中身が違うのに畳んだ"),
        }
    }

    /// **行ごとの真偽と、名前の集合は、同じことを言っている。**
    ///
    /// `probe_imported` へ渡すのは切れ端に居るぶんだけで、突き合わせは**並び**でやる
    /// ——ずれると、**ぶつかっている行が「ぶつかっていない」として送られ**、
    /// バッジが「済」と言い切ってしまう（この工事が塞いだ穴）。
    /// **綴りが違うだけの双子も同じ扱いになること**まで見る。
    #[test]
    fn the_contested_flags_line_up_with_the_names() {
        // 綴りだけが違う双子（**同じ大きさ・同じ時刻**）はぶつかっている。
        // 3本目は名前が違うので関係ない
        let entries: Vec<(String, u64, i64)> = vec![
            ("DSC00001.ARW".into(), 4, 1_600_000_000_000),
            ("dsc00001.arw".into(), 4, 1_600_000_000_000),
            ("DSC00002.ARW".into(), 4, 1_600_000_000_000),
        ];
        assert_eq!(
            contested_flags(&entries),
            vec![true, true, false],
            "並びか畳み方がずれている"
        );

        // 集合の側と食い違わないこと（材料は1つ）
        let set = contested_names(entries.iter().map(|(n, s, m)| (n.as_str(), *s, *m)));
        for ((name, _, _), flag) in entries.iter().zip(contested_flags(&entries)) {
            assert_eq!(is_contested(&set, name), flag, "{name} で食い違った");
        }

        // 受け取り直した側が、同じ答えを引けること（`contested_set`）
        let carried = contested_set(
            entries
                .iter()
                .zip(contested_flags(&entries))
                .filter(|(_, f)| *f)
                .map(|((n, _, _), _)| n.as_str()),
        );
        assert!(is_contested(&carried, "DSC00001.ARW"));
        assert!(is_contested(&carried, "dsc00001.arw"));
        assert!(!is_contested(&carried, "DSC00002.ARW"));
    }

    /// **同じ名前でも、見た目が違えばぶつかっていない**（2026-09-07・ゲート2）。
    ///
    /// `DCIM` が一周したカードや2台で使ったカードには、**同じ名前の別の写真**が並ぶ。
    /// 名前だけで数えると**全部が「分からない」**になり、入っているものが毎回
    /// `?` に戻って隠されず、既定で選び直され、**押すと全件の中身比べが走って
    /// `copied` は 0** になる。**行き先の1本がどちらのものか決められないのは、
    /// 2枚が [`looks_alike`] のときだけ**である。
    ///
    /// **緩める方向の間違いは許されない**ので、**1時間ずれ（夏時間）は
    /// ぶつかっている側**に数えること（[`looks_same`] と同じ目）まで見る。
    #[test]
    fn the_same_name_with_a_different_look_is_not_contested() {
        let base = 1_600_000_000_000;
        let entries: Vec<(String, u64, i64)> = vec![
            // 大きさが違う（圧縮RAW・JPEG。カードで最も普通の形）
            ("DSC00001.ARW".into(), 4, base),
            ("DSC00001.ARW".into(), 8, base),
            // 大きさは同じだが日が違う（非圧縮RAWを別の日に撮った）
            ("DSC00002.ARW".into(), 4, base),
            ("DSC00002.ARW".into(), 4, base + 86_400_000),
            // 大きさも時刻も同じ＝**見分けられない**
            ("DSC00003.ARW".into(), 4, base),
            ("DSC00003.ARW".into(), 4, base),
            // ちょうど1時間ずれ（夏時間）は**同じに見える側**
            ("DSC00004.ARW".into(), 4, base),
            ("DSC00004.ARW".into(), 4, base + 3_600_000),
            // FAT32 の2秒刻みも同じに見える側
            ("DSC00005.ARW".into(), 4, base),
            ("DSC00005.ARW".into(), 4, base + 1_500),
        ];
        assert_eq!(
            contested_flags(&entries),
            vec![false, false, false, false, true, true, true, true, true, true],
            "見分けられる組までぶつかっていると数えている（または見分けられない組を落としている）"
        );
    }

    /// **クラウドを理由に読みを省くのは、行き先の側だけ**（ゲート1の P1・2026-09-07）。
    ///
    /// ウィザードは**クラウドにしか実体が無い行も選ばせる**（☁ の印を出すだけ）。
    /// 取り込み元まで免除すると、`Bytes::Unknown` → 「同じ」と倒れて、
    /// **名前がぶつかっている別の写真が黙って入らない**。
    /// **取り寄せはどのみち走る**（飛ばさない限り `fs::copy` が実体化させる）ので、
    /// **読まずに済むのは飛ばすときだけ**——その判断のために読む。
    ///
    /// 本物のプレースホルダは同期クライアントしか作れないので、`cloud.rs` の試験と同じく
    /// **`OFFLINE` 属性で代用**する。**属性を立てても中身は読める**ので、
    /// 「免除しなければ読んで正しく答える」ことがそのまま出る。**Windows のみ。**
    #[cfg(windows)]
    #[test]
    fn only_the_destination_is_excused_from_reading() {
        use std::os::windows::ffi::OsStrExt;

        const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
        fn set_attr(path: &Path, attr: u32) {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let ok = unsafe {
                windows_sys::Win32::Storage::FileSystem::SetFileAttributesW(wide.as_ptr(), attr)
            };
            assert_ne!(ok, 0, "属性を立てられること");
        }

        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("2020-09-13");
        fs::create_dir_all(&dest_dir).unwrap();
        let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        let mtime_ms = 1_600_000_000_000;

        let existing = dest_dir.join("DSC00001.ARW");
        fs::write(&existing, b"aaaa").unwrap();
        filetime::set_file_mtime(&existing, stamp).unwrap();

        // **取り込み元がクラウドにしか無くても読む。** 中身が違うので連番へ回る
        let src = dir.path().join("DSC00001.ARW");
        fs::write(&src, b"bbbb").unwrap();
        filetime::set_file_mtime(&src, stamp).unwrap();
        set_attr(&src, FILE_ATTRIBUTE_OFFLINE);
        match resolve_dest_path_avoiding(
            &dest_dir,
            "DSC00001.ARW",
            4,
            mtime_ms,
            &TakenPaths::default(),
            Confirm::ByBytes(&src),
        ) {
            DestResolution::CopyTo(p) => assert_eq!(
                p.file_name().and_then(|n| n.to_str()),
                Some("DSC00001-1.ARW")
            ),
            _ => panic!("クラウドの取り込み元を読まずに「取り込み済み」と答えた"),
        }
        set_attr(&src, FILE_ATTRIBUTE_NORMAL);

        // **対照: 行き先の側は免除される。** 中身が違っても畳む
        set_attr(&existing, FILE_ATTRIBUTE_OFFLINE);
        assert!(
            matches!(
                resolve_dest_path_avoiding(
                    &dest_dir,
                    "DSC00001.ARW",
                    4,
                    mtime_ms,
                    &TakenPaths::default(),
                    Confirm::ByBytes(&src),
                ),
                DestResolution::AlreadyImported
            ),
            "行き先の取り寄せを走らせている（挿し直すたびに回線と容量を使う）"
        );
        set_attr(&existing, FILE_ATTRIBUTE_NORMAL);
    }

    /// **取り込み元が読めないのは「取り込み済み」ではない。**
    ///
    /// `Confirm::ByBytes` は読めなかったぶんを畳む。**行き先**が読めないときは
    /// それでよい（上の試験。畳まないと挿し直すたびにライブラリが2倍になる）。
    /// **取り込み元**が読めないとき——**カードを抜かれた・一時的な I/O の失敗**——に
    /// 同じことをすると、**一度も入っていない1枚を「取り込み済み」と報告する**。
    /// 畳まなければ連番へ回り、**コピーがそこで転んで `failed` に数えられる**。
    /// **入っていないものを、入ったことにしない。**
    ///
    /// **`TakenPaths::insert` の doc 自身が「書いた直後の RAW をウイルス対策や
    /// OneDrive が掴む」と書いている**ので、絵空事ではない。
    #[test]
    fn a_source_that_cannot_be_read_is_not_reported_as_imported() {
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("2020-09-13");
        fs::create_dir_all(&dest_dir).unwrap();
        let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);
        let mtime_ms = 1_600_000_000_000;
        let existing = dest_dir.join("DSC00001.ARW");
        fs::write(&existing, b"aaaa").unwrap();
        filetime::set_file_mtime(&existing, stamp).unwrap();

        // 取り込み元が在らない（＝抜かれた）。名前も大きさも時刻も合っているので、
        // **`looks_same` は通る**——止めるのは中身の側だけである
        let gone = dir.path().join("nowhere").join("DSC00001.ARW");
        match resolve_dest_path_avoiding(
            &dest_dir,
            "DSC00001.ARW",
            4,
            mtime_ms,
            &TakenPaths::default(),
            Confirm::ByBytes(&gone),
        ) {
            DestResolution::CopyTo(p) => assert_eq!(
                p.file_name().and_then(|n| n.to_str()),
                Some("DSC00001-1.ARW"),
                "連番の付け方がずれている"
            ),
            _ => panic!("取り込み元が読めないのに「取り込み済み」と答えた"),
        }
    }

    /// **「済」バッジと行き先が同じ規則で決まる**（ゲート1のP2）。
    ///
    /// 組の日付で入ったファイルを、判定側が自分のmtimeで探すと「未取り込み」に
    /// 見える——そこでもう一度取り込むと、同じ写真が別の日のフォルダへ**二重に**
    /// コピーされる。
    #[test]
    fn a_file_dated_through_its_pair_is_still_seen_as_imported() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        // JPGだけがEXIFを持ち、RAWは日付を持たない（mtimeは別の日）
        let jpg = src.join("IMG_0001.jpg");
        let raw = src.join("IMG_0001.dng");
        fs::write(&jpg, jpeg_with_taken_at("2019:08:11 12:00:00")).unwrap();
        fs::write(&raw, b"raw").unwrap();
        filetime::set_file_mtime(&raw, filetime::FileTime::from_unix_time(1_600_000_000, 0))
            .unwrap();

        let config = test_config(&dest);
        assert_eq!(
            is_already_imported(&raw, &config, &no_repeated_names()),
            ImportState::NotImported
        );
        import_from(&src, &config, |_, _, _| {}).unwrap();

        assert_eq!(
            is_already_imported(&raw, &config, &no_repeated_names()),
            ImportState::Imported,
            "組で決めた日付のフォルダを見ていない（もう一度コピーしてしまう）"
        );
        // 実際に二重コピーにならないことまで見る
        let again = import_files(
            std::slice::from_ref(&raw),
            &no_repeated_names(),
            &config,
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(again.copied, 0, "同じ写真をもう一度コピーした");
        assert_eq!(again.skipped, 1);
    }

    /// **RAW+JPGの組は同じ日のフォルダへ**（0.2・`dev/loadmap.md` 1.1）。
    /// 片方だけ撮影日時を持たないとき、組で日付をそろえないと散る。
    #[test]
    fn a_raw_and_jpg_pair_lands_in_the_same_folder() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        // 名前から日付が読める側（段階H-2の `namedate`）と、読めない側
        let dated = src.join("2019-08-11_120000.jpg");
        let undated = src.join("2019-08-11_120000.dng");
        fs::write(&dated, b"jpeg").unwrap();
        fs::write(&undated, b"raw").unwrap();
        // RAW側のmtimeだけ**別の日**にする（そろえないと別フォルダへ行く）
        filetime::set_file_mtime(&undated, filetime::FileTime::from_unix_time(0, 0)).unwrap();

        let config = test_config(&dest);
        import_from(&src, &config, |_, _, _| {}).unwrap();

        let dirs: std::collections::HashSet<String> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| e.path().parent().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(dirs.len(), 1, "組がフォルダをまたいで散っている: {dirs:?}");
    }

    #[test]
    fn same_name_with_a_different_size_avoids_the_clash_by_numbering() {
        let dir = tempfile::tempdir().unwrap();
        let src1 = dir.path().join("usb1");
        let src2 = dir.path().join("usb2");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src1).unwrap();
        fs::create_dir_all(&src2).unwrap();
        // 同じファイル名で中身が違う（別のカメラのDSC_0001.jpg想定）
        fs::write(src1.join("DSC_0001.jpg"), b"first").unwrap();
        fs::write(src2.join("DSC_0001.jpg"), b"second-longer").unwrap();

        let config = test_config(&dest);
        assert_eq!(import_from(&src1, &config, |_, _, _| {}).unwrap().copied, 1);
        assert_eq!(import_from(&src2, &config, |_, _, _| {}).unwrap().copied, 1);

        let names: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"DSC_0001.jpg".to_string()));
        assert!(names.contains(&"DSC_0001-1.jpg".to_string()));
    }

    #[test]
    fn a_file_with_no_mtime_lands_in_today_not_1970() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let f = src.join("broken.jpg");
        fs::write(&f, b"data").unwrap();
        // mtime=Unixエポック0（壊れたFATタイムスタンプ相当）
        filetime::set_file_mtime(&f, filetime::FileTime::from_unix_time(0, 0)).unwrap();

        let config = test_config(&dest);
        let stats = import_from(&src, &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 1);
        let year_dir = fs::read_dir(&dest).unwrap().next().unwrap().unwrap();
        let name = year_dir.file_name().to_string_lossy().into_owned();
        assert_ne!(name, "1970", "エポック0は日付として扱わない");
        assert_ne!(name, "1969");
    }

    #[test]
    fn the_copied_mtime_matches_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let f = src.join("old.jpg");
        fs::write(&f, b"data").unwrap();
        let past = filetime::FileTime::from_unix_time(1_600_000_000, 0); // 2020-09
        filetime::set_file_mtime(&f, past).unwrap();

        let config = test_config(&dest);
        assert_eq!(import_from(&src, &config, |_, _, _| {}).unwrap().copied, 1);
        let copied = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .find(|e| e.file_type().is_file())
            .unwrap();
        let copied_mtime =
            filetime::FileTime::from_last_modification_time(&copied.metadata().unwrap());
        assert_eq!(copied_mtime.unix_seconds(), past.unix_seconds());
    }

    #[test]
    fn no_destination_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let result = import_from(dir.path(), &config, |_, _, _| {});
        assert!(matches!(result, Err(ImportError::NoDestination)));
    }

    #[test]
    fn the_progress_callback_is_called() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        for i in 0..3 {
            fs::write(src.join(format!("p{i}.jpg")), b"x").unwrap();
        }
        let config = test_config(&dest);
        let calls = std::sync::Mutex::new(Vec::new());
        import_from(&src, &config, |done, total, path| {
            calls
                .lock()
                .unwrap()
                .push((done, total, path.file_name().unwrap().to_owned()));
        })
        .unwrap();
        let calls = calls.lock().unwrap();
        // 件数の進みと「いま処理したファイル」が毎回そろって届く
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls.iter().map(|(d, t, _)| (*d, *t)).collect::<Vec<_>>(),
            vec![(1, 3), (2, 3), (3, 3)]
        );
        let mut names: Vec<_> = calls
            .iter()
            .map(|(_, _, n)| n.to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["p0.jpg", "p1.jpg", "p2.jpg"]);
    }

    #[test]
    fn only_the_chosen_files_are_imported() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            fs::write(src.join(name), b"x").unwrap();
        }

        let config = test_config(&dest);
        // チェックを外した c.jpg は渡さない → コピーされない
        let picked = vec![src.join("a.jpg"), src.join("b.jpg")];
        let stats = import_files(&picked, &no_repeated_names(), &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.copied, 2);
        let names: Vec<_> = walkdir::WalkDir::new(&dest)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2);
        assert!(!names.contains(&"c.jpg".to_string()));
    }

    #[test]
    fn a_vanished_file_counts_as_a_failure_and_the_rest_carry_on() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.jpg"), b"x").unwrap();

        let config = test_config(&dest);
        let picked = vec![src.join("消えた.jpg"), src.join("a.jpg")];
        let stats = import_files(&picked, &no_repeated_names(), &config, |_, _, _| {}).unwrap();
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.copied, 1, "1件失敗しても後続は取り込む");
    }

    #[test]
    fn the_already_imported_check_agrees_with_the_import_itself() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("usb");
        let dest = dir.path().join("photos");
        fs::create_dir_all(&src).unwrap();
        let a = src.join("a.jpg");
        fs::write(&a, b"aaa").unwrap();

        let config = test_config(&dest);
        assert_eq!(
            is_already_imported(&a, &config, &no_repeated_names()),
            ImportState::NotImported,
            "取り込む前は未取り込み"
        );
        import_files(
            std::slice::from_ref(&a),
            &no_repeated_names(),
            &config,
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(
            is_already_imported(&a, &config, &no_repeated_names()),
            ImportState::Imported,
            "取り込んだ後は済"
        );
        // 「済」と出るファイルを再度取り込んでもコピーは増えない
        let again = import_files(
            std::slice::from_ref(&a),
            &no_repeated_names(),
            &config,
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(again.copied, 0);
        assert_eq!(again.skipped, 1);
    }

    #[test]
    fn with_no_destination_the_already_imported_check_is_false() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.jpg");
        fs::write(&f, b"x").unwrap();
        assert_eq!(
            is_already_imported(&f, &Config::default(), &no_repeated_names()),
            ImportState::NotImported
        );
    }

    #[test]
    fn every_preset_builds_a_safe_relative_path() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 12,
        };
        let rendered: Vec<String> = FOLDER_PATTERN_PRESETS
            .iter()
            .map(|p| render_folder_pattern(p, date))
            .collect();
        assert_eq!(rendered[0], "2026/2026-08-12", "既定は年/年-月-日");
        assert_eq!(rendered[1], "2026/08/12");
        assert_eq!(rendered[2], "2026/08-12");
        assert_eq!(rendered[3], "2026-08/2026-08-12");
        // 年の階層を自分で作る人向け（コピー先を年フォルダにして、この下に日付だけ）
        assert_eq!(rendered[4], "2026-08-12");
        assert_eq!(rendered[5], "08/2026-08-12");
        assert_eq!(rendered[6], "2026/08");
        assert_eq!(rendered[7], "2026");
        assert_eq!(rendered[8], "", "空パターンはコピー先直下");
        assert_eq!(rendered.len(), FOLDER_PATTERN_PRESETS.len());
        for r in &rendered {
            assert!(!Path::new(r).is_absolute(), "絶対パスにならない: {r}");
        }
    }

    #[test]
    fn a_dangerous_pattern_is_made_harmless() {
        let date = CivilDate {
            year: 2026,
            month: 8,
            day: 12,
        };
        // 設定ファイルを直接編集して壊せてしまうため、コピー先の外へ出さない
        assert_eq!(render_folder_pattern("../../{year}", date), "2026");
        assert_eq!(render_folder_pattern("/etc/{year}", date), "etc/2026");
        assert_eq!(render_folder_pattern(r"C:\evil\{year}", date), "evil/2026");
        assert_eq!(render_folder_pattern("{year}/../..", date), "2026");
        // ファイル名に使えない文字は落とす
        assert_eq!(render_folder_pattern("a*b?{year}", date), "ab2026");
        // Windowsで開けなくなる「末尾のドット・空白」も落とす
        assert_eq!(render_folder_pattern("{year}. / x ", date), "2026/x");
        // 全部落ちたら空（コピー先直下へ取り込む。止めるより失わない方を選ぶ）
        assert_eq!(render_folder_pattern("../..", date), "");
    }
}
