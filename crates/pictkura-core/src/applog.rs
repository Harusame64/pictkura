//! **失敗が利用者に届く道**（`dev/plan.completeness-week.md` 項目2）。
//!
//! `eprintln!` で書いた行は、**配布した Windows のアプリではどこにも出ない**
//! ——`src-tauri/src/main.rs` の `windows_subsystem = "windows"` により
//! リリースビルドはコンソールを持たない。開発中にターミナルから起動した
//! ときだけ読める。**つまり、配った先で起きた失敗は誰にも見えていない。**
//!
//! ここは**DBの隣に追記1本**を置いて、そこだけを直す。
//!
//! - **書くのは失敗した行だけ。** 起動した・取り込んだ・消したは書かない。
//!   **黙っているのが既定**で、ファイルが在ること自体が「何かあった」の印になる
//! - **通信はしない。** 送る仕掛けも、送る先も無い。行はその機械に残るだけで、
//!   利用者が設定から開き、要ると思ったぶんだけ自分で貼る
//! - **上限がある**（[`MAX_BYTES`]）。壊れたファイルが数万枚あるライブラリでは
//!   1枚1行が延々と出るので、際限なく太る道は塞いでおく
//! - **止めない。** 書けなくても失敗を返さない。**ログが書けないことを理由に
//!   アプリが止まるのが、いちばん要らない**
//!
//! **置き場は起動時に1回渡す**（[`set_file`]）。渡す前に呼ばれた `note` は
//! `stderr` にだけ出る——コアのライブラリは CLI のツールからも使われるし、
//! **Tauri の `app_data_dir()` を知っているのはアプリ側だけ**である。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 1本が太れる上限。超えたら `.1` へ送って書き直すので、**最大で2本ぶん**。
///
/// 1 MiB は、失敗の行（100 バイト前後）でおよそ1万件。**それだけ出ていれば
/// 原因は先頭で分かる**ので、これ以上を残す理由がない。
const MAX_BYTES: u64 = 1024 * 1024;

/// ログの置き場。**アプリの起動時に1回だけ決まる。**
static FILE: OnceLock<PathBuf> = OnceLock::new();

/// 書き込みの直列化と、**この起動の見出しを書いたか**（`true` なら書いた）。
///
/// 見出しを兼ねさせているのは、**同じ錠の中で決めないと二度書ける**ため。
static WROTE_HEADER: Mutex<bool> = Mutex::new(false);

/// ログの置き場を決める（アプリの起動時に1回）。
///
/// 2回目以降は**黙って捨てる**。置き場が途中で変わると、**同じ起動の記録が
/// 2つのファイルに割れる**——その分かりにくさを買う理由がない。
pub fn set_file(path: PathBuf) {
    let _ = FILE.set(path);
}

/// いまの置き場。まだ決まっていなければ `None`。
///
/// 設定画面の「ログを開く」が、**在るときだけ**押せるようにするために引く。
pub fn file() -> Option<&'static Path> {
    FILE.get().map(PathBuf::as_path)
}

/// 失敗を1行残す。**`stderr` にも今までどおり出す。**
///
/// 開発中はターミナルで読めるほうが速いので、片方に寄せない。
/// **失敗しても何も返さない**——呼ぶ側は既に失敗の処理をしている最中で、
/// そこへ**ログの失敗という第2の失敗**を渡しても行き場がない。
pub fn note(message: &str) {
    eprintln!("{message}");
    record(message);
}

