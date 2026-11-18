/**
 * 一覧のサムネイルを、取り込み画面と同じ列（[`TileLoader`]）で読ませる。
 *
 * **なぜ列か。** 3万枚の一覧で帯を速く動かすと、`<img loading="lazy">` は通り過ぎるタイルの絵を全部要求した
 * （1回の操作で約2,000枚）。詰まっていたのは WebView の絵の展開で、ほぼ1コア——同時に4枚までは
 * 1秒に読める枚数が伸び、それより増やすと伸びずに待ち時間だけが伸びた（win2 の実測、512px で約400枚/秒）。
 * 古い要求から順に展開するので、届いたころにはそのタイルは画面の外に居た。
 *
 * だから同時に読むのは [`GRID_TILE_READS`] 枚まで、見えているタイルから、画面から外れたタイルは列から捨てる
 * （混んだら新しいものから・待ちすぎたものは捨てる、の考え方。Facebook "Fail at Scale" の adaptive LIFO と
 * CoDel）。速さのしきい値は持たない——マシンが遅ければ、見えている分を優先して読むだけになる。
 *
 * **`src` はタイルの部品（`GridThumb.tsx`）が持つ**（React の属性には渡さない）。読み手の `Image` が読み終えたら、タイルの `<img>` に同じ URL を
 * 入れる（WebView のキャッシュから出る）。展開（`decode()`）まで待って枠を空ける形も試したが、要求は4割減った
 * 代わりに、動かしている間に写真の出る割合が約4割から約1割に落ち、描画も重くなった（win2 の実測。展開が
 * 読み手とタイルで2回になったと見ている）ので待たない。
 * - 描き直しても `src` は消えない。絵の URL が替わった（画質の差し替え・回転）ときは、新しい絵が届くまで
 *   前の絵を出したまま
 * - 遠くへスクロールして戻ったタイルは列を通る。行の組み直しで作り直されただけなら、最近出した絵はすぐ出す
 * - 読めなかった（404）・サムネイルが無い（透明な1x1）なら `src` を外し、地の灰色を見せる（今までの
 *   「サムネイルが無い」と同じ見た目）。結果は覚えないので、タイルを作り直したら問い直す
 */

/** 同時に読むサムネイルの数（win2 の実測の頭打ちの点） */
export const GRID_TILE_READS = 4;
/**
 * 1枚が枠を塞いでよい上限（ms）。サムネイルの読みは 5〜10ms。サムネイルの無い写真は空のタイルがすぐ返るので、
 * 塞ぐのは遅い媒体の上のサムネイルと、働き手が作らない写真（SVG・失敗が上限に達した写真）の原本だけ
 */
export const GRID_SLOT_TIMEOUT_MS = 1500;

/** 届いた絵を、タイルに出してよいか（読めた・「サムネイルが無い」の透明な 1x1 ではない） */
export function paintable(img: { complete: boolean; naturalWidth: number; naturalHeight: number }): boolean {
  return img.complete && img.naturalWidth > 0 && !(img.naturalWidth === 1 && img.naturalHeight === 1);
}

/**
 * 最近出した絵の数。行の組み直し（日を読み込んだ・窓の幅・大きさ・並べ方）ではタイルが作り直されるが、
 * 画面に出ていた絵はまだ展開済みなので、列を通さずすぐ `src` を渡す（出ていた絵を消して浮かび直させない）。
 * 遠くへスクロールして戻ったときは数を超えて忘れているので、列を通る
 */
// 4K の窓でいちばん小さい大きさにすると、画面と前後の控えの行で数百枚が同時に作り直される（/code-review）
export const RECENT_MAX = 2000;

/**
 * 最近出した絵（数に上限。古いものから忘れる）。鍵はタイルの URL、値は実際に出した URL——問い直して出た絵は
 * 回数の付いた URL（`&ask=n`）で出ているので、作り直したタイルはその URL をそのまま使う（素の URL は空の絵を
 * 読んだ URL で、macOS の WebView はそれを返しうる。PR の codex）。出せなかったものは入れない（作り直しで問い直す）
 */
export class RecentThumbs {
  private readonly urls = new Map<string, string>();
  private readonly max: number;

  constructor(max = RECENT_MAX) {
    this.max = max;
  }

  /** 覚えていれば新しい側へ回して、出した URL を返す */
  touch(url: string): string | undefined {
    const shown = this.urls.get(url);
    if (shown === undefined) return undefined;
    this.add(url, shown);
    return shown;
  }

