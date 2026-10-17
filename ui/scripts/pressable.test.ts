/**
 * div で描いた押せるもの（pictkura-dev #65）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { pressable } from "../src/pressable.ts";

const key = (k: string, same = true) => {
  const el = {};
  let prevented = false;
  return {
    e: {
      key: k,
      target: el,
      currentTarget: same ? el : {},
      preventDefault: () => {
        prevented = true;
      },
    } as never,
    prevented: () => prevented,
  };
};

test("Enter and Space press it; other keys do not", () => {
  let n = 0;
  const p = pressable(() => n++);
  for (const k of ["Enter", " "]) {
    const { e, prevented } = key(k);
    p.onKeyDown(e);
    assert.equal(prevented(), true);
  }
  p.onKeyDown(key("a").e);
  assert.equal(n, 2);
  assert.equal(p.role, "button");
  assert.equal(p.tabIndex, 0);
});

test("a key pressed on a button inside it is left to that button", () => {
  let n = 0;
  pressable(() => n++).onKeyDown(key("Enter", false).e);
  assert.equal(n, 0);
});

test("focusable: false gives the role without a tab stop", () => {
  assert.equal(pressable(() => {}, { focusable: false }).tabIndex, undefined);
});