/// ファイルにだけ残す（`stderr` へは出さない）。
///
/// パニックの網（[`install_panic_hook`]）が使う。**あちらは標準の掛け金を
/// そのまま呼び直す**ので、`stderr` には標準の書式が出る。ここでも出すと
/// **同じパニックがターミナルに2回**並ぶ。
fn record(message: &str) {
    let Some(path) = FILE.get() else { return };

    // **文字列は錠の外で作る**（見出しも、要らない周でも作る）。
    // 錠の中でパニックすると、[`install_panic_hook`] が**同じ錠を取りに来て
    // 自分を待つ**——`std` の `Mutex` は再入できないので、落ちる代わりに止まる。
    // 錠の中に残すのは**失敗を `Result` で返すファイル操作だけ**にして、その道を塞ぐ。
    // 見出しを1本ぶん余計に組む値段は、**書くのが失敗した行だけ**なので払える
    let line = format!("{} {}", stamp(), one_line(message));
    let header = header();

    let mut wrote_header = WROTE_HEADER.lock().unwrap_or_else(|e| e.into_inner());
    let size = size_of(path);
    // **退避してから、退避できたかを見て見出しを決める。** 太さから
    // 「これから作り直すはず」と当てにすると、**退避が失敗し続ける台で
    // 見出しが毎行付く**（ゲート2）——ファイルは倍の速さで太り、
    // 1件ごとに版の行が挟まる
    let rotated = rotate_if_full(path, size, MAX_BYTES);
    // **見出しと最初の1行は、1回で書く。** 別々に書くと、上限のすぐ手前で
    // 見出しが上限を跨がせ、**次に来た本文が、いま書いた見出しごと `.1` へ送る**
    // ——新しいファイルが見出しの無い行から始まる（ゲート1の指摘）
    let text = if needs_header(*wrote_header, size, rotated) {
        // 書けても書けなくても**印は付ける**。付けないと、書けない環境では
        // 1行ごとに見出しを試し続けることになる
        *wrote_header = true;
        format!("{header}\n{line}")
    } else {
        line
    };
    let _ = append(path, &text);
}

/// 見出しを付けるか。**この起動の1本目**と、**いま作り直したファイルの1本目**。
///
/// 「1起動に1本」だけで決めると、**上限で作り直したファイルには見出しが無くなる**
/// ——利用者が開いて貼るのは**その新しいほう**なので、**版もOSも書いていない記録**が
/// 報告に届く（ゲート2の指摘）。壊れたファイルが数万枚あるライブラリでは、
/// 1回の起動で上限に届きうる。
///
/// `Some(0)`（無い・空）も付ける側に入れる——**走っている最中に消された**ときに、
/// 作り直したファイルが本文から始まらないように。
/// **`None`（太さを訊けなかった）は付けない**——分からないことを
/// 「新しいファイルだ」と読むと、そこでも見出しが毎行付く。
fn needs_header(wrote_header: bool, size: Option<u64>, rotated: bool) -> bool {
    !wrote_header || rotated || size == Some(0)
}

