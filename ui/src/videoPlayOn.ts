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
 *
 * 選んだものに写真が混じっていても、**流す先が動画のときだけ**流す（`nextIsVideo`。写真へ着いて止まる、にしない。
 * /code-review）。動画のフィルター中は並ぶのがみな動画なので、訊かなくてよい
 *
 * **すべてを繰り返す**（`repeatAll`、videoRepeat.ts）ときは、端で止まらず流してきた向きの最初へ戻る（`wrap`）。
 * 選んだものでは、端に着いたとき（`atEnd`）に戻る先が動画のときだけ戻る（`wrapIsVideo`。写真で止まらない）
 */
export function afterVideoEnded(s: {
  playing: boolean;
  lastDir: ViewerDir | null;
  videosOnly: boolean;
  inScope: boolean;
  curtainUp: boolean;
  /** 選んだものの列で、その向きの次が動画か（選んだものだけで開いているときに訊く） */
  nextIsVideo: (dir: ViewerDir) => boolean;
  /** すべてを繰り返すか（再生リストがあるときだけ真になる。`effectiveRepeat`） */
  repeatAll?: boolean;
  /** 選んだものの列で、その向きにもう先が無いか */
  atEnd?: (dir: ViewerDir) => boolean;
  /** 選んだものの列で、その向きの最初（戻る先）が動画か */
  wrapIsVideo?: (dir: ViewerDir) => boolean;
}): { dir: ViewerDir; wrap: boolean; slideshow: boolean } | null {
  if (s.playing) return { dir: 1, wrap: true, slideshow: true };
  if (!(s.videosOnly || s.inScope) || s.curtainUp) return null;
  const dir = s.lastDir ?? -1;
  const repeatAll = s.repeatAll === true;
  // 動画の絞り込み中は並ぶのがみな動画。繰り返すなら端で最初へ戻る（送りの側が端でだけ戻る）
  if (s.videosOnly) return { dir, wrap: repeatAll, slideshow: false };
  if (s.nextIsVideo(dir)) return { dir, wrap: false, slideshow: false };
  if (repeatAll && s.atEnd?.(dir) && s.wrapIsVideo?.(dir)) return { dir, wrap: true, slideshow: false };
  return null;
}

/**
 * 端から戻るときの着く位置（列の `dir` の向きの最初。消えた席は飛ばす）。`dir` が 1 なら頭から、-1 なら尻から歩く。
 * 着く先が無ければ -1。スライドショーの一周（`dir` は 1）と、すべてを繰り返す（撮った順なら -1）が同じ歩き方をする
 */
export function wrapScopeIndex(length: number, dir: ViewerDir, gone: (k: number) => boolean): number {
  return nextScopeIndex(length, dir === 1 ? -1 : length, dir, gone);
}

/**
 * 選んだものの列で、`from` から `dir` の向きに歩いて着く位置（着く先が無ければ -1）。**一覧から消えた席は飛ばす**
 * ——⚑を外した・消したものが列には残っている。`gone` はその席が読めた日に居ないか（未取得の日は居る側に倒す）。
 * ビューアの送り（`moveViewer`）と、動画が終わったときの「次は動画か」（`afterVideoEnded` の `nextIsVideo`）が同じ
 * 歩き方をするように、ここに1つだけ置く（別々に書くと、送りは消えた動画を飛ばして写真へ着くのに、確かめる側は
 * 消えた動画を見て「動画」と答えた。PR の codex）
 */
export function nextScopeIndex(length: number, from: number, dir: ViewerDir, gone: (k: number) => boolean): number {
  for (let k = from + dir; k >= 0 && k < length; k += dir) {
    if (!gone(k)) return k;
  }
  return -1;
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
