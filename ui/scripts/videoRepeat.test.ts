/**
 * 動画の繰り返し（videoRepeat.ts）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { effectiveRepeat, nextRepeat, parseVideoRepeat } from "../src/videoRepeat.ts";

test("with a playlist the button cycles off → all → one → off", () => {
  assert.equal(nextRepeat("off", true), "all");
  assert.equal(nextRepeat("all", true), "one");
  assert.equal(nextRepeat("one", true), "off");
});

test("without a playlist 'all' cannot be picked: off ⇔ one", () => {
  assert.equal(nextRepeat("off", false), "one");
  assert.equal(nextRepeat("one", false), "off");
});

test("a remembered 'all' counts as off where there is no playlist, and the next press goes to one", () => {
  assert.equal(effectiveRepeat("all", false), "off");
  assert.equal(effectiveRepeat("all", true), "all");
  assert.equal(effectiveRepeat("one", false), "one");
  assert.equal(nextRepeat("all", false), "one");
});

test("unknown or missing saved values read as off", () => {
  assert.equal(parseVideoRepeat(null), "off");
  assert.equal(parseVideoRepeat("loop"), "off");
  assert.equal(parseVideoRepeat("all"), "all");
  assert.equal(parseVideoRepeat("one"), "one");
});
