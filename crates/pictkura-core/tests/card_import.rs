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

/// 行き先に入ったファイルの**中身**（名前ではなく中身で見ないと、
/// 「同じ名前の別物」が入れ替わっていても気づけない）。
fn contents(dest: &Path) -> BTreeSet<String> {
    walkdir::WalkDir::new(dest)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| String::from_utf8_lossy(&fs::read(e.path()).unwrap()).into_owned())
        .collect()
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

/// **カードの中でフォルダが分かれていても、同じ名前の2枚が両方入る。**
///
/// カメラは連番を戻せるし、9999枚で `DCIM` のフォルダが繰り上がるし、
/// 1枚のカードを2台で使うこともある。**`100MSDCF/DSC00001.ARW` と
/// `101MSDCF/DSC00001.ARW` は同じ日のフォルダへ来る。**
///
/// **非圧縮RAWは中身が違ってもサイズが同じ**なので、行き先が埋まっているかを
/// 名前と大きさだけで見ると、**2枚目が「取り込み済み」に化けて黙って消える**
/// ——`copied=1 / skipped=1 / failed=0` で、**成功したように見える。**
/// 2026-09-06 に実際にそうなっていた（ゲート2）。
#[test]
fn two_cards_folders_with_the_same_file_name_both_arrive() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    for sub in ["DCIM/100MSDCF", "DCIM/101MSDCF"] {
        fs::create_dir_all(card.join(sub)).unwrap();
    }
    // 同じ名前・同じ大きさ・中身は別物
    fs::write(card.join("DCIM/100MSDCF/DSC00001.ARW"), b"aaaa").unwrap();
    fs::write(card.join("DCIM/101MSDCF/DSC00001.ARW"), b"bbbb").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    let stats = import_from(&card, &config, |_, _, _| {}).unwrap();

    let bodies: BTreeSet<String> = walkdir::WalkDir::new(&dest)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| String::from_utf8_lossy(&fs::read(e.path()).unwrap()).into_owned())
        .collect();

    assert_eq!(
        stats.copied, 2,
        "2枚あったのに {} 枚しか運んでいない",
        stats.copied
    );
    assert_eq!(stats.skipped, 0, "片方を「取り込み済み」と読んでいる");
    assert!(
        bodies.contains("aaaa") && bodies.contains("bbbb"),
        "中身が両方そろっていない: {bodies:?}"
    );
}

/// **大文字小文字が違うだけの同名でも、両方入る。**
///
/// macOS も Windows も**既定で大文字小文字を畳む**ので、`DSC00001.ARW` を書いたあとの
/// `dsc00001.arw` は**同じファイルに当たる**。覚え書きを `Path` のまま持つと当たらず、
/// **2枚目が消える**。カードには大小が混ざる（PCで触ったカード）。
#[test]
fn the_same_name_in_a_different_case_still_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    for sub in ["DCIM/100MSDCF", "DCIM/101MSDCF"] {
        fs::create_dir_all(card.join(sub)).unwrap();
    }
    fs::write(card.join("DCIM/100MSDCF/DSC00001.ARW"), b"aaaa").unwrap();
    fs::write(card.join("DCIM/101MSDCF/dsc00001.arw"), b"bbbb").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    let stats = import_from(&card, &config, |_, _, _| {}).unwrap();

    assert_eq!(stats.copied, 2, "大小違いの2枚目が消えた");
    assert!(
        contents(&dest).contains("aaaa"),
        "1枚目が消えた: {:?}",
        contents(&dest)
    );
    assert!(
        contents(&dest).contains("bbbb"),
        "2枚目が消えた: {:?}",
        contents(&dest)
    );
}

/// **Unicode の正規化が違うだけの同名でも、両方入る。**
///
/// macOS（APFS）は **NFC と NFD を同じ物として引く**ので、`cafe\u{301}.jpg` を書いたあとの
/// `caf\u{e9}.jpg` は**同じファイルに当たる**。畳み方を自分の規則で当てにいくと
/// （小文字化だけでは）ここで外れて、**2枚目が消える**。
/// 畳まないファイルシステム（Linux）では**別のファイルなので、両方入るのが正しい**
/// ——どちらでも「2枚入る」が答えになる。
#[test]
fn the_same_name_in_a_different_normalisation_still_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    let dest = dir.path().join("photos");
    for sub in ["a", "b"] {
        fs::create_dir_all(src.join(sub)).unwrap();
    }
    // 見た目は同じ「café.jpg」。前者が NFD（e + 合成用アクセント）、後者が NFC
    fs::write(src.join("a").join("cafe\u{301}.jpg"), b"aaaa").unwrap();
    fs::write(src.join("b").join("caf\u{e9}.jpg"), b"bbbb").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    let stats = import_from(&src, &config, |_, _, _| {}).unwrap();

    let got = contents(&dest);
    assert_eq!(stats.copied, 2, "正規化違いの2枚目が消えた: {got:?}");
    assert!(
        got.contains("aaaa") && got.contains("bbbb"),
        "中身がそろっていない: {got:?}"
    );
}

