import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";
import { getHostPlatform, isMac, rememberLocaleChoice, syncMenuLocale, takePendingOpen } from "./api";
import { dirOf } from "./direction";
import { locale } from "./i18n";
import { initTheme } from "./theme";

// 保存済みテーマを最初の描画より前に当てる（起動直後のちらつき防止）
initTheme();

// **表示言語を `<html lang>` に反映する**。`index.html` には `ja` が直書きして
// あるので、英語に切り替えても読み上げソフトは日本語の発音規則のまま英語を読む。
// ここで実際に使う言語に合わせておく（言語の切り替えは読み込み直しを伴うので、
// 起動時に一度当てれば足りる）
document.documentElement.lang = locale;
// **向きも同じく。** アラビア語・ヘブライ語は右から左。CSS（論理プロパティと `[dir="rtl"]`）も
// JS（`direction.ts` の `isRtl`）もこの属性を見る
document.documentElement.dir = dirOf(locale);


// **メニューバーも同じ言語にする**（macOSだけ。Issue #14）。Rustは起動の一瞬だけ
// OSの言語から当てているので、`localStorage` で別の言語を選んでいる人は
// ここで直る。**待たない**——描画を止めてまで揃える話ではない
void syncMenuLocale();
void rememberLocaleChoice();

// 「プログラムから開く」で写真を渡されて起動したか。**描く前に受け取る**——一覧を一度描いてからビューアへ
// 替えると、起動時に一覧がちらつく（2026-11-15 利用者）。渡されていればビューアだけの窓として描き始める
// mac は窓の題の帯を消し、信号のボタンをアプリの上の行に重ねている（tauri.macos.conf.json の titleBarStyle: Overlay）。
// 上の行はボタンの分だけ左を空け、つかんで窓を動かせる（App.css の html.mac-overlay）。**OS はビルドから訊く**——
// UA の isMac は WebView 次第で外れうる（api.ts）。訊けなければ UA に頼る
void Promise.all([
  takePendingOpen().catch(() => null),
  getHostPlatform().catch(() => (isMac ? "macos" : "other")),
]).then(([openAtStart, platform]) => {
    if (platform === "macos") document.documentElement.classList.add("mac-overlay");
    if (openAtStart) document.documentElement.classList.add("viewer-only");
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode>
        <App openAtStart={openAtStart ?? null} />
      </React.StrictMode>,
    );
    // ビューアだけの窓は、黒い地を描いてから見せる（窓は見えない状態で作ってある。白や一覧の地色を出さない）。
    // 描き終わりを待つため、次の 2 コマ後に見せる
    if (openAtStart) {
      requestAnimationFrame(() => requestAnimationFrame(() => void getCurrentWindow().show()));
    }
  });