  add(url: string, shown: string = url): void {
    this.urls.delete(url);
    this.urls.set(url, shown);
    while (this.urls.size > this.max) {
      const oldest = this.urls.keys().next().value;
      if (oldest === undefined) break;
      this.urls.delete(oldest);
    }
  }
}

/**
 * 空のタイル（サムネイルがまだ無い写真）の待ち合わせ。働き手がその写真を片付けたら（`media-updated`）、写真ごとの
 * 知らせの数を1つ進め、待っているタイルを起こして問い直させる。
 *
 * URL が変わるのはサムネイルが書けたときだけなので、それだけを頼ると、ほかの終わり方（素性だけ読んで終わった・
 * 書き換え中で積み直した・失敗した）で灰色のまま残る。問い直せば、その時の状態で答えが決まる——出来ていれば絵、
 * まだなら列の先頭へ押し直して空、作れないと分かれば原本か空（`src-tauri` の `handle_media_request`）。
 * 働き手が知らせない写真（作らない・クラウドだけ・失敗の上限）は起こされないので、問い直しは続かない。
 *
 * 問い直しの URL には知らせの数を足す（`&ask=n`）。同じ URL だと、macOS の WebView が同じ文書で読んだ空の絵を
 * そのまま返しうる（no-store でも）。数はタイルでなくここに持つ——タイルが外れている間に来た知らせも、作り直した
 * タイルが新しい URL で問い直せる（/code-review）
 */
export class BlankWaiters {
  private readonly waiting = new Map<number, Set<() => void>>();
  private readonly notices = new Map<number, number>();
  /** 絵が出る前に画面から去った写真（まとめて `onLeft` へ渡す） */
  private readonly left = new Set<number>();
  private leftTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * `onLeft` は、絵が出る前に画面から去ったタイルの写真を受ける（サムネイルを作る優先の列から降ろす。
   * `api.ts` の `demoteThumbnails`）。ホイールで一気に送ると、通り過ぎたタイルの写真の本番が、送り終えた画面の
   * 写真の前に並んでいた（win2: 積まれた 145〜180 枚のうち、画面に残ったのは 65 枚）
   */
  private readonly onLeft?: (ids: number[]) => void;
  private readonly leftDelayMs: number;
  constructor(onLeft?: (ids: number[]) => void, leftDelayMs = 50) {
    this.onLeft = onLeft;
    this.leftDelayMs = leftDelayMs;
  }

  /**
   * 絵が出る前にタイルが外れた。まとめて `onLeft` へ渡す——渡すときに**また待たれている写真は外す**（同じ写真の
   * タイルが作り直された・もう1枚ある。その頼みの後に降ろすと、見えている写真を降ろしてしまう）
   */
  leave(id: number): void {
    if (!this.onLeft) return;
    this.left.add(id);
    if (this.leftTimer !== null) return;
    this.leftTimer = setTimeout(() => {
      this.leftTimer = null;
      const ids = [...this.left].filter((i) => !this.waiting.has(i));
      this.left.clear();
      if (ids.length > 0) this.onLeft?.(ids);
    }, this.leftDelayMs);
  }

  /** 写真 `id` の知らせの数（来ていなければ 0） */
  count(id: number): number {
    return this.notices.get(id) ?? 0;
  }

  /** 写真 `id` を待っているタイルが居るか */
  isWaited(id: number): boolean {
    return this.waiting.has(id);
  }

  /** 写真 `id` の知らせを待つ。返り値で待つのをやめる */
  wait(id: number, wake: () => void): () => void {
    let set = this.waiting.get(id);
    if (!set) {
      set = new Set();
      this.waiting.set(id, set);
    }
    set.add(wake);
    return () => {
      const s = this.waiting.get(id);
      if (!s) return;
      s.delete(wake);
      if (s.size === 0) this.waiting.delete(id);
    };
  }

  /** 働き手が片付けた写真。数を進め、待っていたタイルを1回ずつ起こす（起こしたら外す。問い直して空ならまた待つ） */
  notify(ids: Iterable<number>): void {
    for (const id of ids) {
      this.notices.set(id, this.count(id) + 1);
      const set = this.waiting.get(id);
      if (!set) continue;
      this.waiting.delete(id);
      for (const wake of set) wake();
    }
  }
}
