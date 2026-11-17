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
 * **`src` はこの ref が持つ**（React には渡さない）。読み手の `Image` が読み終えたら、タイルの `<img>` に同じ URL を
 * 入れる（WebView のキャッシュから出る）。展開（`decode()`）まで待って枠を空ける形も試したが、要求は4割減った
 * 代わりに、動かしている間に写真の出る割合が約4割から約1割に落ち、描画も重くなった（win2 の実測。展開が
 * 読み手とタイルで2回になったと見ている）ので待たない。
 * - 描き直しても `src` は消えない。絵の URL が替わった（画質の差し替え・回転）ときは、新しい絵が届くまで
 *   前の絵を出したまま
 * - 遠くへスクロールして戻ったタイルは列を通る。行の組み直しで作り直されただけなら、最近出した絵はすぐ出す
 * - 読めなかった（404）・サムネイルが無い（透明な1x1）なら `src` を外し、地の灰色を見せる（今までの
 *   「サムネイルが無い」と同じ見た目）。結果は覚えないので、タイルを作り直したら問い直す
 */
import type { TileAttach } from "./useTileQueue.ts";

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
const RECENT_MAX = 400;

export class GridThumbs {
  private readonly loader: TileAttach;
  /** URL ごとの ref。毎回作り直すと、描くたびに付け外しが起きる。付いている要素が無くなったら消す */
  private readonly refs = new Map<string, (el: HTMLImageElement | null) => void>();
  /** ref ごとの、付いている要素と、その列からの外し方（読み終わったら null） */
  private readonly mounted = new Map<string, Map<HTMLImageElement, (() => void) | null>>();
  /** 最近出した絵の URL（挿入順＝古い順）。出せなかったものは覚えない（作り直しで問い直す） */
  private readonly recent = new Map<string, true>();
  private readonly recentMax: number;
  private sweepScheduled = false;

  constructor(loader: TileAttach, recentMax = RECENT_MAX) {
    this.loader = loader;
    this.recentMax = recentMax;
  }

  private remember(url: string): void {
    this.recent.delete(url);
    this.recent.set(url, true);
    while (this.recent.size > this.recentMax) {
      const oldest = this.recent.keys().next().value;
      if (oldest === undefined) break;
      this.recent.delete(oldest);
    }
  }

  /**
   * 付いている要素の無くなった ref を、まとめて1回で控えから消す。作り直しでは、古い要素の null の直後に
   * 新しい要素で同じ ref が呼ばれるので、その場では消さない（消すと次の描画で別の ref が作られ、描くたびに
   * 付け外しが起きた。/code-review）。1枚ごとに時計を立てると、帯を動かす間に数千本立った（win2）
   */
  private scheduleSweep(): void {
    if (this.sweepScheduled) return;
    this.sweepScheduled = true;
    setTimeout(() => {
      this.sweepScheduled = false;
      for (const [url, els] of this.mounted) {
        if (els.size === 0) {
          this.mounted.delete(url);
          this.refs.delete(url);
        }
      }
    }, 0);
  }

  /** タイルの `<img>` に付ける ref */
  refFor(url: string): (el: HTMLImageElement | null) => void {
    const known = this.refs.get(url);
    if (known) return known;
    const ref = (el: HTMLImageElement | null) => {
      let els = this.mounted.get(url);
      if (!el) {
        // React は外した要素を教えないので、文書から外れた要素を外す（同じ URL のタイルが2枚あっても、
        // 残っている方の読みを止めない）
        if (els) {
          for (const [e, detach] of els) {
            if (!e.isConnected) {
              detach?.();
              els.delete(e);
            }
          }
        }
        this.scheduleSweep();
        return;
      }
      // 控えに載せるのは付いたときだけ（捨てられた描画で作られて付かなかった ref を溜めない）
      if (!this.refs.has(url)) this.refs.set(url, ref);
      if (!els) this.mounted.set(url, (els = new Map()));
      if (els.has(el)) return;
      // 最近出した絵は列を通さず、すぐ見せる（行の組み直しで、出ていた絵を浮かび直させない。win2）
      if (this.recent.has(url)) {
        this.remember(url);
        if (el.getAttribute("src") !== url) el.src = url;
        el.setAttribute("data-shown", "");
        els.set(el, null);
        return;
      }
      const detach = this.loader.attach(url, el, (img) => {
        // 読み終わったら列から外す（見張りの知らせを受け続けない）
        const mine = this.mounted.get(url);
        if (mine?.get(el)) {
          mine.get(el)?.();
          mine.set(el, null);
        }
        if (!el.isConnected) return;
        if (paintable(img)) {
          this.remember(url);
          el.src = url;
        } else {
          // 出せる絵が無い（404・「サムネイルが無い」の 1x1）。前の URL の絵が残っていれば消し、地の灰色を
          // 見せる（古い向き・差し替え前の絵を出したままにしない）。覚えないので、作り直しで問い直す
          el.removeAttribute("src");
          el.setAttribute("data-shown", "");
        }
      });
      els.set(el, detach);
    };
    return ref;
  }

  /** 覚えている ref の数（試験用） */
  size(): number {
    return this.refs.size;
  }
}
