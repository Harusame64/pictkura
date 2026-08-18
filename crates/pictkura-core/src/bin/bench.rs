//! 合成データベンチ（plan.md 第3部の検証ツール）。
//!
//! ダミーレコードをDBへ直接大量投入し、実画像なしで
//! タイムラインAPI（サマリ・日単位取得・思い出）と旧list_allの応答時間を計測する。
//!
//! 実行例:
//!   cargo run --release --bin bench -- --count 10000000 --db D:\bench\pictkura-bench.db
//!   cargo run --release --bin bench -- --count 1000000 --legacy   # 旧APIも強制計測

use std::path::PathBuf;
use std::time::Instant;

use pictkura_core::jpeg::ChromaSampling;
use pictkura_core::{Db, ScannedFile};
use rusqlite::{params, Connection};

/// 決定的な擬似乱数（xorshift64*）。ベンチの再現性のためrandクレートは使わない。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn range(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn fmt_ms(d: std::time::Duration) -> String {
    format!("{:.1}ms", d.as_secs_f64() * 1000.0)
}

/// 取り込み元フォルダの閲覧・プレビュー生成を実測する（第5部 段階E）。
/// 実物のUSB/SDカードを挿して測るためのモード。
fn bench_browse(dir: &std::path::Path) {
    let extensions: Vec<String> = pictkura_core::Config::default().import.extensions;

    println!("== pictkura 取り込み元ベンチ ==");
    println!("対象: {}", dir.display());

    let t = Instant::now();
    let listing = pictkura_core::browse::list_dir(dir, &extensions);
    println!(
        "list_dir: {} （フォルダ{} ファイル{}）",
        fmt_ms(t.elapsed()),
        listing.dirs.len(),
        listing.files.len()
    );

    // 画面1枚分（30件）のサムネイルを作る時間＝開いてから絵が出るまでの体感
    let sample: Vec<_> = listing.files.iter().take(30).collect();
    if sample.is_empty() {
        return;
    }
    let t = Instant::now();
    let mut bytes = 0usize;
    let mut made = 0usize;
    for f in &sample {
        if let Some(p) = pictkura_core::browse::preview(&f.path, 512) {
            bytes += p.bytes.len();
            made += 1;
        }
    }
    let total = t.elapsed();
    println!(
        "preview x{made}: {} （1枚あたり {}、平均 {}KB）",
        fmt_ms(total),
        fmt_ms(total / made.max(1) as u32),
        bytes / made.max(1) / 1024
    );
}

/// RAWの埋め込みプレビュー抽出を実測する（第6部 段階F）。
/// 実物のRAWを渡して、取り出せるか・どれくらい速いかを見るためのモード。
fn bench_raw(path: &std::path::Path) {
    println!("== pictkura RAWプレビュー抽出 ==");
    println!("対象: {}", path.display());
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("ファイルサイズ: {:.1}MB", size as f64 / 1024.0 / 1024.0);

    let t = Instant::now();
    let preview = pictkura_core::raw::embedded_preview(path);
    let elapsed = t.elapsed();
    match preview {
        Some(bytes) => {
            let dims = image::load_from_memory(&bytes)
                .map(|img| format!("{}x{}", img.width(), img.height()))
                .unwrap_or_else(|e| format!("デコード失敗: {e}"));
            println!(
                "抽出: {} （{:.0}KB, {dims}）",
                fmt_ms(elapsed),
                bytes.len() as f64 / 1024.0
            );
        }
        None => println!("抽出: {} （見つからなかった）", fmt_ms(elapsed)),
    }

    for (i, block) in pictkura_core::raw::bmff_metadata_blocks(path)
        .into_iter()
        .enumerate()
    {
        let head: Vec<String> = block.iter().take(4).map(|b| format!("{b:02x}")).collect();
        match exif::Reader::new().read_raw(block.clone()) {
            Ok(exif) => {
                let tags: Vec<String> = exif.fields().take(60).map(|f| f.tag.to_string()).collect();
                println!(
                    "  箱{i}: {}バイト 先頭={} 項目{}件 → {}",
                    block.len(),
                    head.join(" "),
                    exif.fields().count(),
                    tags.join(", ")
                );
            }
            Err(e) => println!(
                "  箱{i}: {}バイト 先頭={} 読めない: {e}",
                block.len(),
                head.join(" ")
            ),
        }
    }

    if let Some(out) = std::env::args()
        .position(|a| a == "--save")
        .and_then(|i| std::env::args().nth(i + 1))
    {
        if let Some(bytes) = pictkura_core::raw::embedded_preview(path) {
            std::fs::write(&out, &bytes).expect("保存に失敗");
            println!("保存: {out}");
        }
    }

    let info = pictkura_core::thumbs::read_exif_info(path);
    println!(
        "  撮影情報: カメラ={:?} レンズ={:?} 絞り={:?} シャッター={:?} ISO={:?} 焦点={:?}",
        info.camera, info.lens, info.aperture, info.shutter, info.iso, info.focal
    );

    let t = Instant::now();
    let exif = pictkura_core::thumbs::read_exif(path);
    println!(
        "EXIF: {} （カメラ={:?} 撮影日時={:?}）",
        fmt_ms(t.elapsed()),
        exif.camera,
        exif.taken_at_ms
    );
}

/// フォルダ内のRAWを片端から試し、形式ごとのカバレッジ表を出す（第6部 段階F）。
fn bench_raw_dir(dir: &std::path::Path) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(pictkura_core::raw::is_raw_extension)
        })
        .collect();
    entries.sort();

    println!("== pictkura RAWカバレッジ ==");
    println!("{:<44} {:>9}  結果", "ファイル", "抽出");
    let (mut ok, mut ng) = (0usize, 0usize);
    for path in &entries {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let t = Instant::now();
        let preview = pictkura_core::raw::embedded_preview(path);
        let elapsed = fmt_ms(t.elapsed());
        match preview.and_then(|b| image::load_from_memory(&b).ok()) {
            Some(img) => {
                ok += 1;
                println!(
                    "{name:<44} {elapsed:>9}  ✓ {}x{}",
                    img.width(),
                    img.height()
                );
            }
            None => {
                ng += 1;
                println!("{name:<44} {elapsed:>9}  ✗ 表示できるプレビューが無い");
            }
        }
    }
    println!(
        "
合計: {} 件中 {ok} 件で取得（失敗 {ng} 件）",
        entries.len()
    );
}

/// 原寸表示用JPEGを作る費用を測る（0.2 ① 先読みの深さの根拠）。
///
/// ビューアが `media://full/<id>` で払う値段そのもの。対象は
/// [`pictkura_core::thumbs::needs_display_transcode`] が真の形式だけ
/// （RAW・HEIC・TIFF）——JPEG/PNG/AVIF はRustを素通りするので測る意味がない。
///
/// 2回測るのは、1回目にOSのファイルキャッシュへ載る時間が混ざるため。
/// 先読みの深さを決めるときに見るのは**2回目（温まった側）**である。
fn bench_display_dir(dir: &std::path::Path) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| pictkura_core::thumbs::needs_display_transcode(p))
        .collect();
    entries.sort();

    println!("== pictkura 原寸表示の詰め直し ==");
    println!("対象: {}", dir.display());
    println!(
        "{:<28} {:>9} {:>9} {:>9}  出力",
        "ファイル", "1回目", "2回目", "元MB"
    );
    let mut warm_total = std::time::Duration::ZERO;
    for path in &entries {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let t = Instant::now();
        let cold = pictkura_core::thumbs::display_jpeg(path);
        let cold_ms = fmt_ms(t.elapsed());
        let t = Instant::now();
        let warm = pictkura_core::thumbs::display_jpeg(path);
        let warm_d = t.elapsed();
        warm_total += warm_d;
        let out = match (cold, warm) {
            (Some(bytes), _) | (_, Some(bytes)) => {
                let dims = image::load_from_memory(&bytes)
                    .map(|img| format!("{}x{}", img.width(), img.height()))
                    .unwrap_or_else(|e| format!("デコード失敗: {e}"));
                format!("{:.2}MB {dims}", bytes.len() as f64 / 1024.0 / 1024.0)
            }
            _ => "作れなかった".to_string(),
        };
        println!(
            "{name:<28} {cold_ms:>9} {:>9} {:>9.1}  {out}",
            fmt_ms(warm_d),
            size as f64 / 1024.0 / 1024.0
        );
    }
    if !entries.is_empty() {
        println!(
            "
{}件の平均（2回目）: {}",
            entries.len(),
            fmt_ms(warm_total / entries.len() as u32)
        );
    }
}

