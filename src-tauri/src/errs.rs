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
//! **画面に出した失敗は、記録にも残す**（項目2の受け皿。2026-09-07・利用者の判断）。
//! **詳細はOSの言語で来る**ので、画面で読めなかった人も
//! **報告のときには追える**——[`for_log`] が `鍵: 詳細` に開いて書く。

use std::fmt::Display;

use pictkura_core::applog;
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

/// 鍵＋詳細。**詳細はそのまま渡す**（訳さない）。**記録には残さない。**
///
/// **利用者の入力を断るとき**はこちら——見つからないフォルダを選んだ、
/// 管理されたライブラリを指した、といった**間違いではあるが不具合ではない**もの。
/// これを残すと、**設定を選び直しただけで `pictkura.log` ができ**、
/// 「ファイルが在る＝何かあった」という印が意味を失う（ゲート2）。
///
/// **不具合を記録するのは、[`from_err`] か呼ぶ側の仕事**である
/// ——「サイドカーを」「写真を」のような**文脈を知っているのは呼ぶ側だけ**なので、
/// ここで一律に書くと**文脈の無い行が二重に**並ぶ。
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
    /// **不具合か**（＝記録に残すか）。
    ///
    /// **利用者の選び間違いは不具合ではない**——管理されたライブラリを指した、
    /// コピー先をまだ決めていない、といったものは断るだけで記録しない。
    /// **機械の側が転んだもの**（DB・設定ファイル・読めないフォルダ）は残す。
    fn is_malfunction(&self) -> bool;
}

/// **記録（ログ）へ回すときの姿。**
///
/// **鍵は人の言葉ではない**ので、そのまま記録に落とすと読めない行になる。
/// 継ぎ目を `: ` に開いて、**鍵と詳細の両方を残す**
/// ——鍵は画面に出た文と1対1なので、**利用者が見た文と記録が突き合わせられる。**
///
/// 鍵の付いていない文字列は素通りする。
pub fn for_log(s: &str) -> String {
    s.replacen(SEP, ": ", 1)
}

/// [`Coded`] を実装した型から、画面へ渡す1本の文字列を作る。
pub fn from_err<E: Coded>(e: E) -> String {
    let malfunction = e.is_malfunction();
    let detail = e.detail();
    let s = if detail.is_empty() {
        code(e.code())
    } else {
        coded(e.code(), detail)
    };
    if malfunction {
        applog::note(&for_log(&s));
    }
    s
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
    /// 索引が読み書きできないのは、**いつでも不具合**である。
    fn is_malfunction(&self) -> bool {
        true
    }
}

impl Coded for ConfigError {
    fn code(&self) -> &'static str {
        match self {
            // **読めないと書けないは別の話**。読めないのは権限や壊れたファイル、
            // 書けないのは書き込み先の問題で、利用者にできることが違う。
            //
            // **`Serialize` は書く側**（`Config::save`）で起きるので、
            // 「中身を読めませんでした」には入れない——**何も読んでいない**
            // （ゲート2の指摘）。`errConfigIo` の文は読み書きの両方を言う
            ConfigError::Io(_) | ConfigError::Serialize(_) => "errConfigIo",
            ConfigError::Parse(_) => "errConfigFormat",
        }
    }
    fn detail(&self) -> String {
        match self {
            ConfigError::Io(e) => e.to_string(),
            ConfigError::Parse(e) => e.to_string(),
            ConfigError::Serialize(e) => e.to_string(),
        }
    }
    /// 設定が読めない・書けないのも、機械の側の話。
    fn is_malfunction(&self) -> bool {
        true
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
    /// **書き出し先が使えないのは、選び方の話**——選び直せば済むので残さない。
    fn is_malfunction(&self) -> bool {
        false
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
    /// **読めないのは不具合、選び間違いは違う。**
    fn is_malfunction(&self) -> bool {
        matches!(self, ImportError::SourceUnreadable(_))
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

    #[test]
    fn the_log_reads_the_key_and_the_detail_together() {
        let e = ImportError::SourceUnreadable("/媒体/DCIM".into());
        assert_eq!(for_log(&from_err(e)), "errSourceUnreadable: /媒体/DCIM");
        // 鍵だけのものと、鍵の付いていないものは、そのまま
        assert_eq!(for_log("errNoDestination"), "errNoDestination");
        assert_eq!(for_log("よそのクレートの文言"), "よそのクレートの文言");
    }

    /// 静かな版は、字面だけ同じで記録に触らない。
    /// **記録に残すかは、機械が転んだかで決まる**（選び間違いは残さない）。
    #[test]
    fn only_a_malfunction_is_worth_recording() {
        assert!(!ImportError::NoDestination.is_malfunction());
        assert!(
            !ImportError::SourceIsManagedPackage("/写真.photoslibrary".into()).is_malfunction()
        );
        assert!(ImportError::SourceUnreadable("/媒体/DCIM".into()).is_malfunction());
        assert!(!ExportError::DestIsPackage("/写真.photoslibrary".into()).is_malfunction());
    }

    /// **日本語の文は詳細に混ぜない**——訳した文の隣に原文が並ぶのを防ぐ。
    #[test]
    fn the_japanese_sentence_does_not_ride_along() {
        let e = ImportError::SourceUnreadable("/媒体/DCIM".into());
        assert!(!from_err(e).contains("読めません"));
    }
}
