//! アプリ全体の設定。TOMLファイルと相互変換する。
//!
//! 4つのドメインを持つ:
//! - `[import]`      : USB等からの取り込み動作
//! - `[routing]`     : 取り込んだファイルのコピー先の決定ルール
//! - `[library]`     : ライブラリ（スキャン対象）の場所と除外パターン
//! - `[performance]` : サムネイルサイズやワーカー数などの性能パラメータ

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("設定ファイルの読み込みに失敗: {0}")]
    Io(#[from] std::io::Error),
    #[error("設定ファイルのパースに失敗: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("設定ファイルのシリアライズに失敗: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// アプリ全体の設定ルート。
///
/// すべてのフィールドに `#[serde(default)]` を付けているため、
/// 部分的なTOML（空ファイルを含む）でも常にデフォルト値で補完されて読み込める。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub import: ImportConfig,
    pub routing: RoutingConfig,
    pub library: LibraryConfig,
    pub performance: PerformanceConfig,
    pub editors: EditorsConfig,
}

/// `[editors]` 外部の編集アプリ。
///
/// Lightroom / digiKam と同じ流儀: 一度「他のアプリで開く」で選んだアプリを
/// 覚えてメニューへ足していく。特定アプリを決め打ちで探しに行くことはしない
/// （利用者が実際に使うのがPhotoshopとは限らないため）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorsConfig {
    /// 登録済みの編集アプリ（表示名と実行ファイルのパス）。
    pub apps: Vec<ExternalApp>,
}

/// 外部の編集アプリ1つ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalApp {
    /// メニューに出す名前（既定は実行ファイル名から作る）
    pub name: String,
    pub path: PathBuf,
}

impl EditorsConfig {
    /// アプリを登録する（同じパスが既にあれば何もしない）。登録済みなら false。
    pub fn remember(&mut self, path: &Path) -> bool {
        if self.apps.iter().any(|a| a.path == path) {
            return false;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.apps.push(ExternalApp {
            name,
            path: path.to_path_buf(),
        });
        true
    }
}

/// `[import]` USB等からの取り込み動作。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    /// 前回選択した取り込み元フォルダ（次回のダイアログ初期位置に使う）。
    pub last_source_dir: Option<PathBuf>,
    /// コピー後にファイルサイズを比較して検証するか。
    pub verify_after_copy: bool,
    /// 取り込み対象とする拡張子（小文字で比較）。
    pub extensions: Vec<String>,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            last_source_dir: None,
            verify_after_copy: true,
            extensions: DEFAULT_EXTENSIONS
                .iter()
                .map(|e| (*e).to_string())
                .collect(),
        }
    }
}

/// 取り込み・走査の対象にする拡張子の既定値。
///
/// RAWは「現像せず埋め込みプレビューを見せる」方針なので、
/// 普通の画像と同じ扱いで一覧に出せる（第6部 段階F）。
pub const DEFAULT_EXTENSIONS: &[&str] = &[
    // 普通の画像
    "jpg", "jpeg", "png",
    "webp", // RAW（カメラが書いた表示用JPEGを取り出して扱う）
    "cr2", "cr3", "nef", "nrw", "arw", "sr2", "raf", "orf", "rw2", "pef", "srw", "dng", "rwl",
    "3fr", "iiq", "erf", "mrw", "x3f", "dcr", "kdc",
    // HEIF（iPhoneの既定形式。画素はOSのデコーダで展開する）
    "heic", "heif", "hif",
    // その他のよくある形式。bmp/gif/tiff は image クレートが読み、
    // svg はブラウザがそのまま描く。avif は HEIF と同じ入れ物
    "bmp", "gif", "tif", "tiff", "svg", "avif",
    // 動画（第9部）。画素はOSのサムネイル機構から借り、再生はWebViewに任せる。
    // 一覧に出せることと再生できることは別で、mts/avi は再生だけ外部アプリになる
    "mp4", "m4v", "mov", "webm", "avi", "mts", "m2ts", "mkv", "3gp", "wmv", "mpg", "mpeg",
];

