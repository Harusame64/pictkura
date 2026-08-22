//! 爆速検索のクエリ言語とインデックス用テキスト整形（第4部 段階D）。
//!
//! 爆速の原則:
//! - 検索は**必ずインデックスシーク**に落とす。mediaへのLIKE '%x%'（全表スキャン）は禁止。
//! - 自由語はFTS5（`media_fts`）のMATCHで解決し、返ったrowidで本表を引く。
//! - カメラは低カーディナリティのファセットなので、FTSに載せず
//!   `cameras` 表（数十行）を引いて `camera_id` のインデックスをシークする。
//! - 日付・お気に入りは既存の `day_key` / 部分インデックスの条件として重ねる。
//!
//! 分かち書きしない言語の扱い（N-gram索引）:
//! FTS5標準のunicode61トークナイザは、単語をスペースで区切らない言語
//! （日本語・中国語・タイ語・ラオ語・クメール語・ビルマ語）で文がまるごと
//! 1トークンになり、「沖縄旅行.jpg」に対する「旅行」が引けない。
//! trigramトークナイザは中間一致ができる代わりに2文字クエリ
//! （沖縄・家族・花火…日本語では最頻）が効かない。
//! そこで**索引を書くときにそれらの文字の連続だけをbigramへ展開**する。
//! 「沖縄旅行」→「沖縄 縄旅 旅行」。クエリ側も同じ展開をして**フレーズ検索**に
//! すれば、位置が連続するbigramだけがヒットするため誤検出も出ない。
//! ラテン・キリル・ギリシャ・インド系・アラビア文字などスペースで区切る言語は
//! 展開せず、unicode61のトークン＋前方一致でそのまま引ける。

/// この文字が「分かち書きしない文字体系」か（bigram展開の対象）。
///
/// 単語をスペースで区切らない言語は、unicode61トークナイザだと文がまるごと
/// 1トークンになってしまい部分一致が引けない。日本語・中国語だけでなく
/// タイ語・ラオ語・クメール語・ビルマ語も同じ問題を持つので、まとめて
/// bigram展開の対象にする（話者数の多い言語をなるべく広く拾う）。
///
/// ハングルは分かち書きするが、bigramにしても害はなく、
/// 助詞が続けて書かれる分だけ部分一致が効くようになるので含めている。
fn is_unsegmented(c: char) -> bool {
    matches!(c as u32,
        0x0E00..=0x0E7F   // タイ文字
        | 0x0E80..=0x0EFF // ラオ文字
        | 0x1000..=0x109F // ビルマ文字
        | 0x1780..=0x17FF // クメール文字
        | 0x3040..=0x30FF // ひらがな・カタカナ
        | 0x3400..=0x4DBF // CJK拡張A
        | 0x4E00..=0x9FFF // CJK統合漢字
        | 0xAC00..=0xD7AF // ハングル音節
        | 0xF900..=0xFAFF // CJK互換漢字
        | 0xFF66..=0xFF9F // 半角カナ
    )
}

/// 索引用テキストへ整形する。CJKの連続はbigramの列へ、それ以外はそのまま。
///
/// 例: `"2019 沖縄旅行.jpg"` → `"2019 沖縄 縄旅 旅行 行 .jpg"`
///
/// CJKの前後には必ず空白を入れる（`"沖縄A"` が1トークン `沖縄a` に
/// 化けるのを防ぐ）。1文字だけのCJK連続はその文字自身をトークンにする。
///
/// **末尾の1文字も単独トークンとして足す**。bigramだけだと「沖縄旅行」の
/// 索引語は `沖縄 縄旅 旅行` になり、1文字検索の `"行"*` が
/// 「行で始まる語」を探しても当たらない（行は常にbigramの2文字目にしか出ない）。
/// 末尾の1文字を足せば、どの位置の1文字でも引けるようになる。
/// 追加は連続ごとに1トークンだけなので索引はほとんど増えない。
pub fn index_text(s: &str) -> String {
    expand(s, true)
}