/// HEICの詰め直しの内訳と、JPEGエンコーダの比較を測る（0.2 HEICの詰め直し）。
///
/// 初見のHEICは1枚約953ms掛かり、先読みでは埋められない。内訳は
/// 「WICで展開 428ms ＋ JPEGへ再エンコード 約525ms」で、**後半が
/// `image` クレートの純Rustエンコーダの値段**。mozjpeg（libjpeg-turbo）は
/// 既に依存に入っている（`jpeg.rs` の展開で使っている）ので、
/// 差し替えれば新しい依存なしで縮む見込み——それを数字にする。
///
///   cargo run --release --bin bench -- --heif-encode D:\pics\heic
///
/// **クラウドのみ（OneDriveのプレースホルダ）は測らない**。開いた瞬間に
/// ダウンロードが走り、測っているのが回線速度になる。
fn bench_heif_encode(dir: &std::path::Path) {
    const QUALITY: u8 = 82; // raw.rs::encode_jpeg と同じ

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| pictkura_core::heif::is_heif_path(p))
        .collect();
    entries.sort();
    let total = entries.len();
    entries.retain(|p| !pictkura_core::cloud::is_cloud_only_path(p));

    println!("== pictkura HEICの詰め直し（エンコーダ比較） ==");
    println!("対象: {}", dir.display());
    println!(
        "HEIC {total}件のうち {}件がローカル実体（クラウドのみの {}件は測らない）
",
        entries.len(),
        total - entries.len()
    );
    println!(
        "{:<24} {:>9} {:>8} {:>9} {:>9} {:>9} {:>7} {:>7} {:>7}",
        "ファイル", "WIC展開", "平坦化", "image", "moz444", "moz420", "imgMB", "444MB", "420MB"
    );

    let mut n = 0u32;
    let mut sum_decode = std::time::Duration::ZERO;
    let mut sum_flatten = std::time::Duration::ZERO;
    let mut sum_image = std::time::Duration::ZERO;
    let mut sum_moz444 = std::time::Duration::ZERO;
    let mut sum_moz420 = std::time::Duration::ZERO;
    let (mut sum_image_bytes, mut sum_444_bytes, mut sum_420_bytes) = (0usize, 0usize, 0usize);
    for path in &entries {
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        let t = Instant::now();
        let Some(img) = pictkura_core::heif::decode(path) else {
            println!("{name:<26} {:>10}  ✗ 展開できない", fmt_ms(t.elapsed()));
            continue;
        };
        let decode = t.elapsed();

        // 平坦化（アルファ落とし）は両者に共通の前処理。エンコーダの比較から外す
        let t = Instant::now();
        let rgb = pictkura_core::resize::flatten_onto_white(&img);
        let flatten = t.elapsed();

        // 現行: image クレートの純Rustエンコーダ。**色差を間引かない（4:4:4）**
        let t = Instant::now();
        let mut by_image = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut by_image, QUALITY);
        let image_ok = rgb.write_with_encoder(encoder).is_ok();
        let image_d = t.elapsed();

        // 候補その1: mozjpeg を image と同じ 4:4:4 で回す。
        // **エンコーダだけの差**を見るのはこの列
        let t = Instant::now();
        let by_444 = pictkura_core::jpeg::encode_rgb(&rgb, QUALITY, ChromaSampling::Full);
        let moz444_d = t.elapsed();

        // 候補その2: 4:2:0（カメラが書くJPEGもiPhoneのHEICもこちら）。
        // 速さも小ささもここが最良だが、**間引きの分が混ざっている**
        let t = Instant::now();
        let by_420 = pictkura_core::jpeg::encode_rgb(&rgb, QUALITY, ChromaSampling::Half);
        let moz420_d = t.elapsed();

        let (Some(by_444), Some(by_420), true) = (by_444, by_420, image_ok) else {
            println!("{name:<24} {:>9}  ✗ どれかが詰められない", fmt_ms(decode));
            continue;
        };

        n += 1;
        sum_decode += decode;
        sum_flatten += flatten;
        sum_image += image_d;
        sum_moz444 += moz444_d;
        sum_moz420 += moz420_d;
        sum_image_bytes += by_image.len();
        sum_444_bytes += by_444.len();
        sum_420_bytes += by_420.len();
        let mb = |b: usize| b as f64 / 1024.0 / 1024.0;
        println!(
            "{name:<24} {:>9} {:>8} {:>9} {:>9} {:>9} {:>7.2} {:>7.2} {:>7.2}   {}x{}",
            fmt_ms(decode),
            fmt_ms(flatten),
            fmt_ms(image_d),
            fmt_ms(moz444_d),
            fmt_ms(moz420_d),
            mb(by_image.len()),
            mb(by_444.len()),
            mb(by_420.len()),
            rgb.width(),
            rgb.height()
        );

        // 絵が化けていないことを目視ではなく数字で確かめる（時間の外）。
        // 4:2:0 は色差を捨てているぶん元から離れるので、両方見る
        for (label, bytes) in [("4:4:4", &by_444), ("4:2:0", &by_420)] {
            if let Some(diff) = mean_abs_diff(&rgb, bytes) {
                if diff > 8.0 {
                    println!(
                        "{:<24} △ mozjpeg {label} が元と離れている: 平均差 {diff:.1}",
                        ""
                    );
                }
            }
        }
    }

    if n == 0 {
        println!(
            "
測れるHEICが無かった"
        );
        return;
    }
    let avg = |d: std::time::Duration| fmt_ms(d / n);
    let common = sum_decode + sum_flatten;
    println!(
        "
{n}件の平均: WIC展開 {} ／ 平坦化 {}（どちらもエンコーダ共通）",
        avg(sum_decode),
        avg(sum_flatten)
    );
    println!(
        "{:<22} {:>10} {:>12} {:>10} {:>10}",
        "エンコーダ", "圧縮のみ", "display_jpeg", "対image", "出力"
    );
    let avg_mb = |b: usize| b as f64 / n as f64 / 1024.0 / 1024.0;
    let base = common + sum_image;
    for (label, enc, bytes) in [
        ("image 4:4:4（現行）", sum_image, sum_image_bytes),
        ("mozjpeg 4:4:4", sum_moz444, sum_444_bytes),
        ("mozjpeg 4:2:0", sum_moz420, sum_420_bytes),
    ] {
        let total = common + enc;
        println!(
            "{label:<22} {:>10} {:>12} {:>9.0}% {:>8.2}MB",
            avg(enc),
            avg(total),
            (1.0 - total.as_secs_f64() / base.as_secs_f64()) * 100.0,
            avg_mb(bytes)
        );
    }
    println!(
        "※ image のエンコーダは色差を間引かない（4:4:4）ので、エンコーダだけの差を
   見るなら mozjpeg 4:4:4 と比べること。4:2:0 の分は間引きの手柄が混ざっている"
    );

    bench_wic_scaled_decode(&entries);
}

