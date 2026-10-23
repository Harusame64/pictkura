/**
 * 取り込み画面のタイルの読み手（pictkura-dev #73）。`npm --prefix ui test`。
 *
 * 絵は読み手の `Image` が読み、タイルが途中で消えても読み終わりは知らされる——
 * 同時に読む数はそれで数えている。ここではその受け渡しを、偽の `Image` で見る。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { TileLoader } from "../src/useTileQueue.ts";

type FakeImage = {
  src: string;
  decoding: string;
  draggable: boolean;
  alt: string;
  onload: (() => void) | null;
  onerror: (() => void) | null;
};

function loader(limit = 1) {
  const made: FakeImage[] = [];
  const l = new TileLoader(limit, () => {
    const img: FakeImage = {
      src: "",
      decoding: "",
      draggable: true,
      alt: "x",
      onload: null,
      onerror: null,
    };
    made.push(img);
    return img as unknown as HTMLImageElement;
  });
  /** 選ぶのは次のタスク */
  const turn = () => new Promise((r) => setTimeout(r, 0));
  /** タイルを載せ、前後1画面に入れる。受け取った絵を記録する */
  const tile = (key: string) => {
    const got: unknown[] = [];
    const detach = l.attach(key, {} as Element, (img) => got.push(img));
    l.see(key, true);
    return { got, detach };
  };
  const reading = () => made.filter((m) => m.onload).map((m) => m.src);
  return { l, made, turn, tile, reading };
}

test("読み途中で消えたタイルの枠は、その絵が読み終わるまで空かない", async () => {
  const { made, turn, tile, reading } = loader(1);
  const a = tile("a");
  tile("b");
  await turn();
  assert.deepEqual(reading(), ["a"]);
  a.detach();
  await turn();
  assert.deepEqual(reading(), ["a"]);
  made[0]!.onload!();
  await turn();
  assert.deepEqual(reading(), ["b"]);
  // 渡す先の居ない絵は、誰にも渡らない
  assert.deepEqual(a.got, []);
});

test("読み途中に戻ってきたタイルは、もう一度読まずにその読みの絵を受け取る", async () => {
  const { made, turn, tile } = loader(2);
  const first = tile("a");
  await turn();
  first.detach();
  const again = tile("a");
  await turn();
  assert.equal(made.length, 1);
  made[0]!.onload!();
  assert.deepEqual(again.got, [made[0]]);
});

test("読めなかった絵も枠を空け、タイルには渡す（壊れた絵の印が出る）", async () => {
  const { made, turn, tile, reading } = loader(1);
  const a = tile("a");
  tile("b");
  await turn();
  made[0]!.onerror!();
  await turn();
  assert.deepEqual(a.got, [made[0]]);
  assert.deepEqual(reading(), ["b"]);
});
