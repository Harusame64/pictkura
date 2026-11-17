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
/** 1枚が枠を塞いでよい上限（ms）。サムネイルの読みは 5〜10ms なので、塞ぐのは遅い媒体の原本だけ */
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

/** 最近出した絵の URL（数に上限。古いものから忘れる）。出せなかったものは入れない（作り直しで問い直す） */
export class RecentThumbs {
  private readonly urls = new Map<string, true>();
  private readonly max: number;

  constructor(max = RECENT_MAX) {
    this.max = max;
  }

  /** 覚えていれば新しい側へ回して true */
  touch(url: string): boolean {
    if (!this.urls.has(url)) return false;
    this.add(url);
    return true;
  }

  add(url: string): void {
    this.urls.delete(url);
    this.urls.set(url, true);
    while (this.urls.size > this.max) {
      const oldest = this.urls.keys().next().value;
      if (oldest === undefined) break;
      this.urls.delete(oldest);
    }
  }
}