/// CJKの連続をbigramへ展開する。
///
/// `trailing_unigram` は**索引側だけ**true にすること。クエリ側で足すと、
/// 例えば「沖縄旅行」の検索が `沖縄 縄旅 旅行 行` のフレーズになり、
/// 「沖縄旅行記」（…旅行 行記 記）では4語目の位置に `行記` が来るため
/// 一致しなくなる＝部分一致が壊れる。
fn expand(s: &str, trailing_unigram: bool) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    let mut run: Vec<char> = Vec::new();

    let flush = |run: &mut Vec<char>, out: &mut String| {
        if run.is_empty() {
            return;
        }
        if run.len() == 1 {
            out.push(' ');
            out.push(run[0]);
        } else {
            for w in run.windows(2) {
                out.push(' ');
                out.push(w[0]);
                out.push(w[1]);
            }
            if trailing_unigram {
                // 末尾1文字（bigramの先頭には現れない文字）を単独で引けるようにする。
                // bigram列の**後ろ**に置くので、フレーズ照合（位置が連続するbigram）は壊れない
                out.push(' ');
                out.push(run[run.len() - 1]);
            }
        }
        out.push(' ');
        run.clear();
    };

    for c in s.chars() {
        if is_unsegmented(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// FTS5の文字列リテラルとして引用する（`"` は `""` へエスケープ）。
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// 1語をFTS5のMATCH式へ変換する。
///
/// - CJKの連続 → bigram列の**フレーズ**（`"沖縄 縄旅 旅行"`）。位置が連続する
///   ものだけを拾うのでN-gram特有の誤検出が出ない
/// - それ以外 → unicode61のトークンをそのまま並べる
/// - 式全体の末尾には `*` を付けて**前方一致**にする（打鍵ごとの絞り込み用）。
///   1文字のCJK（`"沖"*`）もこれでヒットする
///
/// トークンが1つも取れない（記号だけ等）場合は `None`。
fn term_to_match(word: &str) -> Option<String> {
    // クエリ側は末尾ユニグラムを足さない（[`expand`] のコメント参照）
    let expanded = expand(word, false);
    let tokens: Vec<&str> = expanded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    // CJK由来のbigram列は連続位置に並ぶため、フレーズ1つにまとめて精度を上げる。
    // ASCII語との混在（"IMG 家族" 等）は隣接を要求すると外れるので、
    // 「CJKの連続」単位でフレーズを作り、語同士はAND（空白区切り）にする。
    let mut parts: Vec<String> = Vec::new();
    let mut phrase: Vec<&str> = Vec::new();
    let is_bigram = |t: &str| t.chars().all(is_unsegmented);
    for tok in tokens {
        if is_bigram(tok) {
            phrase.push(tok);
        } else {
            if !phrase.is_empty() {
                parts.push(quote(&phrase.join(" ")));
                phrase.clear();
            }
            parts.push(quote(tok));
        }
    }
    if !phrase.is_empty() {
        parts.push(quote(&phrase.join(" ")));
    }
    // 末尾だけ前方一致にする（インクリメンタル検索で「途中まで打った語」を拾う）
    if let Some(last) = parts.last_mut() {
        last.push('*');
    }
    Some(parts.join(" "))
}

/// 検索条件。すべての条件はAND（絞り込み）で重なる。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchQuery {
    /// 自由語（ファイル名・フォルダ名・カメラ名のいずれかに一致すれば拾う）
    pub terms: Vec<String>,
    /// `camera:` 指定（カメラ名だけが対象）
    pub camera: Vec<String>,
    /// `folder:` 指定（フォルダ名だけが対象）
    pub folder: Vec<String>,
    /// 表示日の下限（YYYYMMDD整数、含む）
    pub day_from: Option<i64>,
    /// 表示日の上限（YYYYMMDD整数、含む）
    pub day_to: Option<i64>,
    /// お気に入り（★）のみ
    pub favorites_only: bool,
    /// 選別で選んだもの（⚑ Pick。0.2 ②）のみ
    pub picked_only: bool,
    /// 種類（画像・RAW・動画）の指定。`None` は指定なし。
    ///
    /// **空の `Some` は「何にも当たらない」**——`kind:` に知らない値が来たとき、
    /// 条件ごと落とすと絞り込みが黙って消えて全件が出る（`year:` と同じ考え方）。
    pub kinds: Option<Vec<MediaKind>>,
}

/// 一覧の絞り込み（画面左の「すべての画像 / ★ / ⚑」に対応する）。
///
/// **★と⚑は別の棚**。検索語の側では `★ ⚑` と重ねられるが、
/// この入口は1つだけ選ぶ形にしてある（画面の作りに合わせる）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaFilter {
    /// 絞り込みなし
    #[default]
    All,
    /// お気に入り（★）だけ
    Fav,
    /// 選別で選んだもの（⚑）だけ
    Picked,
}

