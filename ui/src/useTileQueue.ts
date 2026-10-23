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
export class TileLoader {
  private readonly queue: TileQueue;
  /** 画面に在るタイル。値は読み終わった絵を受け取る関数 */
  private readonly ready = new Map<string, (img: HTMLImageElement) => void>();
  private readonly elements = new Map<string, Element>();
  private readonly keys = new Map<Element, string>();
  private readonly makeImage: () => HTMLImageElement;
  private root: Element | null = null;
  private observer: IntersectionObserver | null = null;

  /** `makeImage` は試験が差し替える口 */
  constructor(
    limit = TILE_READS,
    makeImage: () => HTMLImageElement = () => new Image(),
  ) {
    this.makeImage = makeImage;
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

  /** タイルを載せる。鍵は絵の URL（タイルごとに違う） */
  attach(
    key: string,
    el: Element,
    ready: (img: HTMLImageElement) => void,
  ): () => void {
    this.keys.set(el, key);
    this.elements.set(key, el);
    this.ready.set(key, ready);
    // 読み途中なら候補には入らず、読み終わったときに `ready` で絵を受け取る
    this.queue.add(key);
    this.observer?.observe(el);
    return () => {
      this.observer?.unobserve(el);
      this.keys.delete(el);
      this.elements.delete(key);
      this.ready.delete(key);
      this.queue.remove(key);
    };
  }

  /** 画面の前後1画面に入った・出た（見張りが呼ぶ。試験も呼ぶ） */
  see(key: string, near: boolean): void {
    this.queue.see(key, near);
  }

  private load(key: string): void {
    const img = this.makeImage();
    img.decoding = "async";
    img.draggable = false;
    img.alt = "";
    const finish = () => {
      img.onload = img.onerror = null;
      this.queue.done(key);
      // 渡す先が居なければ捨てる。居れば、その後は絵はタイルの箱が持つ
      this.ready.get(key)?.(img);
    };
    img.onload = finish;
    img.onerror = finish;
    img.src = key;
  }

  /**
   * 箱の見えている縦の範囲を測り、そこからの距離（px、重なっていれば 0）を答える関数を返す。
   * 上端は貼り付いた道具の帯（`.wizard-toolbar`）の下——帯の裏のタイルは見えていない
   */
  private measure(): (key: string) => number {
    const root = this.root;
    if (!root) return () => Infinity;
    const box = root.getBoundingClientRect();
    const bar = root.querySelector(".wizard-toolbar")?.getBoundingClientRect();
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
): TileLoader {
  const [loader] = useState(() => new TileLoader());
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
