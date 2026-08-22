//! `media://` カスタムプロトコルのURL解釈とMIME判定。
//!
//! Tauri v2 のカスタムプロトコルURLはOSによって形が異なる:
//! - Windows: `http://media.localhost/full/123`
//! - macOS/Linux: `media://localhost/full/123`
//!
//! どちらの形でも同じように解釈できるよう、末尾のパスセグメントだけを見る。

/// `media://` で**何を配信するか**。
///
/// ファイルそのものの種類（画像 / RAW / 動画）は別物で、そちらは
/// [`crate::MediaKind`]（`media.kind` 列）。名前が紛れないよう分けてある。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeKind {
    /// オリジナルファイル
    Full,
    /// サムネイル
    Thumb,
    /// 動画の実体（第9部）。`<video>` へ渡すのでRangeリクエストに応える必要がある
    Video,
}

/// URLが指しているもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaTarget {
    /// ライブラリの画像（DBのIDで引く）
    Library { kind: ServeKind, id: i64 },
    /// 取り込み**前**のファイル（第5部 段階E: 取り込みウィザードのプレビュー）。
    /// DBに無いのでパスで指すしかないが、パスはWindowsの `\` や `:` を含み
    /// URLのエスケープ規則とぶつかるため、**UTF-8バイト列の16進**で運ぶ。
    /// これならURLに現れるのは `[0-9a-f]` だけで、どのWebViewの正規化とも衝突しない。
    Source { path: std::path::PathBuf },
}

/// 16進文字列をバイト列へ戻す（奇数長・16進以外はNone）。
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

/// パーセントエンコーディング（%2F等）を復号する。
/// TauriのconvertFileSrcはパス全体をencodeURIComponentするため、区切りの `/` も
/// `%2F` にエンコードされて届く。復号してから解釈する必要がある。
fn percent_decode(s: &str) -> String {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// メディアURLが指す対象を取り出す。
///
/// 期待する形式（クエリ文字列は無視）:
/// - `.../full/<id>` `.../thumb/<id>` `.../video/<id>` … ライブラリのメディア
/// - `.../src/<16進のパス>` … 取り込み元のファイル
pub fn parse_media_url(url: &str) -> Option<MediaTarget> {
    let path = percent_decode(url.split(['?', '#']).next()?);
    let mut segments = path.split('/').filter(|s| !s.is_empty()).rev();
    let payload = segments.next()?;
    match segments.next()? {
        "full" => Some(MediaTarget::Library {
            kind: ServeKind::Full,
            id: payload.parse().ok()?,
        }),
        "thumb" => Some(MediaTarget::Library {
            kind: ServeKind::Thumb,
            id: payload.parse().ok()?,
        }),
        "video" => Some(MediaTarget::Library {
            kind: ServeKind::Video,
            id: payload.parse().ok()?,
        }),
        "src" => {
            let bytes = decode_hex(payload)?;
            let text = String::from_utf8(bytes).ok()?;
            (!text.is_empty()).then(|| MediaTarget::Source {
                path: std::path::PathBuf::from(text),
            })
        }
        _ => None,
    }
}

/// 拡張子からMIMEタイプを判定する。
pub fn mime_for_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        // ベクタはブラウザがそのまま描くので、原本を返す
        Some("svg") => "image/svg+xml",
        // AVIFもWebViewが直接描ける（サムネイル生成だけOSのデコーダを使う）
        Some("avif") => "image/avif",
        // TIFFはWebViewが描けないので原本は返さない（JPEGへ詰め直す）が、
        // 取り込みウィザードのプレビュー等で型が要る場面のために持つ
        Some("tif") | Some("tiff") => "image/tiff",
        // 動画（第9部）。`<video>` は Content-Type を見てコンテナを選ぶので、
        // octet-stream のままだと再生が始まらない。
        // .mov は QuickTime だが中身はISO-BMFFで、Chromiumは video/quicktime を
        // MP4として扱う（実測で再生できた）
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        // 以下はWebViewが再生できないコンテナ。「既定のアプリで開く」へ逃がすが、
        // 型が要る場面（ドラッグ等）のために持っておく
        Some("avi") => "video/x-msvideo",
        Some("mts") | Some("m2ts") => "video/mp2t",
        Some("mkv") => "video/x-matroska",
        Some("3gp") => "video/3gpp",
        Some("wmv") => "video/x-ms-wmv",
        Some("mpg") | Some("mpeg") => "video/mpeg",
        _ => "application/octet-stream",
    }
}

