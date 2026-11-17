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

test("一覧の読み手: 遅い1枚は時間切れで枠を空け、読み終わったら絵は渡す", async () => {
  const made: FakeImage[] = [];
  const l = new TileLoader(
    1,
    () => {
      const img: FakeImage = { src: "", decoding: "", draggable: true, alt: "x", onload: null, onerror: null };
      made.push(img);
      return img as unknown as HTMLImageElement;
    },
    { slotTimeoutMs: 20, coveredBy: null }, // 待ちは時計の粒（Windows で約16ms）の何倍もとる
  );
  const got: string[] = [];
  for (const k of ["slow", "next"]) {
    l.attach(k, {} as Element, () => got.push(k));
    l.see(k, true);
  }
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(made.map((m) => m.src), ["slow"]);
  await new Promise((r) => setTimeout(r, 200));
  assert.deepEqual(made.map((m) => m.src), ["slow", "next"]);
  made[0]!.onload!();
  assert.deepEqual(got, ["slow"]);
});

test("同じ絵を読み途中に並んだタイルは、もう一度読まず、その読みの終わりで受け取る", async () => {
  const made: FakeImage[] = [];
  const l = new TileLoader(
    2,
    () => {
      const img: FakeImage = { src: "", decoding: "", draggable: true, alt: "x", onload: null, onerror: null };
      made.push(img);
      return img as unknown as HTMLImageElement;
    },
    { slotTimeoutMs: 10, coveredBy: null },
  );
  const got: string[] = [];
  const d1 = l.attach("t1", {} as Element, () => got.push("t1"), "big.jpg");
  l.see("t1", true);
  await new Promise((r) => setTimeout(r, 200)); // 時間切れで枠は空いたが、読みは続いている
  d1(); // そのタイルは外れた
  l.attach("t2", {} as Element, () => got.push("t2"), "big.jpg"); // 作り直したタイル
  l.see("t2", true);
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(made.length, 1);
  assert.equal(made[0]!.src, "big.jpg");
  made[0]!.onload!();
  assert.deepEqual(got, ["t2"]);
});

test("時間切れでも、読み途中が枠の2倍に達していたら枠を空けない（遅い媒体で読みを積み上げない）", async () => {
  const made: FakeImage[] = [];
  const l = new TileLoader(
    1,
    () => {
      const img: FakeImage = { src: "", decoding: "", draggable: true, alt: "x", onload: null, onerror: null };
      made.push(img);
      return img as unknown as HTMLImageElement;
    },
    { slotTimeoutMs: 10, coveredBy: null },
  );
  for (const k of ["a", "b", "c", "d"]) {
    l.attach(k, {} as Element, () => {});
    l.see(k, true);
  }
  await new Promise((r) => setTimeout(r, 300));
  assert.deepEqual(made.map((m) => m.src), ["a", "b"]);
});

test("上限で空けられなかった時間切れの枠は、ほかの読みが終わって上限を割ったら空く", async () => {
  const made: FakeImage[] = [];
  const l = new TileLoader(
    1,
    () => {
      const img: FakeImage = { src: "", decoding: "", draggable: true, alt: "x", onload: null, onerror: null };
      made.push(img);
      return img as unknown as HTMLImageElement;
    },
    { slotTimeoutMs: 10, coveredBy: null },
  );
  for (const k of ["a", "b", "c"]) {
    l.attach(k, {} as Element, () => {});
    l.see(k, true);
  }
  await new Promise((r) => setTimeout(r, 300));
  assert.deepEqual(made.map((m) => m.src), ["a", "b"]); // b は時間切れでも上限（2）で枠を握ったまま
  made[0]!.onload!(); // a が終わって上限を割った
  await new Promise((r) => setTimeout(r, 50));
  assert.deepEqual(made.map((m) => m.src), ["a", "b", "c"]);
});

test("同じ絵の2枚のタイルが同じ選びで始まっても、読みは1回で両方に渡す", async () => {
  const made: FakeImage[] = [];
  const l = new TileLoader(2, () => {
    const img: FakeImage = { src: "", decoding: "", draggable: true, alt: "x", onload: null, onerror: null };
    made.push(img);
    return img as unknown as HTMLImageElement;
  });
  const got: string[] = [];
  for (const k of ["t1", "t2"]) {
    l.attach(k, {} as Element, () => got.push(k), "same.jpg");
    l.see(k, true);
  }
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(made.length, 1);
  made[0]!.onload!();
  assert.deepEqual(got.sort(), ["t1", "t2"]);
});
