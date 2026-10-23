/**
 * 取り込み画面のタイルを読む順番と数（pictkura-dev #73）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { TileQueue, TILE_READS } from "../src/tileQueue.ts";

/** 選ぶのは `flush` したときだけ（実物では次のタスク） */
function queue(limit = TILE_READS) {
  const started: string[] = [];
  const pending: Array<() => void> = [];
  const q = new TileQueue(
    limit,
    (k) => started.push(k),
    (run) => pending.push(run),
  );
  const flush = () => {
    while (pending.length > 0) pending.shift()!();
  };
  return { q, started, flush };
}

const key = (i: number) => `t${i}`;

/**
 * win が下端へ飛んで測った形: 245〜299 が一度に来て、見えているのは 280〜299。
 * 見張りは2つで、**「近い」の知らせが「見えている」より先に届く**順にする
 * （見えているタイルは「近い」にも入る）
 */
function jumpToTheBottom(q: TileQueue) {
  for (let i = 245; i < 300; i++) q.add(key(i), i);
  for (let i = 245; i < 300; i++) q.see(key(i), "near");
  for (let i = 280; i < 300; i++) q.see(key(i), "visible");
}

test("下端へ飛んでも、同時に読むのは決めた本数だけ", () => {
  const { q, started, flush } = queue();
  jumpToTheBottom(q);
  flush();
  assert.equal(q.inFlight().length, TILE_READS);
  assert.equal(started.length, TILE_READS);
});

test("見えているタイルを、上の先読みより先に読む（「近い」の知らせが先に届いても）", () => {
  const { q, started, flush } = queue();
  jumpToTheBottom(q);
  flush();
  while (started.length < 55) {
    q.done(q.inFlight()[0]!);
    flush();
  }
  assert.deepEqual(
    started.slice(0, 20),
    Array.from({ length: 20 }, (_, i) => key(280 + i)),
  );
  assert.deepEqual(
    started.slice(20),
    Array.from({ length: 35 }, (_, i) => key(245 + i)),
  );
});

test("画面から遠いタイルは読まず、近づいたら読む", () => {
  const { q, started, flush } = queue();
  q.add(key(0), 0);
  flush();
  assert.deepEqual(started, []);
  q.see(key(0), "near");
  flush();
  assert.deepEqual(started, [key(0)]);
});

test("読む前に見えなくなったタイルは後回しにし、戻れば読む", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i), i);
    q.see(key(i), "visible");
  }
  flush();
  assert.deepEqual(started, [key(0)]);
  q.see(key(1), "away");
  q.done(key(0));
  flush();
  assert.deepEqual(started, [key(0), key(2)]);
  q.see(key(1), "visible");
  q.done(key(2));
  flush();
  assert.deepEqual(started, [key(0), key(2), key(1)]);
});

test("読み途中で消えたタイルは枠を空ける（読み終わりの知らせが来ないため）", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1]) {
    q.add(key(i), i);
    q.see(key(i), "visible");
  }
  flush();
  assert.deepEqual(started, [key(0)]);
  q.remove(key(0));
  flush();
  assert.deepEqual(started, [key(0), key(1)]);
});

test("並びが変わっても、読み途中・読み終わりのタイルを読み直さない", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1]) {
    q.add(key(i), i);
    q.see(key(i), "visible");
  }
  flush();
  q.reindex(key(0), 5);
  q.done(key(0));
  q.reindex(key(0), 0);
  q.see(key(0), "visible");
  flush();
  assert.deepEqual(started, [key(0), key(1)]);
  assert.deepEqual(q.inFlight(), [key(1)]);
});

test("同じタイルを2回入れても、1回しか読まない", () => {
  const { q, started, flush } = queue();
  q.add(key(0), 0);
  q.add(key(0), 0);
  q.see(key(0), "visible");
  flush();
  q.done(key(0));
  q.see(key(0), "visible");
  flush();
  assert.deepEqual(started, [key(0)]);
});

test("まだ読み始めていないタイルの done は、枠を数え違えない", () => {
  const { q, started, flush } = queue(1);
  for (const i of [0, 1, 2]) {
    q.add(key(i), i);
    q.see(key(i), "visible");
  }
  flush();
  q.done(key(2));
  flush();
  assert.deepEqual(q.inFlight(), [key(0)]);
  assert.deepEqual(started, [key(0)]);
});
