/**
 * 見た目のテーマ（システム追従／ライト／ダーク）。
 *
 * 実体は `html[data-theme]` の付け外しだけ。色は tokens.css が持っていて、
 * 未指定（system）のときは `prefers-color-scheme` に従う。
 * 選択はlocalStorageに置く（設定TOMLへ往復させるほどの情報ではなく、
 * 起動直後の一瞬でも正しいテーマで描きたいため）。
 */
export type ThemeChoice = "system" | "light" | "dark";

const KEY = "pictkura.theme";

export function readTheme(): ThemeChoice {
  const saved = localStorage.getItem(KEY);
  return saved === "light" || saved === "dark" ? saved : "system";
}

export function applyTheme(choice: ThemeChoice) {
  if (choice === "system") {
    document.documentElement.removeAttribute("data-theme");
    localStorage.removeItem(KEY);
  } else {
    document.documentElement.setAttribute("data-theme", choice);
    localStorage.setItem(KEY, choice);
  }
}

/** 起動時に保存済みのテーマを反映する（描画前に呼ぶ） */
export function initTheme() {
  applyTheme(readTheme());
}
