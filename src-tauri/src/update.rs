//! 新しい版が出ていないかを見に行く（0.2）。
//!
//! **このアプリが外へ出す唯一の通信**。送るのは「pictkura の版いくつが聞いている」
//! という名乗り（User-Agent）だけで、写真もファイル名もパスも送らない。受け取るのも
//! 最新の版の名前とページの場所だけ。落として入れ替えるところまではやらない
//! ——押したら既定のブラウザでダウンロードページを開く、そこで終わる。
//!
//! 自動で入れ替える版（0.3で検討）に差し替えるときも、外向きの面はこのファイルに
//! 閉じている。版の比べ方だけは [`pictkura_core::update`] にあり、そちらでテストが回る。

use std::time::Duration;

use tauri::Manager;

use crate::{update_config, AppState};

/// 最新の版を聞く先。**Releases API**（下書きと事前公開は除いて返る）。
const LATEST_API: &str = "https://api.github.com/repos/Harusame64/pictkura/releases/latest";
/// 押したときに開くページ。**紹介サイトのダウンロードの頁**へ送る（2026-08-22）。
/// GitHub の Releases は配布物がそのまま並ぶだけで、どれを落とせばよいかを読み手に
/// 決めさせる——サイト側は見ている端末に合わせて1つに絞って出す。
///
/// APIが返す `html_url` は使わず**こちらで固定する**——開く先を外から来た文字列に
/// 決めさせない。言語は同梱の説明書と同じで、表示言語に合わせて選ぶ。
const DOWNLOAD_PAGE_JA: &str = "https://harusame64.github.io/pictkura/ja/download.html";
const DOWNLOAD_PAGE_EN: &str = "https://harusame64.github.io/pictkura/en/download.html";

/// 待つ上限。起動直後に走るので、**繋がらない回線で長く粘らない**。
const TIMEOUT: Duration = Duration::from_secs(8);
/// 受け取る本文の上限（Releases APIの応答は数KB。桁が違えば読まずに捨てる）。
const BODY_LIMIT: u64 = 256 * 1024;

/// 確認の結果。フロントはこれを見て、控えめな知らせを出すかどうかを決める。
#[derive(serde::Serialize)]
pub struct UpdateCheckDto {
    /// いま動いている版
    pub current: String,
    /// 見つかった最新の版。確認しなかったとき（間隔内）は `None`
    pub latest: Option<String>,
    /// 新しい版が出ているか。**読めない版名は false 側**（[`pictkura_core::update`]）
    pub newer: bool,
    /// 実際に聞きに行ったか。`false` は「まだ間隔が空いていないので見送った」
    pub checked: bool,
}

/// APIの応答から見るところ。**他の項目は読まない**（増えても壊れない）。
#[derive(serde::Deserialize)]
struct LatestRelease {
    tag_name: String,
}

/// 新しい版が出ていないかを確認する。
///
/// `force` が false のときは設定の間隔（24時間）を守り、まだなら**聞きに行かない**
/// （`checked: false` で返る）。設定で切ってあれば `force` のときだけ動く
/// ——「更新を確認」を押したのに黙っているのが一番分かりにくい。
///
/// 通信は [`tauri::async_runtime::spawn_blocking`] の中で行う。ここを同期のまま
/// 走らせると、繋がらない回線で最大8秒、画面ごと止まる。
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle, force: bool) -> Result<UpdateCheckDto, String> {
    let current = app.package_info().version.to_string();
    let state = app.state::<AppState>();
    let now_ms = chrono::Local::now().timestamp_millis();
    if !force {
        let due = {
            let config = crate::lock_ok(&state.config);
            config.update.due(now_ms)
        };
        if !due {
            return Ok(UpdateCheckDto {
                current,
                latest: None,
                newer: false,
                checked: false,
            });
        }
    }
    // **聞きに行った時刻は、結果に関わらず先に控える**。繋がらない回線で
    // 起動のたびに待たされるのを防ぐ（保存に失敗しても確認自体は続ける）
    let _ = update_config(&state, |c| c.update.last_check_ms = now_ms);

    let ua = format!("pictkura/{current}");
    let tag = tauri::async_runtime::spawn_blocking(move || fetch_latest_tag(&ua))
        .await
        .map_err(|e| e.to_string())??;
    let newer = pictkura_core::update::is_newer(&current, &tag);
    Ok(UpdateCheckDto {
        current,
        latest: Some(tag),
        newer,
        checked: true,
    })
}

/// 最新の版の名前（タグ）を1つ取ってくる。
///
/// GitHubのAPIは**User-Agentが無いと弾く**ので必ず名乗る。認証は付けない
/// （公開リポジトリなので要らない。回数の上限はIPあたり1時間60回で、
/// 1日1回の確認とは桁が違う）。
fn fetch_latest_tag(user_agent: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(user_agent)
        .build()
        .into();
    let mut res = agent
        .get(LATEST_API)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("確認できませんでした: {e}"))?;
    let body = res
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()
        .map_err(|e| format!("応答を読めませんでした: {e}"))?;
    let release: LatestRelease =
        serde_json::from_str(&body).map_err(|e| format!("応答の形が違います: {e}"))?;
    Ok(release.tag_name)
}

/// ダウンロードページを既定のブラウザで開く。
///
/// **開く先そのものは渡させない**のが要点。URLを受け取る口にすると、フロント側の
/// 不具合や細工で任意のURLを開かせられる（`open_bundled_doc` と同じ考え方）。
/// 受け取るのは表示言語だけで、知らない言語は英語の頁へ送る。
#[tauri::command]
pub fn open_download_page(lang: Option<String>) -> Result<(), String> {
    let url = match lang.as_deref() {
        Some("ja") => DOWNLOAD_PAGE_JA,
        _ => DOWNLOAD_PAGE_EN,
    };
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// 起動時の自動確認を切り替える。
#[tauri::command]
pub fn set_check_update_on_start(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    update_config(&state, |c| {
        c.update.check_on_start = enabled;
        // 入れ直したときは**次の起動でまず1回聞く**。切っていた間の
        // 「最後に確認した時刻」を根拠に24時間黙られると、入れた意味が見えない
        // （その場で聞きたいときは「更新を確認」のボタンが `force` で通る）
        if enabled {
            c.update.last_check_ms = 0;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 応答から見るのはタグ1つだけで、**他の項目が増えても壊れない**こと。
    #[test]
    fn 応答はタグだけ読む() {
        let body =
            r#"{"tag_name":"v0.2.0","html_url":"https://example.invalid","assets":[],"body":"…"}"#;
        let release: LatestRelease = serde_json::from_str(body).unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
    }

    /// 開く先は**固定**（APIの返す文字列で決めない）。
    #[test]
    fn 開く先は自前のサイトとgithubに固定されている() {
        for url in [DOWNLOAD_PAGE_JA, DOWNLOAD_PAGE_EN] {
            assert!(url.starts_with("https://harusame64.github.io/pictkura/"));
        }
        assert!(LATEST_API.starts_with("https://api.github.com/repos/Harusame64/pictkura/"));
    }
}