/// WICが「縮小しながら展開」できるか、聞いた上で実際に測る。
///
/// エンコーダを替えると、HEICの詰め直しは**展開のほうが重くなる**。
/// JPEGの1/8展開（[`pictkura_core::jpeg::decode_scaled`]）に当たるものが
/// HEVCにも在るなら、そこが次の的になる。
///
/// `GetClosestSize` が希望どおりの寸法を答えても、**内部で原寸まで起こしてから
/// 縮めているだけ**なら1msも得しない。だから聞くだけで終わりにせず、
/// 原寸と縮小の両方を通して時間を並べる。
fn bench_wic_scaled_decode(entries: &[std::path::PathBuf]) {
    const EDGES: [u32; 2] = [4096, 2048];

    // **一番大きい素材で聞く**。元が `max_edge` に収まっていると希望寸法＝原寸に
    // なり、デコーダは当然そのまま返す——それを「縮小に対応していない」と
    // 読み違える。コンテナの申告（0.2ms）だけで並べ替えられるので画素は起こさない
    let mut by_size: Vec<_> = entries
        .iter()
        .map(|p| {
            let px = pictkura_core::heif::display_dimensions(p)
                .map(|(w, h)| u64::from(w) * u64::from(h))
                .unwrap_or(0);
            (px, p)
        })
        .collect();
    by_size.sort_by(|a, b| b.0.cmp(&a.0));
    let sample: Vec<_> = by_size.iter().take(3).map(|(_, p)| *p).collect();
    let Some(first) = sample.first() else {
        return;
    };

    println!(
        "
-- WICの縮小デコード --"
    );
    let mut askable = Vec::new();
    for edge in EDGES {
        let Some(p) = pictkura_core::heif::probe_scaled_decode(first, edge) else {
            println!("長辺{edge}: 問い合わせできなかった");
            return;
        };
        let verdict = match p.scales() {
            Some(true) => "縮めて出せる",
            Some(false) => "原寸しか出せない",
            // 一番大きい素材でも縮小を頼めない＝この素材からは何も言えない
            None => "この素材は既に小さい（縮小を頼んでいない）",
        };
        if p.scales().is_some() {
            askable.push(edge);
        }
        println!(
            "長辺{edge}の答え: {verdict}（原寸 {}x{} → 希望 {}x{} → 出せる {}x{}・形式 {}）",
            p.full.0,
            p.full.1,
            p.requested.0,
            p.requested.1,
            p.closest.0,
            p.closest.1,
            p.closest_format
        );
    }
    if askable.is_empty() {
        println!("縮小を頼める素材が無いので、時間は測らない");
        return;
    }

    // 「縮めて出せる」と答えても、値段が変わらなければ意味が無い。
    // 3枚ずつ通して原寸と並べる（1枚だとファイルキャッシュの当たり外れが乗る）
    let mut full = std::time::Duration::ZERO;
    for path in &sample {
        let t = Instant::now();
        let img = pictkura_core::heif::decode(path);
        full += t.elapsed();
        if img.is_none() {
            println!("原寸: 展開できない");
            return;
        }
    }
    println!(
        "原寸: {}（{}枚の平均）",
        fmt_ms(full / sample.len() as u32),
        sample.len()
    );
    for edge in askable {
        let mut sum = std::time::Duration::ZERO;
        let mut size = None;
        let mut failed = false;
        for path in &sample {
            let t = Instant::now();
            let img = pictkura_core::heif::decode_scaled(path, edge);
            sum += t.elapsed();
            match img {
                Some(img) => size = Some((img.width(), img.height())),
                None => {
                    // 1枚でも落ちたら、その辺の平均は**枚数が合わない**。
                    // 前の1枚の寸法だけ残って正しく見える数字が出るのが一番まずい
                    println!("長辺{edge}: 縮小デコードできない（この辺は測らない）");
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        let Some((w, h)) = size else { continue };
        println!(
            "長辺{edge}: {}（{w}x{h}・原寸比 {:.0}%）",
            fmt_ms(sum / sample.len() as u32),
            sum.as_secs_f64() / full.as_secs_f64() * 100.0
        );
    }
}

/// エンコード結果を読み戻して元の画素と比べる（0〜255の平均差）。
fn mean_abs_diff(src: &image::RgbImage, encoded: &[u8]) -> Option<f64> {
    let back = image::load_from_memory(encoded).ok()?.to_rgb8();
    if back.dimensions() != src.dimensions() {
        return None;
    }
    let sum: f64 = back
        .as_raw()
        .iter()
        .zip(src.as_raw())
        .map(|(a, b)| a.abs_diff(*b) as f64)
        .sum();
    Some(sum / back.as_raw().len() as f64)
}

/// HEIC/HEIF を1枚調べる（第7部 段階G）。
///
/// コンテナから読める素性（寸法・向き）と、OSデコーダで実際に絵になるかを
/// 分けて出す。デコーダが無い環境では前者だけが取れる。
fn bench_heif(path: &std::path::Path) {
    println!("== pictkura HEIF調査: {} ==", path.display());

    let t = Instant::now();
    let info = pictkura_core::heif::read_info(path);
    match info {
        Some(i) => println!(
            "コンテナ: {} 格納={}x{} 回転={} 鏡映={:?} 表示={}x{}",
            fmt_ms(t.elapsed()),
            i.stored_width,
            i.stored_height,
            i.rotation,
            i.mirror,
            i.display_size().0,
            i.display_size().1
        ),
        None => println!("コンテナ: {} 読めない", fmt_ms(t.elapsed())),
    }

    let t = Instant::now();
    let exif = pictkura_core::thumbs::read_exif(path);
    println!(
        "EXIF: {} （カメラ={:?} 撮影日時={:?} 向き={}）",
        fmt_ms(t.elapsed()),
        exif.camera,
        exif.taken_at_ms,
        exif.orientation
    );

    let t = Instant::now();
    match pictkura_core::heif::decode_thumbnail(path) {
        Some(img) => println!(
            "埋め込みサムネイル: {} {}x{}",
            fmt_ms(t.elapsed()),
            img.width(),
            img.height()
        ),
        None => println!("埋め込みサムネイル: {} 取れない", fmt_ms(t.elapsed())),
    }

    let t = Instant::now();
    match pictkura_core::heif::decode(path) {
        Some(img) => println!(
            "主画像デコード: {} {}x{}",
            fmt_ms(t.elapsed()),
            img.width(),
            img.height()
        ),
        None => println!(
            "主画像デコード: {} 失敗（デコーダ未導入の可能性）",
            fmt_ms(t.elapsed())
        ),
    }
}

/// AVIFを1枚調べる（第7部 段階G-6）。
///
/// コンテナから集めた材料（タイル数・切り出し）と、同梱デコーダでの
/// 展開時間を、一覧用（縮小融合）と原寸で分けて出す。
fn bench_avif(path: &std::path::Path, out: Option<&str>) {
    println!("== pictkura AVIF調査: {} ==", path.display());

    let t = Instant::now();
    let Some(source) = pictkura_core::heif::read_avif_source(path) else {
        println!("コンテナ: {} 読めない", fmt_ms(t.elapsed()));
        return;
    };
    let bytes: usize = source.tiles.iter().map(|t| t.len()).sum();
    println!(
        "コンテナ: {} 格納={}x{} 回転={} タイル={}枚({}B) grid={:?} clap={:?} 設定={}B",
        fmt_ms(t.elapsed()),
        source.info.stored_width,
        source.info.stored_height,
        source.info.rotation,
        source.tiles.len(),
        bytes,
        source.grid,
        source.info.crop,
        source.config_obus.len()
    );
    println!(
        "色: {:?}  透過: {}",
        source.color,
        match &source.alpha {
            Some(a) => format!(
                "あり {}B{}",
                a.tiles[0].len(),
                if a.premultiplied {
                    "（掛け済み）"
                } else {
                    ""
                }
            ),
            None => "なし".to_string(),
        }
    );

    for (label, edge, threads) in [
        (
            "一覧用(512px・1スレッド)",
            Some(512),
            pictkura_core::av1::Threads::One,
        ),
        ("原寸(全スレッド)", None, pictkura_core::av1::Threads::All),
    ] {
        let t = Instant::now();
        match pictkura_core::av1::decode(&source, edge, threads) {
            Some(img) => println!(
                "{label}: {} {}x{}",
                fmt_ms(t.elapsed()),
                img.width(),
                img.height()
            ),
            None => println!("{label}: {} 失敗", fmt_ms(t.elapsed())),
        }
    }

    // 色が合っているかを人の目と数値で確かめられるよう、原寸を書き出せるようにする
    if let Some(out) = out {
        if let Some(img) =
            pictkura_core::av1::decode(&source, None, pictkura_core::av1::Threads::All)
        {
            match img.save(out) {
                Ok(()) => println!("書き出し: {out}"),
                Err(e) => println!("書き出し失敗: {e}"),
            }
        }
    }
}

/// フォルダ内のAVIFを片端から試す。
fn bench_avif_dir(dir: &std::path::Path) {
    let mut ok = 0usize;
    let mut ng = 0usize;
    let entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| pictkura_core::heif::is_avif_path(p))
        .collect();
    for path in &entries {
        let t = Instant::now();
        match pictkura_core::av1::decode_file(path, Some(512), pictkura_core::av1::Threads::One) {
            Some(img) => {
                ok += 1;
                println!(
                    "  OK  {:>8} {}x{}  {}",
                    fmt_ms(t.elapsed()),
                    img.width(),
                    img.height(),
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            None => {
                ng += 1;
                println!(
                    "  NG  {:>8} {}",
                    fmt_ms(t.elapsed()),
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }
    }
    println!(
        "AVIF: {}枚中 {ok}枚が展開できた（失敗 {ng}枚）",
        entries.len()
    );
}

/// フォルダ内のHEIFを片端から試し、カバレッジ表を出す（第7部 段階G）。
fn bench_heif_dir(dir: &std::path::Path) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| pictkura_core::heif::is_heif_path(p))
        .collect();
    entries.sort();

    println!("== pictkura HEIFカバレッジ ==");
    println!(
        "{:<40} {:>8} {:>9} {:>9}  結果",
        "ファイル", "コンテナ", "サムネ", "主画像"
    );
    let (mut ok, mut ng) = (0usize, 0usize);
    for path in &entries {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let t = Instant::now();
        let info = pictkura_core::heif::read_info(path);
        let t_info = fmt_ms(t.elapsed());
        let t = Instant::now();
        let thumb = pictkura_core::heif::decode_thumbnail(path);
        let t_thumb = fmt_ms(t.elapsed());
        let t = Instant::now();
        let full = pictkura_core::heif::decode(path);
        let t_full = fmt_ms(t.elapsed());
        match (&info, &full) {
            (Some(i), Some(img)) => {
                ok += 1;
                let expected = i.display_size();
                let agree = expected == (img.width(), img.height());
                println!(
                    "{name:<40} {t_info:>8} {t_thumb:>9} {t_full:>9}  {} {}x{}{}",
                    if agree { "✓" } else { "△" },
                    img.width(),
                    img.height(),
                    if agree {
                        String::new()
                    } else {
                        format!("（コンテナ申告は {}x{}）", expected.0, expected.1)
                    }
                );
            }
            _ => {
                ng += 1;
                println!(
                    "{name:<40} {t_info:>8} {t_thumb:>9} {t_full:>9}  ✗ {}",
                    if info.is_none() {
                        "コンテナが読めない"
                    } else {
                        "デコードできない"
                    }
                );
            }
        }
        if thumb.is_none() && full.is_some() {
            println!("{:<40} （埋め込みサムネイル無し）", "");
        }
    }
    println!(
        "
合計: {} 件中 {ok} 件で表示可（失敗 {ng} 件）",
        entries.len()
    );
}

/// サムネイル生成の段階別ベンチ（SIMD化の効果を測るためのモード）。
///
/// 実写のJPEG等が入ったフォルダを渡すと、1枚ずつ
/// 「デコード → 事前縮小 → 仕上げ縮小 → WebPエンコード」の各段階を計り、
/// 現行（imageクレートのスカラー）とSIMD（fast_image_resize）を並べて出す。
///
///   cargo run --release --bin bench -- --thumb-dir D:\pics --size 512
fn bench_thumb_dir(dir: &std::path::Path, thumb_size: u32) {
    let extensions: Vec<String> = pictkura_core::Config::default().import.extensions;
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        println!("対象の画像が無い: {}", dir.display());
        return;
    }

    println!("== pictkura サムネイル段階別ベンチ ==");
    println!(
        "対象: {} （{}枚, 目標{}px）",
        dir.display(),
        paths.len(),
        thumb_size
    );

    // 段階ごとの合計時間。scalar/simd を並べて持つ
    let mut decode = std::time::Duration::ZERO;
    let mut encode = std::time::Duration::ZERO;
    let mut pre_scalar = std::time::Duration::ZERO;
    let mut pre_simd = std::time::Duration::ZERO;
    let mut fin_scalar = std::time::Duration::ZERO;
    let mut fin_simd = std::time::Duration::ZERO;
    let mut pixels = 0u64;
    let mut done = 0usize;
    // 画質の差（現行実装との平均絶対差）を見る
    let mut diff_sum = 0f64;
    // 間引き展開（JPEGのDCTを一部だけ使う）の計測
    let mut decode_scaled = std::time::Duration::ZERO;
    let mut fin_scaled = std::time::Duration::ZERO;
    let mut diff_scaled = 0f64;
    let mut scaled_ok = 0usize;

    for path in &paths {
        let t = Instant::now();
        let Ok(img) = image::open(path) else { continue };
        decode += t.elapsed();
        let (iw, ih) = (img.width(), img.height());
        pixels += iw as u64 * ih as u64;
        done += 1;

        // 事前縮小（2倍サイズへの荒落とし）: 現行は thumbnail、SIMDは面積平均
        let (pw, ph) = pictkura_core::resize::fit_within(iw, ih, thumb_size * 2);
        let big = iw.max(ih) > thumb_size * 3;

        let t = Instant::now();
        let pre_s = if big {
            img.thumbnail(thumb_size * 2, thumb_size * 2)
        } else {
            img.clone()
        };
        pre_scalar += t.elapsed();

        let t = Instant::now();
        let pre_v = if big {
            pictkura_core::resize::box_filter(&img, pw, ph)
        } else {
            img.clone()
        };
        pre_simd += t.elapsed();

        // 仕上げ（Lanczos3）
        let (fw, fh) = pictkura_core::resize::fit_within(pre_s.width(), pre_s.height(), thumb_size);
        let t = Instant::now();
        let fin_s = pre_s.resize(
            thumb_size,
            thumb_size,
            image::imageops::FilterType::Lanczos3,
        );
        fin_scalar += t.elapsed();

        let t = Instant::now();
        let fin_v = pictkura_core::resize::lanczos3(&pre_v, fw, fh);
        fin_simd += t.elapsed();

        // 画質の差（同じ寸法のときだけ）
        let (a, b) = (fin_s.to_rgb8(), fin_v.to_rgb8());
        if a.dimensions() == b.dimensions() {
            let sum: u64 = a
                .as_raw()
                .iter()
                .zip(b.as_raw())
                .map(|(x, y)| x.abs_diff(*y) as u64)
                .sum();
            diff_sum += sum as f64 / a.as_raw().len() as f64;
        }

        // 間引き展開: 目標の大きさまでDCTで落としてから仕上げるだけ（事前縮小が要らない）
        if pictkura_core::jpeg::is_jpeg_path(path) {
            let t = Instant::now();
            let small = pictkura_core::jpeg::decode_scaled(path, thumb_size);
            let dt = t.elapsed();
            if let Some(small) = small {
                decode_scaled += dt;
                let (sw, sh) =
                    pictkura_core::resize::fit_within(small.width(), small.height(), thumb_size);
                let t = Instant::now();
                let out = pictkura_core::resize::lanczos3(&small, sw, sh);
                fin_scaled += t.elapsed();
                scaled_ok += 1;
                let c = out.to_rgb8();
                if c.dimensions() == a.dimensions() {
                    let sum: u64 = a
                        .as_raw()
                        .iter()
                        .zip(c.as_raw())
                        .map(|(x, y)| x.abs_diff(*y) as u64)
                        .sum();
                    diff_scaled += sum as f64 / a.as_raw().len() as f64;
                }
            }
        }

        let t = Instant::now();
        let rgb = fin_v.to_rgb8();
        let _ = webp::Encoder::from_rgb(&rgb, rgb.width(), rgb.height()).encode(82.0);
        encode += t.elapsed();
    }

    if done == 0 {
        println!("1枚も開けなかった");
        return;
    }
    let n = done as u32;
    let mp = pixels as f64 / done as f64 / 1_000_000.0;
    println!("開けた: {done}枚（平均 {mp:.1}MP）\n");

    let row = |label: &str, scalar: std::time::Duration, simd: std::time::Duration| {
        let s = scalar.as_secs_f64();
        let v = simd.as_secs_f64();
        println!(
            "  {label:<16} 現行 {:>8} / SIMD {:>8}  → {:.2}倍",
            fmt_ms(scalar / n),
            fmt_ms(simd / n),
            if v > 0.0 { s / v } else { 0.0 }
        );
    };
    println!("1枚あたり:");
    println!("  {:<16} {:>8}", "デコード", fmt_ms(decode / n));
    row("事前縮小", pre_scalar, pre_simd);
    row("仕上げ縮小", fin_scalar, fin_simd);
    println!("  {:<16} {:>8}", "WebPエンコード", fmt_ms(encode / n));

    let total_s = decode + pre_scalar + fin_scalar + encode;
    let total_v = decode + pre_simd + fin_simd + encode;
    println!();
    row("合計", total_s, total_v);
    row("縮小だけ", pre_scalar + fin_scalar, pre_simd + fin_simd);
    println!(
        "\n画質: 現行との平均絶対差 {:.2}/255（0に近いほど見分けがつかない）",
        diff_sum / done as f64
    );

    if scaled_ok > 0 {
        let m = scaled_ok as u32;
        // エンコードは共通なので1枚あたりの値をそのまま足す
        let per_image = (decode_scaled + fin_scaled) / m + encode / n;
        let base = total_s / n;
        println!(
            "
-- 間引き展開（DCTを一部だけ使う・JPEG {scaled_ok}枚）--"
        );
        println!("  {:<16} {:>8}", "展開", fmt_ms(decode_scaled / m));
        println!("  {:<16} {:>8}", "仕上げ縮小", fmt_ms(fin_scaled / m));
        println!("  {:<16} {:>8}", "WebPエンコード", fmt_ms(encode / n));
        println!(
            "  {:<16} {:>8} （現行 {} → {:.2}倍）",
            "合計",
            fmt_ms(per_image),
            fmt_ms(base),
            base.as_secs_f64() / per_image.as_secs_f64().max(1e-9)
        );
        println!(
            "  画質: 現行との平均絶対差 {:.2}/255",
            diff_scaled / scaled_ok as f64
        );
    }
}

/// JPEG展開だけを比べる（どこが効いているのかを切り分けるためのモード）。
///
///   cargo run --release --bin bench -- --decode-dir D:\pics --size 512
fn bench_decode_dir(dir: &std::path::Path, thumb_size: u32) {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| pictkura_core::jpeg::is_jpeg_path(p))
        .collect();
    paths.sort();
    if paths.is_empty() {
        println!("JPEGが無い: {}", dir.display());
        return;
    }
    println!("== pictkura JPEG展開ベンチ ==");
    println!(
        "対象: {} （{}枚, 目標{}px）",
        dir.display(),
        paths.len(),
        thumb_size
    );

    let mut zune = std::time::Duration::ZERO;
    let mut jd_full = std::time::Duration::ZERO;
    let mut jd_small = std::time::Duration::ZERO;
    let mut n = 0u32;
    let mut small_dims = (0u32, 0u32);
    let mut full_dims = (0u32, 0u32);

    for path in &paths {
        let t = Instant::now();
        let Ok(a) = image::open(path) else { continue };
        let t_zune = t.elapsed();

        // mozjpeg を原寸で（＝間引きの効果を差し引くための基準）
        let t = Instant::now();
        let b = pictkura_core::jpeg::decode_scaled(path, a.width().max(a.height()));
        let t_full = t.elapsed();
        if b.is_none() {
            continue;
        }

        let t = Instant::now();
        let Some(c) = pictkura_core::jpeg::decode_scaled(path, thumb_size) else {
            continue;
        };
        // 3通り全部そろった画像だけを平均に入れる。途中で抜けたものの時間を
        // 分子にだけ足すと、1枚あたりが実際より重く見える
        jd_small += t.elapsed();
        zune += t_zune;
        jd_full += t_full;
        full_dims = (a.width(), a.height());
        small_dims = (c.width(), c.height());
        n += 1;
    }
    if n == 0 {
        println!("比較できる画像が無かった");
        return;
    }
    println!(
        "原寸 {}x{} → 間引き後 {}x{}",
        full_dims.0, full_dims.1, small_dims.0, small_dims.1
    );
    println!(
        "  {:<26} {:>8}",
        "zune-jpeg（現行・原寸）",
        fmt_ms(zune / n)
    );
    println!("  {:<26} {:>8}", "mozjpeg（原寸）", fmt_ms(jd_full / n));
    println!("  {:<26} {:>8}", "mozjpeg（間引き）", fmt_ms(jd_small / n));
    println!(
        "\n間引きで減ったぶん: {} → {} （{:.2}倍）。ここが逆変換と色の補間の取り分で、",
        fmt_ms(jd_full / n),
        fmt_ms(jd_small / n),
        jd_full.as_secs_f64() / jd_small.as_secs_f64().max(1e-9)
    );
    println!("残りはハフマン復号（係数を1つずつ読む処理）で、間引いても減らない——");
    println!("だから、そこが速いデコーダでないと現行を大きくは引き離せない。");
}

/// 本番の `process_one` をそのまま回して、高品質サムネイル生成の実時間を測る。
///
/// 段階別ベンチ（`--thumb-dir`）が部品ごとの計測なのに対し、こちらは
/// 「UIが可視領域を要求してから絵が出るまで」に実際に走る道をそのまま通す。
///
///   cargo run --release --bin bench -- --pipeline D:\pics --size 512
fn bench_pipeline(dir: &std::path::Path, thumb_size: u32) {
    let extensions: Vec<String> = pictkura_core::Config::default().import.extensions;
    let mut files: Vec<ScannedFile> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let ok = path
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| extensions.iter().any(|y| y.eq_ignore_ascii_case(x)))
                .unwrap_or(false);
            if !ok {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some(ScannedFile {
                path,
                size: meta.len() as i64,
                mtime_ms: 1,
            })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if files.is_empty() {
        println!("対象の画像が無い: {}", dir.display());
        return;
    }

    let work = std::env::temp_dir().join("pictkura-pipeline-bench");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let db_path = work.join("bench.db");
    let thumbs_dir = work.join("thumbs");

    let mut db = Db::open(&db_path).expect("DB初期化に失敗");
    db.upsert_files(&files).expect("投入に失敗");
    let ids: Vec<i64> = db.list_all().unwrap().iter().map(|r| r.id).collect();

    println!("== pictkura サムネイル生成ベンチ（本番経路）==");
    println!(
        "対象: {} （{}枚, 目標{}px）",
        dir.display(),
        ids.len(),
        thumb_size
    );

    // 1周目: 背景の自動パス（want_final = false）。埋め込みサムネイルの即席書き出しと
    // メタデータ収集まで。ここで true を渡すと、埋め込みの無い画像は1周目で
    // 高品質まで作ってしまい、2周目が「暖まったキャッシュでの作り直し」になる
    let t = Instant::now();
    let mut provisional = 0usize;
    for id in &ids {
        if let Ok(pictkura_core::thumbs::ThumbOutcome::Provisional) =
            pictkura_core::thumbs::process_one(&mut db, &thumbs_dir, thumb_size, *id, false)
        {
            provisional += 1;
        }
    }
    let first = t.elapsed();
    println!(
        "即席（埋め込み流用）{provisional}枚: 1枚あたり {}",
        fmt_ms(first / ids.len().max(1) as u32)
    );

    // 2周目: フル展開からの高品質生成。ここが今回速くした道
    let t = Instant::now();
    let mut final_count = 0usize;
    let mut other = 0usize;
    let mut bytes = 0u64;
    for id in &ids {
        // want_final = true ＝ 可視領域の要求（高品質まで作る）
        match pictkura_core::thumbs::process_one(&mut db, &thumbs_dir, thumb_size, *id, true) {
            Ok(pictkura_core::thumbs::ThumbOutcome::Final) => final_count += 1,
            Ok(_) => other += 1,
            Err(e) => println!("  失敗 id={id}: {e}"),
        }
        if let Ok(Some(rec)) = db.get_by_id(*id) {
            if let Some(p) = rec.thumb_path {
                bytes += std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    let total = t.elapsed();
    println!(
        "高品質 {final_count}枚（その他 {other}枚）: {} 全体、1枚あたり {}",
        fmt_ms(total),
        fmt_ms(total / final_count.max(1) as u32)
    );
    println!(
        "サムネイル平均 {:.0}KB",
        bytes as f64 / final_count.max(1) as f64 / 1024.0
    );
    let _ = std::fs::remove_dir_all(&work);
}

/// 動画のコンテナ読み取りを実測する（第9部）。
///
///   cargo run --release --bin bench -- --video D:\movies
fn bench_video(target: &std::path::Path) {
    let mut paths: Vec<PathBuf> = if target.is_dir() {
        std::fs::read_dir(target)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| pictkura_core::video::is_video_path(p))
            .collect()
    } else {
        vec![target.to_path_buf()]
    };
    paths.sort();
    if paths.is_empty() {
        println!("動画が無い: {}", target.display());
        return;
    }
    println!("== pictkura 動画のコンテナ読み取り ==");
    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let t = Instant::now();
        let info = pictkura_core::video::read_info(path);
        let elapsed = t.elapsed();
        // OSのサムネイル機構から絵が借りられるかも見る
        let t2 = Instant::now();
        let thumb = pictkura_core::shell::thumbnail(path, 512);
        let thumb_ms = t2.elapsed();
        let thumb_note = match &thumb {
            Some(img) => {
                // 真っ白／真っ黒に潰れていないかを数値で見る。
                // Shellのalphaを誤って信じると全面白になることがある
                let rgb = img.to_rgb8();
                let (mut min, mut max, mut sum) = (255u32, 0u32, 0u64);
                for px in rgb.pixels() {
                    let l = (u32::from(px[0]) + u32::from(px[1]) + u32::from(px[2])) / 3;
                    min = min.min(l);
                    max = max.max(l);
                    sum += u64::from(l);
                }
                let mean = sum / (rgb.pixels().len().max(1) as u64);
                format!(
                    "絵 {}x{} {} 明るさ min={min} max={max} 平均={mean} → {}",
                    img.width(),
                    img.height(),
                    fmt_ms(thumb_ms),
                    if max - min > 12 {
                        "絵が入っている"
                    } else {
                        "**単色に潰れている**"
                    }
                )
            }
            None => format!("絵なし {}", fmt_ms(thumb_ms)),
        };
        match info {
            Some(i) => {
                let secs = i.duration_ms.unwrap_or(0) as f64 / 1000.0;
                let taken = match i.taken_at_ms {
                    Some(ms) => chrono::DateTime::from_timestamp_millis(ms)
                        .map(|d| {
                            d.with_timezone(&chrono::Local)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        })
                        .unwrap_or_else(|| "変換不可".into()),
                    None => "不明".into(),
                };
                println!(
                    "  {:>8}  {}x{}  {:.1}秒  撮影 {}  再生 {}  {name}",
                    fmt_ms(elapsed),
                    i.width,
                    i.height,
                    secs,
                    taken,
                    if pictkura_core::video::plays_in_webview(path) {
                        "内"
                    } else {
                        "外"
                    },
                );
                println!("            {thumb_note}");
            }
            None => println!(
                "  {:>8}  コンテナは対象外  {name}
            {thumb_note}",
                fmt_ms(elapsed)
            ),
        }
    }
}

/// OSのプロパティから素性を借りられるか実測する（第9部 段階H）。
///
/// 一番大事なのは**クラウドにしか実体が無いファイルを落とさずに済むか**なので、
/// 問い合わせの前後でファイル属性を並べて出す。属性が変わっていなければ
/// ハイドレート（実体のダウンロード）は起きていない。
fn bench_shell_meta(target: &std::path::Path) {
    #[cfg(windows)]
    fn attrs(path: &std::path::Path) -> u32 {
        use std::os::windows::fs::MetadataExt;
        std::fs::metadata(path)
            .map(|m| m.file_attributes())
            .unwrap_or(0)
    }
    #[cfg(not(windows))]
    fn attrs(_path: &std::path::Path) -> u32 {
        0
    }

    let mut paths: Vec<PathBuf> = if target.is_dir() {
        std::fs::read_dir(target)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect()
    } else {
        vec![target.to_path_buf()]
    };
    paths.sort();
    if paths.is_empty() {
        println!("ファイルが無い: {}", target.display());
        return;
    }

    println!("== pictkura OSプロパティ読み取り ==");
    println!("{:<34} {:>8}  結果", "ファイル", "時間");
    for path in &paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let cloud = pictkura_core::cloud::is_cloud_only_path(path);
        let before = attrs(path);
        let t = Instant::now();
        let meta = pictkura_core::shell::metadata(path);
        let elapsed = fmt_ms(t.elapsed());
        let after = attrs(path);

        let body = match meta {
            Some(m) => {
                let size = if m.width > 0 {
                    format!("{}x{}", m.width, m.height)
                } else {
                    "寸法なし".to_string()
                };
                let taken = m
                    .taken_at_ms
                    .map(|ms| {
                        chrono::DateTime::from_timestamp_millis(ms)
                            .map(|d| {
                                d.with_timezone(&chrono::Local)
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string()
                            })
                            .unwrap_or_else(|| ms.to_string())
                    })
                    .unwrap_or_else(|| "撮影日時なし".to_string());
                let dur = m
                    .duration_ms
                    .map(|ms| format!("{:.1}秒", ms as f64 / 1000.0))
                    .unwrap_or_else(|| "長さなし".to_string());
                format!("{size} {taken} {dur}")
            }
            None => "何も取れない".to_string(),
        };
        let hydrated = if before != after {
            format!("  ⚠ 属性が変わった 0x{before:08x}→0x{after:08x}")
        } else if cloud {
            "  （クラウドのみ・落ちていない）".to_string()
        } else {
            String::new()
        };
        println!("{name:<34} {elapsed:>8}  {body}{hydrated}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(target) = arg_value(&args, "--shell-dump") {
        let path = std::path::Path::new(&target);
        let props = pictkura_core::shell::dump_properties(path);
        println!("== {} 件 ==", props.len());
        for (name, value) in props {
            println!("  {name:<42} = {value}");
        }
        return;
    }
    if let Some(target) = arg_value(&args, "--shell-meta") {
        bench_shell_meta(std::path::Path::new(&target));
        return;
    }
    if let Some(target) = arg_value(&args, "--video") {
        bench_video(std::path::Path::new(&target));
        return;
    }
    if let Some(dir) = arg_value(&args, "--pipeline") {
        let size = arg_value(&args, "--size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(512);
        bench_pipeline(std::path::Path::new(&dir), size);
        return;
    }
    if let Some(dir) = arg_value(&args, "--decode-dir") {
        let size = arg_value(&args, "--size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(512);
        bench_decode_dir(std::path::Path::new(&dir), size);
        return;
    }
    if let Some(dir) = arg_value(&args, "--thumb-dir") {
        let size = arg_value(&args, "--size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(512);
        bench_thumb_dir(std::path::Path::new(&dir), size);
        return;
    }
    if let Some(dir) = arg_value(&args, "--avif-dir") {
        bench_avif_dir(std::path::Path::new(&dir));
        return;
    }
    if let Some(file) = arg_value(&args, "--avif") {
        bench_avif(
            std::path::Path::new(&file),
            arg_value(&args, "--out").as_deref(),
        );
        return;
    }
    if let Some(dir) = arg_value(&args, "--heif-encode") {
        bench_heif_encode(std::path::Path::new(&dir));
        return;
    }
    if let Some(dir) = arg_value(&args, "--heic-dir") {
        bench_heif_dir(std::path::Path::new(&dir));
        return;
    }
    if let Some(file) = arg_value(&args, "--heic") {
        bench_heif(std::path::Path::new(&file));
        return;
    }
    if let Some(dir) = arg_value(&args, "--raw-dir") {
        bench_raw_dir(std::path::Path::new(&dir));
        return;
    }
    if let Some(dir) = arg_value(&args, "--display-dir") {
        bench_display_dir(std::path::Path::new(&dir));
        return;
    }
    if let Some(file) = arg_value(&args, "--raw") {
        bench_raw(std::path::Path::new(&file));
        return;
    }
    if let Some(dir) = arg_value(&args, "--browse") {
        bench_browse(std::path::Path::new(&dir));
        return;
    }
    let count: u64 = arg_value(&args, "--count")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000_000);
    let db_path = arg_value(&args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("pictkura-bench.db"));
    let force_legacy = args.iter().any(|a| a == "--legacy");
    let keep = args.iter().any(|a| a == "--keep");

    // 前回のDBが残っていたら消してクリーンな状態から始める
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", db_path.display()));
    }

    println!("== pictkura 合成データベンチ ==");
    println!("件数: {count}  DB: {}", db_path.display());

    // スキーマ初期化はDb::openに任せ、バルク投入は生接続で行う
    drop(Db::open(&db_path).expect("DB初期化に失敗"));
    let conn = Connection::open(&db_path).expect("DBを開けない");
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "synchronous", "OFF").unwrap();

    // 投入を速くするためインデックスと検索索引のトリガを外す。
    // インデックスは投入後に張り直し（構築時間も計測対象）、トリガは
    // 次のDb::open（CREATE TRIGGER IF NOT EXISTS）で自動的に戻る
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_media_day_sort;
         DROP INDEX IF EXISTS idx_media_fav_day;
         DROP INDEX IF EXISTS idx_media_taken_at;
         DROP TRIGGER IF EXISTS media_fts_ai;
         DROP TRIGGER IF EXISTS media_fts_ad;
         DROP TRIGGER IF EXISTS media_fts_au;",
    )
    .unwrap();
    // カメラ表を用意する（合成レコードはこのIDを参照する）
    for name in CAMERAS {
        conn.execute("INSERT OR IGNORE INTO cameras (name) VALUES (?1)", [name])
            .unwrap();
    }

    let insert_start = Instant::now();
    insert_synthetic(&conn, count);
    println!(
        "投入: {} ({:.0}件/秒)",
        fmt_ms(insert_start.elapsed()),
        count as f64 / insert_start.elapsed().as_secs_f64()
    );

    let index_start = Instant::now();
    conn.execute_batch(
        "CREATE INDEX idx_media_taken_at ON media(taken_at_ms DESC);
         CREATE INDEX idx_media_day_sort
             ON media(day_key DESC, COALESCE(taken_at_ms, mtime_ms) DESC, id DESC);
         CREATE INDEX idx_media_fav_day
             ON media(day_key DESC, COALESCE(taken_at_ms, mtime_ms) DESC, id DESC)
             WHERE favorite = 1;",
    )
    .unwrap();
    println!("インデックス構築: {}", fmt_ms(index_start.elapsed()));
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
    drop(conn);

    let size_mb = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0) as f64 / 1e6;
    println!("DBサイズ: {size_mb:.0}MB");

    // ---- 検索インデックス（第4部 段階D）----
    // Db::openでトリガが張り直され、parent_dirの移行も走る。
    // 既存ライブラリの後追い索引化（アプリではバックグラウンドで進む処理）を計測する
    let mut db = Db::open(&db_path).expect("DBを開けない");
    let (_, max_id) = db.fts_build_range().unwrap();
    let t = Instant::now();
    while db.fts_build_step(max_id, 20_000).unwrap().1 < max_id {}
    println!("検索インデックス構築: {}", fmt_ms(t.elapsed()));
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
    }
    let indexed_mb = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0) as f64 / 1e6;
    println!(
        "DBサイズ(索引込み): {indexed_mb:.0}MB (+{:.0}MB)",
        indexed_mb - size_mb
    );

    // インクリメンタル検索の実態に合わせ、1文字・打鍵途中の前方一致も測る
    for probe in [
        "沖",
        "沖縄",
        "旅行",
        "運動会",
        "IMG_00000",
        "IMG_000001",
        "camera:α7",
    ] {
        let query = pictkura_core::parse_query(probe, false);
        let t = Instant::now();
        let hits = db.search_count(&query).unwrap();
        let count_ms = fmt_ms(t.elapsed());
        let t = Instant::now();
        let summary = db.search_summary(&query).unwrap();
        let summary_ms = fmt_ms(t.elapsed());
        let day_ms = match summary.iter().max_by_key(|d| d.count) {
            Some(day) => {
                let t = Instant::now();
                db.search_day(day.day_key, &query).unwrap();
                fmt_ms(t.elapsed())
            }
            None => "-".to_string(),
        };
        println!(
            "検索 {probe:12}: {hits}件 / count {count_ms} / summary {summary_ms} ({}日) / day {day_ms}",
            summary.len()
        );
    }

    let t = Instant::now();
    let cameras = db.list_cameras().unwrap();
    println!(
        "list_cameras: {}機種 {}",
        cameras.len(),
        fmt_ms(t.elapsed())
    );

    // ---- 計測（アプリと同じDb API経由）----
    let db = Db::open(&db_path).expect("DBを開けない");

    let t = Instant::now();
    let n = db.count().unwrap();
    println!("count: {n}件 {}", fmt_ms(t.elapsed()));

    let t = Instant::now();
    let summary = db.timeline_summary(false).unwrap();
    println!(
        "timeline_summary: {}日分 {}  ← 起動時にUIへ渡す索引",
        summary.len(),
        fmt_ms(t.elapsed())
    );

    let busiest = summary.iter().max_by_key(|d| d.count).unwrap().clone();
    let t = Instant::now();
    let day = db.list_day(busiest.day_key, false).unwrap();
    println!(
        "list_day(最多日 {} {}枚): {}件 {}  ← スクロールで1日分を取得",
        busiest.day_key,
        busiest.count,
        day.len(),
        fmt_ms(t.elapsed())
    );

    let t = Instant::now();
    let memories = db.list_memories(24).unwrap();
    println!(
        "list_memories: {}件 {}",
        memories.len(),
        fmt_ms(t.elapsed())
    );

    let t = Instant::now();
    let favs = db.count_favorites().unwrap();
    println!("count_favorites: {favs}件 {}", fmt_ms(t.elapsed()));

    let t = Instant::now();
    let fav_summary = db.timeline_summary(true).unwrap();
    println!(
        "timeline_summary(★のみ): {}日分 {}",
        fav_summary.len(),
        fmt_ms(t.elapsed())
    );

    // 差分ゼロ再スキャンのSQL差分検知（一時テーブル＋外部結合）。
    // 1000万件でScannedFileベクタ自体が数GBになるため200万件までに制限
    if count <= 2_000_000 {
        let files: Vec<ScannedFile> = synthetic_files(count).collect();
        let root = PathBuf::from("D:/bench");
        let mut db = Db::open(&db_path).unwrap();
        let t = Instant::now();
        let (a, c, r) = db
            .apply_scan(
                &files,
                std::slice::from_ref(&root),
                std::slice::from_ref(&root),
                None,
            )
            .unwrap();
        println!(
            "apply_scan(差分ゼロ再スキャン): 追加{a} 変更{c} 削除{r} {}",
            fmt_ms(t.elapsed())
        );
    }

    // 旧API: 全件転送（比較用）。大規模ではメモリを食い潰すため既定でスキップ
    if count <= 2_000_000 || force_legacy {
        let t = Instant::now();
        let all = db.list_all().unwrap();
        println!(
            "[旧] list_all: {}件 {}  ← 段階A以前は起動のたびにこれを全件JSON化していた",
            all.len(),
            fmt_ms(t.elapsed())
        );
    } else {
        println!("[旧] list_all: スキップ（--legacyで強制実行可）");
    }

    if !keep {
        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", db_path.display()));
        }
        println!("(一時DBを削除。--keepで保持)");
    }
}

