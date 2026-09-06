//! **カメラのカードをそのまま挿したときに、RAW が1枚も落ちない**ことを確かめる。
//!
//! 取り込みの単体テストは `import.rs` の中にあるが、あちらは `IMG_0001.dng` の
//! ような**小文字・平置き**で組み立てている。**実物はそうではない**——カメラは
//! `DCIM/100OLYMP/P1010026.ORF` のように**大文字の拡張子**で、**入れ子**に書く。
//! 判定は `eq_ignore_ascii_case` なので通るはずだが、**通ることを押さえた試験が
//! 無かった**（2026-09-06）。
//!
//! ここが落ちるということは、**利用者がカードを挿しても RAW が入らない**か、
//! **ウィザードの一覧に出ない**ということである。`.ori` を足した #113 のように
//! **拡張子の一覧を触る変更**で効く。
//!
//! 見ているのは3つ——**丸ごと取り込む経路**（`import_from`）、
//! **「中を全部さらう」経路**（`list_tree`）、**1階層ずつ辿る経路**（`list_dir`）。
//! この3つは別々の関数なので、**片方だけ直して片方を忘れる**ことがありうる。

// テストを組み立てる側（一時ファイルの書き出し等）の `unwrap()` は許す。
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use pictkura_core::{import_from, Config};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// カメラが実際に書く形。**拡張子は大文字**、**`DCIM` の下**、**RAW と JPG は組**。
///
/// `.ORI` はハイレゾ撮影の2本目（`.ORF`・`.ORI`・`.JPG` の3本が1回のシャッターで
/// 出る）。`PRIVATE/M4ROOT` は Sony の動画の置き場で、`DCIM` の外にある。
const ON_THE_CARD: &[(&str, &str)] = &[
    ("DCIM/100OLYMP", "P1010026.ORF"),
    ("DCIM/100OLYMP", "P1010026.ORI"),
    ("DCIM/100OLYMP", "P1010026.JPG"),
    ("DCIM/100CANON", "IMG_0001.CR3"),
    ("DCIM/100CANON", "IMG_0001.JPG"),
    ("DCIM/100MSDCF", "DSC00001.ARW"),
    // 小文字が混ざることもある（PCで触ったカード）
    ("DCIM/101MSDCF", "DSC00002.arw"),
    ("DCIM/100_FUJI", "DSCF0001.RAF"),
    ("DCIM/100ND750", "_DSC0001.NEF"),
    ("PRIVATE/M4ROOT/CLIP", "C0001.MP4"),
];

fn lay_out_a_card(card: &Path) {
    for (sub, name) in ON_THE_CARD {
        fs::create_dir_all(card.join(sub)).unwrap();
        fs::write(card.join(sub).join(name), b"x").unwrap();
    }
}

fn extensions() -> Vec<String> {
    pictkura_core::config::DEFAULT_EXTENSIONS
        .iter()
        .map(|e| (*e).to_string())
        .collect()
}

/// **フォルダ丸ごとの取り込みで、カードの中身が1枚も落ちない。**
#[test]
fn every_raw_on_the_card_is_copied() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    lay_out_a_card(&card);

    // カードには消したファイルの置き場がある。**そこは拾わない**
    // （拾うと、利用者が消したはずの写真が戻ってくる）
    fs::create_dir_all(card.join(".Trashes")).unwrap();
    fs::write(card.join(".Trashes/deleted.CR3"), b"x").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());

    let stats = import_from(&card, &config, |_, _, _| {}).unwrap();

    let copied: BTreeSet<String> = walkdir::WalkDir::new(&dest)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    for (_, name) in ON_THE_CARD {
        assert!(copied.contains(*name), "{name} が入っていない: {copied:?}");
    }
    assert!(
        !copied.contains("deleted.CR3"),
        "消したファイルの置き場まで拾っている"
    );
    assert_eq!(stats.copied, ON_THE_CARD.len(), "件数が合わない");
    assert_eq!(stats.failed, 0);
}

/// **ウィザードの一覧にも、同じものが全部出る。**
///
/// 取り込めても一覧に出なければ、利用者は選べない——**「USBに写真が無い」と
/// 同じ見え方**になる。
#[test]
fn every_raw_on_the_card_is_listed() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    lay_out_a_card(&card);
    let exts = extensions();

    // 「メディアの中を全部さらう」側
    let tree = pictkura_core::list_tree(&card, &exts, 20_000);
    let listed: BTreeSet<&str> = tree.files.iter().map(|f| f.name.as_str()).collect();
    assert!(!tree.truncated, "この枚数で打ち切られている");
    assert!(!tree.incomplete, "走査が最後まで届いていない");
    for (_, name) in ON_THE_CARD {
        assert!(listed.contains(name), "{name} が一覧に出ない: {listed:?}");
    }

    // 1階層ずつ辿る側。`DCIM` 直下に画像は無いが、下に階層はある
    let top = pictkura_core::browse::list_dir(&card, &exts);
    let top_dirs: Vec<&str> = top.dirs.iter().map(|d| d.name.as_str()).collect();
    assert!(top_dirs.contains(&"DCIM"), "DCIM が見えない: {top_dirs:?}");

    let hires = pictkura_core::browse::list_dir(&card.join("DCIM/100OLYMP"), &exts);
    let names: Vec<&str> = hires.files.iter().map(|f| f.name.as_str()).collect();
    for name in ["P1010026.ORF", "P1010026.ORI", "P1010026.JPG"] {
        assert!(names.contains(&name), "{name} が出ない: {names:?}");
    }
}

/// **同じカードをもう一度挿しても、二重にコピーしない。**
///
/// RAW は圧縮しない機種があり、**中身が違ってもサイズが同じ**になる。
/// ここが緩むと、**カード1枚が丸ごと二重に入る**か、逆に**別の写真を
/// 「同じもの」と見なして1枚落とす**。
#[test]
fn inserting_the_same_card_twice_copies_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    lay_out_a_card(&card);

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());

    let first = import_from(&card, &config, |_, _, _| {}).unwrap();
    assert_eq!(first.copied, ON_THE_CARD.len());

    let second = import_from(&card, &config, |_, _, _| {}).unwrap();
    assert_eq!(second.copied, 0, "同じカードをもう一度コピーした");
    assert_eq!(second.skipped, ON_THE_CARD.len(), "「済」と見なせていない");
}
