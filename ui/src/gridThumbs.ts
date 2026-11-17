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
 * **`src` はこの ref が持つ**（React には渡さない）。読み手の `Image` が読んで展開まで済ませてから、タイルの
 * `<img>` に同じ URL を入れる（WebView のキャッシュから出て、展開済みの絵がすぐ描ける）。
 * - 描き直しても `src` は消えない。絵の URL が替わった（画質の差し替え・回転）ときは、新しい絵が届くまで
 *   前の絵を出したまま
 * - 一度出た絵も、タイルが作り直されたら（スクロールで戻った）列を通る——戻ったときも展開の数を絞る
 * - 読めなかった（404）・サムネイルが無い（透明な1x1）なら `src` を入れない。もう一度要求しないためで、
 *   見た目は地の灰色（今までの「サムネイルが無い」と同じ）
 */
import type { TileAttach } from "./useTileQueue.ts";

/** 同時に読むサムネイルの数（win2 の実測の頭打ちの点） */
export const GRID_TILE_READS = 4;

/** 届いた絵を、タイルに出してよいか（読めた・1x1 ではない） */
export function paintable(img: { complete: boolean; naturalWidth: number }): boolean {
  return img.complete && img.naturalWidth > 1;
}

export class GridThumbs {
  private readonly loader: TileAttach;
  /** URL ごとの ref。毎回作り直すと、描くたびに付け外しが起きる。外れたら消す */
  private readonly refs = new Map<string, (el: HTMLImageElement | null) => void>();

  constructor(loader: TileAttach) {
    this.loader = loader;
  }

  /** タイルの `<img>` に付ける ref */
  refFor(url: string): (el: HTMLImageElement | null) => void {
    const known = this.refs.get(url);
    if (known) return known;
    let detach: (() => void) | null = null;
    const ref = (el: HTMLImageElement | null) => {
      detach?.();
      detach = null;
      if (!el) {
        this.refs.delete(url);
        return;
      }
      detach = this.loader.attach(url, el, (img) => {
        if (!el.isConnected) return;
        if (paintable(img)) el.src = url;
        // 出せる絵が無い。地の灰色を見せる（`revealThumb` と同じ印）
        else el.setAttribute("data-shown", "");
      });
    };
    this.refs.set(url, ref);
    return ref;
  }

  /** 覚えている ref の数（試験用） */
  size(): number {
    return this.refs.size;
  }
}