/// 既定の拡張子に動画が漏れなく入っているか。
///
/// `DEFAULT_EXTENSIONS` と [`crate::video::VIDEO_EXTENSIONS`] は別々に書いてある。
/// 片方だけに足すとどちらの向きでも黙って壊れる:
/// - 走査側に無い → そのファイルは**見つからない**
/// - `video` 側に無い → 画像として扱われ、`source_dimensions` が失敗するうえ
///   `can_serve_original` が真を返して**動画本体を丸ごと配りかねない**
#[cfg(test)]
fn video_extensions_are_all_scanned() -> Result<(), String> {
    for ext in crate::video::VIDEO_EXTENSIONS {
        if !DEFAULT_EXTENSIONS.contains(ext) {
            return Err(format!("走査対象に入っていない動画の拡張子: {ext}"));
        }
    }
    Ok(())
}

/// 拡張子の設定を新しい既定へ引き上げるべきか。
///
/// RAW対応で既定の一覧が増えたが、**ユーザーが自分で編集した設定は尊重する**。
/// 「旧バージョンの既定そのまま」のときだけ差し替える（判別できるのはこの形だけ）。
fn upgrade_extensions(current: &[String]) -> Option<Vec<String>> {
    /// これまでの既定。どれかと完全一致なら「触っていない」と判断できる
    const LEGACY_DEFAULTS: &[&[&str]] = &[
        // 第6部より前（RAW対応なし）
        &["jpg", "jpeg", "png"],
        // 第6部（RAW対応。HEIFはまだ無い）
        &[
            "jpg", "jpeg", "png", "webp", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "raf", "orf",
            "rw2", "pef", "srw", "dng", "rwl", "3fr", "iiq", "erf", "mrw", "x3f", "dcr", "kdc",
        ],
        // 第7部 段階G（HEIF対応。bmp/gif/tiff/svg/avif はまだ無い）
        &[
            "jpg", "jpeg", "png", "webp", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "raf", "orf",
            "rw2", "pef", "srw", "dng", "rwl", "3fr", "iiq", "erf", "mrw", "x3f", "dcr", "kdc",
            "heic", "heif", "hif",
        ],
        // 第9部より前（bmp/gif/tiff/svg/avif まで。動画はまだ無い）
        &[
            "jpg", "jpeg", "png", "webp", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "raf", "orf",
            "rw2", "pef", "srw", "dng", "rwl", "3fr", "iiq", "erf", "mrw", "x3f", "dcr", "kdc",
            "heic", "heif", "hif", "bmp", "gif", "tif", "tiff", "svg", "avif",
        ],
    ];
    let normalized: Vec<String> = current.iter().map(|e| e.to_ascii_lowercase()).collect();
    LEGACY_DEFAULTS
        .iter()
        .any(|legacy| normalized == *legacy)
        .then(|| {
            DEFAULT_EXTENSIONS
                .iter()
                .map(|e| (*e).to_string())
                .collect()
        })
}

// 除外パターンには拡張子のような「引き上げ」を**置かない**。
//
// 一度は入れたが、`upgrade_extensions` と同じLEGACY_DEFAULTS方式では
// **利用者が自分で消した除外を毎回復活させてしまう**。新しい既定は
// 旧既定＋1件なので、`*.photoslibrary` を消した設定は旧既定と同じ形になり、
// 起動のたびに書き戻される。除外を編集するUIは無く、TOMLの手編集が
// 唯一の逃げ道なので、それを塞ぐと直す手段が無くなる。
//
// v0.1はまだ配っていないため、引き上げが要る既存の環境は開発機だけ
// （TOMLを消せば済む）。**配ったあとに既定を変える必要が出たら、
// 中身から推測するのではなく設定にスキーマ版を持たせること。**

/// `[routing]` 取り込んだファイルのコピー先の決定ルール。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    /// コピー先のルートフォルダ。ユーザーが任意に指定する。
    pub destination: Option<PathBuf>,
    /// コピー先のサブフォルダ構成。`{year}` `{month}` `{day}` を撮影日で置換する。
    pub folder_pattern: String,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            destination: None,
            folder_pattern: "{year}/{year}-{month}-{day}".into(),
        }
    }
}