/// いまの太さ。**無ければ 0、訊けなければ `None`。**
///
/// 「訊けなかった」を 0 と同じ扱いにしない。0 は「新しいファイル」の意味を
/// 持っていて、**見出しを付ける判断に使っている**からである。
fn size_of(path: &Path) -> Option<u64> {
    match fs::metadata(path) {
        Ok(m) => Some(m.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(0),
        Err(_) => None,
    }
}

/// 太っていたら**1本だけ残して**退避する。**実際に置き換えたときだけ `true`。**
///
/// 消さずに名前を替えるのは、**上限に当たった瞬間の直前が、たいてい知りたい所**
/// だからである。名前替えに失敗したらそのまま追記を続ける
/// ——**上限を守れないほうが、記録を失うよりまし**。
fn rotate_if_full(path: &Path, size: Option<u64>, max: u64) -> bool {
    if !size.is_some_and(|n| n >= max) {
        return false;
    }
    // **`rename` は行き先が在っても置き換える**（Unix の `rename(2)` と、
    // Windows の `MoveFileExW` に `MOVEFILE_REPLACE_EXISTING` を付けた呼び出し
    // ——`std` の `sys/fs/windows.rs`）。**2周目で失敗して上限が効かなくなる、
    // という道は無い**（ゲート1の P1。事実と違うので据え置いた）。
    // **ただし、開いている相手が居れば Windows では失敗しうる**ので、
    // 「置き換えたつもり」ではなく**置き換えた事実**を返す
    fs::rename(path, previous(path)).is_ok()
}

/// 追記する。**渡された文字列は改行ごと1回で書く**——見出しと最初の1行のように、
/// **離れては困る組**を呼ぶ側が畳んで渡せるようにするため。
///
/// **フォルダは作らない。** `--unregister-autoplay` は**アンインストーラが呼ぶ**
/// ので、そこで失敗を書こうとして `%APPDATA%\<identifier>` を作り直すと、
/// **消している最中のフォルダが残骸として戻る**（ゲート2の指摘）。
/// 置き場が無いなら書かない——`open` がそう言って返る。
fn append(path: &Path, line: &str) -> std::io::Result<()> {
    // **開きっぱなしにしない**（1行ごとに開いて閉じる）。開いたまま持つと、
    // **Windowsでは利用者がログを消せず**、**退避の `rename` も自分で塞ぐ**。
    // 書くのは失敗した行だけなので、この値段は払える（ゲート2の指摘・据え置き）
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

/// **捕まえていないパニックも記録に残す**（アプリの起動時に1回）。
///
/// [`crate::panics::catching`] が張ってあるのは**解読器を通る道だけ**である。
/// それ以外のスレッドで落ちると、**配布ビルドではそのスレッドが黙って
/// 死ぬだけ**——画面には何も出ず、`stderr` はどこにも無い。
///
/// **標準の掛け金は外さずに呼び直す。** 外すと、開発中のターミナルから
/// いつもの書式とバックトレースの案内が消える。
///
/// 網に掛かるパニックは**2行になる**（ここの1行と `catching` の1行）。
/// 承知のうえで、**言っていることが違う**からそうしている
/// ——ここは**ソースのどこで**落ちたか、あちらは**どのファイルで**落ちたか。
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "場所不明".to_string());
        record(&format!(
            "パニック（{at}）: {}",
            describe_panic(info.payload())
        ));
        previous(info);
    }));
}

/// パニックの中身を人が読める形にする。
///
/// `panic!("...")` の文字列は `&str` か `String` のどちらかで届く。
/// それ以外（`panic_any`）は型が分からないので、そう書く。
pub fn describe_panic(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "（内容の分からないパニック）".to_string()
    }
}

/// この起動の見出し。**版と OS が要る**——利用者から届く行は、
/// たいてい**どの版で起きたかが書かれていない**。
///
/// 起動のたびに1本入るので、**どこからが今回か**の区切りにもなる。
fn header() -> String {
    format!(
        "{} --- pictkura {} ({}) ---",
        stamp(),
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    )
}

/// 行の頭に置く時刻。**地域つき**にする——届いた行を Windows のイベントログや
/// Defender の記録と突き合わせるとき、時差が分からないと並べられない。
fn stamp() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f %:z")
        .to_string()
}

