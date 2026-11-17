import { useEffect, useRef, useState, type RefObject } from "react";
import { TileQueue, TILE_READS } from "./tileQueue.ts";

/**
 * 取り込み画面のタイルを [`TileQueue`] の順で読ませる。
 *
 * **絵はここが自前の `Image` で読む。** 読み終わったら、その要素をタイルに渡す。
 * タイルが読み途中に消えても `Image` は残って読み終わりを知らせるので、
 * 同時に読む数を取りこぼさない。読み途中に戻ってきたタイルには、その読みの絵を渡す。
 * 読み終わった後で戻ってきたタイルは、もう一度順番に並ぶ（WebView のキャッシュから
 * 出ることが多い）。
 *
 * 見張り（`IntersectionObserver`）は、画面の上下に1画面ずつ広げた範囲の1つだけ。
 * 見えているかどうかは見張りに訊かず、選ぶときに位置を測る——見張りを2つにすると、
 * 知らせの届く順で、見えているタイルより先読みのタイルが先に選ばれうる
 */
/** タイルを列に載せる口（一覧の `GridThumb.tsx` が使う。試験が差し替える） */
export interface TileAttach {
  attach(key: string, el: Element, ready: (img: HTMLImageElement) => void, src?: string): () => void;
}

export interface TileLoaderOptions {
  /**
   * 1枚の読みが枠を塞いでよい上限（ms）。過ぎたら枠を空け、読み終わったら絵は渡す。取り込み画面は持たない
   * （遅い媒体で読みが積み上がる）。一覧は持つ——サムネイルが無い写真は原本を読むので、NAS や USB の大きな
   * ファイル4枚で一覧の全部が止まりうる（/code-review）
   */
  slotTimeoutMs?: number;
  /** 箱の上端を覆う帯の選択子（その裏のタイルは見えていない扱い）。既定は取り込み画面の帯、null で無し */
  coveredBy?: string | null;
}

export class TileLoader implements TileAttach {
  private readonly queue: TileQueue;
  /** 画面に在るタイル。値は読み終わった絵を受け取る関数 */
  private readonly ready = new Map<string, (img: HTMLImageElement) => void>();
  private readonly elements = new Map<string, Element>();
  private readonly keys = new Map<Element, string>();
  private readonly makeImage: () => HTMLImageElement;
  private root: Element | null = null;
  private observer: IntersectionObserver | null = null;

  private readonly opts: TileLoaderOptions;
  /** 読み始めて、まだ終わっていない絵（時間切れで枠を空けたものも含む）と、その終わりで受け取る鍵 */
  private readonly reading = new Map<string, Set<string>>();
  /** 鍵ごとの絵の URL（`attach` の `src`） */
  private readonly srcs = new Map<string, string>();
  private readonly limit: number;
  /** 時間切れだが、早めに空けた読みが枠の数に達していたので枠を空けていない鍵 */
  private readonly overdue = new Set<string>();
  /** 時間切れで早めに枠を空け、まだ読み途中の鍵（枠の数まで） */
  private readonly early = new Set<string>();

  /** `makeImage` は試験が差し替える口 */
  constructor(
    limit = TILE_READS,
    makeImage: () => HTMLImageElement = () => new Image(),
    opts: TileLoaderOptions = {},
  ) {
    this.makeImage = makeImage;
    this.opts = opts;
    this.limit = limit;
    this.queue = new TileQueue(limit, {
      start: (key) => this.load(key),
      measure: () => this.measure(),
      defer: (run) => setTimeout(run, 0),
    });
  }

