/**
 * 消えている UI の上を押したか（pictkura-dev #75）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { IDLE_HIDDEN_UI, IDLE_HIT_TARGETS, pointInAnyRect } from "../src/idleHit.ts";

const tools = { left: 1700, top: 0, right: 1920, bottom: 60 };
const nav = { left: 0, top: 500, right: 60, bottom: 580 };

test("a click where the hidden toolbar or arrow was drawn counts as on the hidden UI", () => {
  assert.equal(pointInAnyRect(1886, 30, [tools, nav]), true);
  assert.equal(pointInAnyRect(30, 540, [tools, nav]), true);
  //縁も含む
  assert.equal(pointInAnyRect(1700, 0, [tools]), true);
});

test("a click on the backdrop elsewhere does not, so it still closes the viewer", () => {
  assert.equal(pointInAnyRect(960, 540, [tools, nav]), false);
  assert.equal(pointInAnyRect(1699, 30, [tools]), false);
});

test("an element with no size (not drawn) is ignored", () => {
  assert.equal(pointInAnyRect(0, 0, [{ left: 0, top: 0, right: 0, bottom: 0 }]), false);
});

test("the list matches the elements App.css hides while idle", () => {
  const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");
  const block = css.slice(0, css.indexOf("{", css.indexOf(".viewer.idle")) + 1);
  const hidden = [...block.matchAll(/\.viewer\.idle (\.[\w-]+)/g)].map((m) => m[1]);
  assert.deepEqual([...hidden].sort(), [...IDLE_HIDDEN_UI].sort());
});

test("every hidden part is hit-tested, through its drawn contents for the full-width bands", () => {
  for (const sel of IDLE_HIDDEN_UI) {
    assert.ok(IDLE_HIT_TARGETS.some((t) => t === sel || t.startsWith(sel + " ")), sel);
  }
  //帯と題名は箱でなく中身
  assert.ok(IDLE_HIT_TARGETS.includes(".viewer-strip > *"));
  assert.ok(IDLE_HIT_TARGETS.includes(".viewer-caption > *"));
  assert.ok(!IDLE_HIT_TARGETS.includes(".viewer-strip"));
  assert.ok(!IDLE_HIT_TARGETS.includes(".viewer-caption"));
});
