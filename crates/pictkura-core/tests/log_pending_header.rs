//! **書けなかった見出しは、まだ借りたまま**（PRのcodex の P2）。
//!
//! 置き場は**プロセスに1つ**（`OnceLock`）なので、`panic_log.rs` とは
//! **別の試験用バイナリ**に分けてある。
//!
//! 起こしているのは、**この起動の1本目が書けなかった**状況である
//! ——書けたことにして印を付けると、**次に書けた行が、前の起動の見出しの下**に
//! 並ぶ。読む人はそれを**前の版の記録**として読む。

use pictkura_core::applog;

#[test]
fn a_header_that_could_not_be_written_is_still_owed() {
    let dir = tempfile::tempdir().unwrap();
    // **まだ無いフォルダ**の中を指す。`applog` はフォルダを作らないので書けない
    // （アンインストールの途中でフォルダを戻さないため）
    let folder = dir.path().join("あとから出来るフォルダ");
    let path = folder.join("pictkura.log");
    applog::set_file(path.clone());

    applog::note("書けなかった失敗");
    assert!(!folder.exists(), "書けない側でフォルダを作っていないこと");

    // フォルダができたら、**次の1本目に見出しが付く**
    std::fs::create_dir(&folder).unwrap();
    applog::note("書けた失敗");

    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines = text.lines();
    let header = lines.next().unwrap();
    assert!(header.contains(env!("CARGO_PKG_VERSION")), "{header}");
    let body = lines.next().unwrap();
    assert!(body.contains("書けた失敗"), "{body}");
    assert!(lines.next().is_none(), "書けなかった行は残らない: {text}");
}
