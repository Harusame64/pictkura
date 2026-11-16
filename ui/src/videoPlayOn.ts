/**
 * 動画を続けて見る（pictkura-dev #75）。ビューアの決め方だけをここに置き、App.tsx から使う。
 */

/** ビューアの送りの向き。1 は一覧の次（グリッドは新しい順なので、ひとつ古いもの）、-1 は前（ひとつ新しいもの） */
export type ViewerDir = 1 | -1;

/**
 * 動画が終わったときの送り方。送らないなら null。
 * - スライドショー中: 今までどおり次へ、末尾では先頭へ戻る
 * - 動画のフィルター中（一覧が動画だけ）か、選んだものだけで開いている（`inScope`。選んだ動画が再生リストになる）
 *   とき、手前に幕が無ければ: 最後に自分で動かした向きへ続ける。**まだ動かしていなければ撮った順**（-1＝ひとつ
 *   新しいもの。一覧は新しい順なので、古い→新しいは前へ歩く）——オペラや演劇を 1 幕→2 幕の順に撮ると、一覧の
 *   向きのままでは最終幕から逆に流れた（2026-11-16 利用者の選択）。端では止まる（一周しない）
 * - それ以外は送らない（写真の混じった一覧で写真へ飛んで止まらない。幕の裏でビューアを替えない）
 */
export function afterVideoEnded(s: {
  playing: boolean;
  lastDir: ViewerDir | null;
  videosOnly: boolean;
  inScope: boolean;
  curtainUp: boolean;
}): { dir: ViewerDir; wrap: boolean; slideshow: boolean } | null {
  if (s.playing) return { dir: 1, wrap: true, slideshow: true };
  if (!(s.videosOnly || s.inScope) || s.curtainUp) return null;
  return { dir: s.lastDir ?? -1, wrap: false, slideshow: false };
}

/**
 * 選んだものだけで開くときの最初の1枚の位置（列は一覧と同じ新しい順）。**選んだのが動画だけなら、いちばん古い
 * もの**——撮った順に続けて流すので、新しい端から開くと何も流れずに終わる。写真が混じれば今までどおり先頭
 */
export function scopeStartIndex(items: readonly { is_video: boolean }[]): number {
  return items.length > 0 && items.every((it) => it.is_video) ? items.length - 1 : 0;
}

/**
 * 全画面になったのが動画の要素だけか（動画の再生バーの全画面ボタン）。そこではビューアの前後の矢印や
 * ←/→ が届かないので、ウィンドウの全画面に替える
 */
export function isVideoOnlyFullscreen(el: { tagName?: string } | null | undefined): boolean {
  return el?.tagName === "VIDEO";
}
