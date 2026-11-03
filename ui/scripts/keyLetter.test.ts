import { test } from "node:test";
import assert from "node:assert/strict";
import { letterOf } from "../src/keyLetter.ts";

test("ラテン文字は刻印どおり（大文字も小文字に）", () => {
  assert.equal(letterOf({ key: "p", code: "KeyP" }), "p");
  assert.equal(letterOf({ key: "P", code: "KeyP" }), "p");
});

test("AZERTY・QWERTZ は位置ではなく刻印で読む", () => {
  // AZERTY: `Q` の位置に `A`。位置へ落とすと Ctrl+A が Ctrl+Q になってしまう
  assert.equal(letterOf({ key: "a", code: "KeyQ" }), "a");
  // QWERTZ: `Y` の位置に `Z`
  assert.equal(letterOf({ key: "z", code: "KeyY" }), "z");
});

test("ラテン文字でない配列（露語の ЙЦУКЕН）は物理位置へ落とす", () => {
  assert.equal(letterOf({ key: "з", code: "KeyP" }), "p");
  assert.equal(letterOf({ key: "ч", code: "KeyX" }), "x");
  assert.equal(letterOf({ key: "Г", code: "KeyU" }), "u");
  assert.equal(letterOf({ key: "л", code: "KeyK" }), "k");
});

test("文字でないキーはそのまま（位置が文字キーでなければ落とさない）", () => {
  assert.equal(letterOf({ key: "Escape", code: "Escape" }), "Escape");
  assert.equal(letterOf({ key: " ", code: "Space" }), " ");
  assert.equal(letterOf({ key: ",", code: "Comma" }), ",");
  assert.equal(letterOf({ key: "?", code: "Digit7" }), "?");
});
