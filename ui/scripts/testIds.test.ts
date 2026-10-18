/**
 * 試験と自動操作の手がかり（pictkura-dev #65）。`id="pk-…"` は UI Automation の AutomationId になるので、
 * 画面に1つしか無い部品にだけ付け、ソースの中でも重ねない。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

import { openWithId } from "../src/automationIds.ts";

const src = new URL("../src/", import.meta.url);
const files = readdirSync(src).filter((f) => f.endsWith(".tsx"));
const read = (f: string) => readFileSync(new URL(f, src), "utf8");

/** `id="pk-x"` と `id={c ? "pk-a" : "pk-b"}` の両方から、書いてある pk- の名前を拾う（`data-id=` は拾わない） */
function fixedIds(s: string): { id: string; at: number }[] {
  const out: { id: string; at: number }[] = [];
  for (const m of s.matchAll(/(?<![\w-])id=("pk-[A-Za-z0-9-]+"|\{[^}]*\})/g)) {
    for (const n of m[1].matchAll(/"(pk-[A-Za-z0-9-]+)"/g)) out.push({ id: n[1], at: m.index! });
  }
  return out;
}

test("each fixed pk- id appears once in the source", () => {
  const seen = new Map<string, string>();
  let n = 0;
  for (const f of files) {
    for (const { id } of fixedIds(read(f))) {
      assert.ok(!seen.has(id), `${id}: ${seen.get(id)} and ${f}`);
      seen.set(id, f);
      n++;
    }
  }
  // 取りこぼし（置き換えが効かなかった）を見分ける床
  assert.ok(n >= 30, `only ${n}`);
});

test("every pk- id has the matching data-testid on the same element", () => {
  let checked = 0;
  for (const f of files) {
    const s = read(f);
    for (const { id, at } of fixedIds(s)) {
      // 同じ要素の属性（次の子要素 `<` か要素の終わりまで）。複数行に分けて書いてあっても見る
      const rest = s.slice(at);
      const end = rest.search(/\n\s*(\/?>|<)/);
      const tag = rest.slice(0, end < 0 ? 400 : end);
      assert.match(tag, new RegExp(`data-testid=(\\{[^}]*)?"${id.slice(3)}"`), `${id} in ${f}`);
      checked++;
    }
  }
  assert.ok(checked >= 30, `only ${checked}`);
});

test("menu item ids are unique", () => {
  const s = read("App.tsx");
  const ids = [...s.matchAll(/\bid: "([a-z0-9-]+)"/g)].map((m) => m[1]);
  assert.ok(ids.length >= 15, `only ${ids.length}`);
  assert.equal(new Set(ids).size, ids.length, ids.join(","));
});

test("open-with ids come from the app's name, numbered only when needed", () => {
  const all = ["Adobe Photoshop 2026", "GIMP", "写真アプリ", "gimp"];
  assert.equal(openWithId("Adobe Photoshop 2026", 0, all), "open-with-adobe-photoshop-2026");
  // 英数字が無い名前は番号で
  assert.equal(openWithId("写真アプリ", 2, all), "open-with-2");
  // 同じ形になる名前どうしは番号を添えて分ける
  assert.equal(openWithId("GIMP", 1, all), "open-with-gimp-1");
  assert.equal(openWithId("gimp", 3, all), "open-with-gimp-3");
});
