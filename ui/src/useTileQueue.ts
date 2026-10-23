import { useEffect, useRef, useState, type RefObject } from "react";
import { TileQueue, TILE_READS, type TileView } from "./tileQueue";

/**
 * 取り込み画面のタイルを [`TileQueue`] の順で読ませる。
 *
 * 見え方は2つの `IntersectionObserver` で取る。1つは画面そのもの（visible）、
 * もう1つは上下に1画面ずつ広げた範囲（near）。
 */
export class TileLoader {
  private readonly queue: TileQueue;
  private readonly starters = new Map<string, () => void>();
  private readonly keys = new Map<Element, string>();
  private readonly seen = new Map<
    string,
    { visible: boolean; near: boolean }
  >();
  private visible: IntersectionObserver | null = null;
  private near: IntersectionObserver | null = null;

  constructor(limit = TILE_READS) {
    this.queue = new TileQueue(limit, (key) => this.starters.get(key)?.());
  }

  /**
   * スクロールする箱が決まったら見張りを立てる。子の effect は親より先に走るので、
   * それまでに登録されたタイルもここで見張りに載せる
   */
  watch(root: Element): () => void {
    const view = (
      flag: "visible" | "near",
      entries: IntersectionObserverEntry[],
    ) => {
      for (const e of entries) {
        const key = this.keys.get(e.target);
        if (key === undefined) continue;
        const s = this.seen.get(key) ?? { visible: false, near: false };
        s[flag] = e.isIntersecting;
        this.seen.set(key, s);
        const v: TileView = s.visible ? "visible" : s.near ? "near" : "away";
        this.queue.see(key, v);
      }
    };
    this.visible = new IntersectionObserver((es) => view("visible", es), {
      root,
    });
    this.near = new IntersectionObserver((es) => view("near", es), {
      root,
      rootMargin: "100% 0px",
    });
    for (const el of this.keys.keys()) {
      this.visible.observe(el);
      this.near.observe(el);
    }
    return () => {
      this.visible?.disconnect();
      this.near?.disconnect();
      this.visible = null;
      this.near = null;
    };
  }

  attach(
    key: string,
    index: number,
    el: Element,
    start: () => void,
  ): () => void {
    this.keys.set(el, key);
    this.starters.set(key, start);
    this.queue.add(key, index);
    this.visible?.observe(el);
    this.near?.observe(el);
    return () => {
      this.visible?.unobserve(el);
      this.near?.unobserve(el);
      this.keys.delete(el);
      this.starters.delete(key);
      this.seen.delete(key);
      this.queue.remove(key);
    };
  }

  /** 一覧の並びが変わった（絞り込みなど）。読み途中の枠には触らない */
  reindex(key: string, index: number): void {
    this.queue.reindex(key, index);
  }

  done(key: string): void {
    this.queue.done(key);
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
  index: number,
): [RefObject<HTMLSpanElement>, boolean] {
  const ref = useRef<HTMLSpanElement>(null);
  const [go, setGo] = useState(false);
  const first = useRef(index);
  // 登録はタイルが在る間ずっと。`go` で外すと、読み途中の枠が読み終わる前に空く
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    return loader.attach(key, first.current, el, () => setGo(true));
  }, [loader, key]);
  useEffect(() => loader.reindex(key, index), [loader, key, index]);
  return [ref, go];
}
