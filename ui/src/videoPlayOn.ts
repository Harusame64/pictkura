/**
 * 動画を続けて見る（pictkura-dev #75）。ビューアの決め方だけをここに置き、App.tsx から使う。
 */

/** ビューアの送りの向き。1 は一覧の次（グリッドは新しい順なので、ひとつ古いもの）、-1 はひとつ新しいもの */
export type ViewerDir = 1 | -1;

/**
 * 送ったあとに覚えておく向き。自分で動かした向きだけを覚え、スライドショーの送りでは変えない。
 * ← で来たら -1（新しい方＝時系列）、→ で来たら 1（古い方）
 */
export function rememberDir(prev: ViewerDir | null, dir: ViewerDir, slideshow: boolean): ViewerDir | null {
  return slideshow ? prev : dir;
}

/**
 * 動画が終わったときの送り方。
 * - スライドショー中: 今までどおり次へ、末尾では先頭へ戻る
 * - それ以外: 最後に自分で動かした向きへ続ける。まだ動かしていなければ → と同じ。端では止まる（一周しない）
 */
export function afterVideoEnded(
  playing: boolean,
  lastDir: ViewerDir | null,
): { dir: ViewerDir; wrap: boolean; slideshow: boolean } {
  if (playing) return { dir: 1, wrap: true, slideshow: true };
  return { dir: lastDir ?? 1, wrap: false, slideshow: false };
}

/**
 * 全画面になったのが動画の要素だけか（動画の再生バーの全画面ボタン）。そこではビューアの前後の矢印や
 * ←/→ が届かないので、アプリの全画面に差し替える
 */
export function isVideoOnlyFullscreen(el: { tagName?: string } | null | undefined): boolean {
  return el?.tagName === "VIDEO";
}
