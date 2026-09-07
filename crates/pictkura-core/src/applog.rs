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
//!   ——だから**「何か」の中身を選ぶ**:
//!   - **機械が転んだものは書く。** 画面に出たものも含めて
//!     （`errs::from_err` が不具合と判じたもの・2026-09-07）。**報告と突き合わせられる**
//!   - **利用者の選び間違いは書かない。** 無いフォルダを選んだ、管理された
//!     ライブラリを指した——**選び直せば済む**話で、これを書くと
//!     **設定をいじっただけでファイルができる**
//!   - **画面で分かる失敗も書かない。** サムネイルが1枚出ないのはその場に灰色で出るし、
//!     `ThumbError` には「OSにデコーダが無い」まで混ざるので、全部書くと
//!     **本当に見たい失敗が押し出される**
//! - **通信はしない。** 送る仕掛けも、送る先も無い。行はその機械に残るだけで、
//!   利用者が設定から開き、要ると思ったぶんだけ自分で貼る
//! - **文は英語で書く**（2026-09-07・利用者の判断）。**画面は利用者の言語、
//!   記録は報告の言語**——貼られた先（GitHub の issue）でそのまま読める。
//!   コミットとPRを英語で書くのと同じ位置づけである。
//!   **OSや外のクレートの文言は、そのOSの言語のまま混ざる**（訳せない）
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
use std::time::{Duration, Instant};

/// 1本が太れる上限。超えたら `.1` へ送って書き直すので、**最大で2本ぶん**。
///
/// 1 MiB は、失敗の行（100 バイト前後）でおよそ1万件。**それだけ出ていれば
/// 原因は先頭で分かる**ので、これ以上を残す理由がない。
const MAX_BYTES: u64 = 1024 * 1024;

/// 1行が伸びられる上限（バイト）。**これを越えたら切って「以下略」を付ける。**
///
/// 4 KiB は、いちばん長いパスと、そこに付く説明が丸ごと入る長さである。
const MAX_LINE: usize = 4096;

/// ログの置き場。**アプリの起動時に1回だけ決まる。**
static FILE: OnceLock<PathBuf> = OnceLock::new();

/// 書くときの決めごと。**1つの錠の中で決めて、その中で書く。**
///
/// **2つに分けると順番が壊れる**（ゲート1の指摘）——畳み込みの判断だけ先に錠を取り、
/// 書くのは別の錠、という形にしていたので、**別のスレッドが間に割り込むと
/// 「上の行が N 回」の主張が嘘になる**。判断と書き込みを離さない。
struct LogState {
    /// **この起動の見出しを書いたか**（`true` なら書いた）。
    wrote_header: bool,
    /// **直前に書けた行**と、そのあと畳んだ回数（[`Repeat`]）。
    repeated: Option<Repeat>,
}

/// 続いている同じ行の控え。
///
/// 同じ失敗が続けて出る道がある——DBが詰まっているあいだ、画面の要求が
/// 何度も同じ失敗で返るなど。1件1行で書くと、**上限のログが1種類で埋まり、
/// 本当に見たい失敗が押し出される**（ゲート2）。**続いた分は数えて、
/// 別の行が来たときに1行で言う。**
struct Repeat {
    /// 畳む対象の行。**書けた行だけがここに入る**——書けなかった行を入れると、
    /// **一時的に書けなかっただけの失敗が、以後ずっと畳まれて消える**
    /// （PRのcodex の指摘）
    line: String,
    /// そのあと黙った回数。
    count: u64,
    /// **最後に何かを書いた時刻。** これが無いと、**同じ失敗が続いたまま
    /// アプリが終わったときに、記録が「1回だけ起きた」と嘘をつく**
    /// ——固まったアプリを利用者が落とすのは、まさにその形である（ゲート2）。
    ///
    /// **畳むのは、記録が在るあいだだけ**である——利用者が消したら畳まずに書き直す
    /// （説明書が「消してよい」と言っている。PRのcodex）
    since: Instant,
}

