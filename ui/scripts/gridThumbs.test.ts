/** 一覧のサムネイルの列（src/gridThumbs.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";
import { GridThumbs, paintable } from "../src/gridThumbs.ts";
import type { TileAttach } from "../src/useTileQueue.ts";

function fakeLoader() {
  const attached = new Map<string, (img: HTMLImageElement) => void>();
  const detached: string[] = [];
  const l: TileAttach = {
    attach(key, _el, ready) {
      attached.set(key, ready);
      return () => {
        attached.delete(key);
        detached.push(key);
      };
    },
  };
  return { l, attached, detached };
}
function tile(connected = true) {
  const attrs = new Map<string, string>();
  return {
    isConnected: connected,
    src: "",
    setAttribute: (k: string, v: string) => void attrs.set(k, v),
    attrs,
  } as unknown as HTMLImageElement & { attrs: Map<string, string> };
}
const got = (w: number) => ({ complete: true, naturalWidth: w }) as HTMLImageElement;

test("読んで展開が済むまで src を入れず、済んだら同じ URL を入れる", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = tile();
  g.refFor("u1")(el);
  assert.equal(el.src, "");
  attached.get("u1")!(got(512));
  assert.equal(el.src, "u1");
});

test("読めなかった・1x1 なら src を入れず（もう一度要求しない）、地の灰色を見せる印を付ける", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  for (const [u, w] of [["e", 0], ["one", 1]] as const) {
    const el = tile();
    g.refFor(u)(el);
    attached.get(u)!(got(w));
    assert.equal(el.src, "", u);
    assert.equal(el.attrs.get("data-shown"), "", u);
  }
});

test("絵の URL が替わったら、新しい絵が届くまで前の絵を出したまま", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = tile();
  g.refFor("v1")(el);
  attached.get("v1")!(got(512));
  // 描き直し: React は古い ref に null、新しい ref に要素を渡す
  g.refFor("v1")(null);
  g.refFor("v2")(el);
  assert.equal(el.src, "v1");
  attached.get("v2")!(got(512));
  assert.equal(el.src, "v2");
});

test("同じ URL には同じ ref を返し、外れたら列からも ref の控えからも消す", () => {
  const { l, detached } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("u1");
  assert.equal(g.refFor("u1"), ref);
  ref(tile());
  ref(null);
  assert.deepEqual(detached, ["u1"]);
  assert.equal(g.size(), 0);
});

test("外れた要素には、読み終わっても src を入れない", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = tile(false);
  g.refFor("u1")(el);
  attached.get("u1")!(got(512));
  assert.equal(el.src, "");
});

test("paintable は読めた 2px 以上の絵だけ", () => {
  assert.equal(paintable({ complete: true, naturalWidth: 512 }), true);
  assert.equal(paintable({ complete: true, naturalWidth: 1 }), false);
  assert.equal(paintable({ complete: false, naturalWidth: 512 }), false);
});