/// 合成データのカメラ機種（検索・ファセットの計測用）。
const CAMERAS: [&str; 5] = [
    "SONY ILCE-7M3",
    "SONY α7R V",
    "Apple iPhone 15 Pro",
    "NIKON D850",
    "Canon EOS R5",
];

/// 合成データのフォルダ名。日本語の中間一致検索を実測するため和名を混ぜる。
const FOLDERS: [&str; 8] = [
    "2019-08 沖縄旅行",
    "家族写真",
    "運動会",
    "花火大会",
    "DCIM/100MSDCF",
    "2022 京都",
    "Screenshots",
    "camera-roll",
];

/// 合成レコード1件分（投入とapply_scanベンチの両方から使い、乱数列を一致させる）。
struct SyntheticRecord {
    path: String,
    size: i64,
    mtime_ms: i64,
    taken_at_ms: Option<i64>,
    width: i64,
    height: i64,
    favorite: bool,
    /// camerasテーブルのID（1始まり）
    camera_id: i64,
}

/// i番目の合成レコードを生成する（決定的）。
/// 2010〜2025年へ「最近の年ほど多い」重み付きで分布させ、5%は撮影日時なし（mtimeのみ）。
fn synthetic_record(i: u64, rng: &mut Rng) -> SyntheticRecord {
    // 重み: year_index+1 に比例（2010年=1, 2025年=16）
    let years = 16u64;
    let total_weight = years * (years + 1) / 2;
    let mut pick = rng.range(total_weight);
    let mut year_idx = 0;
    for y in 0..years {
        if pick < y + 1 {
            year_idx = y;
            break;
        }
        pick -= y + 1;
    }
    // 2010-01-01 00:00 UTC を起点に年365日換算で日時を作る（ベンチ用の近似で十分）
    const DAY_MS: i64 = 24 * 3600 * 1000;
    const BASE_MS: i64 = 1_262_304_000_000; // 2010-01-01T00:00:00Z
    let day_in_year = rng.range(365) as i64;
    let ms_in_day = rng.range(DAY_MS as u64) as i64;
    let taken = BASE_MS + year_idx as i64 * 365 * DAY_MS + day_in_year * DAY_MS + ms_in_day;
    let mtime = taken + rng.range(30 * DAY_MS as u64) as i64; // 取り込みは撮影から30日以内
    let has_exif = rng.range(100) < 95;
    let size = 1_000_000 + rng.range(9_000_000) as i64;
    let (width, height) = match rng.range(4) {
        0 => (4032, 3024),
        1 => (3024, 4032),
        2 => (1920, 1080),
        _ => (6000, 4000),
    };
    let favorite = rng.range(100) < 1; // 1%をお気に入りに
    let folder = FOLDERS[rng.range(FOLDERS.len() as u64) as usize];
    let camera_id = rng.range(CAMERAS.len() as u64) as i64 + 1;
    SyntheticRecord {
        // 実ライブラリと同じく「ルート／年フォルダ／イベント名」の3階層にする
        path: format!("D:/bench/{:03}/{folder}/IMG_{i:09}.jpg", i % 512),
        size,
        mtime_ms: mtime,
        taken_at_ms: has_exif.then_some(taken),
        width,
        height,
        favorite,
        camera_id,
    }
}

