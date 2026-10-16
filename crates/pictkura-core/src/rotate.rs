//! 写真を回す: JPEG の EXIF の向き（IFD0 の 0x0112）の2バイトだけをその場で書き換える
//! （`dev/adr.file-info-and-context-menu.md` の B-2、R）。
//!
//! 画素には触らない（劣化しない）。同じ実体を保つ（送り出しのハードリンク）ため、一時ファイルへ
//! 書いて置き換えることもしない。書き換える前の姿は控え（`edited_files`、[`crate::db::EditRecord`]）に
//! 残し、「元に戻す」と取り込みの照合がそれを読む。
//!
//! 手順（[`apply`]）:
//! 1. 錠の外で [`prepare`] が断る場所を見て、中身の要約を取る
//! 2. 呼び出し側が走査の錠と `Db` の錠を取ってから [`apply`]。`media` の版と、ファイルの大きさ・
//!    更新日時・向きの2バイトが [`prepare`] のときのままか確かめる
//! 3. 「これから書く」を控えに確定させてから、2バイトを書いて `sync_all`、更新日時を付けて読み返す
//! 4. `media` と控えを1つのトランザクションで確定させる。どこで落ちても書き戻す。
//!    書き戻しの前に落ちたら、次の起動で [`recover`] が書き戻す

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::db::{Db, DbError, EditRecord, EditSettle};

/// 回す向き。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// 時計回りに90度
    Right,
    /// 反時計回りに90度
    Left,
    /// pictkura が回す前の向きへ戻す
    Restore,
}

/// 回せなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum RotateError {
    #[error("not a JPEG")]
    NotJpeg,
    /// EXIF の IFD0 に向き（SHORT・1件・1〜8）が無い
    #[error("the file declares no EXIF orientation")]
    NoOrientation,
    /// XMP（中か横の `.xmp`）が向きを持っている。EXIF だけ変えると、XMP を優先する道具と食い違う
    #[error("an XMP orientation is present")]
    XmpOrientation,
    /// 実体がクラウドにしか無い（書くと取り寄せが走る）
    #[error("the file is only in the cloud")]
    CloudOnly,
    /// カメラのカードか DCIM の下（取り込み元になりうる場所は書き換えない）
    #[error("the file is on a camera card")]
    CameraCard,
    /// メタデータをまだ読んでいない行
    #[error("the file has not been read yet")]
    NotReady,
    /// 調べてから書くまでの間に、ファイルか行が変わった
    #[error("the file changed meanwhile")]
    Changed,
    /// 戻す控えが無い（回していない・外で書き換えられた）
    #[error("nothing to restore")]
    NothingToRestore,
    /// 更新日時を前より後にできなかった（書き戻した）
    #[error("the modification time did not move")]
    MtimeDidNotMove,
    /// 書いたあとの失敗で、書き戻すのにも失敗した。ファイルは回ったまま
    #[error("the file was changed and could not be put back: {0}")]
    LeftChanged(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// JPEG の中の、向きの2バイト。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    /// ファイルの頭からの位置
    pub offset: u64,
    /// TIFF のバイト順が MM（ビッグエンディアン）
    pub big_endian: bool,
    /// 今の値（1〜8）
    pub value: u16,
}

impl Spot {
    /// `value` をこのファイルのバイト順で並べた2バイト
    pub fn bytes_of(&self, value: u16) -> [u8; 2] {
        if self.big_endian {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        }
    }

    pub fn bytes(&self) -> [u8; 2] {
        self.bytes_of(self.value)
    }

    /// ファイルのバイト順で並んだ2バイトを値へ
    pub fn value_of(&self, bytes: [u8; 2]) -> u16 {
        if self.big_endian {
            u16::from_be_bytes(bytes)
        } else {
            u16::from_le_bytes(bytes)
        }
    }
}

/// ヘッダを読んで分かったこと。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    /// 最初の Exif APP1 の IFD0 にある向き
    pub spot: Option<Spot>,
    /// XMP の APP1 が `tiff:Orientation` を持っている
    pub xmp_orientation: bool,
}

const ORIENTATION_TAG: u16 = 0x0112;
const SHORT: u16 = 3;
const XMP_TAG: &[u8] = b"tiff:Orientation";
/// TIFF の名前空間。接頭辞は `tiff:` とは限らない（`xmlns:foo="…tiff/1.0/"` と `foo:Orientation`。PR の codex P2）
const XMP_TIFF_NS: &[u8] = b"http://ns.adobe.com/tiff/1.0/";
const XMP_ORIENTATION: &[u8] = b":Orientation";
const XMP_HEADER: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// SOS より前に見る区切りの数の上限（壊れたファイルで回り続けない）
const MAX_SEGMENTS: usize = 512;