  /**
   * スクロールする箱が決まったら見張りを立てる。子の effect は親より先に走るので、
   * それまでに登録されたタイルもここで見張りに載せる
   */
  watch(root: Element): () => void {
    this.root = root;
    this.observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const key = this.keys.get(e.target);
          if (key !== undefined) this.see(key, e.isIntersecting);
        }
      },
      { root, rootMargin: "100% 0px" },
    );
    for (const el of this.keys.keys()) this.observer.observe(el);
    return () => {
      this.observer?.disconnect();
      this.observer = null;
      this.root = null;
    };
  }

  /**
   * タイルを載せる。鍵はタイルごとに違うこと。`src` を渡さなければ鍵が絵の URL（取り込み画面）。
   * 一覧は同じ写真のタイルが同時に2枚ありうるので、タイルごとの鍵と絵の URL を分けて渡す
   */
  attach(
    key: string,
    el: Element,
    ready: (img: HTMLImageElement) => void,
    src: string = key,
  ): () => void {
    this.keys.set(el, key);
    this.elements.set(key, el);
    this.ready.set(key, ready);
    this.srcs.set(key, src);
    // 同じ絵を読み途中（時間切れで枠を空けた読みも含む）なら並べず、その読みの終わりで受け取る
    // （並べると同じ大きなファイルをもう一度読む。/code-review）
    const waiting = this.reading.get(src);
    if (waiting) waiting.add(key);
    else this.queue.add(key);
    this.observer?.observe(el);
    return () => {
      this.observer?.unobserve(el);
      this.keys.delete(el);
      this.elements.delete(key);
      this.ready.delete(key);
      this.srcs.delete(key);
      this.reading.get(src)?.delete(key);
      this.queue.remove(key);
    };
  }

  /** 画面の前後1画面に入った・出た（見張りが呼ぶ。試験も呼ぶ） */
  see(key: string, near: boolean): void {
    this.queue.see(key, near);
  }

  private load(key: string): void {
    const src = this.srcs.get(key) ?? key;
    // 同じ絵がもう読み途中なら、その読みの終わりで受け取る（同じ選びで2枚のタイルが並んで始まったとき）
    const already = this.reading.get(src);
    if (already) {
      already.add(key);
      this.queue.done(key);
      return;
    }
    const waiters = new Set<string>([key]);
    this.reading.set(src, waiters);
    const img = this.makeImage();
    img.decoding = "async";
    img.draggable = false;
    img.alt = "";
    // 時間切れ: 枠を空けて次へ進む。ただし読み途中（時間切れのものを含む）が上限の2倍に達していたら空けない
    // ——遅い媒体で読みが積み上がらないように（取り込み画面の実測で、並ぶほど1本あたりが遅くなった。dev #73）
    const timer =
      this.opts.slotTimeoutMs === undefined
        ? undefined
        : setTimeout(() => {
            // 早めに枠を空けた読みは、枠と同じ数まで。超えるなら、ほかの読みが終わったときに空ける
            // （`releaseOverdue`）。読み途中の総数は、列の枠＋早めに空けた読み＝枠の2倍を超えない
            if (this.early.size < this.limit) {
              this.early.add(key);
              this.queue.done(key);
            } else this.overdue.add(key);
          }, this.opts.slotTimeoutMs);
    const finish = () => {
      img.onload = img.onerror = null;
      this.reading.delete(src);
      this.overdue.delete(key);
      this.early.delete(key);
      if (timer !== undefined) clearTimeout(timer);
      // 時間切れで先に枠を空けていれば、二度目は何もしない（`TileQueue.done` は読み途中の鍵だけを数える）
      this.queue.done(key);
      // 渡す先が居なければ捨てる。居れば、その後は絵はタイルの箱が持つ。読みの途中に並んだ同じ絵のタイルにも渡す
      for (const k of waiters) this.ready.get(k)?.(img);
      this.releaseOverdue();
    };
    img.onload = finish;
    img.onerror = finish;
    img.src = src;
  }

  /**
   * 時間切れなのに空けられなかった枠を、早めに空けた読みが枠の数を割った分だけ空ける（1つずつ数えて空ける——
   * 読み途中の総数で見ると、代わりの読みが始まる前にまとめて空けてしまい、8本が11本になった。PR の codex）
   */
  private releaseOverdue(): void {
    for (const k of this.overdue) {
      if (this.early.size >= this.limit) return;
      this.overdue.delete(k);
      this.early.add(k);
      this.queue.done(k);
    }
  }

  /**
   * 箱の見えている縦の範囲を測り、そこからの距離（px、重なっていれば 0）を答える関数を返す。
   * 上端は貼り付いた道具の帯（`.wizard-toolbar`）の下——帯の裏のタイルは見えていない
   */
  private measure(): (key: string) => number {
    const root = this.root;
    if (!root) return () => Infinity;
    const box = root.getBoundingClientRect();
    const cover = this.opts.coveredBy === undefined ? ".wizard-toolbar" : this.opts.coveredBy;
    const bar = cover === null ? undefined : root.querySelector(cover)?.getBoundingClientRect();
    const top = Math.max(box.top, bar?.bottom ?? box.top);
    return (key) => {
      const el = this.elements.get(key);
      if (!el) return Infinity;
      const r = el.getBoundingClientRect();
      if (r.bottom <= top) return top - r.bottom + 1;
      if (r.top >= box.bottom) return r.top - box.bottom + 1;
      return 0;
    };
  }
}

/**
 * スクロールする箱（`root`）ごとに1つの [`TileLoader`] を持つ。
 * `shown` は箱が描かれているか——閉じている間は箱が無いので、開いたときに見張りを立てる
 */
export function useTileLoader(
  root: RefObject<Element | null>,
  shown: boolean,
  limit = TILE_READS,
  opts: TileLoaderOptions = {},
): TileLoader {
  const [loader] = useState(() => new TileLoader(limit, undefined, opts));
  useEffect(() => {
    const el = root.current;
    if (!shown || !el) return;
    return loader.watch(el);
  }, [loader, root, shown]);
  return loader;
}

/**
 * 1枚のタイルを載せ、読み終わった絵を `ref` の箱へ差し込む。
 * 一度出した絵は、画面から外れても消さない
 */
export function useTileImage(
  loader: TileLoader,
  key: string,
): RefObject<HTMLSpanElement> {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    return loader.attach(key, el, (img) => {
      if (img.parentNode !== el) el.replaceChildren(img);
    });
  }, [loader, key]);
  return ref;
}
