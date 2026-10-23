/**
 * 取り込み画面のタイルを、**何枚まで同時にカードから読むか**と、**どれから読むか**を決める
 * （pictkura-dev #73）。
 *
 * `<img loading="lazy">` に任せると、WebView は画面の前後をまとめて要求し、
 * アプリはそれを全部同時に読む。win の実測（dev `measure73-jump-win.md`）では、
 * 下端へ飛んだ瞬間に 55 本、中ほどで 100 本が同時に走り、USB の読みは 1 本ずつの
 * 約 34 MB/s から 16〜19 MB/s に落ちた。見えている 20 枚が揃うまで 23 秒かかった。
 * スキャン（`probe_imported`）もその後ろで待たされた（SD で 7.6 秒が 17.8 秒に）。
 *
 * だから要求は画面の側で絞る。**見えているタイルを先に、その次に前後の先読み**。
 * 見えなくなったタイルは、まだ読み始めていなければ後回しにする。
 * 読み始めた要求は取り消せない（アプリの側では、届いた要求を最後まで読む）。
 */

/** 同時に読むタイルの数。1 本だとデコードの間カードが遊ぶので 2 本 */
export const TILE_READS = 2;

/** タイルが画面のどこに居るか */
export type TileView = "visible" | "near" | "away";

export class TileQueue {
  private readonly index = new Map<string, number>();
  private readonly view = new Map<string, TileView>();
  private readonly waiting = new Set<string>();
  private readonly running = new Set<string>();

  private readonly limit: number;
  private readonly start: (key: string) => void;
  private readonly defer: (run: () => void) => void;
  private scheduled = false;

  /**
   * `defer` は選ぶのを後へ回す口。見え方は2つの見張りから別々の呼び出しで届き、
   * その間にマイクロタスクが走る。すぐ選ぶと、先に届いた「近い」だけを見て
   * 先読みのタイルから読み始めてしまう（見えているタイルも「近い」に入るため）
   */
  constructor(
    limit: number,
    start: (key: string) => void,
    defer: (run: () => void) => void = (run) => setTimeout(run, 0),
  ) {
    this.limit = limit;
    this.start = start;
    this.defer = defer;
  }

  /** タイルを待ち行列に入れる（`index` は一覧の並び。同じ見え方のタイル同士はこの順） */
  add(key: string, index: number): void {
    if (this.index.has(key)) return;
    this.index.set(key, index);
    this.view.set(key, "away");
    this.waiting.add(key);
    this.pump();
  }

  /** 並びだけを変える。読み途中・読み終わりのタイルを待ち行列へ戻さない */
  reindex(key: string, index: number): void {
    if (!this.index.has(key)) return;
    this.index.set(key, index);
  }

  /**
   * タイルが画面から消えた。読み途中なら、その枠も空ける——
   * `<img>` が消えると読み終わりの知らせが来ず、枠が永久に埋まったままになるため
   */
  remove(key: string): void {
    this.index.delete(key);
    this.view.delete(key);
    this.waiting.delete(key);
    this.running.delete(key);
    this.pump();
  }

  /** 見え方が変わった */
  see(key: string, view: TileView): void {
    if (!this.index.has(key)) return;
    this.view.set(key, view);
    this.pump();
  }

  /** 読み終わった（失敗も含む） */
  done(key: string): void {
    if (this.running.delete(key)) this.pump();
  }

  /** いま読んでいるタイル（試験用） */
  inFlight(): string[] {
    return [...this.running];
  }

  private pick(): string | undefined {
    let best: string | undefined;
    let bestRank = Infinity;
    for (const key of this.waiting) {
      const v = this.view.get(key);
      // 画面から遠いタイルは読まない。近づいたときに `see` が呼ばれる
      if (v === "away" || v === undefined) continue;
      const rank = (v === "visible" ? 0 : 1e9) + (this.index.get(key) ?? 0);
      if (rank < bestRank) {
        bestRank = rank;
        best = key;
      }
    }
    return best;
  }

  private pump(): void {
    if (this.scheduled) return;
    this.scheduled = true;
    this.defer(() => {
      this.scheduled = false;
      this.fill();
    });
  }

  private fill(): void {
    while (this.running.size < this.limit) {
      const key = this.pick();
      if (key === undefined) return;
      this.waiting.delete(key);
      this.running.add(key);
      this.start(key);
    }
  }
}