/// JPEG の頭から SOS の手前までの区切りを読み、向きの位置と XMP の向きを探す。
/// JPEG でなければ既定値（どちらも無し）
pub fn read_header(r: &mut (impl Read + Seek)) -> std::io::Result<Header> {
    let mut out = Header::default();
    let mut soi = [0u8; 2];
    if read_full(r, &mut soi)? < 2 || soi != [0xFF, 0xD8] {
        return Ok(out);
    }
    let mut pos: u64 = 2;
    let mut seen_exif = false;
    for _ in 0..MAX_SEGMENTS {
        // 区切りの前の詰め物（0xFF の並び）を飛ばす
        let mut b = [0u8; 1];
        if read_full(r, &mut b)? < 1 || b[0] != 0xFF {
            return Ok(out);
        }
        pos += 1;
        let marker = loop {
            if read_full(r, &mut b)? < 1 {
                return Ok(out);
            }
            pos += 1;
            if b[0] != 0xFF {
                break b[0];
            }
        };
        match marker {
            // 画像の本体か終わり。ここから先に EXIF は来ない
            0xDA | 0xD9 => return Ok(out),
            // 長さを持たない区切り
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        let mut len = [0u8; 2];
        if read_full(r, &mut len)? < 2 {
            return Ok(out);
        }
        let len = u16::from_be_bytes(len) as u64;
        if len < 2 {
            return Ok(out);
        }
        let body_start = pos + 2;
        let body_len = len - 2;
        if marker == 0xE1 {
            let mut body = vec![0u8; body_len as usize];
            if read_full(r, &mut body)? < body.len() {
                return Ok(out);
            }
            if !seen_exif && body.starts_with(b"Exif\0\0") {
                // **最初の Exif だけを見る**（読む側の道具もそこしか見ない）
                seen_exif = true;
                out.spot = orientation_in_tiff(&body[6..], body_start + 6);
            } else if body.starts_with(XMP_HEADER) && xmp_declares_orientation(body.as_slice()) {
                out.xmp_orientation = true;
            }
        } else {
            r.seek(SeekFrom::Start(body_start + body_len))?;
        }
        pos = body_start + body_len;
    }
    Ok(out)
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

/// TIFF（Exif の中身）の IFD0 から向きを探す。`base` は `tiff` の頭のファイル上の位置
fn orientation_in_tiff(tiff: &[u8], base: u64) -> Option<Spot> {
    let big_endian = match tiff.get(..2)? {
        b"II" => false,
        b"MM" => true,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(i..i + 2)?.try_into().ok()?;
        Some(if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(i..i + 4)?.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    for k in 0..count {
        let entry = ifd + 2 + 12 * k;
        if u16_at(entry)? != ORIENTATION_TAG {
            continue;
        }
        if u16_at(entry + 2)? != SHORT || u32_at(entry + 4)? != 1 {
            return None;
        }
        let value = u16_at(entry + 8)?;
        if !(1..=8).contains(&value) {
            return None;
        }
        return Some(Spot {
            offset: base + (entry + 8) as u64,
            big_endian,
            value,
        });
    }
    None
}

/// 向きの値を回す。左右反転を含む値（2・4・5・7）も、反転を保ったまま回る
pub fn turned(value: u16, turn: Turn) -> u16 {
    // 時計回りに90度ずつ: 1→6→3→8→1、反転側は 2→7→4→5→2
    const RIGHT: [u16; 9] = [0, 6, 7, 8, 5, 2, 3, 4, 1];
    const LEFT: [u16; 9] = [0, 8, 5, 6, 7, 4, 1, 2, 3];
    let v = value as usize;
    if !(1..=8).contains(&v) {
        return value;
    }
    match turn {
        Turn::Right => RIGHT[v],
        Turn::Left => LEFT[v],
        Turn::Restore => value,
    }
}

/// 90度系（5〜8）か。縦横の入れ替わりを見る
fn is_quarter(value: u16) -> bool {
    (5..=8).contains(&value)
}

/// 中身の要約（FNV-1a 64）。**向きの2バイトは 0 として読む**ので、向きだけ違うファイルは同じ値になる。
/// 元に戻す前に「pictkura が書いた後、ほかに誰も中身を変えていない」ことを確かめる
/// （大きさと更新日時を保ったままの書き換えも拾う）。暗号の強さは要らない
pub fn content_hash(path: &Path, offset: u64) -> std::io::Result<i64> {
    hash_of(&mut File::open(path)?, offset)
}

/// [`content_hash`] を、開いている手で（頭から読む）
fn hash_of(f: &mut (impl Read + Seek), offset: u64) -> std::io::Result<i64> {
    f.seek(SeekFrom::Start(0))?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buf = vec![0u8; 1 << 20];
    let mut at: u64 = 0;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        // 向きの2バイトがこの塊に掛かっていれば、そこだけ 0 にしてから流す
        for p in [offset, offset + 1] {
            if (at..at + n as u64).contains(&p) {
                buf[(p - at) as usize] = 0;
            }
        }
        for &byte in &buf[..n] {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        at += n as u64;
    }
    Ok(hash as i64)
}

/// カメラのカードか DCIM の下か。**取り込み元になりうる場所は書き換えない**——回したカードの写真は
/// 大きさは同じで更新日時だけ新しくなり、取り込みが「別の写真」と見て二重に入れる。
///
/// 見るのは「途中に DCIM がある」と「そのボリュームの根に DCIM がある」の2つ。取り外せるドライブ
/// 全部は断らない（USB の HDD にライブラリを置く人を締め出さない）
pub fn on_camera_card(path: &Path) -> bool {
    let has_dcim_component = path
        .parent()
        .into_iter()
        .flat_map(|p| p.components())
        .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("DCIM"));
    has_dcim_component || volume_root(path).is_some_and(|root| root.join("DCIM").is_dir())
}

/// そのファイルが載っているボリュームの根
#[cfg(unix)]
fn volume_root(path: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let mut cur = path.parent()?.to_path_buf();
    let dev = std::fs::metadata(&cur).ok()?.dev();
    while let Some(parent) = cur.parent() {
        match std::fs::metadata(parent) {
            Ok(m) if m.dev() == dev => cur = parent.to_path_buf(),
            _ => break,
        }
    }
    // 起動ディスクの根（`/`）は取り外せるカードではない
    (cur.parent().is_some()).then_some(cur)
}

#[cfg(windows)]
fn volume_root(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut root = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => root.push(c.as_os_str()),
            _ => break,
        }
    }
    (!root.as_os_str().is_empty()).then_some(root)
}

#[cfg(not(any(unix, windows)))]
fn volume_root(_path: &Path) -> Option<PathBuf> {
    None
}

/// 横に置いた `.xmp` が向きを持っているか。
///
/// 拡張子を置き換える形（`IMG_0001.xmp`）は、**同じ名前の相方（RAW 等）が居れば見ない**——RAW+JPEG の組では
/// それは RAW の現像設定で、JPEG の向きではない（組は JPEG だけを回す、ADR）。足す形（`IMG_0001.JPG.xmp`）は
/// 必ずこの写真のもの（PR のゲート2）
///
/// **読めなければ誤りを返す**（断る側へ倒す。PR の codex P2）——共有の錠や権限で一時的に読めないだけで
/// 「向きは無い」と読むと、その `.xmp` が持つ向きと食い違う EXIF を書く
fn sidecar_declares_orientation(path: &Path) -> std::io::Result<bool> {
    let has_partner = has_partner(path);
    // 候補は自分で並べる: `sidecars_of` は見つけられなかったもの（I/O の誤りを含む）を黙って飛ばすので、
    // 一時的に見えない `.xmp` を「無い」と読んでしまう（PR の codex P2）。無いと分かったものだけ飛ばす
    let mut appended = path.as_os_str().to_os_string();
    appended.push(".xmp");
    let mut candidates = vec![PathBuf::from(appended)];
    if !has_partner {
        candidates.push(path.with_extension("xmp"));
    }
    if !cfg!(windows) {
        let upper: Vec<PathBuf> = candidates.iter().map(|c| c.with_extension("XMP")).collect();
        candidates.extend(upper);
    }
    for x in candidates {
        match std::fs::read(&x) {
            Ok(bytes) if xmp_declares_orientation(&bytes) => return Ok(true),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}

/// XMP が向きを持っているか。`tiff:Orientation` か、TIFF の名前空間を別の接頭辞で結んだ `…:Orientation`。
/// 名前空間が在って `:Orientation` が在れば断る側へ倒す（ほかの名前空間に `Orientation` は無い）
fn xmp_declares_orientation(xmp: &[u8]) -> bool {
    let has = |needle: &[u8]| xmp.windows(needle.len()).any(|w| w == needle);
    has(XMP_TAG) || (has(XMP_TIFF_NS) && has(XMP_ORIENTATION))
}

/// 同じフォルダに、拡張子だけ違う RAW が在るか（組の相方。`.aae`・`.txt` やフォルダは相方ではない。PR のゲート1）
fn has_partner(path: &Path) -> bool {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem()) else {
        return false;
    };
    let stem = stem.to_string_lossy().to_lowercase();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let other = entry.path();
        other
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            == Some(stem.clone())
            && crate::raw::is_raw_path(&other)
            && other.is_file()
    })
}

/// 錠の外で調べた結果（[`prepare`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ms: i64,
    pub spot: Spot,
    pub content_hash: i64,
}

/// 錠の外で、回せるファイルか調べて中身の要約を取る（ファイルを丸ごと読むので錠の中でしない）
pub fn prepare(path: &Path) -> Result<Prepared, RotateError> {
    if !crate::jpeg::is_jpeg_path(path) {
        return Err(RotateError::NotJpeg);
    }
    if crate::cloud::is_cloud_only_path(path) {
        return Err(RotateError::CloudOnly);
    }
    if on_camera_card(path) {
        return Err(RotateError::CameraCard);
    }
    let before = std::fs::metadata(path)?;
    let header = read_header(&mut File::open(path)?)?;
    if header.xmp_orientation || sidecar_declares_orientation(path)? {
        return Err(RotateError::XmpOrientation);
    }
    let spot = header.spot.ok_or(RotateError::NoOrientation)?;
    let content_hash = content_hash(path, spot.offset)?;
    let after = std::fs::metadata(path)?;
    let stamp = |m: &std::fs::Metadata| (m.len(), crate::scanner::mtime_of(m));
    if stamp(&before) != stamp(&after) {
        return Err(RotateError::Changed);
    }
    Ok(Prepared {
        path: path.to_path_buf(),
        size: after.len(),
        mtime_ms: crate::scanner::mtime_of(&after),
        spot,
        content_hash,
    })
}

