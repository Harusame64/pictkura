/**
 * 動画を続けて見る（pictkura-dev #75）。ビューアの決め方だけをここに置き、App.tsx から使う。
 */

/** ビューアの送りの向き。1 は一覧の次（グリッドは新しい順なので、ひとつ古いもの）、-1 は前（ひとつ新しいもの） */
export type ViewerDir = 1 | -1;

/**
 * 覚えておく向き。利用者が自分で動かした向き（キー・前後のボタン・帯）だけを覚え、スライドショーの送りでは変えない。
 * 向きは画面の左右ではなく一覧の前後で持つ——右から左の言語では ← が「次」（1）になる
 */
export function rememberDir(prev: ViewerDir | null, dir: ViewerDir, slideshow: boolean): ViewerDir | null {
  return slideshow ? prev : dir;
}

/**
 * 動画が終わったときの送り方。送らないなら null。
 * - スライドショー中: 今までどおり次へ、末尾では先頭へ戻る
 * - 動画のフィルター中（一覧が動画だけ）で、手前に幕が無い: 最後に自分で動かした向きへ続ける。
 *   まだ動かしていなければ 1（次＝ひとつ古い）と同じ。端では止まる（一周しない）
 * - それ以外は送らない（写真の混じった一覧で写真へ飛んで止まらない。幕の裏でビューアを替えない）
 */
export function afterVideoEnded(s: {
  playing: boolean;
  lastDir: ViewerDir | null;
  videosOnly: boolean;
  curtainUp: boolean;
}): { dir: ViewerDir; wrap: boolean; slideshow: boolean } | null {
  if (s.playing) return { dir: 1, wrap: true, slideshow: true };
  if (!s.videosOnly || s.curtainUp) return null;
  return { dir: s.lastDir ?? 1, wrap: false, slideshow: false };
}

/**
 * 全画面になったのが動画の要素だけか（動画の再生バーの全画面ボタン）。そこではビューアの前後の矢印や
 * ←/→ が届かないので、アプリの全画面に差し替える
 */
export function isVideoOnlyFullscreen(el: { tagName?: string } | null | undefined): boolean {
  return el?.tagName === "VIDEO";
}
