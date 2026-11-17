/** 一覧のサムネイルの列（src/gridThumbs.ts）。`npm --prefix ui test` */
import { test } from "node:test";
import assert from "node:assert/strict";
import { GridThumbs, type TileAttach } from "../src/gridThumbs.ts";

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
const img = () => ({ isConnected: true, src: "" }) as unknown as HTMLImageElement;

test("読み終わるまで src を渡さず、読み終わったら同じ URL を入れて覚える", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = img();
  g.refFor("u1")(el);
  assert.equal(el.src, "");
  assert.equal(g.isLoaded("u1"), false);
  attached.get("u1")!({} as HTMLImageElement);
  assert.equal(el.src, "u1");
  assert.equal(g.isLoaded("u1"), true);
});

test("同じ URL には同じ ref を返す（描くたびに付け外ししない）", () => {
  const g = new GridThumbs(fakeLoader().l);
  assert.equal(g.refFor("u1"), g.refFor("u1"));
});

test("タイルが消えたら列から外す（画面から外れたものは読まない）", () => {
  const { l, detached } = fakeLoader();
  const g = new GridThumbs(l);
  const ref = g.refFor("u1");
  ref(img());
  ref(null);
  assert.deepEqual(detached, ["u1"]);
});

test("読み終えた URL は上限を超えたら古いものから忘れる", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l, 2);
  for (const u of ["a", "b", "c"]) {
    g.refFor(u)(img());
    attached.get(u)!({} as HTMLImageElement);
  }
  assert.equal(g.isLoaded("a"), false);
  assert.equal(g.isLoaded("b"), true);
  assert.equal(g.isLoaded("c"), true);
});

test("外れた後に読み終わっても、外れた要素には src を入れない", () => {
  const { l, attached } = fakeLoader();
  const g = new GridThumbs(l);
  const el = { isConnected: false, src: "" } as unknown as HTMLImageElement;
  g.refFor("u1")(el);
  attached.get("u1")!({} as HTMLImageElement);
  assert.equal(el.src, "");
  assert.equal(g.isLoaded("u1"), true);
});
