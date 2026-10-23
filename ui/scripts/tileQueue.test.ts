/**
 * 取り込み画面のタイルを読む順番と数（pictkura-dev #73）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { TileQueue, TILE_READS, TILE_STALL_MS } from "../src/tileQueue.ts";

/**
 * 選ぶのは `flush` したときだけ（実物では次のタスク）。時計は `tick` で進める。
 * 画面からの距離は `at` に書く（書いていないタイルは測れない＝`Infinity`）
 */
function queue(limit = TILE_READS) {
  const started: string[] = [];
  const at = new Map<string, number>();
  const pending: Array<() => void> = [];
  let now = 0;
  const timers: Array<{ due: number; run: () => void }> = [];
  const q = new TileQueue(limit, {
    start: (k) => started.push(k),
    measure: () => (k) => at.get(k) ?? Infinity,
    defer: (run) => pending.push(run),
    after: (ms, run) => timers.push({ due: now + ms, run }),
  });
  const flush = () => {
    while (pending.length > 0) pending.shift()!();
  };
  const tick = (ms: number) => {
    now += ms;
    for (const t of timers.splice(0)) {
      if (t.due <= now) t.run();
      else timers.push(t);
    }
    flush();
  };
  return { q, started, at, flush, tick };
}

const key = (i: number) => `t${i}`;
const keys = (from: number, to: number) =>
  Array.from({ length: to - from }, (_, i) => key(from + i));

/**
 * win が下端へ飛んで測った形: 245〜299 が前後1画面に入り、見えているのは 280〜299。
 * 1行 5 枚、1行 200px として、見えている範囲の上端からの距離を置く
 */
function jumpToTheBottom(q: TileQueue, at: Map<string, number>) {
  for (let i = 245; i < 300; i++) {
    q.add(key(i));
    at.set(key(i), i >= 280 ? 0 : (Math.floor((279 - i) / 5) + 1) * 200 - 199);
  }
  for (let i = 245; i < 300; i++) q.see(key(i), true);
}

/** 読み途中を全部終わらせながら、全部始まるまで回す */
function drain(
  q: TileQueue,
  started: string[],
  flush: () => void,
  total: number,
) {
  flush();
  while (started.length < total) {
    for (const k of q.inFlight()) q.done(k);
    flush();
  }
}

test("下端へ飛んでも、同時に読むのは決めた本数だけ", () => {
  const { q, started, at, flush } = queue();
  jumpToTheBottom(q, at);
  flush();
  assert.equal(q.inFlight().length, TILE_READS);
  assert.equal(started.length, TILE_READS);
});

test("見えているタイルを先に、その後は見えている範囲に近い行から読む", () => {
  const { q, started, at, flush } = queue();
  jumpToTheBottom(q, at);
  drain(q, started, flush, 55);
  assert.deepEqual(new Set(started.slice(0, 20)), new Set(keys(280, 300)));
  // 先読みは 275〜279 の行が先、245〜249 の行が最後
  assert.deepEqual(new Set(started.slice(20, 25)), new Set(keys(275, 280)));
  assert.deepEqual(new Set(started.slice(50)), new Set(keys(245, 250)));
});

test("距離は選ぶときに測る（知らせが届いた後で見えるようになったタイルも先に読む）", () => {
  const { q, started, at, flush } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i));
    at.set(key(i), 500);
    q.see(key(i), true);
  }
  at.set(key(2), 0);
  flush();
  assert.deepEqual(started, [key(2)]);
});

test("前後1画面の外のタイルは読まず、入ったら読む", () => {
  const { q, started, at, flush } = queue();
  q.add(key(0));
  at.set(key(0), 0);
  flush();
  assert.deepEqual(started, []);
  q.see(key(0), true);
  flush();
  assert.deepEqual(started, [key(0)]);
});

test("読む前に範囲から出たタイルは後回しにし、戻れば読む", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  assert.deepEqual(started, [key(0)]);
  q.see(key(1), false);
  q.done(key(0));
  flush();
  assert.deepEqual(started, [key(0), key(2)]);
  q.see(key(1), true);
  q.done(key(2));
  flush();
  assert.deepEqual(started, [key(0), key(2), key(1)]);
});

test("読み途中で消えたタイルは枠をすぐ空ける（読み終わりの知らせはもう来ない。数はアプリが守る）", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  q.remove(key(0));
  flush();
  assert.deepEqual(started, [key(0), key(1)]);
});

test("測れないタイル同士の順は、測れるタイルより後ろに揃う", () => {
  const { q, started, at, flush } = queue(1);
  for (const i of [0, 1, 2, 3]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  at.set(key(3), 0);
  flush();
  assert.deepEqual(started, [key(3)]);
});

test("答えの来ない読みは、時間切れで枠を空ける", () => {
  const { q, started, flush, tick } = queue();
  for (const i of [0, 1, 2]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  assert.deepEqual(started, [key(0), key(1)]);
  tick(TILE_STALL_MS - 1);
  assert.deepEqual(started, [key(0), key(1)]);
  tick(1);
  assert.deepEqual(started, keys(0, 3));
});

test("読み終わった後の時間切れは、別の読みの枠を空けない", () => {
  const { q, started, flush, tick } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  tick(TILE_STALL_MS - 1);
  q.done(key(0)); // key(1) が始まる
  flush();
  tick(1); // key(0) の時間切れ
  assert.deepEqual(started, [key(0), key(1)]);
  assert.deepEqual(q.inFlight(), [key(1)]);
});

test("読み始めた・読み終わったタイルを、もう一度読まない", () => {
  const { q, started, flush } = queue();
  q.add(key(0));
  q.add(key(0));
  q.see(key(0), true);
  flush();
  q.see(key(0), false);
  q.see(key(0), true);
  q.done(key(0));
  q.see(key(0), true);
  flush();
  assert.deepEqual(started, [key(0)]);
});

test("まだ読み始めていないタイルの done は、枠を数え違えない", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  q.done(key(2));
  flush();
  assert.deepEqual(q.inFlight(), [key(0)]);
  assert.deepEqual(started, [key(0)]);
});

test("消えて戻ったタイルの新しい読みを、前の読みの時間切れで空けない", () => {
  const { q, started, flush, tick } = queue();
  q.add(key(0));
  q.see(key(0), true);
  flush();
  q.remove(key(0));
  tick(TILE_STALL_MS - 1);
  q.add(key(0));
  q.see(key(0), true);
  flush();
  for (const i of [1, 2]) {
    q.add(key(i));
    q.see(key(i), true);
  }
  flush();
  assert.deepEqual(started, [key(0), key(0), key(1)]);
  tick(1); // 1回目の読みの時間切れ
  assert.deepEqual(started, [key(0), key(0), key(1)]);
});