/// 改行を潰して**1件を1行**にする。
///
/// パニックの中身は複数行のことがあり、そのまま流すと**1件が10行に見える**。
/// 数えるときに効くので、ここで畳んでおく。
fn one_line(message: &str) -> String {
    message
        .split(['\n', '\r'])
        .map(str::trim_end)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 1つ前のログの名前（`pictkura.log` → `pictkura.log.1`）。
///
/// 拡張子を**置き換えない**のが要点で、`with_extension` を使うと
/// `pictkura.1` になり、`.log` として開ける関連付けから外れる。
fn previous(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".1");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "1本目").unwrap();
        append(&path, "2本目").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "1本目\n2本目\n");
    }

    /// **無いフォルダは作らない。** アンインストーラの途中で書こうとしても、
    /// 消しているフォルダを戻さない（ゲート2の指摘）。
    #[test]
    fn a_missing_folder_is_not_created_on_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("もう無いフォルダ");
        assert!(append(&gone.join("pictkura.log"), "書けない").is_err());
        assert!(!gone.exists(), "書けなかったフォルダを作っていないこと");
    }

    /// 上限に当たったら**1本だけ残して**置き換える（2本より増えない）。
    #[test]
    fn passing_the_cap_moves_the_old_one_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "むかしの行").unwrap();
        assert!(rotate_if_full(&path, size_of(&path), 8));
        append(&path, "いまの行").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "いまの行\n");
        assert_eq!(fs::read_to_string(previous(&path)).unwrap(), "むかしの行\n");

        // もう1周しても、増えるのではなく**古いほうが押し出される**
        assert!(rotate_if_full(&path, size_of(&path), 8));
        append(&path, "つぎの行").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "つぎの行\n");
        assert_eq!(fs::read_to_string(previous(&path)).unwrap(), "いまの行\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    /// **まだ細いファイルは触らない**——`false` は「置き換えていない」の意味で、
    /// 見出しを付けるかの判断がそれに乗っている。
    #[test]
    fn a_log_below_the_cap_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "まだ細い").unwrap();
        assert!(!rotate_if_full(&path, size_of(&path), MAX_BYTES));
        assert!(!previous(&path).exists());
        // 太さを訊けなかったときも触らない
        assert!(!rotate_if_full(&path, None, 8));
    }

    #[test]
    fn a_file_that_is_not_there_is_zero_long_but_an_unreadable_one_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(size_of(&dir.path().join("まだ無い")), Some(0));
        let path = dir.path().join("pictkura.log");
        append(&path, "1行").unwrap();
        assert_eq!(size_of(&path), Some("1行\n".len() as u64));
    }

    /// 見出しは**この起動の1本目**と、**いま作り直したファイルの1本目**に付く。
    #[test]
    fn a_fresh_file_gets_a_header_even_mid_run() {
        // まだ何も書いていない起動
        assert!(needs_header(false, Some(0), false));
        assert!(needs_header(false, Some(4), false));
        // 書いたあとの、まだ余裕のあるファイル
        assert!(!needs_header(true, Some(4), false));
        // いま作り直したファイルと、消されたファイル
        assert!(needs_header(true, Some(0), true));
        assert!(needs_header(true, Some(0), false));
        // **退避に失敗して太いままの台**では、見出しを毎行付けない（ゲート2）
        assert!(!needs_header(true, Some(9_999_999), false));
        // **太さを訊けなかったとき**も付けない
        assert!(!needs_header(true, None, false));
    }

    #[test]
    fn the_old_name_keeps_the_log_extension() {
        assert_eq!(
            previous(Path::new("/tmp/pictkura.log")),
            PathBuf::from("/tmp/pictkura.log.1")
        );
    }

    #[test]
    fn the_panic_payload_is_read_as_text_whichever_way_it_came() {
        assert_eq!(describe_panic(&"借り物の文字列"), "借り物の文字列");
        assert_eq!(
            describe_panic(&"持ち物の文字列".to_string()),
            "持ち物の文字列"
        );
        assert_eq!(describe_panic(&7u8), "（内容の分からないパニック）");
    }

    #[test]
    fn a_panic_spanning_lines_is_folded_into_one() {
        assert_eq!(one_line("壊れた\r\nファイル\n"), "壊れた / ファイル");
        assert_eq!(one_line("1行のまま"), "1行のまま");
    }

    /// 置き場が決まっていなくても落ちない（CLIのツールやテストから呼ばれる道）。
    #[test]
    fn without_a_place_to_write_it_only_goes_to_stderr() {
        // このテスト用バイナリで `set_file` を呼ぶ者は居ない（呼ぶのはアプリ側だけ）。
        // 誰かが呼び始めたら、この行が先に落ちて教えてくれる
        assert!(file().is_none());
        note("置き場が無くても落ちない");
    }
}