/// 同じ行が続いていても、**これだけ黙ったら1行書く**。
///
/// **記録が黙っていられる上限**である。詰まったDBで4000回続いても1分ごとに
/// 「まだ続いている」と言うので、**強制終了されても失われるのは最後の1分ぶん**になる。
const FLUSH_AFTER: Duration = Duration::from_secs(60);

static STATE: Mutex<LogState> = Mutex::new(LogState {
    wrote_header: false,
    repeated: None,
});

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

    // **文字列は錠の外で作る**。錠の中でパニックすると、[`install_panic_hook`] が
    // **同じ錠を取りに来て自分を待つ**——`std` の `Mutex` は再入できないので、
    // 落ちる代わりに止まる。錠の中に残すのは**失敗を `Result` で返すファイル操作**と、
    // 数を数える `format!` だけにして、その道を塞ぐ。
    // **時刻は1つで足りる**——畳んだ数の行と本文は、同じ瞬間に書かれる
    let folded = one_line(message);
    let now = stamp();
    let header = header();
    let dropped_note = format!(
        "{now} the previous record could not be moved aside, so it was dropped to keep the cap"
    );

    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());

    // **記録そのものが消えていたら、畳まない。** 説明書は「消しても構いません
    // （また必要になれば作られます）」と約束している——畳んだままだと、
    // **消した直後に同じ失敗が起きても、どこにも残らない**（PRのcodex）。
    // `stat` 1回は、追記1回よりずっと安い
    let gone = size_of(path) == Some(0);

    // **同じ行が続いたら、書かずに数える**——ただし**黙りっぱなしにはしない**
    if let Some(rep) = st.repeated.as_mut() {
        if rep.line == folded && !gone {
            rep.count += 1;
            if rep.since.elapsed() < FLUSH_AFTER {
                return;
            }
            // 1分ぶん黙った。**まだ続いていることを1行で言って、数え直す**
            // **どの行だったかを書く。** 「上の行」は、退避や削除のあとには
            // **もう上に無い**——文脈の無い要約が1分ごとに並ぶだけになる
            // （PRのcodex）。別の行が来たときの枝と同じ形にそろえる
            let said = format!(
                "(repeated {} more times, still going): {}",
                rep.count, rep.line
            );
            // **数え直すのは書いたあと。** `write_line` は `st` ごと要るので、
            // ここで `rep` の借りを終える
            let wrote = write_line(&mut st, path, &now, &header, &dropped_note, &said);
            if let Some(rep) = st.repeated.as_mut() {
                after_flush(rep, wrote);
            }
            return;
        }
    }

    // 別の行が来た。**黙っていた分を、先に1行で言う**
    if let Some(rep) = st.repeated.take() {
        if rep.count > 0 {
            let said = format!(
                "(the line above repeated {} more times): {}",
                rep.count, rep.line
            );
            write_line(&mut st, path, &now, &header, &dropped_note, &said);
        }
    }
    // **書けたときだけ畳み始める。** 書けなかった行を覚えると、
    // **書けるようになっても、同じ失敗は二度と記録されない**（PRのcodex）
    if write_line(&mut st, path, &now, &header, &dropped_note, &folded) {
        st.repeated = Some(Repeat {
            line: folded,
            count: 0,
            since: Instant::now(),
        });
    }
}

/// **抱えたままの数を、いま吐き出す。**（終わるときに1回だけ呼ぶ）
///
/// 畳み込みは**次を待って**数を世に出す——同じ失敗がもう一度来るか、別の行が来るか。
/// **どちらも来ないまま終わる道が在る**: 同じ失敗が60秒のうちに何千回か起きて、
/// **そこで止まった**とき。時計を持った番人は居ないので、
/// **その数はプロセスと一緒に消える**——記録には「1回起きた」だけが残り、
/// **千回だったことは誰も知らない**（PRのcodex）。
///
/// **落とされたとき（強制終了・パニックの直後）までは救えない。**
/// それでも**1本目の行は既に書かれている**ので、失敗そのものは残る。
/// ここが救うのは**数のほう**である。
pub fn flush_pending() {
    let Some(path) = FILE.get() else { return };

    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(rep) = st.repeated.take() else {
        return;
    };
    if rep.count == 0 {
        return; // 抱えていない。**空のファイルを作らない**
    }
    // **文字列は錠の中で作っている**——ここは終わり際の1回きりで、
    // `record` と違って**この道からパニックの網へ入る流れが無い**
    let now = stamp();
    let header = header();
    let dropped_note = format!(
        "{now} the previous record could not be moved aside, so it was dropped to keep the cap"
    );
    let said = format!(
        "(the line above repeated {} more times): {}",
        rep.count, rep.line
    );
    write_line(&mut st, path, &now, &header, &dropped_note, &said);
}

