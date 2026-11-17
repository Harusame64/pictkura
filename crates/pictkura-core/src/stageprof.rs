//! SPIKE (do not merge): per-stage timing of the thumbnail workers, for the first-load profile.
//! Enabled when the environment variable `PICTKURA_STAGEPROF` names a file; every 5 s a line of
//! cumulative `stage=ms/count` is appended to it.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const STAGES: [&str; 10] = [
    "pop_wait",
    "pre",
    "process",
    "begin_wait",
    "body",
    "commit",
    "finish",
    "on_done",
    "job",
    "other",
];
static NS: [AtomicU64; 10] = [const { AtomicU64::new(0) }; 10];
static N: [AtomicU64; 10] = [const { AtomicU64::new(0) }; 10];
static ON: OnceLock<bool> = OnceLock::new();

pub fn on() -> bool {
    *ON.get_or_init(|| {
        let Ok(path) = std::env::var("PICTKURA_STAGEPROF") else {
            return false;
        };
        let start = Instant::now();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(5));
            let mut line = format!("t={:.1}", start.elapsed().as_secs_f64());
            for (i, name) in STAGES.iter().enumerate() {
                let ms = NS[i].load(Ordering::Relaxed) / 1_000_000;
                let n = N[i].load(Ordering::Relaxed);
                line.push_str(&format!(" {name}={ms}/{n}"));
            }
            line.push('\n');
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = f.write_all(line.as_bytes());
            }
        });
        true
    })
}

pub fn add(stage: usize, since: Instant) {
    if on() {
        NS[stage].fetch_add(since.elapsed().as_nanos() as u64, Ordering::Relaxed);
        N[stage].fetch_add(1, Ordering::Relaxed);
    }
}

static ERRS: AtomicU64 = AtomicU64::new(0);

/// 失敗した仕事を数え、最初の200件は理由を書き出す
pub fn error(id: i64, what: &str) {
    if !on() {
        return;
    }
    NS[9].fetch_add(0, Ordering::Relaxed);
    N[9].fetch_add(1, Ordering::Relaxed);
    if ERRS.fetch_add(1, Ordering::Relaxed) < 200 {
        if let Ok(path) = std::env::var("PICTKURA_STAGEPROF") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(format!("{path}.errors"))
            {
                let _ = writeln!(f, "id={id} {what}");
            }
        }
    }
}
