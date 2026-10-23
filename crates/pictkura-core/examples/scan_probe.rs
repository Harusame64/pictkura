//! 開発用の調べ道具。**配布物には入らない**。
//!
//! ここは `unwrap()` を許している——条件が揃わなければその場で落ちて理由を
//! 見せるのが正しい。本体側の方針は `Cargo.toml` の `[workspace.lints.clippy]`。
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! 取り込み画面のスキャン（`is_already_imported`）を、何本並べて回すと速いか
//! （pictkura-dev #73 の spike）:
//!
//! ```text
//! cargo run --release -p pictkura-core --example scan_probe -- <取り込み元> <行き先> <本数> <full|date|stat> <出力.csv>
//! ```
//!
//! - `full`: `is_already_imported` そのもの（アプリの `probe_imported` と同じ）
//! - `date`: 撮影日時の読み（`read_exif_meta`）だけ
//! - `stat`: `metadata` だけ
//!
//! 1件ごとの行（名前・大きさ・開始と終了の ms・答え・スレッド）を CSV に書き、
//! 合計を標準出力に1行出す。並びは名前順で、`<本数>` のスレッドが前から1件ずつ取る。
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

fn walk(dir: &Path, exts: &[String], out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, exts, out);
        } else if pictkura_core::scanner::has_target_extension(&p, exts) {
            out.push(p);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, src, dest, threads, mode, csv] = args.as_slice() else {
        panic!("usage: scan_probe <src> <dest> <threads> <full|date|stat> <out.csv>");
    };
    let threads: usize = threads.parse().unwrap();
    assert!(matches!(mode.as_str(), "full" | "date" | "stat"), "mode");

    let mut config = pictkura_core::Config::default();
    config.routing.destination = Some(PathBuf::from(dest));
    let mut files = Vec::new();
    walk(Path::new(src), &config.import.extensions, &mut files);
    files.sort();
    // アプリは名前のぶつかりを一覧全体から数えて渡す。ここも同じにする
    let names: Vec<(String, u64, i64)> = files
        .iter()
        .map(|p| {
            let m = std::fs::metadata(p).unwrap();
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                m.len(),
                mtime,
            )
        })
        .collect();
    let contested_names =
        pictkura_core::contested_names(names.iter().map(|(n, s, m)| (n.as_str(), *s, *m)));
    let contested = pictkura_core::contested_set(contested_names.iter().map(String::as_str));
    let edits = pictkura_core::Edits::default();

    let next = AtomicUsize::new(0);
    let rows = Mutex::new(Vec::new());
    let t0 = Instant::now();
    std::thread::scope(|s| {
        for t in 0..threads {
            let (files, next, rows, config, contested, edits, mode) = (
                &files,
                &next,
                &rows,
                &config,
                &contested,
                &edits,
                mode.as_str(),
            );
            s.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(p) = files.get(i) else { break };
                let start = t0.elapsed().as_secs_f64() * 1000.0;
                let answer = match mode {
                    "full" => format!(
                        "{:?}",
                        pictkura_core::is_already_imported(p, config, contested, edits)
                    ),
                    "date" => format!("{:?}", pictkura_core::thumbs::read_exif_meta(p).taken_at_ms),
                    _ => format!("{}", std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)),
                };
                let end = t0.elapsed().as_secs_f64() * 1000.0;
                rows.lock().unwrap().push((i, start, end, answer, t));
            });
        }
    });
    let total = t0.elapsed().as_secs_f64();

    let mut rows = rows.into_inner().unwrap();
    rows.sort_by_key(|r| r.0);
    let mut f = std::fs::File::create(csv).unwrap();
    writeln!(f, "name,size,start_ms,end_ms,answer,thread").unwrap();
    for (i, start, end, answer, t) in &rows {
        let (name, size, _) = &names[*i];
        writeln!(f, "{name},{size},{start:.2},{end:.2},{answer},{t}").unwrap();
    }
    println!(
        "mode={mode} threads={threads} files={} total_s={total:.3} csv={csv}",
        files.len()
    );
}