/// 周期の要約を出したあとの、畳み込みの立て直し。
///
/// **数え直すのは、言えたときだけ。** 先に 0 へ戻すと、**書けなかった1分ぶんが
/// どこにも出ないまま消える**——記録は「4312回」を「1回」と言い、
/// **続いている不具合を小さく見せる**（PRのcodex）。
///
/// **時計のほうは、書けても書けなくても進める。** 言えたときだけ進めると、
/// 書けない台では**繰り返しが来るたびに開きに行く**
/// ——「1分に1回」の約束が、**失敗の数だけの `open`** に化ける。
fn after_flush(rep: &mut Repeat, wrote: bool) {
    rep.since = Instant::now();
    if wrote {
        rep.count = 0;
    }
}

/// 1行を、上限と見出しの面倒を見ながら書く（[`record`] の中身）。**書けたら `true`。**
///
/// **錠は呼ぶ側が握っている**——判断と書き込みを離さないため。
fn write_line(
    st: &mut LogState,
    path: &Path,
    now: &str,
    header: &str,
    dropped_note: &str,
    folded: &str,
) -> bool {
    let line = format!("{now} {folded}");
    // **これから書く分まで見て**上限を判断する。太さだけで決めると、
    // **最後の1行が上限を越えたまま残る**（ゲート1の指摘）。
    // **書きうる3行を全部数える**——見出しも、捨てたことわりも
    // （数え漏らすとその分だけ越える・ゲート2）
    let wanted = (header.len() + dropped_note.len() + line.len() + 3) as u64;

    let size = size_of(path);
    // **先に片付けてから、その結果を見て見出しを決める。** 太さから
    // 「これから作り直すはず」と当てにすると、**退避が失敗し続ける台で
    // 見出しが毎行付く**（ゲート2）——ファイルは倍の速さで太り、
    // 1件ごとに版の行が挟まる
    let rotation = make_room(path, size, wanted, MAX_BYTES);

    // **見出しと最初の1行は、1回で書く。** 別々に書くと、上限のすぐ手前で
    // 見出しが上限を跨がせ、**次に来た本文が、いま書いた見出しごと `.1` へ送る**
    // ——新しいファイルが見出しの無い行から始まる（ゲート1の指摘）
    let mut text = String::new();
    let with_header = needs_header(st.wrote_header, size, rotation);
    if with_header {
        text.push_str(header);
        text.push('\n');
    }
    if rotation == Rotation::Dropped {
        // **捨てたことは、捨てた場所に書く。** 黙って消すと、
        // 「前の行はどこへ行った」に答えられない
        text.push_str(dropped_note);
        text.push('\n');
    }
    text.push_str(&line);

    // **書けたときだけ「見出しを書いた」ことにする。** 先に印を付けると、
    // 1本目が書けなかった台で**この起動の行が、前の起動の見出しの下に並ぶ**
    // ——読む人は**前の版の記録だと読む**（PRのcodex）
    let wrote = append(path, &text).is_ok();
    if wrote && with_header {
        st.wrote_header = true;
    }
    wrote
}

/// 太った記録の片付け方（[`make_room`] の答え）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Rotation {
    /// まだ細い。何もしていない
    NotNeeded,
    /// `.1` へ送った。前の記録は読める
    Moved,
    /// **送れなかったので、その場で空にした。** 前の記録は失われる
    Dropped,
    /// **どちらもできなかった。** 上限を越えたまま書き足す
    Stuck,
}