/// 回した結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    /// 書いた向き
    pub orientation: u16,
    /// まだ「元に戻す」ができる（pictkura が回す前と向きが違う）
    pub restorable: bool,
}

/// 2バイトを書いて `sync_all`（試験で落ちた形を作る）
#[cfg(test)]
fn write_bytes(path: &Path, offset: u64, bytes: [u8; 2]) -> std::io::Result<()> {
    let mut f = OpenOptions::new().write(true).open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(&bytes)?;
    f.sync_all()
}

/// 書くために開いた**その手で**、向きの位置と今の2バイト、中身の要約が調べたときのままか確かめてから書き、
/// 同じ手で更新日時を付けて `sync_all` する（PR の codex P1）。確かめてから開き直すと、その間に差し替えられた
/// 別のファイルへ書きうる。更新日時も同期してから返す——返った後に控えを確定させるので、日時だけが電源断で
/// 失われると、控えと合わなくなる。読み返した更新日時を返す。合わなければ何も書かずに `Changed`
fn write_checked(
    path: &Path,
    spot: Spot,
    expect_hash: i64,
    new_bytes: [u8; 2],
    mtime_ms: i64,
) -> Result<i64, RotateError> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    if read_header(&mut f)?.spot != Some(spot) || hash_of(&mut f, spot.offset)? != expect_hash {
        return Err(RotateError::Changed);
    }
    write_on(&mut f, spot.offset, new_bytes, mtime_ms)?;
    Ok(crate::scanner::mtime_of(&f.metadata()?))
}

/// 開いている手で2バイトを書き、更新日時を付けて `sync_all`
fn write_on(f: &mut File, offset: u64, bytes: [u8; 2], mtime_ms: i64) -> std::io::Result<()> {
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(&bytes)?;
    let t = filetime::FileTime::from_unix_time(
        mtime_ms.div_euclid(1000),
        (mtime_ms.rem_euclid(1000) as u32) * 1_000_000,
    );
    filetime::set_file_handle_times(f, None, Some(t))?;
    f.sync_all()
}

fn read_bytes(path: &Path, offset: u64) -> std::io::Result<[u8; 2]> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut b = [0u8; 2];
    f.read_exact(&mut b)?;
    Ok(b)
}

/// 更新日時を付けて、読み返した値を返す（試験で落ちた形を作る）
#[cfg(test)]
fn set_mtime(path: &Path, ms: i64) -> std::io::Result<i64> {
    let t = filetime::FileTime::from_unix_time(
        ms.div_euclid(1000),
        (ms.rem_euclid(1000) as u32) * 1_000_000,
    );
    filetime::set_file_mtime(path, t)?;
    Ok(crate::scanner::mtime_of(&std::fs::metadata(path)?))
}

/// 2バイトと更新日時を書き戻す。**書き戻す手で**同じファイルか（向きの位置・中身の要約）確かめてから
fn put_back(
    path: &Path,
    offset: u64,
    expect_hash: i64,
    bytes: [u8; 2],
    mtime_ms: i64,
) -> Result<(), RotateError> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let same = read_header(&mut f)?
        .spot
        .is_some_and(|s| s.offset == offset)
        && hash_of(&mut f, offset)? == expect_hash;
    if !same {
        return Err(RotateError::Changed);
    }
    Ok(write_on(&mut f, offset, bytes, mtime_ms)?)
}

/// 書き戻す。書き戻せなければ、回ったままであることを返す。
///
/// **先に今の姿を見る**: 書けなかった（ほかのアプリが開いていて開けない等）ならファイルは元のままで、
/// 書き戻しも同じ理由で落ちる。そこで「変わったまま」と言うと嘘になる
fn undo_write(
    path: &Path,
    offset: u64,
    expect_hash: i64,
    bytes: [u8; 2],
    mtime_ms: i64,
    why: RotateError,
) -> RotateError {
    let untouched = read_bytes(path, offset).is_ok_and(|b| b == bytes)
        && std::fs::metadata(path).is_ok_and(|m| crate::scanner::mtime_of(&m) == mtime_ms);
    if untouched {
        return why;
    }
    match put_back(path, offset, expect_hash, bytes, mtime_ms) {
        Ok(()) => why,
        Err(e) => RotateError::LeftChanged(format!("{why}; putting back failed: {e}")),
    }
}