/// **中身まで同じなら、1枚に畳む。**
///
/// カードに控えのフォルダがあることはある（利用者が作った backup、
/// ディスクイメージから戻したカード、カードへ丸ごと写したフォルダ）。
/// **同じ名前でぶつかったときに中身を見ないと、同じ写真が2枚に増える**
/// ——サイドカーも一緒に増える。見るのは**ぶつかったときだけ**なので、
/// 挿し直しで全件読むことにはならない。
#[test]
fn an_identical_copy_on_the_same_card_is_folded_into_one() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    for sub in ["DCIM/100MSDCF", "BACKUP/100MSDCF"] {
        fs::create_dir_all(card.join(sub)).unwrap();
    }
    // 名前も大きさも中身も同じ
    fs::write(card.join("DCIM/100MSDCF/DSC00001.ARW"), b"aaaa").unwrap();
    fs::write(card.join("BACKUP/100MSDCF/DSC00001.ARW"), b"aaaa").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    let stats = import_from(&card, &config, |_, _, _| {}).unwrap();

    let names: Vec<String> = walkdir::WalkDir::new(&dest)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(stats.copied, 1, "同じ写真が増えた: {names:?}");
    assert_eq!(stats.skipped, 1);
    assert_eq!(
        names,
        vec!["DSC00001.ARW".to_string()],
        "連番が付いた: {names:?}"
    );
}

/// **素の名前が埋まっていても、同じ中身は畳む。**
///
/// 前の周の別の写真が `DSC00001.ARW` を埋めていると、**今回の1枚目自体が `-1` に付く**。
/// 素の名前しか見ないと、**同じ中身の2枚目が `-2` に付いて増える。**
#[test]
fn an_identical_copy_is_folded_even_when_the_plain_name_is_taken() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);

    // 1周目: 素の名前を、別の写真で埋める
    fs::create_dir_all(card.join("DCIM/100MSDCF")).unwrap();
    let other = card.join("DCIM/100MSDCF/DSC00001.ARW");
    // **大きさは変えておく**——同じだと 2 周目の1枚目が「取り込み済み」に当たって、
    // 確かめたい所（連番の先を見るか）まで進まない
    fs::write(&other, b"zzzzzz").unwrap();
    filetime::set_file_mtime(&other, stamp).unwrap();
    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    assert_eq!(import_from(&card, &config, |_, _, _| {}).unwrap().copied, 1);
    fs::remove_file(&other).unwrap();

    // 2周目: 同じ名前・同じ中身が2箇所にある
    fs::create_dir_all(card.join("BACKUP/100MSDCF")).unwrap();
    for p in [
        card.join("DCIM/100MSDCF/DSC00001.ARW"),
        card.join("BACKUP/100MSDCF/DSC00001.ARW"),
    ] {
        fs::write(&p, b"aaaa").unwrap();
        filetime::set_file_mtime(&p, stamp).unwrap();
    }
    let second = import_from(&card, &config, |_, _, _| {}).unwrap();

    let names: BTreeSet<String> = walkdir::WalkDir::new(&dest)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(second.copied, 1, "同じ中身が2枚入った: {names:?}");
    assert!(
        !names.contains("DSC00001-2.ARW"),
        "連番が増えている: {names:?}"
    );
}