/// `[library]` ライブラリ（スキャン対象）の場所と除外パターン。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryConfig {
    /// スキャン対象のルートフォルダ群。
    pub roots: Vec<PathBuf>,
    /// スキャン時に枝刈りするディレクトリ名・ファイル名のパターン。
    /// （フェーズ2のスキャナーがこのパターンで丸ごとスキップする）
    pub exclude_patterns: Vec<String>,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            // アプリが管理するパッケージ（`*.photoslibrary` など）は
            // 見た目こそフォルダなので走査が素通りし、中のUUID名の内部ファイルを
            // 索引してしまう（実測: 14,938件・サムネイル5,399枚・84MB）。
            // `.*` はドットで始まる名前が対象なので、この名前には当たらない。
            // **Windowsでも効かせる**——Macから移った人が外付けHDDやNASへ
            // コピーしていれば、ただのフォルダとして同じ事故が起きる。
            // 一覧は取り込み元と共有する（`MANAGED_PACKAGE_PATTERNS`）
            exclude_patterns: [".*", "Thumbs.db", "$RECYCLE.BIN"]
                .iter()
                .chain(crate::scanner::MANAGED_PACKAGE_PATTERNS)
                .map(|p| (*p).to_string())
                .collect(),
        }
    }
}

/// `[performance]` 性能パラメータ。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    /// 生成するサムネイルの長辺ピクセル数。
    pub thumbnail_size: u32,
    /// サムネイル生成ワーカー数。0 = CPUコア数から自動決定。
    pub worker_threads: usize,
    /// 高品質サムネイルのディスクキャッシュ上限（MB）。0 = 無制限。
    /// 超過分は利用時刻の古い順にLRU削除される（段階B-3）。
    pub thumb_cache_mb: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            thumbnail_size: 512,
            worker_threads: 0,
            thumb_cache_mb: 4096,
        }
    }
}

