//! **画面に出す失敗に、辞書の鍵を付ける**（`dev/plan.completeness-week.md` 項目3）。
//!
//! Rust 側の文言は**日本語決め打ち**で、そのまま `Result<_, String>` に載って
//! 画面へ出ていた。**アプリは6言語で話すのに、失敗のときだけ日本語で話す**
//! ——英語で使っている人が写真を消し損ねると、そこだけ日本語が混じる。
//!
//! ## 形（最小形）
//!
//! **文字列の頭を辞書の鍵にする。** `errTrashFailed\u{1}アクセスが拒否されました`
//! のように、**鍵**と**詳細**を [`SEP`] で継ぐ。フロントの `errText` が
//! 鍵を辞書で引き、**詳細はそのまま**添える。
//!
//! - **詳細は訳さない。** OSの文言・パス・SQLiteの理由は、こちらが持っている
//!   言葉ではない。**訳せないものを訳したふりをしない**
//! - **知らない鍵はそのまま出す**（フロント側）。**失うより出す**——
//!   まだ鍵を付けていない道が残っていても、画面から文字が消えることはない
//! - **鍵は辞書のキーそのもの**にする。2つの名前を突き合わせる手間を作らない。
//!   **`ui/scripts/i18n.test.ts` が、この鍵が `ja.ts` に在ることを見る**
//!   ——辞書に無い鍵を足したら、テストが落ちる
//!
//! **原因の細かい所はログにも残す**（項目2）。画面は一行でよく、
//! 追いかけるための材料は `pictkura.log` にある。

use std::fmt::Display;

use pictkura_core::config::ConfigError;
use pictkura_core::db::DbError;
use pictkura_core::export::ExportError;
use pictkura_core::import::ImportError;

/// 鍵と詳細の継ぎ目。**画面に出ない制御文字**を使う——`:` や `|` は
/// パスにもOSの文言にも出るので、区切りに使うと詳細の途中で切れる。
pub const SEP: char = '\u{1}';

/// 鍵だけ（詳細の無い失敗）。
pub fn code(code: &str) -> String {
    code.to_string()
}

/// 鍵＋詳細。**詳細はそのまま渡す**（訳さない）。
pub fn coded(code: &str, detail: impl Display) -> String {
    format!("{code}{SEP}{detail}")
}

/// こちらが定義したエラー型を、鍵と詳細に開く。
///
/// **`Display` を詳細に使わない。** あちらは日本語の文まで含んでいて、
/// それを詳細として添えると**訳した文の隣に日本語が並ぶ**。
pub trait Coded {
    /// 辞書の鍵（＝ `ja.ts` のキー）。
    fn code(&self) -> &'static str;
    /// 訳せない部分（パス・OSの文言）。無ければ空。
    fn detail(&self) -> String;
}

/// [`Coded`] を実装した型から、画面へ渡す1本の文字列を作る。
pub fn from_err<E: Coded>(e: E) -> String {
    let detail = e.detail();
    if detail.is_empty() {
        code(e.code())
    } else {
        coded(e.code(), detail)
    }
}

impl Coded for DbError {
    fn code(&self) -> &'static str {
        "errDb"
    }
    fn detail(&self) -> String {
        match self {
            DbError::Sqlite(e) => e.to_string(),
        }
    }
}

impl Coded for ConfigError {
    fn code(&self) -> &'static str {
        match self {
            // **読めないと書けないは別の話**。読めないのは権限や壊れたファイル、
            // 書けないのは書き込み先の問題で、利用者にできることが違う
            ConfigError::Io(_) => "errConfigIo",
            ConfigError::Parse(_) | ConfigError::Serialize(_) => "errConfigFormat",
        }
    }
    fn detail(&self) -> String {
        match self {
            ConfigError::Io(e) => e.to_string(),
            ConfigError::Parse(e) => e.to_string(),
            ConfigError::Serialize(e) => e.to_string(),
        }
    }
}

impl Coded for ExportError {
    fn code(&self) -> &'static str {
        match self {
            ExportError::DestUnusable(_) => "errExportDest",
            ExportError::DestIsPackage(_) => "errExportManaged",
        }
    }
    fn detail(&self) -> String {
        match self {
            ExportError::DestUnusable(p) | ExportError::DestIsPackage(p) => p.display().to_string(),
        }
    }
}

impl Coded for ImportError {
    fn code(&self) -> &'static str {
        match self {
            ImportError::NoDestination => "errNoDestination",
            ImportError::SourceUnreadable(_) => "errSourceUnreadable",
            ImportError::SourceIsManagedPackage(_) => "errSourceManaged",
        }
    }
    fn detail(&self) -> String {
        match self {
            ImportError::NoDestination => String::new(),
            ImportError::SourceUnreadable(p) | ImportError::SourceIsManagedPackage(p) => {
                p.display().to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_without_detail_stays_a_bare_key() {
        assert_eq!(from_err(ImportError::NoDestination), "errNoDestination");
    }

    #[test]
    fn a_detail_rides_after_the_separator() {
        let e = ImportError::SourceUnreadable("/媒体/DCIM".into());
        assert_eq!(from_err(e), format!("errSourceUnreadable{SEP}/媒体/DCIM"));
    }

    /// **日本語の文は詳細に混ぜない**——訳した文の隣に原文が並ぶのを防ぐ。
    #[test]
    fn the_japanese_sentence_does_not_ride_along() {
        let e = ImportError::SourceUnreadable("/媒体/DCIM".into());
        assert!(!from_err(e).contains("読めません"));
    }
}