/// **片割れが後の周で来たときは、取り込めない——が、増えもしない。**
///
/// 「名前も大きさも更新時刻も同じで、中身が別」は、**両方を読まないと見分けられない**。
/// 読むということは**カードを挿し直すたびに全ファイルを読む**ということなので、
/// この工程は取らない（`looks_same` の説明）。**ここで押さえるのは「増えない」ほう**
/// ——直しかけたとき、**同じ写真が2枚になって、片割れは消えたまま**になった
/// （2026-09-06。ゲート2の助言どおりに直したら、そうなった）。
#[test]
fn a_twin_in_a_later_run_is_not_taken_but_nothing_is_duplicated() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    fs::create_dir_all(card.join("DCIM/100MSDCF")).unwrap();
    let first = card.join("DCIM/100MSDCF/DSC00001.ARW");
    fs::write(&first, b"aaaa").unwrap();

    // **時刻は最初から止めておく。** あとから動かすと**行き先のフォルダごと変わる**
    // （日付で振り分けるので）。止めておかないと、1周目に2秒以上かかったとき
    // （冷えたCI・Windowsの走査）に `looks_same` が外れて、
    // **中身と関係なくこの試験が落ちる**
    let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&first, stamp).unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    assert_eq!(import_from(&card, &config, |_, _, _| {}).unwrap().copied, 1);

    fs::create_dir_all(card.join("DCIM/101MSDCF")).unwrap();
    let twin = card.join("DCIM/101MSDCF/DSC00001.ARW");
    fs::write(&twin, b"bbbb").unwrap();
    filetime::set_file_mtime(&twin, stamp).unwrap();
    let second = import_from(&card, &config, |_, _, _| {}).unwrap();

    assert_eq!(second.copied, 0, "見分けられないものを運んでいる");
    let got: Vec<String> = contents(&dest).into_iter().collect();
    assert_eq!(
        got,
        vec!["aaaa".to_string()],
        "同じ写真が増えている: {got:?}"
    );
}

/// **ウィザードから選んだ場合も同じ**（`import_files`）。
///
/// 丸ごと取り込みと選択取り込みは**別の関数**なので、片方だけ直して片方を忘れうる。
/// カードの `DCIM` が分かれていれば、**同じ名前の2枚を両方選ぶ**のは普通に起きる。
#[test]
fn the_wizard_path_carries_both_of_a_same_named_pair() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    for sub in ["DCIM/100MSDCF", "DCIM/101MSDCF"] {
        fs::create_dir_all(card.join(sub)).unwrap();
    }
    let a = card.join("DCIM/100MSDCF/DSC00001.ARW");
    let b = card.join("DCIM/101MSDCF/DSC00001.ARW");
    fs::write(&a, b"aaaa").unwrap();
    fs::write(&b, b"bbbb").unwrap();

    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    let stats = pictkura_core::import::import_files(&[a, b], &config, |_, _, _| {}).unwrap();

    assert_eq!(stats.copied, 2, "選んだ2枚のうち1枚が消えた");
    assert!(contents(&dest).contains("aaaa") && contents(&dest).contains("bbbb"));
}

/// **壊れたリンクが行き先に在っても、それを消さない。**
///
/// `Path::exists()` は**リンクを辿る**ので、辿った先が無いリンクは「空いている」と読まれる。
/// そこへコピーして転んだとき、片付けを素通しでやると、**こちらが作った物ではなく
/// 元から在ったリンクが消える。**
#[cfg(unix)]
#[test]
fn a_dangling_link_in_the_destination_is_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("E");
    let dest = dir.path().join("photos");
    let stamp = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    fs::create_dir_all(card.join("DCIM/100MSDCF")).unwrap();
    let src = card.join("DCIM/100MSDCF/DSC00001.ARW");
    fs::write(&src, b"aaaa").unwrap();
    filetime::set_file_mtime(&src, stamp).unwrap();

    // 行き先のフォルダを、日付の振り分けと同じ形で先に作る
    let mut config = Config::default();
    config.routing.destination = Some(dest.clone());
    import_from(&card, &config, |_, _, _| {}).unwrap();
    let day_dir = walkdir::WalkDir::new(&dest)
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_file())
        .map(|e| e.path().parent().unwrap().to_path_buf())
        .unwrap();

    // 同じフォルダに、辿れないリンクを置く（別名で）
    let link = day_dir.join("DSC00099.ARW");
    std::os::unix::fs::symlink(dir.path().join("nowhere/none.ARW"), &link).unwrap();

    // その名前で取り込ませる（コピーは転ぶ——リンクの先の親が無い）
    let second = card.join("DCIM/100MSDCF/DSC00099.ARW");
    fs::write(&second, b"bbbb").unwrap();
    filetime::set_file_mtime(&second, stamp).unwrap();
    import_from(&card, &config, |_, _, _| {}).unwrap();

    assert!(
        fs::symlink_metadata(&link).is_ok(),
        "元から在ったリンクを消している"
    );
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
