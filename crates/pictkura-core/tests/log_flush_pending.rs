//! **抱えたままの数は、終わるときに出る**（PRのcodex の P2・7件目）。
//!
//! 畳み込みは**次を待って**数を世に出す——同じ失敗がもう一度来るか、別の行が来るか。
//! **どちらも来ないまま終わる道が在る**: 同じ失敗が60秒のうちに何千回か起きて、
//! **そこで止まった**とき。時計を持った番人は居ないので、そのままだと
//! **記録には「1回起きた」だけが残り、千回だったことは誰も知らない**。
//!
//! 置き場は**プロセスに1つ**（`OnceLock`）なので、ほかの記録の試験とは
//! **別の試験用バイナリ**に分けてある。**この中でも `#[test]` を分けない**
//! ——`FILE` も `STATE` もプロセスに1つなので、分けると**並列で走って
//! 互いの数を横取りする**（畳み込みを見る試験は、必ず1本にまとめる）。

use pictkura_core::applog;

#[test]
fn a_burst_that_stopped_still_says_how_many_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pictkura.log");
    applog::set_file(path.clone());

    // 同じ失敗が4回。**書かれるのは1本目だけ**で、あとの3回は畳まれる
    for _ in 0..4 {
        applog::note("同じ失敗");
    }
    let before = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        before.lines().filter(|l| l.contains("同じ失敗")).count(),
        1,
        "畳んでいる間は1行のはず: {before}"
    );
    assert!(!before.contains("repeated"), "まだ数は出ていない: {before}");

    // **ここで終わる。** 60秒は経っておらず、別の行も来ていない
    applog::flush_pending();

    let after = std::fs::read_to_string(&path).unwrap();
    let last = after.lines().next_back().unwrap();
    assert!(last.contains("repeated 3 more times"), "{last}");
    // **どの行だったかを名指しする**（「上の行」は退避や削除のあとには上に無い）
    assert!(last.contains("同じ失敗"), "{last}");

    // **2回呼んでも増えない。** 抱えていないものを吐き出さない
    // ——「ファイルが在る＝何かあった」の印を、終わり際の1回で壊さないこと
    applog::flush_pending();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
}