/// 上限に届いていたら、次の行を書く場所を空ける。
///
/// **送るのが第一。** 消さずに名前を替えるのは、**上限に当たった瞬間の直前が、
/// たいてい知りたい所**だからである。
///
/// **送れなかったら、その場で空にする。** Windowsでは、**利用者が開いている
/// ログは名前を替えられない**（`FILE_SHARE_DELETE` を付けない閲覧器は珍しくない）
/// ——そして**「ログを開く」は説明書が勧めている操作**である。
/// そこで諦めて追記を続けると、**上限は上限でなくなる**（ゲート2の指摘）。
/// **書いてある約束のほうを守る**——古い記録を捨てる代わりに、
/// [`Rotation::Dropped`] を返して**捨てたと書かせる**。
fn make_room(path: &Path, size: Option<u64>, wanted: u64, max: u64) -> Rotation {
    // **空のファイルは退避しない**——1行が単独で上限より長くても、
    // 送る先に入れるものが無い（長さそのものは [`one_line`] が抑えている）
    if !size.is_some_and(|n| n > 0 && n.saturating_add(wanted) > max) {
        return Rotation::NotNeeded;
    }
    // **`rename` は行き先が在っても置き換える**（Unix の `rename(2)` と、
    // Windows の `MoveFileExW` に `MOVEFILE_REPLACE_EXISTING` を付けた呼び出し
    // ——`std` の `sys/fs/windows.rs`）。**2周目で失敗して上限が効かなくなる、
    // という道は無い**（ゲート1の P1。事実と違うので据え置いた）。
    // **ただし、開いている相手が居れば Windows では失敗しうる**ので、
    // 「置き換えたつもり」ではなく**置き換えた事実**を返す
    if fs::rename(path, previous(path)).is_ok() {
        return Rotation::Moved;
    }
    match OpenOptions::new().write(true).truncate(true).open(path) {
        Ok(_) => Rotation::Dropped,
        // 開くことすらできない台では、**書けないだけ**で終わる（下の `append` が断る）
        Err(_) => Rotation::Stuck,
    }
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
fn needs_header(wrote_header: bool, size: Option<u64>, rotation: Rotation) -> bool {
    !wrote_header || matches!(rotation, Rotation::Moved | Rotation::Dropped) || size == Some(0)
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
            .unwrap_or_else(|| "unknown location".to_string());
        record(&format!("panic ({at}): {}", describe_panic(info.payload())));
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
        "(a panic whose contents could not be read)".to_string()
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
    let folded = message
        .split(['\n', '\r'])
        .map(str::trim_end)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    if folded.len() <= MAX_LINE {
        return folded;
    }
    // **1件で上限を食い潰させない。** パニックの中身は長さの当てが無く
    // （丸ごとの文字列が payload に乗ることがある）、1行が上限を越えれば
    // **その1件で記録が全部押し出される**
    let mut end = MAX_LINE;
    while !folded.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (truncated)", &folded[..end])
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
        assert_eq!(make_room(&path, size_of(&path), 1, 8), Rotation::Moved);
        append(&path, "いまの行").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "いまの行\n");
        assert_eq!(fs::read_to_string(previous(&path)).unwrap(), "むかしの行\n");

        // もう1周しても、増えるのではなく**古いほうが押し出される**
        assert_eq!(make_room(&path, size_of(&path), 1, 8), Rotation::Moved);
        append(&path, "つぎの行").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "つぎの行\n");
        assert_eq!(fs::read_to_string(previous(&path)).unwrap(), "いまの行\n");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    /// **言えなかった要約は、数えを持ったまま次へ回す。**
    ///
    /// 書けない台（ログを別のソフトが握っている・置き場が消えた）で数え直すと、
    /// **その1分ぶんは二度と出てこない**。続いている不具合が、記録の上では
    /// 1分ごとに「1回」ずつ起きているように見える。
    #[test]
    fn a_summary_that_could_not_be_written_keeps_its_count() {
        // **起動から1分未満の台では過去を作れない**（`Instant` は起動からの単調な時計）。
        // その台では時計の側の確認だけ飛ばす——数えの側は必ず見る
        let long_ago = Instant::now().checked_sub(FLUSH_AFTER * 2);
        let mut rep = Repeat {
            line: "同じ失敗".to_string(),
            count: 7,
            since: long_ago.unwrap_or_else(Instant::now),
        };

        after_flush(&mut rep, false);
        assert_eq!(rep.count, 7, "書けなかった1分ぶんを捨てないこと");
        if long_ago.is_some() {
            assert!(rep.since.elapsed() < FLUSH_AFTER, "次に言うのは1分後");
        }

        // 言えたら、そこで数え直す
        after_flush(&mut rep, true);
        assert_eq!(rep.count, 0);
    }

    /// **まだ細いファイルは触らない**——`NotNeeded` は「何もしていない」の意味で、
    /// 見出しを付けるかの判断がそれに乗っている。
    #[test]
    fn a_log_below_the_cap_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "まだ細い").unwrap();
        assert_eq!(
            make_room(&path, size_of(&path), 1, MAX_BYTES),
            Rotation::NotNeeded
        );
        assert!(!previous(&path).exists());
        // 太さを訊けなかったときも触らない
        assert_eq!(make_room(&path, None, 1, 8), Rotation::NotNeeded);
    }

    /// **送れない相手でも、上限は守る。** `.1` の側を「送れない場所」に
    /// 仕立てて（同じ名前のフォルダを置く）、その場で空になることを見る。
    ///
    /// Windows で本当に起きるのは「利用者がログを開いている」場合だが、
    /// **`rename` が失敗したあとの道は同じ**である。
    #[test]
    fn a_log_that_cannot_be_moved_aside_is_emptied_instead() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "捨てられる行").unwrap();
        // `.1` の名前をフォルダが押さえていると、ファイルは被せられない
        fs::create_dir(previous(&path)).unwrap();

        assert_eq!(make_room(&path, size_of(&path), 1, 8), Rotation::Dropped);
        assert_eq!(size_of(&path), Some(0), "上限を越えたまま残っていないこと");
        append(&path, "あとの行").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "あとの行\n");
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
        assert!(needs_header(false, Some(0), Rotation::NotNeeded));
        assert!(needs_header(false, Some(4), Rotation::NotNeeded));
        // 書いたあとの、まだ余裕のあるファイル
        assert!(!needs_header(true, Some(4), Rotation::NotNeeded));
        // いま作り直したファイル（送った・その場で空にした）と、消されたファイル
        assert!(needs_header(true, Some(0), Rotation::Moved));
        assert!(needs_header(true, Some(9_999_999), Rotation::Dropped));
        assert!(needs_header(true, Some(0), Rotation::NotNeeded));
        // **どちらもできず太いままの台**では、見出しを毎行付けない（ゲート2）
        assert!(!needs_header(true, Some(9_999_999), Rotation::Stuck));
        // **太さを訊けなかったとき**も付けない
        assert!(!needs_header(true, None, Rotation::NotNeeded));
    }

    /// **これから書く分で越えるなら、書く前に退避する**（ゲート1の指摘）。
    /// 太さだけで決めると、最後の1行が上限を越えたまま残る。
    #[test]
    fn the_line_about_to_be_written_counts_towards_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.log");
        append(&path, "6バイト").unwrap(); // 1 + 3*3 + 1 = 11 バイト
        let size = size_of(&path);
        assert_eq!(size, Some(11));
        // まだ入る
        assert_eq!(make_room(&path, size, 4, 16), Rotation::NotNeeded);
        // これを書くと越える
        assert_eq!(make_room(&path, size, 6, 16), Rotation::Moved);
    }

    /// **1件で記録を押し出させない。** 長すぎる行は切って「以下略」を付ける。
    #[test]
    fn a_line_too_long_is_cut_at_a_character_boundary() {
        let long = "あ".repeat(MAX_LINE); // 3 * MAX_LINE バイト
        let cut = one_line(&long);
        assert!(cut.len() <= MAX_LINE + "… (truncated)".len());
        assert!(cut.ends_with("… (truncated)"));
        // 切った先が文字の途中でないこと（`String` である時点で保証されるが、
        // **切り方を変えたときに気付ける**ように見ておく）
        assert!(cut.starts_with("あああ"));
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
        assert_eq!(
            describe_panic(&7u8),
            "(a panic whose contents could not be read)"
        );
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