/// 回す（または元に戻す）。**呼び出し側は走査の錠と `Db` の錠を持っていること**
/// ——ほかの書き手（走査の反映）が、書いた後・確定させる前の更新日時を見て行を作り直さないように。
///
/// `now_ms` は今の時刻（試験で決めるため外から渡す）
pub fn apply(
    db: &mut Db,
    id: i64,
    turn: Turn,
    prep: &Prepared,
    now_ms: i64,
) -> Result<Applied, RotateError> {
    let record = db.get_by_id(id)?.ok_or(RotateError::Changed)?;
    if record.path != prep.path {
        return Err(RotateError::Changed);
    }
    // 抽出が済んでいない行は断る。撮影日時をまだ読んでいないまま更新日時を動かすと、
    // 抽出が新しい更新日時を「撮影日時」として拾う
    if record.taken_source.is_none() {
        return Err(RotateError::NotReady);
    }
    let path = &prep.path;
    let size = prep.size as i64;
    // ファイルと行が prepare のときのままか
    let meta = std::fs::metadata(path)?;
    let now_stamp = (meta.len(), crate::scanner::mtime_of(&meta));
    let header = read_header(&mut File::open(path)?)?;
    if now_stamp != (prep.size, prep.mtime_ms)
        || header.spot != Some(prep.spot)
        || record.size != size
        || record.mtime_ms != prep.mtime_ms
    {
        return Err(RotateError::Changed);
    }
    // 中身も prepare のときのままか（大きさ・向き・日時を保ったまま書き換えられた。FAT の粗い日時で起きうる。
    // PR の codex P2）。古い要約を控えに書くと、次に回すとき控えが外れて元の姿を上書きする
    if content_hash(path, prep.spot.offset)? != prep.content_hash {
        return Err(RotateError::Changed);
    }
    // 横の `.xmp` も今のうちに見直す（prepare の後に現像ソフトが作ったかもしれない。PR の codex P2）
    if sidecar_declares_orientation(path)? {
        return Err(RotateError::XmpOrientation);
    }
    let key = crate::paths::fold(path);
    let current = prep.spot.bytes();
    // 控えが今のファイルを指しているか。外れていれば（外で書き換えられた）捨てて新しく始める
    let ledger = db.edit_of(&key)?;
    if ledger.as_ref().is_some_and(|e| e.pending.is_some()) {
        // 落ちたあとの後始末（recover）がまだ
        return Err(RotateError::Changed);
    }
    let ledger = ledger.filter(|e| {
        e.size == size
            && e.offset == prep.spot.offset as i64
            && e.edited_bytes == current
            && e.content_hash == prep.content_hash
            && crate::import::looks_alike(
                (prep.size, e.edited_mtime_ms),
                (prep.size, prep.mtime_ms),
            )
    });
    let (orig_bytes, orig_mtime_ms) = match &ledger {
        Some(e) => (e.orig_bytes, e.orig_mtime_ms),
        None => (current, prep.mtime_ms),
    };
    // 回して pictkura が回す前の向きへ戻ったなら、「元に戻す」と同じに扱う: 更新日時も元へ戻し、控えを消す
    // （win の実機確認、2026-10-16）。新しい日時のまま残すと、中身は元どおりなのに日時だけ違い、
    // 向きが元なので「元に戻す」も出せない——日時を取り戻す道が無くなる
    let turn = match turn {
        Turn::Right | Turn::Left
            if ledger.is_some()
                && prep.spot.bytes_of(turned(prep.spot.value, turn)) == orig_bytes =>
        {
            Turn::Restore
        }
        other => other,
    };
    let (new_bytes, target_mtime) = match turn {
        Turn::Restore => {
            if ledger.is_none() || orig_bytes == current {
                return Err(RotateError::NothingToRestore);
            }
            (orig_bytes, orig_mtime_ms)
        }
        Turn::Right | Turn::Left => {
            let new_value = turned(prep.spot.value, turn);
            // 前に付けた時刻・元の時刻より2秒以上後（FAT の2秒刻みでも前へ進む）
            let issued = db.last_issued_mtime(&key)?.unwrap_or(i64::MIN / 2);
            let target = now_ms
                .max(orig_mtime_ms + 2_000)
                .max(issued + 2_000)
                .max(prep.mtime_ms + 2_000);
            (prep.spot.bytes_of(new_value), target)
        }
    };
    let begin = EditRecord {
        path_key: key.clone(),
        path: path.clone(),
        fold_name: fold_name_of(path),
        size,
        offset: prep.spot.offset as i64,
        orig_bytes,
        orig_mtime_ms,
        edited_bytes: current,
        edited_mtime_ms: prep.mtime_ms,
        content_hash: prep.content_hash,
        pending: Some((new_bytes, target_mtime)),
    };
    if db.edit_begin(id, record.version(), &begin)?.is_none() {
        return Err(RotateError::Changed);
    }
    // ここから先はファイルを変える。どこで落ちても書き戻し、控えの「これから書く」を外す
    let offset = prep.spot.offset;
    // 外せなければ印が残り、次の起動の `recover` までそのファイルは「変わった」と断られる。記録には残す
    let forget_pending = |db: &mut Db| {
        let mut back = begin.clone();
        back.pending = None;
        let cleared = if ledger.is_some() {
            db.put_edit(&back)
        } else {
            db.edit_forget(&key)
        };
        if let Err(e) = cleared {
            crate::applog::note(&format!(
                "could not clear the rotation mark of {} (cleared on the next launch): {e}",
                path.display()
            ));
        }
    };
    let written = write_checked(path, prep.spot, prep.content_hash, new_bytes, target_mtime);
    let readback = match written {
        Ok(ms) => ms,
        Err(e) => {
            let err = undo_write(path, offset, prep.content_hash, current, prep.mtime_ms, e);
            if !matches!(err, RotateError::LeftChanged(_)) {
                forget_pending(db);
            }
            return Err(err);
        }
    };
    let (keep, issued) = match turn {
        Turn::Restore => {
            // 日時が戻らない台（SMB 等）では控えを残す: 取り込みの照合が、読み返した日時で引けるように
            let back =
                crate::import::looks_alike((prep.size, readback), (prep.size, orig_mtime_ms));
            (!back, None)
        }
        Turn::Right | Turn::Left => {
            if readback <= prep.mtime_ms {
                let err = undo_write(
                    path,
                    offset,
                    prep.content_hash,
                    current,
                    prep.mtime_ms,
                    RotateError::MtimeDidNotMove,
                );
                if !matches!(err, RotateError::LeftChanged(_)) {
                    forget_pending(db);
                }
                return Err(err);
            }
            (true, Some(target_mtime))
        }
    };
    let new_value = prep.spot.value_of(new_bytes);
    let settle = EditSettle {
        path_key: key.clone(),
        bytes: new_bytes,
        mtime_ms: readback,
        swap_dimensions: is_quarter(prep.spot.value) != is_quarter(new_value),
        keep,
        issued_mtime_ms: issued,
    };
    let settled = db.edit_settle(id, record.version(), &settle);
    match settled {
        Ok(Some(())) => Ok(Applied {
            orientation: new_value,
            restorable: keep && new_bytes != orig_bytes,
        }),
        other => {
            let why = match other {
                Err(e) => RotateError::Db(e),
                _ => RotateError::Changed,
            };
            let err = undo_write(path, offset, prep.content_hash, current, prep.mtime_ms, why);
            if !matches!(err, RotateError::LeftChanged(_)) {
                forget_pending(db);
            }
            Err(err)
        }
    }
}

/// 名前だけを畳む（取り込みの照合の索引。畳み方は `import.rs` の1か所）
fn fold_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| crate::import::fold_name(&n.to_string_lossy()))
        .unwrap_or_default()
}