/// apply_scanベンチ用のスキャン結果（投入済みレコードとpath/size/mtimeが一致する）。
fn synthetic_files(count: u64) -> impl Iterator<Item = ScannedFile> {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    (0..count).map(move |i| {
        let r = synthetic_record(i, &mut rng);
        ScannedFile {
            path: PathBuf::from(r.path),
            size: r.size,
            mtime_ms: r.mtime_ms,
        }
    })
}

/// レコードを直接INSERTする（メタデータ抽出・サムネイル生成済みの状態を再現）。
fn insert_synthetic(conn: &Connection, count: u64) {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    const BATCH: u64 = 100_000;
    let mut inserted = 0u64;
    while inserted < count {
        let n = BATCH.min(count - inserted);
        conn.execute_batch("BEGIN").unwrap();
        {
            let mut stmt = conn
                .prepare_cached(
                    "INSERT INTO media
                     (path, size, mtime_ms, width, height, taken_at_ms, day_key,
                      thumb_path, thumb_state, favorite, camera_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                        CAST(strftime('%Y%m%d', COALESCE(?6, ?3)/1000, 'unixepoch', 'localtime') AS INTEGER),
                        ?7, 2, ?8, ?9)",
                )
                .unwrap();
            for i in inserted..inserted + n {
                let r = synthetic_record(i, &mut rng);
                stmt.execute(params![
                    r.path,
                    r.size,
                    r.mtime_ms,
                    r.width,
                    r.height,
                    r.taken_at_ms,
                    format!("thumbs/{:02x}/{i}.webp", i % 256),
                    r.favorite as i64,
                    r.camera_id,
                ])
                .unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        inserted += n;
        if inserted.is_multiple_of(1_000_000) || inserted == count {
            println!("  … {inserted}/{count}");
        }
    }
}