/// メディアの種類（画面左の「種類」に対応する）。
///
/// **拡張子だけで決める**。中身を読まなくても分けられるので、走査のたびに
/// 1行ぶんの文字列比較で済む。DBには `media.kind` として数値で持たせ、
/// 絞り込みは索引シークに落ちる（一覧のLIKEは禁止という原則を守るため）。
///
/// 数値はDBに書いてあるので**並びを変えないこと**。増やすときは末尾に足す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    /// そのまま見える画像（JPEG・PNG・HEIC・AVIF・TIFF…）
    Photo = 0,
    /// RAW（現像前。[`crate::raw::RAW_EXTENSIONS`]）
    Raw = 1,
    /// 動画（[`crate::video::VIDEO_EXTENSIONS`]）
    Video = 2,
}

impl MediaKind {
    /// 拡張子から決める。RAWでも動画でもなければ「そのまま見える画像」。
    ///
    /// **RAWを先に見る**。`.dng` のように動画側と紛れる綴りは無いが、
    /// 判定の順番で結果が変わらないことを読んで分かるようにしておく。
    pub fn from_extension(ext: &str) -> Self {
        if crate::raw::is_raw_extension(ext) {
            Self::Raw
        } else if crate::video::is_video_extension(ext) {
            Self::Video
        } else {
            Self::Photo
        }
    }

    /// パスの拡張子から決める（拡張子が無ければ画像扱い）。
    pub fn from_path(path: &std::path::Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map_or(Self::Photo, Self::from_extension)
    }

    /// 検索語の `kind:` に書ける名前から読む。読めなければ `None`。
    ///
    /// 英語と日本語の両方を受ける。画面から渡すのは英語の綴りだけだが、
    /// 検索ボックスに直接打つ人のために、表示している言葉でも通るようにする。
    pub fn parse(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            // **`jpg` は別名にしない**。そう打つ人は PNG や HEIC を外したいはずで、
            // 「画像ぜんぶ」が返ると期待とずれる（形式ごとの絞り込みは持っていない）
            "photo" | "image" | "写真" | "画像" => Some(Self::Photo),
            "raw" => Some(Self::Raw),
            "video" | "movie" | "動画" => Some(Self::Video),
            _ => None,
        }
    }
}

impl SearchQuery {
    /// 入口の絞り込みだけの（＝検索語なしの）クエリ。
    pub fn filtered(filter: MediaFilter) -> Self {
        Self {
            favorites_only: filter == MediaFilter::Fav,
            picked_only: filter == MediaFilter::Picked,
            ..Default::default()
        }
    }

    /// 全件表示（絞り込みなし）か。既存のタイムライン経路と同じ計画で実行できる。
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.camera.is_empty()
            && self.folder.is_empty()
            && self.day_from.is_none()
            && self.day_to.is_none()
            && !self.favorites_only
            && !self.picked_only
            && self.kinds.is_none()
    }

    /// FTSを引く必要があるか（自由語・フォルダのいずれかがある）。
    /// カメラはFTSではなく `cameras` 表＋`camera_id` のインデックスで解決する。
    pub fn needs_fts(&self) -> bool {
        !self.terms.is_empty() || !self.folder.is_empty()
    }

    /// 自由語ごとの `media_fts MATCH ?` 式を、元の語とセットで返す。
    ///
    /// **語ごとに分けて返す**のは、自由語が「名前・フォルダに一致」だけでなく
    /// 「カメラ名に一致」でも拾えるようにするため（DB側で語単位に
    /// `FTS一致 OR camera_id一致` を組む）。条件同士はAND（絞り込み）で重なる。
    ///
    /// カメラ指定をFTSの列に入れないのは、機種数が数十しかない低カーディナリティの
    /// ファセットで、全行ぶんの索引エントリを持つのが容量・速度ともに無駄なため。
    ///
    /// MATCH式が `None` の語は**索引語を1つも作れない**もの（記号・絵文字だけ等）。
    /// 条件を落とすと絞り込みが消えて全件が返ってしまうため、呼び出し側は
    /// 「一致なし」として扱うこと（カメラ名に当たる可能性は残る）。
    pub fn term_matches(&self) -> Vec<(&str, Option<String>)> {
        self.terms
            .iter()
            .map(|t| (t.as_str(), term_to_match(t)))
            .collect()
    }

    /// `folder:` 指定ごとのMATCH式（folder列に限定）。
    pub fn folder_matches(&self) -> Vec<String> {
        self.folder
            .iter()
            .filter_map(|w| term_to_match(w).map(|m| format!("{{folder}} : ({m})")))
            .collect()
    }
}

