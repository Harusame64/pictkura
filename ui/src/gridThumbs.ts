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
 * 絵は読み手の `Image` が読み、読み終わったらタイルの `<img>` に同じ URL を入れる（WebView のキャッシュから出る）。
 * 読み終えた URL は覚えておき、次に描くときは列を通さず `src` を渡す
 */
import { TileLoader } from "./useTileQueue.ts";

/** 同時に読むサムネイルの数（win2 の実測の頭打ちの点） */
export const GRID_TILE_READS = 4;

/** 覚えておく「読み終えた URL」の数。超えたら古いものから忘れる（忘れたものは列を通ってキャッシュから出る） */
const LOADED_MAX = 4000;

/** 列の口（試験が差し替える） */
export interface TileAttach {
  attach(key: string, el: Element, ready: (img: HTMLImageElement) => void): () => void;
}

export class GridThumbs {
  private readonly loader: TileAttach;
  private readonly max: number;
  /** 読み終えた URL（挿入順＝古い順。触れたら後ろへ回す） */
  private readonly loaded = new Map<string, true>();
  /** URL ごとの ref。毎回作り直すと、描くたびに付け外しが起きる */
  private readonly refs = new Map<string, (el: HTMLImageElement | null) => void>();

  constructor(loader: TileAttach, max = LOADED_MAX) {
    this.loader = loader;
    this.max = max;
  }

  isLoaded(url: string): boolean {
    if (!this.loaded.has(url)) return false;
    this.loaded.delete(url);
    this.loaded.set(url, true);
    return true;
  }

  private markLoaded(url: string): void {
    this.loaded.delete(url);
    this.loaded.set(url, true);
    while (this.loaded.size > this.max) {
      const oldest = this.loaded.keys().next().value;
      if (oldest === undefined) break;
      this.loaded.delete(oldest);
    }
  }

  /** まだ読んでいないタイルの `<img>` に付ける ref。読み終わったら `src` を入れる */
  refFor(url: string): (el: HTMLImageElement | null) => void {
    const known = this.refs.get(url);
    if (known) return known;
    let detach: (() => void) | null = null;
    const ref = (el: HTMLImageElement | null) => {
      detach?.();
      detach = null;
      if (!el) return;
      detach = this.loader.attach(url, el, () => {
        this.markLoaded(url);
        if (el.isConnected) el.src = url;
      });
    };
    this.refs.set(url, ref);
    // 描かれなくなったタイルの ref を溜めない。作り直された ref は、古い ref に null が来てから付く
    if (this.refs.size > this.max) this.refs.clear();
    return ref;
  }
}

/** 一覧の箱を見張る読み手 */
export function makeGridLoader(): TileLoader {
  return new TileLoader(GRID_TILE_READS);
}
