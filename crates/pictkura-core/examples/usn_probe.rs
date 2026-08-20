//! 開発用の調べ道具。**配布物には入らない**。
//!
//! ここは `unwrap()` を許している——条件が揃わなければその場で落ちて理由を
//! 見せるのが正しい。本体側の方針は `Cargo.toml` の `[workspace.lints.clippy]`。
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]

//! USNジャーナル読み取りの手動プローブ（開発用）。
//! 管理者権限なしで FSCTL_READ_UNPRIVILEGED_USN_JOURNAL が機能するかを確認する:
//! `cargo run -p pictkura-core --example usn_probe`
use pictkura_core::usn::*;

fn main() {
    let tmp = std::env::temp_dir();
    let vol = volume_of(&tmp).expect("volume");
    let pos = match read_changes_since(&vol, None) {
        UsnOutcome::FullScanNeeded(Some(p)) => {
            println!("journal OK: {vol} pos={}", p.to_meta());
            p
        }
        UsnOutcome::FullScanNeeded(None) => {
            println!("journal INACCESSIBLE on {vol}");
            return;
        }
        UsnOutcome::Delta(_) => unreachable!("位置なしでDeltaは返らない"),
    };
    let probe = tmp.join("pictkura_usn_probe.jpg");
    std::fs::write(&probe, b"probe").unwrap();
    match read_changes_since(&vol, Some(&pos)) {
        UsnOutcome::Delta(d) => {
            println!(
                "DELTA: {} records, {} dirty dirs",
                d.record_count,
                d.dirty_dirs.len()
            );
            let t = tmp
                .to_string_lossy()
                .to_lowercase()
                .trim_end_matches('\\')
                .to_string();
            let hit = d
                .dirty_dirs
                .iter()
                .any(|p| p.to_string_lossy().to_lowercase() == t);
            println!("temp dir in dirty set: {hit}");
        }
        UsnOutcome::FullScanNeeded(p) => println!("FALLBACK (pos={:?})", p.map(|p| p.to_meta())),
    }
    let _ = std::fs::remove_file(&probe);
}