impl Config {
    /// TOML文字列からパースする。欠けているフィールドはデフォルト値で補完される。
    pub fn from_toml_str(s: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(s)?)
    }

    /// TOML文字列にシリアライズする。
    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// ファイルから読み込む。ファイルが存在しない場合はデフォルト設定を返す。
    ///
    /// 読み込み時に**設定の引き上げ**を行う: 対象拡張子が旧バージョンの既定
    /// そのままなら、新しい既定（RAWを含む）へ差し替える。
    /// 自分で編集した設定は触らない。
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                let mut config = Self::from_toml_str(&s)?;
                if let Some(upgraded) = upgrade_extensions(&config.import.extensions) {
                    config.import.extensions = upgraded;
                }
                Ok(config)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// ファイルへ保存する。親ディレクトリがなければ作成する。
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml_string()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// 動画の拡張子が走査対象から漏れていないこと（二重管理の歯止め）
    #[test]
    fn 動画の拡張子はすべて走査対象に入っている() {
        super::video_extensions_are_all_scanned().unwrap();
    }

    #[test]
    fn 旧既定の拡張子は新しい既定へ引き上げられる() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.toml");
        std::fs::write(
            &path,
            "[import]
extensions = [\"jpg\", \"jpeg\", \"png\"]
",
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert!(
            config.import.extensions.iter().any(|e| e == "cr3"),
            "RAWが対象に加わる"
        );
        assert!(
            config.import.extensions.iter().any(|e| e == "heic"),
            "HEIFが対象に加わる"
        );
    }

    /// RAW対応の版から上げた人にもHEIFが届くこと（第7部 段階G）。
    #[test]
    fn raw対応時代の既定からもheifへ引き上げられる() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.toml");
        let legacy = "[import]
extensions = [\"jpg\", \"jpeg\", \"png\", \"webp\", \"cr2\", \"cr3\", \"nef\", \"nrw\", \"arw\", \"sr2\", \"raf\", \"orf\", \"rw2\", \"pef\", \"srw\", \"dng\", \"rwl\", \"3fr\", \"iiq\", \"erf\", \"mrw\", \"x3f\", \"dcr\", \"kdc\"]
";
        std::fs::write(&path, legacy).unwrap();

        let config = Config::load(&path).unwrap();
        assert!(
            config.import.extensions.iter().any(|e| e == "heic"),
            "HEIFが対象に加わる"
        );
        assert!(
            config.import.extensions.iter().any(|e| e == "cr3"),
            "RAWは残る"
        );
    }

    #[test]
    fn 自分で編集した拡張子は尊重される() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.toml");
        std::fs::write(
            &path,
            "[import]
extensions = [\"jpg\"]
",
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.import.extensions, vec!["jpg".to_string()]);
    }

    use super::*;

    #[test]
    fn デフォルト設定はラウンドトリップできる() {
        let config = Config::default();
        let toml = config.to_toml_string().unwrap();
        let parsed = Config::from_toml_str(&toml).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn 空のtomlはデフォルト値になる() {
        let parsed = Config::from_toml_str("").unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn 部分的なtomlは欠けたフィールドがデフォルトで補完される() {
        let toml = r#"
[performance]
thumbnail_size = 512
"#;
        let parsed = Config::from_toml_str(toml).unwrap();
        assert_eq!(parsed.performance.thumbnail_size, 512);
        // 指定していないフィールドはデフォルト
        assert_eq!(parsed.performance.worker_threads, 0);
        assert!(parsed.import.verify_after_copy);
        assert_eq!(parsed.routing.folder_pattern, "{year}/{year}-{month}-{day}");
    }

    #[test]
    fn 全ドメインを指定したtomlを読める() {
        let toml = r#"
[import]
last_source_dir = "E:/DCIM"
verify_after_copy = false
extensions = ["jpg", "arw"]

[routing]
destination = "D:/Pictures"
folder_pattern = "{year}/{month}"

[library]
roots = ["D:/Pictures", "D:/OldPhotos"]
exclude_patterns = ["backup"]

[performance]
thumbnail_size = 256
worker_threads = 4
"#;
        let parsed = Config::from_toml_str(toml).unwrap();
        assert_eq!(
            parsed.import.last_source_dir,
            Some(PathBuf::from("E:/DCIM"))
        );
        assert!(!parsed.import.verify_after_copy);
        assert_eq!(parsed.import.extensions, vec!["jpg", "arw"]);
        assert_eq!(
            parsed.routing.destination,
            Some(PathBuf::from("D:/Pictures"))
        );
        assert_eq!(parsed.routing.folder_pattern, "{year}/{month}");
        assert_eq!(parsed.library.roots.len(), 2);
        assert_eq!(parsed.library.exclude_patterns, vec!["backup"]);
        assert_eq!(parsed.performance.thumbnail_size, 256);
        assert_eq!(parsed.performance.worker_threads, 4);
    }

    /// 監視・USN側も同じ既定で弾くこと（走査側の試験は scanner.rs にある）。
    #[test]
    fn 既定の除外は監視側でも写真ライブラリを弾く() {
        let patterns = LibraryConfig::default().exclude_patterns;
        let excluded = |p: &str| crate::scanner::is_excluded_path(Path::new(p), &patterns);

        assert!(excluded(
            "/Users/me/Pictures/写真ライブラリ.photoslibrary/originals/0/x.heic"
        ));
        assert!(!excluded("/Users/me/Pictures/2020/a.jpg"));
    }

    /// 除外パターンは**書いてあるとおりに読む**。拡張子のような引き上げをすると、
    /// 利用者が消した除外を毎回復活させてしまう（除外を編集するUIは無く、
    /// TOMLの手編集が唯一の逃げ道なので、塞ぐと直す手段が無くなる）。
    #[test]
    fn 除外パターンは書いてあるとおりに読む() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pictkura.toml");

        // 新しい既定から `*.photoslibrary` を消した設定
        std::fs::write(
            &path,
            "[library]\nexclude_patterns = [\".*\", \"Thumbs.db\", \"$RECYCLE.BIN\"]\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(
            config.library.exclude_patterns,
            vec![".*", "Thumbs.db", "$RECYCLE.BIN"],
            "消したものが書き戻されない"
        );
    }

    #[test]
    fn 存在しないファイルのloadはデフォルト設定を返す() {
        let path = Path::new("Z:/definitely/does/not/exist/pictkura.toml");
        let config = Config::load(path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn 壊れたtomlはエラーになる() {
        let result = Config::from_toml_str("this is not toml [[[");
        assert!(matches!(result, Err(ConfigError::Parse(_))));
    }
}
