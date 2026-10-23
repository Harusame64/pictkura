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
 * だから要求は画面の側で絞る。読む候補は画面の前後1画面に居るタイルだけで、
 * **選ぶたびに画面からの距離を測り、近いものから**読む。見えているタイルは距離 0。
 *
 * **同時に読む数はアプリの側でも守る**（`src-tauri/src/lib.rs` の `SOURCE_PREVIEW_LANES`）。
 * 画面の側では読みが本当に終わった時を知れない——読み途中でタイルが消えると
 * 読み終わりの知らせが来ず、アプリはその読みを最後まで続ける。だからここの数は
 * 「どれから頼むか」のためのもので、タイルが消えたら枠はすぐ空けて次を頼む。
 * 頼み過ぎた分はアプリの側で待つ。
 */

/** 同時に読むタイルの数。1 本だとデコードの間カードが遊ぶので 2 本 */
export const TILE_READS = 2;

/**
 * 1本の読みに枠を貸しておく長さ。過ぎたら、読み終わっていなくても次へ回す——
 * 答えの来ない要求が2本あるだけで画面全体が止まらないように。アプリの側は空きを
 * 最大 5 秒待ち、USB で重い1枚（FFF 102MB）は1本で約 3 秒かかるので、その和より長く
 */
export const TILE_STALL_MS = 10000;

export type TileQueueEnv = {
  /** タイルの絵を要求させる */
  start: (key: string) => void;
  /**
   * 選ぶときに1回呼ぶ。返す関数がタイルの画面からの距離（見えていれば 0、
   * 測れなければ `Infinity`）を答える。箱の位置を1回の選びで1回だけ測るため
   */
  measure: () => (key: string) => number;
  /** 選ぶのを後へ回す（見え方の知らせをまとめて受けてから選ぶため） */
  defer: (run: () => void) => void;
  /** `ms` 後に `run` する */
  after: (ms: number, run: () => void) => void;
};

export class TileQueue {
  private readonly limit: number;
  private readonly env: TileQueueEnv;
  /** まだ読んでいない、画面の前後1画面に居るタイル */
  private readonly near = new Set<string>();
  /** まだ読んでいない、遠いタイル */
  private readonly away = new Set<string>();
  /** 読み途中。値は始めた順の番号（古い時間切れが、同じタイルの新しい読みを空けないため） */
  private readonly running = new Map<string, number>();
  private serial = 0;
  private scheduled = false;

  constructor(limit: number, env: TileQueueEnv) {
    this.limit = limit;
    this.env = env;
  }

  /**
   * タイルが画面に現れた。初めは遠い扱いで、近づいたら `see` が呼ばれる。
   * 鍵はタイルごとに違うこと（同じ鍵の2枚は見分けられない）
   */
  add(key: string): void {
    if (this.near.has(key) || this.away.has(key)) return;
    this.away.add(key);
  }

  /**
   * タイルが画面から消えた。読み途中なら枠も空ける——読み終わりの知らせはもう来ない。
   * アプリの側はその読みを続けるが、同時に読む数はあちらで守る
   */
  remove(key: string): void {
    this.near.delete(key);
    this.away.delete(key);
    if (this.running.delete(key)) this.pump();
  }

  /** 画面の前後1画面に入った（`true`）か、出た（`false`） */
  see(key: string, near: boolean): void {
    if (!this.near.has(key) && !this.away.has(key)) return; // 消えたか、読み始めた後
    (near ? this.away : this.near).delete(key);
    (near ? this.near : this.away).add(key);
    this.pump();
  }

  /** 読み終わった（失敗も含む） */
  done(key: string): void {
    if (this.running.delete(key)) this.pump();
  }

  /** いま枠を使っている読み（試験用） */
  inFlight(): string[] {
    return [...this.running.keys()];
  }

  private pump(): void {
    if (this.scheduled) return;
    this.scheduled = true;
    this.env.defer(() => {
      this.scheduled = false;
      this.fill();
    });
  }

  private fill(): void {
    if (this.running.size >= this.limit || this.near.size === 0) return;
    const distance = this.env.measure();
    // `Infinity - Infinity` は NaN になるので、引き算で比べない
    const ranked = [...this.near]
      .map((key) => ({ key, d: distance(key) }))
      .sort((a, b) => (a.d < b.d ? -1 : a.d > b.d ? 1 : 0));
    for (const { key } of ranked) {
      if (this.running.size >= this.limit) return;
      this.near.delete(key);
      const id = ++this.serial;
      this.running.set(key, id);
      this.env.after(TILE_STALL_MS, () => {
        if (this.running.get(key) === id) this.done(key);
      });
      this.env.start(key);
    }
  }
}