/// 入力文字列を空白区切りのトークンへ分割する（`"..."` でくくると空白を含められる）。
fn tokenize(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' => quoted = !quoted,
            // 全角スペースも区切りとして扱う（日本語入力のまま検索できるように）
            c if !quoted && (c.is_whitespace() || c == '\u{3000}') => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 日付として認める年の範囲。`year:` と `2019年` で食い違わないよう、
/// **両方がこれを使う**。
const MIN_YEAR: i64 = 1800;
const MAX_YEAR: i64 = 9999;

/// 「何にも当たらない範囲」を作るための番兵（`day_from > day_to`）。
const MIN_DAY_KEY: i64 = MIN_YEAR * 10000;
const MAX_DAY_KEY: i64 = MAX_YEAR * 10000 + 1231;

/// `year:` の値を年の範囲（その年の元日〜大晦日）にする。
///
/// 受けるのは**4桁の数字だけ**。`parse_date_range` と違って前後関係が
/// はっきりしている（`year:` と明示されている）ので、単位も区切りも要らない。
///
/// **全角の数字も受ける**。`年:` という別名は日本語入力の利用者向けなのに、
/// IMEが全角のままだと弾かれる——しかも `year:` は指定として食べるので、
/// **絞ったつもりで全件が出る**という一番たちの悪い壊れ方をする。
///
/// 年として認める範囲は `parse_date_range` と揃える（`year:1500` と `1500年` で
/// 挙動が食い違わないように）。
fn parse_year(value: &str) -> Option<(i64, i64)> {
    // 全角数字（Ｕ+FF10〜FF19）を半角へ寄せる
    let value: String = value
        .trim()
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            other => other,
        })
        .collect();
    if value.len() != 4 || !value.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year: i64 = value.parse().ok()?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    Some((year * 10000 + 101, year * 10000 + 1231))
}

/// 日付らしいトークンを (下限, 上限) のday_key範囲へ変換する。
///
/// 認識する形（区切りは `-` `/` `.` と 年月日）:
/// `2019年` `2019-08` `2019/8` `2019年8月` `2019-08-11` `2019年8月11日`
///
/// **裸の4桁数字（`2019`）は日付として扱わない**。ファイル名の検索語と
/// 区別できないため、日付で絞りたいときは `2019年` のように単位を付けるか、
/// `year:2019` と書く（UIのコマンドパレットが日付候補を出して補う）。
fn parse_date_range(token: &str) -> Option<(i64, i64)> {
    let mut nums: Vec<i64> = Vec::new();
    let mut cur = String::new();
    let mut had_unit = false;
    for c in token.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                nums.push(cur.parse().ok()?);
                cur.clear();
            }
            match c {
                '-' | '/' | '.' => {}
                '年' | '月' | '日' => had_unit = true,
                _ => return None, // 想定外の文字が混ざるものは日付ではない
            }
        }
    }
    if !cur.is_empty() {
        nums.push(cur.parse().ok()?);
    }
    // 単位も区切りもない裸の数字は日付扱いしない
    if nums.len() == 1 && !had_unit {
        return None;
    }
    let year = *nums.first()?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    match nums.len() {
        1 => Some((year * 10000 + 101, year * 10000 + 1231)),
        2 => {
            let m = nums[1];
            if !(1..=12).contains(&m) {
                return None;
            }
            Some((year * 10000 + m * 100 + 1, year * 10000 + m * 100 + 31))
        }
        3 => {
            let (m, d) = (nums[1], nums[2]);
            if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                return None;
            }
            let key = year * 10000 + m * 100 + d;
            Some((key, key))
        }
        _ => None,
    }
}

