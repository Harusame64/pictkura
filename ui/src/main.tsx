import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import { rememberLocaleChoice, syncMenuLocale, takePendingOpen } from "./api";
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
void takePendingOpen()
  .catch(() => null)
  .then((openAtStart) => {
    if (openAtStart) document.documentElement.classList.add("viewer-only");
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode>
        <App openAtStart={openAtStart ?? null} />
      </React.StrictMode>,
    );
    // ビューアだけの窓は、黒い地を描いてから見せる（窓は見えない状態で作ってある。白や一覧の地色を出さない）。
    // 描き終わりを待つため、次の 2 コマ後に見せる
    // Windows では隠したまま（cloak）見せて合成させ、さらに 2 コマ後に隠しを解く（#314。lib.rs の reveal_main_window）
    if (openAtStart) {
      const reveal = (phase: "begin" | "end") => invoke<number>("reveal_main_window", { phase });
      // n コマ待ってから次へ
      const afterFrames = (n: number, f: () => void) =>
        n <= 0 ? f() : requestAnimationFrame(() => afterFrames(n - 1, f));
      afterFrames(2, () =>
        void reveal("begin")
          // 隠しを解くまでのコマ数は Rust が返す（試作: 環境変数 PK_REVEAL_FRAMES で変えて測る）
          .then((frames) => afterFrames(frames, () => void reveal("end")))
          .catch(() => void getCurrentWindow().show()),
      );
    }
  });
