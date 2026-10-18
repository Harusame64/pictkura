/**
 * 試験と自動操作の手がかり（pictkura-dev #65）。`id="pk-…"` は UI Automation の AutomationId になるので、
 * 画面に1つしか無い部品にだけ付け、ソースの中でも重ねない。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const src = new URL("../src/", import.meta.url);
const files = readdirSync(src).filter((f) => f.endsWith(".tsx"));
const ids = files.flatMap((f) =>
  [...readFileSync(new URL(f, src), "utf8").matchAll(/\bid="(pk-[a-z-]+)"/g)].map((m) => `${m[1]} (${f})`),
);

test("each fixed pk- id appears once in the source", () => {
  const seen = new Map<string, string>();
  for (const entry of ids) {
    const id = entry.split(" ")[0];
    assert.ok(!seen.has(id), `${id}: ${seen.get(id)} and ${entry}`);
    seen.set(id, entry);
  }
  // 取りこぼし（置き換えが効かなかった）を見分ける床。ビューアのツール・帯・幕の外枠ぶん
  assert.ok(ids.length >= 25, `only ${ids.length}`);
});

test("every pk- id has a matching data-testid on the same element", () => {
  for (const f of files) {
    const s = readFileSync(new URL(f, src), "utf8");
    for (const m of s.matchAll(/\bid="pk-([a-z-]+)" data-testid="([a-z-]+)"/g)) {
      assert.equal(m[1], m[2], f);
    }
  }
});