/// 書いている途中で落ちた控え（「これから書く」が残っている行）を片付ける。起動時、走査の前に呼ぶ。
///
/// 確定（[`Db::edit_settle`]）は「これから書く」を同じトランザクションで消すので、残っていれば
/// `media` は書く前のまま。ファイルを書く前の姿へ戻して `media` に合わせる。
/// 記録に残すための一行ずつを返す
pub fn recover(db: &mut Db) -> Result<Vec<String>, DbError> {
    let mut notes = Vec::new();
    for e in db.edits_pending()? {
        let Some((pending_bytes, _)) = e.pending else {
            continue;
        };
        let offset = e.offset as u64;
        let mut settled = e.clone();
        settled.pending = None;
        // 初めて回そうとしていた行（控えの中身が元のまま）は、片付けたら消す
        let fresh = e.edited_bytes == e.orig_bytes && e.edited_mtime_ms == e.orig_mtime_ms;
        let finish = |db: &mut Db| {
            if fresh {
                db.edit_forget(&e.path_key)
            } else {
                db.put_edit(&settled)
            }
        };
        // **1つの手で確かめて書き戻す**（PR の codex P1）: 確かめてから開き直すと、その間に差し替えられた
        // 別のファイルへ書きうる
        let mut f = match OpenOptions::new().read(true).write(true).open(&e.path) {
            Ok(f) => f,
            // フォルダごと見えないなら、ドライブが抜けているか共有が切れているだけかもしれない。
            // 控えを捨てると、戻ってきたときに書きかけのまま戻せなくなる（PR のゲート2）
            Err(err)
                if err.kind() == std::io::ErrorKind::NotFound
                    && e.path.parent().is_some_and(Path::is_dir) =>
            {
                db.edit_forget(&e.path_key)?;
                notes.push(format!(
                    "rotation ledger: {} is gone; forgot it",
                    e.path.display()
                ));
                continue;
            }
            Err(err) => {
                notes.push(format!(
                    "rotation ledger: could not open {}: {err}; will retry",
                    e.path.display()
                ));
                continue;
            }
        };
        // **同じファイルか先に確かめる**（PR のゲート1）: 落ちた後・起動の前に外で差し替えられていれば、
        // その位置の2バイトがたまたま同じでも別の中身である。大きさ・向きの位置・中身の要約が合わなければ
        // 触らずに控えを捨てる
        let checked = (|| -> std::io::Result<Option<([u8; 2], i64)>> {
            let meta = f.metadata()?;
            if meta.len() as i64 != e.size {
                return Ok(None);
            }
            if read_header(&mut f)?.spot.is_none_or(|s| s.offset != offset) {
                return Ok(None);
            }
            if hash_of(&mut f, offset)? != e.content_hash {
                return Ok(None);
            }
            f.seek(SeekFrom::Start(offset))?;
            let mut now = [0u8; 2];
            f.read_exact(&mut now)?;
            Ok(Some((now, crate::scanner::mtime_of(&meta))))
        })();
        let (now, mtime) = match checked {
            Ok(Some(found)) => found,
            Ok(None) => {
                db.edit_forget(&e.path_key)?;
                notes.push(format!(
                    "rotation ledger: {} changed outside pictkura; forgot it",
                    e.path.display()
                ));
                continue;
            }
            // 読めなかっただけ（共有が切れた等）なら、違うとは言えない。控えは残して次の起動でやり直す
            Err(err) => {
                notes.push(format!(
                    "rotation ledger: could not check {}: {err}; will retry",
                    e.path.display()
                ));
                continue;
            }
        };
        if now == pending_bytes && pending_bytes != e.edited_bytes {
            // 書いたところで落ちた。書く前へ戻す
            match write_on(&mut f, offset, e.edited_bytes, e.edited_mtime_ms) {
                Ok(()) => {
                    // 落ちる前に、途中の姿から作られた絵や作りかけの作業を捨てさせる
                    db.edit_reset_row(&e.path)?;
                    finish(db)?;
                    notes.push(format!(
                        "rotation ledger: put {} back after an interrupted write",
                        e.path.display()
                    ));
                }
                Err(err) => notes.push(format!(
                    "rotation ledger: could not put {} back: {err}; will retry",
                    e.path.display()
                )),
            }
        } else if now == e.edited_bytes {
            // 2バイトは書く前のまま。日時が書く前と違えば（書き戻しの途中で落ちた）戻す。**ぴったりで比べる**
            // ——行は書く前の日時を持っているので、許容幅で「同じ」と見て残すと、走査が変わったと見て
            // 行を作り直す（付ける日時は書く前の2秒後になりうる。PR のゲート2）
            if mtime != e.edited_mtime_ms {
                if let Err(err) = write_on(&mut f, offset, e.edited_bytes, e.edited_mtime_ms) {
                    notes.push(format!(
                        "rotation ledger: could not reset the time of {}: {err}; will retry",
                        e.path.display()
                    ));
                    continue;
                }
            }
            // 書いたあと・後始末の書き戻しの後で落ちたこともありうる。途中の姿から作られた絵を捨てさせる
            // （書いていなかったなら、作り直しが1回増えるだけ。PR の codex P2）
            db.edit_reset_row(&e.path)?;
            finish(db)?;
        } else {
            // 外で書き換えられた。控えは当てにならない
            db.edit_forget(&e.path_key)?;
            notes.push(format!(
                "rotation ledger: {} changed outside pictkura; forgot it",
                e.path.display()
            ));
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Dimensions;
    use crate::scanner::ScannedFile;
    use crate::thumbs::apply_orientation;
    use crate::TakenAt;
    use crate::TakenSource;

    /// 小さな JPEG に、IFD0 に向き（SHORT）を持つ Exif の APP1 を差し込む。
    /// 向きの前に別の項目（Make）を1つ置いて、先頭の項目しか見ない実装を落とす
    fn jpeg_with(orientation: u16, big_endian: bool, xmp: Option<&str>) -> Vec<u8> {
        let img = image::RgbImage::from_fn(6, 4, |x, y| {
            image::Rgb([(x * 40) as u8, (y * 60) as u8, 90])
        });
        let mut body = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut body, image::ImageFormat::Jpeg)
            .unwrap();
        let body = body.into_inner();
        let u16b = |v: u16| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let u32b = |v: u32| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut tiff: Vec<u8> = if big_endian {
            b"MM".to_vec()
        } else {
            b"II".to_vec()
        };
        tiff.extend(u16b(42));
        tiff.extend(u32b(8));
        tiff.extend(u16b(2)); // 項目は2つ
        tiff.extend(u16b(0x010F)); // Make（ASCII 4バイト以内）
        tiff.extend(u16b(2));
        tiff.extend(u32b(4));
        tiff.extend(b"Cam\0");
        tiff.extend(u16b(ORIENTATION_TAG));
        tiff.extend(u16b(SHORT));
        tiff.extend(u32b(1));
        tiff.extend(u16b(orientation));
        tiff.extend([0, 0]);
        tiff.extend(u32b(0));
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        let mut out = vec![0xFF, 0xD8];
        let mut segment = |payload: &[u8]| {
            out.extend([0xFF, 0xE1]);
            out.extend(((payload.len() + 2) as u16).to_be_bytes());
            out.extend(payload);
        };
        segment(&app1);
        if let Some(text) = xmp {
            let mut x = XMP_HEADER.to_vec();
            x.extend(text.as_bytes());
            segment(&x);
        }
        out.extend(&body[2..]);
        out
    }

    #[test]
    fn the_header_points_at_the_two_bytes_in_either_byte_order() {
        for big_endian in [false, true] {
            let bytes = jpeg_with(6, big_endian, None);
            let header = read_header(&mut std::io::Cursor::new(&bytes)).unwrap();
            let spot = header.spot.expect("向きが見つかる");
            assert_eq!(spot.value, 6);
            assert_eq!(spot.big_endian, big_endian);
            let at = spot.offset as usize;
            assert_eq!([bytes[at], bytes[at + 1]], spot.bytes());
            assert!(!header.xmp_orientation);
        }
    }

    #[test]
    fn xmp_with_an_orientation_is_seen() {
        let bytes = jpeg_with(1, false, Some("<x tiff:Orientation=\"1\"/>"));
        let header = read_header(&mut std::io::Cursor::new(&bytes)).unwrap();
        assert!(header.xmp_orientation);
        assert!(header.spot.is_some(), "XMP の前の Exif も読む");
        let bytes = jpeg_with(1, false, Some("<x dc:title=\"a\"/>"));
        assert!(
            !read_header(&mut std::io::Cursor::new(&bytes))
                .unwrap()
                .xmp_orientation
        );
    }

    #[test]
    fn a_turn_matches_what_the_viewer_draws() {
        // 横に非対称な絵。回した値で描いた絵 = 前の値で描いた絵を90度回したもの
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(3, 2, |x, y| {
            image::Rgb([(x * 80 + y * 7) as u8, (y * 100) as u8, x as u8])
        }));
        for v in 1..=8u16 {
            let drawn = apply_orientation(img.clone(), v as u8);
            let right = apply_orientation(img.clone(), turned(v, Turn::Right) as u8);
            let left = apply_orientation(img.clone(), turned(v, Turn::Left) as u8);
            assert_eq!(right.to_rgb8(), drawn.rotate90().to_rgb8(), "右へ {v}");
            assert_eq!(left.to_rgb8(), drawn.rotate270().to_rgb8(), "左へ {v}");
            assert_eq!(turned(turned(v, Turn::Right), Turn::Left), v);
        }
    }

    #[test]
    fn the_hash_ignores_only_the_two_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        let bytes = jpeg_with(1, false, None);
        std::fs::write(&a, &bytes).unwrap();
        let spot = read_header(&mut File::open(&a).unwrap())
            .unwrap()
            .spot
            .unwrap();
        let base = content_hash(&a, spot.offset).unwrap();
        let mut turned_bytes = bytes.clone();
        turned_bytes[spot.offset as usize] = 6;
        std::fs::write(&a, &turned_bytes).unwrap();
        assert_eq!(content_hash(&a, spot.offset).unwrap(), base);
        let mut other = bytes.clone();
        let last = other.len() - 3;
        other[last] ^= 1;
        std::fs::write(&a, &other).unwrap();
        assert_ne!(content_hash(&a, spot.offset).unwrap(), base);
    }

    /// ライブラリに1枚、抽出が済んだ行を置く
    fn library(name: &str, orientation: u16) -> (tempfile::TempDir, Db, i64, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, jpeg_with(orientation, false, None)).unwrap();
        // 元の更新日時をはっきり過去に（「前より後」を見分けられるように）
        filetime::set_file_mtime(&path, filetime::FileTime::from_unix_time(1_600_000_000, 0))
            .unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[ScannedFile {
            path: path.clone(),
            size: meta.len() as i64,
            mtime_ms: crate::scanner::mtime_of(&meta),
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;
        db.update_metadata(
            id,
            Dimensions::original(6, 4),
            TakenAt::tag(Some(1_500_000_000_000), TakenSource::Exif),
            None,
        )
        .unwrap();
        (dir, db, id, path)
    }

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn turning_writes_one_byte_and_restoring_gives_the_file_back() {
        let (dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let orig_mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        // 作り終えたサムネイルがある行（古い向きの絵を配り続けないこと）
        db.update_thumb_path(id, &dir.path().join("1.webp"), 2, Some(10))
            .unwrap();
        let before = db.get_by_id(id).unwrap().unwrap();

        let prep = prepare(&path).unwrap();
        let done = apply(&mut db, id, Turn::Right, &prep, NOW).unwrap();
        assert_eq!(
            done,
            Applied {
                orientation: 6,
                restorable: true
            }
        );
        let now = std::fs::read(&path).unwrap();
        assert_eq!(now.len(), original.len());
        let changed: Vec<usize> = (0..now.len()).filter(|&i| now[i] != original[i]).collect();
        assert_eq!(
            changed,
            vec![prep.spot.offset as usize],
            "変わるのは1バイトだけ"
        );
        let mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        assert_eq!(mtime, NOW, "更新日時は今");
        let row = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(row.mtime_ms, mtime, "行は読み返した更新日時");
        assert_eq!(row.rev, before.rev + 1);
        assert_eq!(
            (row.width, row.height),
            (Some(4), Some(6)),
            "縦横が入れ替わる"
        );
        assert_eq!(row.taken_at_ms, before.taken_at_ms, "撮影日時は動かない");
        assert_eq!(row.day_key, before.day_key);
        assert_eq!(row.thumb_state, 0);
        assert!(row.thumb_path.is_none());

        // 続けてもう一度。前に付けた時刻より後になる（同じ時刻を渡しても）
        let prep = prepare(&path).unwrap();
        assert_eq!(
            apply(&mut db, id, Turn::Right, &prep, NOW)
                .unwrap()
                .orientation,
            3
        );
        let second = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        assert!(second >= NOW + 2_000, "{second}");
        let row = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(
            (row.width, row.height),
            (Some(6), Some(4)),
            "180度は横のまま"
        );

        // 元に戻す: 中身も更新日時も元どおり、控えは消え、付けた時刻は残る
        let prep = prepare(&path).unwrap();
        let back = apply(&mut db, id, Turn::Restore, &prep, NOW).unwrap();
        assert_eq!(
            back,
            Applied {
                orientation: 1,
                restorable: false
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            orig_mtime
        );
        let key = crate::paths::fold(&path);
        assert!(db.edit_of(&key).unwrap().is_none());
        assert_eq!(db.last_issued_mtime(&key).unwrap(), Some(second));
        assert_eq!(db.get_by_id(id).unwrap().unwrap().mtime_ms, orig_mtime);

        // 戻した後に回すと、前に付けた時刻より後になる
        let prep = prepare(&path).unwrap();
        apply(&mut db, id, Turn::Left, &prep, NOW).unwrap();
        let third = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        assert!(third >= second + 2_000, "{third}");
    }

    #[test]
    fn turning_back_to_the_original_gives_the_original_time_back() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let orig_mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        let prep = prepare(&path).unwrap();
        apply(&mut db, id, Turn::Right, &prep, NOW).unwrap();
        let prep = prepare(&path).unwrap();
        let back = apply(&mut db, id, Turn::Left, &prep, NOW).unwrap();
        assert_eq!(
            back,
            Applied {
                orientation: 1,
                restorable: false
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            orig_mtime
        );
        assert!(db.edit_of(&crate::paths::fold(&path)).unwrap().is_none());
        assert_eq!(db.get_by_id(id).unwrap().unwrap().mtime_ms, orig_mtime);
    }

    #[test]
    fn nothing_to_restore_on_a_file_never_turned() {
        let (_dir, mut db, id, path) = library("a.jpg", 6);
        let prep = prepare(&path).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Restore, &prep, NOW),
            Err(RotateError::NothingToRestore)
        ));
    }

    #[test]
    fn a_file_changed_after_prepare_is_left_alone() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        filetime::set_file_mtime(&path, filetime::FileTime::from_unix_time(1_650_000_000, 0))
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Right, &prep, NOW),
            Err(RotateError::Changed)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(db.edit_of(&crate::paths::fold(&path)).unwrap().is_none());
    }

    #[test]
    fn a_file_rewritten_with_the_same_size_and_time_is_left_alone() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        // 大きさ・向き・日時を保ったまま中身だけ書き換えられた
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 3;
        bytes[last] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        set_mtime(&path, prep.mtime_ms).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Right, &prep, NOW),
            Err(RotateError::Changed)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn recovery_resets_the_row_even_when_the_bytes_were_already_back() {
        let (dir, mut db, id, path) = library("a.jpg", 1);
        db.update_thumb_path(id, &dir.path().join("1.webp"), 2, Some(10))
            .unwrap();
        let rev = db.get_by_id(id).unwrap().unwrap().rev;
        let prep = prepare(&path).unwrap();
        // 後始末の書き戻しが済んだところで落ちた（2バイトも日時も書く前の姿）
        begin_pending(&mut db, id, &path, &prep, prep.spot.bytes_of(6), NOW);
        recover(&mut db).unwrap();
        let row = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(row.rev, rev + 1);
        assert!(row.thumb_path.is_none());
    }

    #[test]
    fn a_sidecar_made_after_prepare_stops_the_turn() {
        let (dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let prep = prepare(&path).unwrap();
        std::fs::write(dir.path().join("a.jpg.xmp"), "<x tiff:Orientation=\"6\"/>").unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Right, &prep, NOW),
            Err(RotateError::XmpOrientation)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(db.edits_pending().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_sidecar_that_cannot_be_read_stops_the_turn() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, _db, _id, path) = library("a.jpg", 1);
        let xmp = dir.path().join("a.jpg.xmp");
        std::fs::write(&xmp, "<x/>").unwrap();
        std::fs::set_permissions(&xmp, std::fs::Permissions::from_mode(0o000)).unwrap();
        let got = prepare(&path);
        std::fs::set_permissions(&xmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(got, Err(RotateError::Io(_))), "{got:?}");
    }

    #[test]
    fn the_write_checks_the_handle_it_writes_through() {
        let (_dir, _db, _id, path) = library("a.jpg", 1);
        let hash = prepare(&path).unwrap().content_hash;
        let original = std::fs::read(&path).unwrap();
        let spot = read_header(&mut File::open(&path).unwrap())
            .unwrap()
            .spot
            .unwrap();
        // 調べたときと違う姿（別のファイルに差し替わった）を渡す
        let stale = Spot { value: 3, ..spot };
        assert!(matches!(
            write_checked(&path, stale, hash, spot.bytes_of(6), NOW),
            Err(RotateError::Changed)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original, "何も書かない");
        // 位置は同じでも中身が違う（同じ機種の別の写真に差し替わった）
        assert!(matches!(
            write_checked(&path, spot, hash ^ 1, spot.bytes_of(6), NOW),
            Err(RotateError::Changed)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original, "何も書かない");
        assert_eq!(
            write_checked(&path, spot, hash, spot.bytes_of(6), NOW).unwrap(),
            NOW
        );
    }

    #[test]
    fn putting_back_checks_it_is_the_same_file() {
        let (_dir, _db, _id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        // 書き戻す前に別の中身へ差し替わった（要約が合わない）
        assert!(matches!(
            put_back(
                &path,
                prep.spot.offset,
                prep.content_hash ^ 1,
                prep.spot.bytes_of(6),
                NOW
            ),
            Err(RotateError::Changed)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original, "触らない");
        put_back(
            &path,
            prep.spot.offset,
            prep.content_hash,
            prep.spot.bytes_of(6),
            NOW,
        )
        .unwrap();
        assert_eq!(
            read_bytes(&path, prep.spot.offset).unwrap(),
            prep.spot.bytes_of(6)
        );
    }

    #[test]
    fn an_xmp_orientation_under_another_prefix_is_seen() {
        assert!(xmp_declares_orientation(
            br#"<x xmlns:foo="http://ns.adobe.com/tiff/1.0/" foo:Orientation="6"/>"#
        ));
        assert!(xmp_declares_orientation(br#"<x tiff:Orientation="6"/>"#));
        assert!(!xmp_declares_orientation(
            br#"<x xmlns:tiff="http://ns.adobe.com/tiff/1.0/" tiff:Make="A"/>"#
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_sidecar_that_cannot_be_looked_at_stops_the_turn() {
        use std::os::unix::fs::PermissionsExt;
        // フォルダに入れない（`is_file` も `canonicalize` も誤りになる）と、`.xmp` が在るかどうか分からない
        let (dir, _db, _id, path) = library("a.jpg", 1);
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        let inner = locked.join("b.jpg");
        std::fs::copy(&path, &inner).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let got = sidecar_declares_orientation(&inner);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(got.is_err(), "{got:?}");
    }

    #[test]
    fn a_row_not_read_yet_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, jpeg_with(1, false, None)).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        db.upsert_files(&[ScannedFile {
            path: path.clone(),
            size: meta.len() as i64,
            mtime_ms: crate::scanner::mtime_of(&meta),
        }])
        .unwrap();
        let id = db.list_all().unwrap()[0].id;
        let prep = prepare(&path).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Right, &prep, NOW),
            Err(RotateError::NotReady)
        ));
    }

    #[test]
    fn places_and_files_that_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let card = dir.path().join("DCIM").join("100CANON");
        std::fs::create_dir_all(&card).unwrap();
        let on_card = card.join("a.jpg");
        std::fs::write(&on_card, jpeg_with(1, false, None)).unwrap();
        assert!(matches!(prepare(&on_card), Err(RotateError::CameraCard)));

        let xmp = dir.path().join("b.jpg");
        std::fs::write(&xmp, jpeg_with(1, false, Some("tiff:Orientation"))).unwrap();
        assert!(matches!(prepare(&xmp), Err(RotateError::XmpOrientation)));

        let side = dir.path().join("c.jpg");
        std::fs::write(&side, jpeg_with(1, false, None)).unwrap();
        assert!(prepare(&side).is_ok());
        std::fs::write(dir.path().join("c.xmp"), "<x tiff:Orientation=\"6\"/>").unwrap();
        assert!(matches!(prepare(&side), Err(RotateError::XmpOrientation)));

        let png = dir.path().join("d.png");
        std::fs::write(&png, b"x").unwrap();
        assert!(matches!(prepare(&png), Err(RotateError::NotJpeg)));

        // 向きを持たない JPEG
        let plain = dir.path().join("e.jpg");
        let mut body = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(2, 2))
            .write_to(&mut body, image::ImageFormat::Jpeg)
            .unwrap();
        std::fs::write(&plain, body.into_inner()).unwrap();
        assert!(matches!(prepare(&plain), Err(RotateError::NoOrientation)));
    }

    #[test]
    fn a_failed_settle_puts_the_file_back() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let orig_mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        // 確定の UPDATE だけを落とす
        db.execute_for_test(
            "CREATE TRIGGER no_settle BEFORE UPDATE OF mtime_ms ON media \
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        );
        let prep = prepare(&path).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Right, &prep, NOW),
            Err(RotateError::Db(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            orig_mtime
        );
        assert!(
            db.edit_of(&crate::paths::fold(&path)).unwrap().is_none(),
            "印も消える"
        );
        assert_eq!(db.get_by_id(id).unwrap().unwrap().mtime_ms, orig_mtime);
    }

    #[test]
    fn a_file_that_cannot_be_opened_for_writing_is_reported_as_untouched() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let prep = prepare(&path).unwrap();
        // ほかのアプリが握っているのと同じく、書くために開けない
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms.clone()).unwrap();
        let got = apply(&mut db, id, Turn::Right, &prep, NOW);
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
        assert!(matches!(got, Err(RotateError::Io(_))), "{got:?}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(db.edits_pending().unwrap().is_empty(), "印は残さない");
    }

    #[test]
    fn recovery_puts_back_a_write_that_was_interrupted() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let original = std::fs::read(&path).unwrap();
        let orig_mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        let prep = prepare(&path).unwrap();
        let key = crate::paths::fold(&path);
        // 「これから書く」を確定させ、書いて日時を付けたところで落ちた形
        let new = prep.spot.bytes_of(6);
        let rec = EditRecord {
            path_key: key.clone(),
            path: path.clone(),
            fold_name: "a.jpg".into(),
            size: prep.size as i64,
            offset: prep.spot.offset as i64,
            orig_bytes: prep.spot.bytes(),
            orig_mtime_ms: orig_mtime,
            edited_bytes: prep.spot.bytes(),
            edited_mtime_ms: orig_mtime,
            content_hash: prep.content_hash,
            pending: Some((new, NOW)),
        };
        let version = db.get_by_id(id).unwrap().unwrap().version();
        db.edit_begin(id, version, &rec).unwrap().unwrap();
        write_bytes(&path, prep.spot.offset, new).unwrap();
        set_mtime(&path, NOW).unwrap();

        let notes = recover(&mut db).unwrap();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            orig_mtime
        );
        assert!(
            db.edit_of(&key).unwrap().is_none(),
            "初めての回転だった控えは消える"
        );
        assert!(db.edits_pending().unwrap().is_empty());

        // 2回目の回転の途中で落ちた: 1回目の姿へ戻し、控えは1回目のまま残る
        let prep = prepare(&path).unwrap();
        apply(&mut db, id, Turn::Right, &prep, NOW).unwrap();
        let first = std::fs::read(&path).unwrap();
        let first_mtime = crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap());
        let mut rec = db.edit_of(&key).unwrap().unwrap();
        let prep = prepare(&path).unwrap();
        let new = prep.spot.bytes_of(3);
        rec.pending = Some((new, NOW + 10_000));
        let version = db.get_by_id(id).unwrap().unwrap().version();
        db.edit_begin(id, version, &rec).unwrap().unwrap();
        write_bytes(&path, prep.spot.offset, new).unwrap();
        recover(&mut db).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first);
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            first_mtime
        );
        let kept = db.edit_of(&key).unwrap().unwrap();
        assert!(kept.pending.is_none());
        assert_eq!(kept.edited_bytes, prep.spot.bytes());
        assert_eq!(kept.orig_bytes, prep.spot.bytes_of(1));
    }

    #[test]
    fn recovery_leaves_a_file_replaced_after_the_crash_alone() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        let key = crate::paths::fold(&path);
        let new = prep.spot.bytes_of(6);
        let orig_mtime = prep.mtime_ms;
        let rec = EditRecord {
            path_key: key.clone(),
            path: path.clone(),
            fold_name: "a.jpg".into(),
            size: prep.size as i64,
            offset: prep.spot.offset as i64,
            orig_bytes: prep.spot.bytes(),
            orig_mtime_ms: orig_mtime,
            edited_bytes: prep.spot.bytes(),
            edited_mtime_ms: orig_mtime,
            content_hash: prep.content_hash,
            pending: Some((new, NOW)),
        };
        let version = db.get_by_id(id).unwrap().unwrap().version();
        db.edit_begin(id, version, &rec).unwrap().unwrap();
        write_bytes(&path, prep.spot.offset, new).unwrap();
        // 起動の前に外で書き換えられた（向きの2バイトは書いたままに見える）
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 3;
        bytes[last] ^= 1;
        std::fs::write(&path, &bytes).unwrap();

        recover(&mut db).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "触らない");
        assert!(db.edit_of(&key).unwrap().is_none(), "控えは捨てる");
    }

    #[test]
    fn a_raw_partners_xmp_does_not_block_the_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let jpg = dir.path().join("IMG_0001.JPG");
        std::fs::write(&jpg, jpeg_with(1, false, None)).unwrap();
        std::fs::write(dir.path().join("IMG_0001.CR3"), b"raw").unwrap();
        std::fs::write(
            dir.path().join("IMG_0001.xmp"),
            "<x tiff:Orientation=\"6\"/>",
        )
        .unwrap();
        assert!(prepare(&jpg).is_ok(), "RAW の現像設定");
        // 相方が RAW でなければ、置き換える形の .xmp はこの JPEG のもの
        let solo = dir.path().join("IMG_0002.JPG");
        std::fs::write(&solo, jpeg_with(1, false, None)).unwrap();
        std::fs::write(dir.path().join("IMG_0002.aae"), b"x").unwrap();
        std::fs::write(
            dir.path().join("IMG_0002.xmp"),
            "<x tiff:Orientation=\"6\"/>",
        )
        .unwrap();
        assert!(matches!(prepare(&solo), Err(RotateError::XmpOrientation)));
        // 足す形はこの JPEG のもの
        std::fs::write(
            dir.path().join("IMG_0001.JPG.xmp"),
            "<x tiff:Orientation=\"6\"/>",
        )
        .unwrap();
        assert!(matches!(prepare(&jpg), Err(RotateError::XmpOrientation)));
    }

    /// 「これから書く」を確定させた控えを置く（落ちた形を作るため）
    fn begin_pending(
        db: &mut Db,
        id: i64,
        path: &Path,
        prep: &Prepared,
        new: [u8; 2],
        at: i64,
    ) -> String {
        let key = crate::paths::fold(path);
        let rec = EditRecord {
            path_key: key.clone(),
            path: path.to_path_buf(),
            fold_name: "a.jpg".into(),
            size: prep.size as i64,
            offset: prep.spot.offset as i64,
            orig_bytes: prep.spot.bytes(),
            orig_mtime_ms: prep.mtime_ms,
            edited_bytes: prep.spot.bytes(),
            edited_mtime_ms: prep.mtime_ms,
            content_hash: prep.content_hash,
            pending: Some((new, at)),
        };
        let version = db.get_by_id(id).unwrap().unwrap().version();
        db.edit_begin(id, version, &rec).unwrap().unwrap();
        key
    }

    #[test]
    fn recovery_resets_a_time_left_two_seconds_off() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        // 書き戻しの途中: 2バイトは戻ったが、日時は付けた値（書く前の2秒後）のまま
        let key = begin_pending(
            &mut db,
            id,
            &path,
            &prep,
            prep.spot.bytes_of(6),
            prep.mtime_ms + 2_000,
        );
        set_mtime(&path, prep.mtime_ms + 2_000).unwrap();
        recover(&mut db).unwrap();
        assert_eq!(
            crate::scanner::mtime_of(&std::fs::metadata(&path).unwrap()),
            prep.mtime_ms
        );
        assert!(db.edit_of(&key).unwrap().is_none());
    }

    #[test]
    fn recovery_keeps_the_mark_while_the_folder_is_out_of_reach() {
        let (dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        let key = begin_pending(&mut db, id, &path, &prep, prep.spot.bytes_of(6), NOW);
        // ドライブが抜けた（フォルダごと見えない）
        let away = dir.path().with_extension("away");
        std::fs::rename(dir.path(), &away).unwrap();
        recover(&mut db).unwrap();
        std::fs::rename(&away, dir.path()).unwrap();
        assert!(
            db.edit_of(&key)
                .unwrap()
                .is_some_and(|e| e.pending.is_some()),
            "印は残す"
        );
        // ファイルだけ消えた
        std::fs::remove_file(&path).unwrap();
        recover(&mut db).unwrap();
        assert!(db.edit_of(&key).unwrap().is_none());
    }

    #[test]
    fn recovery_makes_the_thumbnail_be_made_again() {
        let (dir, mut db, id, path) = library("a.jpg", 1);
        db.update_thumb_path(id, &dir.path().join("1.webp"), 2, Some(10))
            .unwrap();
        let rev = db.get_by_id(id).unwrap().unwrap().rev;
        let prep = prepare(&path).unwrap();
        let new = prep.spot.bytes_of(6);
        begin_pending(&mut db, id, &path, &prep, new, NOW);
        write_bytes(&path, prep.spot.offset, new).unwrap();
        recover(&mut db).unwrap();
        let row = db.get_by_id(id).unwrap().unwrap();
        assert_eq!(row.rev, rev + 1, "作りかけの作業を捨てさせる");
        assert!(row.thumb_path.is_none());
        assert_eq!(row.mtime_ms, prep.mtime_ms, "日時は書く前のまま");
    }

    #[test]
    fn a_ledger_left_stale_by_an_outside_edit_does_not_restore() {
        let (_dir, mut db, id, path) = library("a.jpg", 1);
        let prep = prepare(&path).unwrap();
        apply(&mut db, id, Turn::Right, &prep, NOW).unwrap();
        // 外で中身が書き換えられた（大きさと更新日時は同じに保たれた）
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 3;
        bytes[last] ^= 1;
        let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, &bytes).unwrap();
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(mtime)).unwrap();
        let prep = prepare(&path).unwrap();
        assert!(matches!(
            apply(&mut db, id, Turn::Restore, &prep, NOW),
            Err(RotateError::NothingToRestore)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "触らない");
    }
}