/// 検索文字列を [`SearchQuery`] へ解析する。
///
/// 対応する指定:
/// - `camera:α7` / `cam:α7` / `カメラ:α7` — カメラ名で絞る
/// - `folder:沖縄` / `dir:沖縄` / `フォルダ:沖縄` — フォルダ名で絞る
/// - `year:2019` / `年:2019` — その年だけに絞る（`2019年` と同じ範囲）
/// - `2019年` `2019-08` `2019年8月11日` — 撮影日で絞る
/// - `★` / `fav:` — お気に入りのみ
/// - `⚑` / `pick:` — 選別で選んだもののみ（0.2 ②）
/// - `kind:raw` / `type:video` / `種類:動画` — 種類（画像・RAW・動画）で絞る
/// - それ以外 — 自由語（ファイル名・フォルダ名・カメラ名が対象）
pub fn parse_query(input: &str, filter: MediaFilter) -> SearchQuery {
    let mut q = SearchQuery::filtered(filter);
    for token in tokenize(input) {
        // `key:value` 指定
        if let Some((key, value)) = token.split_once(':') {
            let value = value.trim();
            let matched = match key.to_ascii_lowercase().as_str() {
                "camera" | "cam" | "カメラ" => {
                    if !value.is_empty() {
                        q.camera.push(value.to_string());
                    }
                    true
                }
                "folder" | "dir" | "フォルダ" => {
                    if !value.is_empty() {
                        q.folder.push(value.to_string());
                    }
                    true
                }
                // **年だけで絞る口**。裸の `2019` を日付にしない方針（ファイル名の
                // 検索語と区別できない）は変えないが、日本語の「年」に当たる単位が
                // 無い言語では、そのままだと**検索ボックスから年で絞る手段が無い**。
                // `camera:` `folder:` と同じ形で、言語に依らない入口を用意する。
                "year" | "年" => {
                    // 値が年として読めなくても、`year:` は指定として食べる。
                    // 検索語に落とすと `year:abc` がファイル名の検索になって、
                    // 「絞ったつもりが全然違うものが出る」ことになる。
                    //
                    // **読めなかったときは何にも当たらない範囲にする**。ただ無視すると
                    // 絞り込みが黙って消えて**全件が出る**——絞ったつもりの利用者には
                    // 一番分かりにくい壊れ方になる。0件なら打ち間違いに気付ける
                    let (from, to) = parse_year(value).unwrap_or((MAX_DAY_KEY, MIN_DAY_KEY));
                    q.day_from = Some(q.day_from.map_or(from, |v: i64| v.max(from)));
                    q.day_to = Some(q.day_to.map_or(to, |v: i64| v.min(to)));
                    true
                }
                "fav" | "favorite" | "★" => {
                    q.favorites_only = true;
                    true
                }
                "pick" | "picked" | "選別" => {
                    q.picked_only = true;
                    true
                }
                // 種類（画像・RAW・動画）。画面左の「種類」もこの口を通る。
                //
                // **知らない値は何にも当たらないようにする**（`year:` と同じ）。
                // 黙って無視すると絞ったつもりで全件が出て、一番気付きにくい
                "kind" | "type" | "種類" => {
                    let asked: Vec<MediaKind> = MediaKind::parse(value).into_iter().collect();
                    // 2つめ以降はANDで重ねる（違う種類を並べれば0件になる）
                    q.kinds = Some(match q.kinds.take() {
                        Some(prev) => prev.into_iter().filter(|k| asked.contains(k)).collect(),
                        None => asked,
                    });
                    true
                }
                _ => false,
            };
            if matched {
                continue;
            }
        }
        if token == "★" {
            q.favorites_only = true;
            continue;
        }
        if token == "⚑" {
            q.picked_only = true;
            continue;
        }
        if let Some((from, to)) = parse_date_range(&token) {
            // 複数の日付指定は範囲の共通部分（＝より狭い方）を採る
            q.day_from = Some(q.day_from.map_or(from, |v: i64| v.max(from)));
            q.day_to = Some(q.day_to.map_or(to, |v: i64| v.min(to)));
            continue;
        }
        q.terms.push(token);
    }
    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 種類は拡張子で決まる() {
        use std::path::Path;
        assert_eq!(MediaKind::from_path(Path::new("a.JPG")), MediaKind::Photo);
        assert_eq!(MediaKind::from_path(Path::new("a.cr3")), MediaKind::Raw);
        assert_eq!(MediaKind::from_path(Path::new("a.MOV")), MediaKind::Video);
        // 知らない綴り・拡張子なしは「そのまま見える画像」側に寄せる
        assert_eq!(MediaKind::from_path(Path::new("a.xyz")), MediaKind::Photo);
        assert_eq!(MediaKind::from_path(Path::new("a")), MediaKind::Photo);
    }

    /// 画面が `kind:` を**先頭に置く**理由（`ui/src/api.ts` の `withKind`）。
    ///
    /// 打ちかけの引用符（`"holiday` と入れた途中の状態）があると、そのうしろに
    /// 足した指定は自由語の一部として飲み込まれる。ここを変えるなら、
    /// 畳み込む側も一緒に見直すこと。
    #[test]
    fn 閉じていない引用符はうしろの指定を飲み込む() {
        let kinds = |s: &str| parse_query(s, MediaFilter::All).kinds;
        assert_eq!(kinds("\"holiday kind:raw"), None, "うしろだと消える");
        assert_eq!(
            kinds("kind:raw \"holiday"),
            Some(vec![MediaKind::Raw]),
            "先頭なら引用符より前なので必ず読まれる"
        );
    }

    #[test]
    fn 種類の指定を読む() {
        let kinds = |s: &str| parse_query(s, MediaFilter::All).kinds;
        assert_eq!(kinds("kind:raw"), Some(vec![MediaKind::Raw]));
        assert_eq!(kinds("種類:動画"), Some(vec![MediaKind::Video]));
        assert_eq!(kinds("type:Photo"), Some(vec![MediaKind::Photo]));
        // 指定なしは条件を置かない
        assert_eq!(kinds("沖縄"), None);
        // **知らない値は何にも当たらない**（黙って全件を出さない）
        assert_eq!(kinds("kind:abc"), Some(vec![]));
        // 違う種類を並べたらANDで重なって0件
        assert_eq!(kinds("kind:raw kind:video"), Some(vec![]));
        // 同じ種類を並べても変わらない
        assert_eq!(kinds("kind:raw kind:raw"), Some(vec![MediaKind::Raw]));
        // 種類の指定だけでも「絞り込みあり」
        assert!(!parse_query("kind:video", MediaFilter::All).is_empty());
    }

    #[test]
    fn cjkはbigramへ展開される() {
        // 索引側は末尾1文字も単独トークンにする（どの位置の1文字でも引けるように）
        assert_eq!(index_text("沖縄旅行"), " 沖縄 縄旅 旅行 行 ");
        // 1文字のCJKはその文字自身
        assert_eq!(index_text("空"), " 空 ");
        // ASCIIは展開しない。CJKの前後には必ず空白が入る
        assert_eq!(index_text("2019 沖縄A"), "2019  沖縄 縄 A");
        assert_eq!(index_text("DSC00123.JPG"), "DSC00123.JPG");
        // クエリ側は末尾1文字を足さない（足すと部分一致が壊れる）
        assert_eq!(expand("沖縄旅行", false), " 沖縄 縄旅 旅行 ");
    }

    #[test]
    fn 分かち書きしない言語はまとめてbigramになる() {
        // タイ語（สวัสดี = こんにちは）。unicode61だと1トークンで中間一致できない
        assert_eq!(expand("สวัส", false), " สว วั ัส ");
        // ハングルも対象（助詞が続けて書かれるぶん部分一致が効くようになる）
        assert_eq!(expand("한국", false), " 한국 ");
        // スペースで区切る言語（ラテン・キリル・アラビア）は展開しない。
        // unicode61のトークン＋前方一致でそのまま引ける
        assert_eq!(expand("Ferien", false), "Ferien");
        assert_eq!(expand("Отпуск", false), "Отпуск");
        assert_eq!(expand("رحلة", false), "رحلة");
    }

    #[test]
    fn 分かち書きしない言語も中間一致のmatch式になる() {
        // 語の途中（วั）が phrase の一部として引ける
        assert_eq!(term_to_match("สวัส").unwrap(), "\"สว วั ัส\"*");
        // ラテン語はトークンそのまま＋前方一致
        assert_eq!(term_to_match("Ferien").unwrap(), "\"Ferien\"*");
    }

    #[test]
    fn cjk語はフレーズ_ascii語は前方一致になる() {
        assert_eq!(term_to_match("沖縄旅行").unwrap(), "\"沖縄 縄旅 旅行\"*");
        assert_eq!(term_to_match("旅行").unwrap(), "\"旅行\"*");
        assert_eq!(term_to_match("沖").unwrap(), "\"沖\"*");
        assert_eq!(term_to_match("dsc").unwrap(), "\"dsc\"*");
        // 記号で区切られたASCIIは複数トークンのAND、末尾だけ前方一致
        assert_eq!(term_to_match("ILCE-7M3").unwrap(), "\"ILCE\" \"7M3\"*");
        // トークンが取れないものはNone（MATCH式に空文字を渡さない）
        assert_eq!(term_to_match("!!!"), None);
        assert_eq!(term_to_match(""), None);
    }

    #[test]
    fn 引用符はエスケープされる() {
        let m = term_to_match("a\"b").unwrap();
        assert_eq!(m, "\"a\" \"b\"*");
    }

    #[test]
    fn yearは年だけで絞れる() {
        // `2019年` と同じ範囲になる
        let q = parse_query("year:2019", crate::MediaFilter::All);
        assert_eq!(q.day_from, Some(20190101));
        assert_eq!(q.day_to, Some(20191231));
        assert!(q.terms.is_empty(), "検索語には落ちない");
        // 日本語の単位でも同じ
        let q = parse_query("年:2019", crate::MediaFilter::All);
        assert_eq!(q.day_from, Some(20190101));
        // 他の条件と併用できる（範囲は狭い方に寄る）
        let q = parse_query("沖縄 year:2019 2019年8月", crate::MediaFilter::All);
        assert_eq!(q.terms, vec!["沖縄"]);
        assert_eq!(q.day_from, Some(20190801));
        assert_eq!(q.day_to, Some(20190831));
        // 全角のままでも通る（`年:` は日本語入力の利用者向けなので、ここが
        // 効かないと「絞ったつもりで全件」という一番たちの悪い壊れ方をする）
        let q = parse_query("年:２０１９", crate::MediaFilter::All);
        assert_eq!(q.day_from, Some(20190101));
        assert_eq!(q.day_to, Some(20191231));
        assert!(q.terms.is_empty());
        // **年として読めない値でも検索語には落とさない**。落とすと
        // 「絞ったつもりが全然違うものが出る」ことになる。代わりに
        // **何にも当たらない範囲**にして、0件で打ち間違いに気付けるようにする
        let q = parse_query("year:abc", crate::MediaFilter::All);
        assert!(q.terms.is_empty());
        assert!(
            q.day_from.unwrap() > q.day_to.unwrap(),
            "読めない年は何にも当たらない範囲になる"
        );
        for bad in ["19", "20190", "12x4", "", "1500"] {
            assert_eq!(parse_year(bad), None, "年として読めない: {bad}");
        }
        assert_eq!(parse_year("2019"), Some((20190101, 20191231)));
        // 年の範囲は `2019年` 側と揃っている
        assert_eq!(parse_year("1800"), parse_date_range("1800年"));
        assert_eq!(parse_year("1799"), parse_date_range("1799年"));
    }

    #[test]
    fn 日付は単位か区切りがあるときだけ認識する() {
        assert_eq!(parse_date_range("2019年"), Some((20190101, 20191231)));
        assert_eq!(parse_date_range("2019-08"), Some((20190801, 20190831)));
        assert_eq!(parse_date_range("2019年8月"), Some((20190801, 20190831)));
        assert_eq!(parse_date_range("2019/8/11"), Some((20190811, 20190811)));
        assert_eq!(
            parse_date_range("2019年8月11日"),
            Some((20190811, 20190811))
        );
        // 裸の数字はファイル名の検索語と区別できないので日付にしない
        assert_eq!(parse_date_range("2019"), None);
        assert_eq!(parse_date_range("1234"), None);
        // 妥当でない月日・年は日付ではない
        assert_eq!(parse_date_range("2019-13"), None);
        assert_eq!(parse_date_range("2019/8/32"), None);
        assert_eq!(parse_date_range("99年"), None);
        // 日付ではない文字列
        assert_eq!(parse_date_range("DSC00123.JPG"), None);
        assert_eq!(parse_date_range("沖縄"), None);
    }

    #[test]
    fn クエリを条件へ分解する() {
        let q = parse_query("沖縄 camera:α7 2019年8月 ★", crate::MediaFilter::All);
        assert_eq!(q.terms, vec!["沖縄"]);
        assert_eq!(q.camera, vec!["α7"]);
        assert_eq!((q.day_from, q.day_to), (Some(20190801), Some(20190831)));
        assert!(q.favorites_only);
        assert!(!q.is_empty());
        assert!(q.needs_fts());
    }

    /// 選別の印（⚑）は★とは**別の入口**であること（0.2 ②）
    #[test]
    fn 選別の印はお気に入りとは別の条件になる() {
        let q = parse_query("⚑", crate::MediaFilter::All);
        assert!(q.picked_only && !q.favorites_only);
        let q = parse_query("pick:1", crate::MediaFilter::All);
        assert!(q.picked_only && q.terms.is_empty());
        let q = parse_query("", crate::MediaFilter::Picked);
        assert!(q.picked_only && !q.favorites_only && !q.is_empty());
        let q = parse_query("★ ⚑", crate::MediaFilter::All);
        assert!(q.picked_only && q.favorites_only, "検索語では重ねられる");
        assert!(parse_query("", crate::MediaFilter::All).is_empty());
    }

    #[test]
    fn 引用符で空白を含む語を指定できる() {
        let q = parse_query("\"家族 写真\" folder:\"2019 夏\"", crate::MediaFilter::All);
        assert_eq!(q.terms, vec!["家族 写真"]);
        assert_eq!(q.folder, vec!["2019 夏"]);
    }

    #[test]
    fn 全角スペースも区切りになる() {
        let q = parse_query("沖縄　花火", crate::MediaFilter::All);
        assert_eq!(q.terms, vec!["沖縄", "花火"]);
    }

    #[test]
    fn 空のクエリは絞り込みなし() {
        let q = parse_query("   ", crate::MediaFilter::All);
        assert!(q.is_empty());
        assert!(!q.needs_fts());
        assert!(q.term_matches().is_empty());
        // ★だけなら絞り込みはあるがFTSは不要
        let q = parse_query("", crate::MediaFilter::Fav);
        assert!(!q.is_empty());
        assert!(!q.needs_fts());
    }

    #[test]
    fn 自由語とフォルダ指定はそれぞれのmatch式になる() {
        let q = parse_query("沖縄 dsc folder:旅行", crate::MediaFilter::All);
        assert_eq!(
            q.term_matches(),
            vec![
                ("沖縄", Some("\"沖縄\"*".to_string())),
                ("dsc", Some("\"dsc\"*".to_string()))
            ]
        );
        assert_eq!(q.folder_matches(), vec!["{folder} : (\"旅行\"*)"]);
    }

    #[test]
    fn 索引語を作れない語は落とさずnoneで返す() {
        // 条件ごと落とすと絞り込みが消えて全件が返ってしまうため、
        // 「一致なし」としてDB側で扱えるよう語自体は残す
        let q = parse_query("!!! 沖縄", crate::MediaFilter::All);
        assert_eq!(
            q.term_matches(),
            vec![("!!!", None), ("沖縄", Some("\"沖縄\"*".to_string()))]
        );
        assert!(!q.is_empty(), "絞り込みは効いている");
    }

    #[test]
    fn カメラ指定はftsに含めない() {
        // camerasテーブル＋camera_idのインデックスで解決するため、MATCH式には出ない
        let q = parse_query("camera:α7", crate::MediaFilter::All);
        assert!(q.term_matches().is_empty());
        assert!(q.folder_matches().is_empty());
        assert!(!q.needs_fts());
        assert!(!q.is_empty(), "絞り込み条件としては有効");
    }

    #[test]
    fn 日付範囲は狭い方へ絞り込まれる() {
        let q = parse_query("2019年 2019年8月", crate::MediaFilter::All);
        assert_eq!((q.day_from, q.day_to), (Some(20190801), Some(20190831)));
    }
}