/// `Range` ヘッダの解釈結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeRequest {
    /// 応えられる区間（開始・終了とも**含む**）
    Satisfiable { start: u64, end: u64 },
    /// 形は分かるが応えられない（範囲外・長さ0のファイル）→ 416
    Unsatisfiable,
    /// **読めないヘッダ**（単位が違う・書式が壊れている）→ 無かったことにする。
    /// RFC 9110 は「理解できない Range は無視して普通に返せ」と定めている。
    /// 416を返すと、Rangeを付けたばかりに**ファイルが取れなくなる**
    Ignore,
}

/// `Range: bytes=...` ヘッダを1区間に解釈する。
///
/// `<video>` はシーク・先読みのたびに部分要求を出すので、これに応えられないと
/// 再生自体が成立しない（Chromiumは206が返らないとシークを諦める）。
///
/// - `bytes=0-` … 先頭から末尾まで
/// - `bytes=100-199` … 100〜199バイト目
/// - `bytes=-500` … 末尾500バイト
///
/// 複数区間（`bytes=0-99,200-299`）は**最初の区間だけ**返す。multipart/byteranges を
/// 組み立てる価値がない——ブラウザの動画再生は単一区間しか投げてこない。
pub fn parse_range(header: &str, len: u64) -> RangeRequest {
    let Some(spec) = header.trim().strip_prefix("bytes=") else {
        // `items=0-10` のような未知の単位。無視して丸ごと返す
        return RangeRequest::Ignore;
    };
    let Some(first) = spec.split(',').next().map(str::trim) else {
        return RangeRequest::Ignore;
    };
    let Some((start_text, end_text)) = first.split_once('-') else {
        // `bytes=100` のようにハイフンが無い＝書式違反
        return RangeRequest::Ignore;
    };
    let (start_text, end_text) = (start_text.trim(), end_text.trim());
    let (start, end) = if start_text.is_empty() {
        // 末尾からのNバイト
        let Ok(suffix) = end_text.parse::<u64>() else {
            return RangeRequest::Ignore;
        };
        if len == 0 || suffix == 0 {
            return RangeRequest::Unsatisfiable;
        }
        (len.saturating_sub(suffix), len - 1)
    } else {
        let Ok(start) = start_text.parse::<u64>() else {
            return RangeRequest::Ignore;
        };
        let end = match end_text {
            "" => len.saturating_sub(1),
            text => match text.parse::<u64>() {
                Ok(end) => end.min(len.saturating_sub(1)),
                Err(_) => return RangeRequest::Ignore,
            },
        };
        (start, end)
    };
    if len == 0 || start >= len || start > end {
        return RangeRequest::Unsatisfiable;
    }
    RangeRequest::Satisfiable { start, end }
}

/// Rangeヘッダを見て「どう返すか」を決めた結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeReply {
    /// 丸ごと返す（200 OK）
    Whole,
    /// 部分応答（206 Partial Content）。開始・終了とも**含む**
    Partial { start: u64, end: u64 },
    /// 満たせない（416 Range Not Satisfiable）
    Unsatisfiable,
}

