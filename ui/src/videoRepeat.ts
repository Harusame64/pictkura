/**
 * 動画の繰り返し（2026-11-27 利用者「動画のループ再生が欲しい、1ファイルのみと複数ファイルで分けたい」）。
 *
 * 1つのボタン（と `L`）で「オフ → すべて → 1本 → オフ」と巡る（音楽のアプリと同じ形。利用者の選択）。
 * - `one`: 今の動画を繰り返す（`<video loop>`）
 * - `all`: 再生リスト（動画の絞り込み中、または選んだものだけで開いているとき）を流し終えたら、流してきた向きの
 *   最初へ戻る。**再生リストが無いときは `all` を選べない**（オフ ⇔ 1本だけ巡る）。覚えた値が `all` でも、
 *   再生リストの無い所では `off` として効く（ボタンもオフに見せる）
 *
 * 覚える先は `localStorage`（音量と同じ、端末ごと）。保存に失敗しても、この窓の中では値が効く
 */
const KEY = "pictkura.videoRepeat";

export type VideoRepeat = "off" | "all" | "one";

/** 保存した値を読む。読めない・知らない値ならオフ */
export function parseVideoRepeat(raw: string | null): VideoRepeat {
  return raw === "all" || raw === "one" ? raw : "off";
}

/** いま効いている値。再生リストが無ければ `all` はオフ */
export function effectiveRepeat(mode: VideoRepeat, hasPlaylist: boolean): VideoRepeat {
  return mode === "all" && !hasPlaylist ? "off" : mode;
}

/** ボタン（`L`）を押したときの次の値。巡り方は、いま効いている値から数える */
export function nextRepeat(mode: VideoRepeat, hasPlaylist: boolean): VideoRepeat {
  const now = effectiveRepeat(mode, hasPlaylist);
  if (now === "off") return hasPlaylist ? "all" : "one";
  if (now === "all") return "one";
  return "off";
}

/** この窓の中で最後に決まった値。保存先より先に見る */
let current: VideoRepeat | null = null;

export function loadVideoRepeat(): VideoRepeat {
  if (current) return current;
  try {
    current = parseVideoRepeat(localStorage.getItem(KEY));
  } catch {
    current = "off";
  }
  return current;
}

export function saveVideoRepeat(mode: VideoRepeat): void {
  current = mode;
  try {
    localStorage.setItem(KEY, mode);
  } catch {
    // 覚えられなくても、この窓の中では `current` が効いている
  }
}
