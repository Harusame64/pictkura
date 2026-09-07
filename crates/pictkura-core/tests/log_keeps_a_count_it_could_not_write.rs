//! **言えなかった要約は、抱えたまま次へ回す**——「別の行が来た」枝（ゲート2）。
//!
//! 周期の枝には [`applog::flush_pending`] と `after_flush` が在って、
//! **数は言えたときだけ手放す**という規則を守っている。**同じ規則が、
//! 「別の行が来た」枝だけ抜けていた**——`take()` が先に落としてしまうので、
//! 要約が書けなかった瞬間に**4000回が「1回」になる**。
//!
//! 置き場は**プロセスに1つ**（`OnceLock`）なので、`#[test]` は1本にまとめる。

use pictkura_core::applog;
use std::fs;

#[test]
fn a_summary_that_could_not_be_written_is_still_owed() {
    let dir = tempfile::tempdir().unwrap();
    // **あとで消せるフォルダ**の中に置く。`applog` はフォルダを作らないので、
    // 消せばそのまま「書けない台」になる（権限を触らずに再現できる）
    let folder = dir.path().join("あとで消すフォルダ");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("pictkura.log");
    applog::set_file(path.clone());

    // 同じ失敗が3回。**書かれるのは1本目だけ**で、2回ぶんが畳まれる
    for _ in 0..3 {
        applog::note("同じ失敗");
    }
    assert_eq!(
        fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter(|l| l.contains("同じ失敗"))
            .count(),
        1
    );

    // **書けない台にする。** ここで別の行が来ると、要約を書こうとして失敗する
    fs::remove_dir_all(&folder).unwrap();
    applog::note("別の失敗");
    assert!(!folder.exists(), "書けない側でフォルダを作っていないこと");

    // **書けるようになったら、抱えていた数が出てくる**
    fs::create_dir(&folder).unwrap();
    applog::note("別の失敗");

    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("repeated 2 more times"),
        "書けなかった1分ぶんを捨てていない: {text}"
    );
    // **どの行だったかも残っている**
    assert!(text.contains("同じ失敗"), "{text}");
    // **新しい行も、書けるようになってから出る**
    assert!(text.contains("別の失敗"), "{text}");
}
