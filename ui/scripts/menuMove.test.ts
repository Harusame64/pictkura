/**
 * 右クリックメニューのキーの行き先（`menuMove.ts`）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { menuMoveIndex } from "../src/menuMove.ts";

test("arrows move one item and wrap at the ends", () => {
  assert.equal(menuMoveIndex("ArrowDown", 0, 5), 1);
  assert.equal(menuMoveIndex("ArrowDown", 4, 5), 0);
  assert.equal(menuMoveIndex("ArrowUp", 0, 5), 4);
  assert.equal(menuMoveIndex("ArrowUp", 3, 5), 2);
});

test("with no item focused, down goes to the first and up to the last", () => {
  assert.equal(menuMoveIndex("ArrowDown", -1, 5), 0);
  assert.equal(menuMoveIndex("ArrowUp", -1, 5), 4);
});

test("Home and End go to the ends; other keys and empty menus do nothing", () => {
  assert.equal(menuMoveIndex("Home", 3, 5), 0);
  assert.equal(menuMoveIndex("End", 1, 5), 4);
  assert.equal(menuMoveIndex("Enter", 1, 5), null);
  assert.equal(menuMoveIndex("ArrowDown", -1, 0), null);
});
