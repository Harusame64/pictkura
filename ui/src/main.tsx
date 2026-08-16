import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";
import { locale } from "./i18n";
import { initTheme } from "./theme";

// 保存済みテーマを最初の描画より前に当てる（起動直後のちらつき防止）
initTheme();

// **表示言語を `<html lang>` に反映する**。`index.html` には `ja` が直書きして
// あるので、英語に切り替えても読み上げソフトは日本語の発音規則のまま英語を読む。
// ここで実際に使う言語に合わせておく（言語の切り替えは読み込み直しを伴うので、
// 起動時に一度当てれば足りる）
document.documentElement.lang = locale;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
