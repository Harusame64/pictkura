import { useEffect, useRef, useState, type RefObject } from "react";
import { TileQueue, TILE_READS } from "./tileQueue";

/**
 * 取り込み画面のタイルを [`TileQueue`] の順で読ませる。
 *
 * 見張り（`IntersectionObserver`）は、画面の上下に1画面ずつ広げた範囲の1つだけ。
 * 見えているかどうかは見張りに訊かず、選ぶときに位置を測る——見張りを2つにすると、
 * 知らせの届く順で、見えているタイルより先読みのタイルが先に選ばれうる
 */
export class TileLoader {
  private readonly queue: TileQueue;
  private readonly starters = new Map<string, () => void>();
  private readonly elements = new Map<string, Element>();
  private readonly keys = new Map<Element, string>();
  private root: Element | null = null;
  private observer: IntersectionObserver | null = null;

  constructor(limit = TILE_READS) {
    this.queue = new TileQueue(limit, {
      start: (key) => this.starters.get(key)?.(),
      measure: () => this.measure(),
      defer: (run) => setTimeout(run, 0),
      after: (ms, run) => setTimeout(run, ms),
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
          if (key !== undefined) this.queue.see(key, e.isIntersecting);
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

  attach(key: string, el: Element, start: () => void): () => void {
    this.keys.set(el, key);
    this.elements.set(key, el);
    this.starters.set(key, start);
    this.queue.add(key);
    this.observer?.observe(el);
    return () => {
      this.observer?.unobserve(el);
      this.keys.delete(el);
      this.elements.delete(key);
      this.starters.delete(key);
      this.queue.remove(key);
    };
  }

  done(key: string): void {
    this.queue.done(key);
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
 * 1枚のタイルが読み始めてよいか。読み始めたら（読み終わっても）`true` のまま——
 * 一度出した絵は、画面から外れても消さない
 */
export function useTileTurn(
  loader: TileLoader,
  key: string,
): [RefObject<HTMLSpanElement>, boolean] {
  const ref = useRef<HTMLSpanElement>(null);
  const [go, setGo] = useState(false);
  // 登録はタイルが在る間ずっと。`go` で外すと、同じタイルがもう一度候補に入る
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    return loader.attach(key, el, () => setGo(true));
  }, [loader, key]);
  return [ref, go];
}