/// 動画の要求に対する応答の作り方を決める。
///
/// - `chunk` … 1回の応答で返す最大バイト数。`<video>` は `bytes=0-`（末尾まで）を
///   投げてくるが、**要求より少なく返してよい**のがRangeの規則なので、ここで刻む。
///   刻まないと2GBの動画を丸ごとメモリへ載せることになる
/// - `whole_limit` … Rangeヘッダが無いときに丸ごと返して良い上限
pub fn plan_range(header: Option<&str>, len: u64, chunk: u64, whole_limit: u64) -> RangeReply {
    debug_assert!(chunk > 0);
    // ヘッダが無いときと「読めないヘッダ」は同じ扱い（RFC 9110: 理解できない
    // Range は無視する）
    let request = header.map_or(RangeRequest::Ignore, |h| parse_range(h, len));
    match request {
        RangeRequest::Satisfiable { start, end } => RangeReply::Partial {
            start,
            end: end.min(start + chunk - 1),
        },
        RangeRequest::Unsatisfiable => RangeReply::Unsatisfiable,
        // 小さければ丸ごと返す
        RangeRequest::Ignore if len <= whole_limit => RangeReply::Whole,
        // 大きいファイルは先頭だけを206で返す。**規格からは外れている**
        // （206は要求されたRangeへの応答なので、要求が無ければ200であるべき）。
        // それでもこうするのは、200で正しく返すには2GBを丸ごとメモリへ載せる
        // 必要があり、アプリが落ちる方が確実に困るため。Tauriのプロトコル応答は
        // `Vec<u8>` 固定で**ストリームを返せない**ので逃げ道が無い。
        // 実害が無いのは、この経路の相手が**自前の `<video>` だけ**で、
        // Chromiumのメディア再生は必ず `Range: bytes=0-` を付けてくるから
        // （＝この枝はほぼ死んでいる）。ストリーム応答が使えるようになったら
        // 200へ戻す
        RangeRequest::Ignore => RangeReply::Partial {
            start: 0,
            end: (chunk - 1).min(len - 1),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 追加した形式のmimeを返す() {
        assert_eq!(mime_for_path(std::path::Path::new("a.bmp")), "image/bmp");
        assert_eq!(mime_for_path(std::path::Path::new("a.gif")), "image/gif");
        assert_eq!(
            mime_for_path(std::path::Path::new("a.SVG")),
            "image/svg+xml"
        );
        assert_eq!(mime_for_path(std::path::Path::new("a.avif")), "image/avif");
        assert_eq!(mime_for_path(std::path::Path::new("a.tiff")), "image/tiff");
    }

    use std::path::Path;

    fn library(kind: ServeKind, id: i64) -> Option<MediaTarget> {
        Some(MediaTarget::Library { kind, id })
    }

    #[test]
    fn windows形式のurlを解釈できる() {
        assert_eq!(
            parse_media_url("http://media.localhost/full/123"),
            library(ServeKind::Full, 123)
        );
        assert_eq!(
            parse_media_url("http://media.localhost/thumb/45"),
            library(ServeKind::Thumb, 45)
        );
    }

    #[test]
    fn mac_linux形式のurlを解釈できる() {
        assert_eq!(
            parse_media_url("media://localhost/full/7"),
            library(ServeKind::Full, 7)
        );
    }

    #[test]
    fn パーセントエンコードされたurlを解釈できる() {
        // convertFileSrcはパス全体をencodeURIComponentする
        assert_eq!(
            parse_media_url("http://media.localhost/thumb%2F14"),
            library(ServeKind::Thumb, 14)
        );
        assert_eq!(
            parse_media_url("media://localhost/full%2F3"),
            library(ServeKind::Full, 3)
        );
    }

    #[test]
    fn クエリ文字列は無視される() {
        assert_eq!(
            parse_media_url("http://media.localhost/full/123?v=2"),
            library(ServeKind::Full, 123)
        );
    }

    #[test]
    fn 不正なurlはnone() {
        assert_eq!(parse_media_url("http://media.localhost/full/abc"), None);
        assert_eq!(parse_media_url("http://media.localhost/unknown/1"), None);
        assert_eq!(parse_media_url("http://media.localhost/"), None);
        assert_eq!(parse_media_url(""), None);
    }

    #[test]
    fn 取り込み元のパスを16進から復元できる() {
        // "D:\DCIM\a.jpg" のUTF-8バイト列を16進にしたもの
        let hex: String = r"D:\DCIM.jpg"
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            parse_media_url(&format!("http://media.localhost/src/{hex}?v=1")),
            Some(MediaTarget::Source {
                path: std::path::PathBuf::from(r"D:\DCIM.jpg")
            })
        );
        // 日本語（マルチバイト）のパスも往復する
        let hex_ja: String = "E:/写真/海.jpg"
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            parse_media_url(&format!("media://localhost/src/{hex_ja}")),
            Some(MediaTarget::Source {
                path: std::path::PathBuf::from("E:/写真/海.jpg")
            })
        );
    }

    #[test]
    fn 壊れた16進のsrcは拒否する() {
        assert_eq!(parse_media_url("http://media.localhost/src/zz"), None);
        assert_eq!(parse_media_url("http://media.localhost/src/abc"), None);
        assert_eq!(parse_media_url("http://media.localhost/src/"), None);
    }

    #[test]
    fn 動画のurlを解釈できる() {
        assert_eq!(
            parse_media_url("http://media.localhost/video/9"),
            library(ServeKind::Video, 9)
        );
    }

    #[test]
    fn 動画のmimeを返す() {
        assert_eq!(mime_for_path(Path::new("a.MP4")), "video/mp4");
        assert_eq!(mime_for_path(Path::new("a.mov")), "video/quicktime");
        assert_eq!(mime_for_path(Path::new("a.m2ts")), "video/mp2t");
    }

    fn sat(start: u64, end: u64) -> RangeRequest {
        RangeRequest::Satisfiable { start, end }
    }

    #[test]
    fn rangeヘッダを解釈する() {
        assert_eq!(parse_range("bytes=0-", 1000), sat(0, 999));
        assert_eq!(parse_range("bytes=100-199", 1000), sat(100, 199));
        assert_eq!(parse_range("bytes=900-", 1000), sat(900, 999));
        // 末尾からの指定
        assert_eq!(parse_range("bytes=-500", 1000), sat(500, 999));
        assert_eq!(parse_range("bytes=-5000", 1000), sat(0, 999));
        // 終端がファイル長を超えたら丸める
        assert_eq!(parse_range("bytes=0-99999", 1000), sat(0, 999));
        // 複数区間は最初だけ
        assert_eq!(parse_range("bytes=0-99,200-299", 1000), sat(0, 99));
    }

    #[test]
    fn 満たせないrangeは416() {
        assert_eq!(
            parse_range("bytes=1000-", 1000),
            RangeRequest::Unsatisfiable
        );
        assert_eq!(
            parse_range("bytes=1500-1600", 1000),
            RangeRequest::Unsatisfiable
        );
        assert_eq!(
            parse_range("bytes=200-100", 1000),
            RangeRequest::Unsatisfiable
        );
        assert_eq!(parse_range("bytes=-0", 1000), RangeRequest::Unsatisfiable);
        assert_eq!(parse_range("bytes=0-", 0), RangeRequest::Unsatisfiable);
    }

    #[test]
    fn 読めないrangeは無視する() {
        // RFC 9110: 理解できない Range は無視して普通に返す。416で突き返すと、
        // Rangeを付けたせいでファイルが取れなくなる
        assert_eq!(parse_range("items=0-10", 1000), RangeRequest::Ignore);
        assert_eq!(parse_range("bytes=abc", 1000), RangeRequest::Ignore);
        assert_eq!(parse_range("bytes=100", 1000), RangeRequest::Ignore);
        assert_eq!(parse_range("", 1000), RangeRequest::Ignore);
        // 無視された要求は「ヘッダが無い」のと同じ＝小さいファイルは丸ごと
        assert_eq!(
            plan_range(Some("items=0-10"), 1000, CHUNK, WHOLE),
            RangeReply::Whole
        );
    }

    const CHUNK: u64 = 4 * 1024 * 1024;
    const WHOLE: u64 = 64 * 1024 * 1024;

    #[test]
    fn rangeが無ければ小さいファイルは丸ごと返す() {
        assert_eq!(plan_range(None, 1000, CHUNK, WHOLE), RangeReply::Whole);
        assert_eq!(plan_range(None, WHOLE, CHUNK, WHOLE), RangeReply::Whole);
        // 空ファイルも200（0バイト）で返す
        assert_eq!(plan_range(None, 0, CHUNK, WHOLE), RangeReply::Whole);
    }

    #[test]
    fn rangeが無くても大きいファイルは先頭だけ返す() {
        assert_eq!(
            plan_range(None, WHOLE + 1, CHUNK, WHOLE),
            RangeReply::Partial {
                start: 0,
                end: CHUNK - 1
            }
        );
    }

    #[test]
    fn 要求が末尾までなら刻んで返す() {
        // `<video>` の最初の要求。2GBを丸ごと載せず4MiBだけ返す
        let len = 2 * 1024 * 1024 * 1024;
        assert_eq!(
            plan_range(Some("bytes=0-"), len, CHUNK, WHOLE),
            RangeReply::Partial {
                start: 0,
                end: CHUNK - 1
            }
        );
        // シーク後の続き（途中から末尾まで）も同じ幅で刻む
        assert_eq!(
            plan_range(Some("bytes=1000000000-"), len, CHUNK, WHOLE),
            RangeReply::Partial {
                start: 1_000_000_000,
                end: 1_000_000_000 + CHUNK - 1
            }
        );
    }

    #[test]
    fn 短い要求はそのまま返す() {
        // moovを探す小さな要求は刻まれない
        assert_eq!(
            plan_range(Some("bytes=0-1023"), 100_000_000, CHUNK, WHOLE),
            RangeReply::Partial {
                start: 0,
                end: 1023
            }
        );
        // 末尾に届く要求はファイル長で止まる
        assert_eq!(
            plan_range(Some("bytes=99999000-"), 100_000_000, CHUNK, WHOLE),
            RangeReply::Partial {
                start: 99_999_000,
                end: 99_999_999
            }
        );
    }

    #[test]
    fn 満たせない要求は416を返す計画になる() {
        assert_eq!(
            plan_range(Some("bytes=200000-"), 1000, CHUNK, WHOLE),
            RangeReply::Unsatisfiable
        );
    }

    #[test]
    fn mime判定() {
        assert_eq!(mime_for_path(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_for_path(Path::new("a.webp")), "image/webp");
        assert_eq!(
            mime_for_path(Path::new("a.bin")),
            "application/octet-stream"
        );
    }
}
