//! **捕まえたパニックが、本当にファイルまで届くか**（完成度週間の項目2）。
//!
//! 単体試験は `applog` の中の部品を見ているだけで、**`catching` から
//! ファイルまでの配線**は見ていない。ここが切れていると、
//! **配布ビルドでだけ黙る**——手元では `stderr` に出るので気付けない。
//!
//! 置き場は**プロセスに1つ**（`OnceLock`）なので、**1本の試験に畳んである**。
//! 順番にも意味がある——網（`catching`）を先に通し、**掛け金を張るのはそのあと**。
//! 逆にすると同じパニックが2行になり、どちらの経路で来た行か分からなくなる。

use pictkura_core::{applog, panics};

#[test]
fn a_panic_reaches_the_file_both_with_and_without_the_net() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pictkura.log");
    applog::set_file(path.clone());
    assert_eq!(applog::file(), Some(path.as_path()));

    // 1. 網に掛かる側（サムネイル生成が壊れたファイルを踏んだとき）
    let out = panics::catching("IMG_0100.CR3", || -> i32 {
        panic!("壊れたハフマン表")
    });
    assert!(out.is_none());

    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines = text.lines();
    // 見出しが1本目。**版が入っていること**——届いた行に版が無いと追えない
    let header = lines.next().unwrap();
    assert!(header.contains(env!("CARGO_PKG_VERSION")), "{header}");
    let caught = lines.next().unwrap();
    assert!(caught.contains("caught a panic (IMG_0100.CR3)"), "{caught}");
    assert!(caught.contains("壊れたハフマン表"), "{caught}");
    assert!(lines.next().is_none(), "1件で1行のはず: {text}");

    // 2. 網の外側（`catching` を通らないスレッドで落ちたとき）
    applog::install_panic_hook();
    let fell = std::panic::catch_unwind(|| panic!("網の外で落ちる"));
    assert!(fell.is_err());

    let text = std::fs::read_to_string(&path).unwrap();
    let hooked = text.lines().last().unwrap();
    assert!(hooked.contains("網の外で落ちる"), "{hooked}");
    // **どのソースの何行目か**が入る（網の側は「どのファイルで」を言う）。
    // **区切りで綴らない**——`Location::file()` は cargo が rustc へ渡した綴りを
    // そのまま返すので、**Windowsでは `tests\panic_log.rs`** になる（ゲート2の指摘）
    assert!(hooked.contains("panic_log.rs:"), "{hooked}");

    // 3. **出荷時の形**（掛け金が先に張ってある）では、網に掛かった1件が**2行**になる。
    //
    // 上の 1 が1行で済んだのは**掛け金を張る前**だったからで、実際のアプリは
    // `setup` で先に張る。**上限（`MAX_BYTES`）に効くのはこちらの本数**なので、
    // 意図した2行であることを写しておく（ゲート2の指摘）
    let before = std::fs::read_to_string(&path).unwrap().lines().count();
    let out = panics::catching("IMG_0101.CR3", || -> i32 {
        panic!("網の中で落ちる")
    });
    assert!(out.is_none());
    let after: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .skip(before)
        .map(str::to_string)
        .collect();
    assert_eq!(after.len(), 2, "掛け金と網で1本ずつ: {after:?}");

    // 4. **同じ行が続いたら畳む**（詰まった失敗で記録が埋まらないように）
    let before = std::fs::read_to_string(&path).unwrap().lines().count();
    applog::note("同じ理由で続けて落ちる");
    applog::note("同じ理由で続けて落ちる");
    applog::note("同じ理由で続けて落ちる");
    let lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .skip(before)
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 1, "3回でも1行: {lines:?}");

    // **別の行が来たときに、黙っていた分を言う**
    applog::note("別の理由");
    let lines: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .skip(before + 1)
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2, "畳んだ数の1行と、新しい1行: {lines:?}");
    assert!(lines[0].contains("repeated 2 more times"), "{:?}", lines[0]);
    assert!(lines[1].contains("別の理由"), "{:?}", lines[1]);

    // 5. **消されたら、畳まずに書き直す**（説明書が「消してよい」と言っている）。
    //    畳んだままだと、**消した直後の同じ失敗がどこにも残らない**
    applog::note("消したあとにも起きる失敗");
    std::fs::remove_file(&path).unwrap();
    applog::note("消したあとにも起きる失敗");
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "見出しと本文がそろっていること: {lines:?}");
    assert!(
        lines[0].contains(env!("CARGO_PKG_VERSION")),
        "{:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("消したあとにも起きる失敗"),
        "{:?}",
        lines[1]
    );
    assert!(after[0].contains("panic ("), "{:?}", after[0]);
    assert!(
        after[1].contains("caught a panic (IMG_0101.CR3)"),
        "{:?}",
        after[1]
    );
}
