/** 一覧のサムネイルの列（src/gridThumbs.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";
import { GridThumbs, paintable } from "../src/gridThumbs.ts";
import type { TileAttach } from "../src/useTileQueue.ts";

/** 偽の列: 載せたタイルの受け取り口を、要素ごとに持つ */
function fakeLoader() {
  const attached = new Map<unknown, (img: HTMLImageElement) => void>();
  const keys = new Map<unknown, string>();
  const detached: string[] = [];
  const l: TileAttach = {
    attach(key, el, ready) {
      attached.set(el, ready);
      keys.set(el, key);
      return () => {
        attached.delete(el);
        detached.push(key);
      };
    },
  };
  const deliver = (el: unknown, img: HTMLImageElement) => attached.get(el)!(img);
  return { l, attached, keys, detached, deliver };
}
type Tile = HTMLImageElement & { attrs: Map<string, string>; isConnected: boolean };
function tile(): Tile {
  const attrs = new Map<string, string>();
  const t = {
    isConnected: true,
    src: "",
    setAttribute: (k: string, v: string) => void attrs.set(k, v),
    removeAttribute: (k: string) => {
      if (k === "src") t.src = "";
      attrs.delete(k);
    },
    getAttribute: (k: string) => (k === "src" ? t.src || null : (attrs.get(k) ?? null)),
    attrs,
  };
  return t as unknown as Tile;
}
/** React が要素を外す: 文書から外れてから ref に null */
function unmount(ref: (el: HTMLImageElement | null) => void, el: Tile) {
  el.isConnected = false;
  ref(null);
}
const got = (w: number, h = w) => ({ complete: true, naturalWidth: w, naturalHeight: h }) as HTMLImageElement;
const tick = () => new Promise((r) => setTimeout(r, 0));

test("読み終わるまで src を入れず、読み終わったら同じ URL を入れ、列から外す", () => {
  const { l, deliver, detached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = tile();
  g.refFor("u1")(el);
  assert.equal(el.src, "");
  deliver(el, got(512));
  assert.equal(el.src, "u1");
  assert.deepEqual(detached, ["u1"]);
});

test("読めなかった・1x1 なら src を外して地の灰色を見せ、覚えない（作り直しで問い直す）", () => {
  const { l, deliver, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("none");
  const a = tile();
  a.src = "old";
  ref(a);
  deliver(a, got(1, 1));
  assert.equal(a.src, "");
  assert.equal(a.attrs.get("data-shown"), "");
  unmount(ref, a);
  const b = tile();
  ref(b);
  assert.equal(attached.has(b), true);
});

test("作り直し（古い要素が外れ、すぐ新しい要素）では同じ ref を使い続ける", async () => {
  const g = new GridThumbs(fakeLoader().l);
  const ref = g.refFor("u1");
  const a = tile();
  ref(a);
  unmount(ref, a);
  ref(tile());
  await tick();
  assert.equal(g.refFor("u1"), ref);
});

test("付いている要素が無くなった ref は、後でまとめて控えから消す", async () => {
  const { l, detached } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("u1");
  const a = tile();
  ref(a);
  unmount(ref, a);
  assert.deepEqual(detached, ["u1"]);
  await tick();
  assert.equal(g.size(), 0);
});

test("最近出した絵は、作り直したタイルに列を通さずすぐ入れ、すぐ見せる", () => {
  const { l, deliver, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("u1");
  const a = tile();
  ref(a);
  deliver(a, got(512));
  unmount(ref, a);
  const b = tile();
  ref(b);
  assert.equal(b.src, "u1");
  assert.equal(b.attrs.get("data-shown"), "");
  assert.equal(attached.has(b), false);
});

test("最近出した絵の数を超えたら、古いものは列を通る", () => {
  const { l, deliver, attached } = fakeLoader();
  const g = new GridThumbs(l, 1);
  for (const u of ["a", "b"]) {
    const el = tile();
    g.refFor(u)(el);
    deliver(el, got(512));
  }
  const el = tile();
  g.refFor("a")(el);
  assert.equal(el.src, "");
  assert.equal(attached.has(el), true);
});

test("同じ URL のタイルが2枚あっても、片方が外れて残る方の読みを止めない", () => {
  const { l, deliver } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("dup");
  const a = tile();
  const b = tile();
  ref(a);
  ref(b);
  unmount(ref, a);
  deliver(b, got(512));
  assert.equal(b.src, "dup");
});

test("外れた要素には、読み終わっても src を入れない", () => {
  const { l, deliver } = fakeLoader();
  const g = new GridThumbs(l);
  const el = tile();
  g.refFor("u1")(el);
  el.isConnected = false;
  deliver(el, got(512));
  assert.equal(el.src, "");
});

test("付かなかった ref（捨てられた描画）は控えに残らない", () => {
  const g = new GridThumbs(fakeLoader().l);
  g.refFor("a");
  g.refFor("b");
  assert.equal(g.size(), 0);
});

test("paintable は読めた絵で、「サムネイルが無い」の 1x1 だけを外す（幅1の細長い絵は出す）", () => {
  assert.equal(paintable({ complete: true, naturalWidth: 512, naturalHeight: 341 }), true);
  assert.equal(paintable({ complete: true, naturalWidth: 1, naturalHeight: 512 }), true);
  assert.equal(paintable({ complete: true, naturalWidth: 1, naturalHeight: 1 }), false);
  assert.equal(paintable({ complete: true, naturalWidth: 0, naturalHeight: 0 }), false);
  assert.equal(paintable({ complete: false, naturalWidth: 512, naturalHeight: 341 }), false);
});
