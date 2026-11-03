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

test("拡張ラテン文字と AltGr は位置へ落とさない（字を打っただけでショートカットにしない）", () => {
  // ラトビア語の AltGr+K は `ķ`。位置で `k` と読むと Ctrl+Alt+K がパレットを開く
  assert.equal(letterOf({ key: "ķ", code: "KeyK", altKey: true }), "ķ");
  assert.equal(letterOf({ key: "ķ", code: "KeyK" }), "ķ");
  // 独語の `ö`・仏語の `é` も刻印どおり
  assert.equal(letterOf({ key: "é", code: "Digit2" }), "é");
  assert.equal(letterOf({ key: "ö", code: "Semicolon" }), "ö");
  // キリル文字でも Alt が混じれば落とさない
  assert.equal(letterOf({ key: "з", code: "KeyP", altKey: true }), "з");
  // ギリシャ文字（ラテンでない別の字）は落とす
  assert.equal(letterOf({ key: "π", code: "KeyP" }), "p");
});

test("文字でないキーはそのまま（位置が文字キーでなければ落とさない）", () => {
  assert.equal(letterOf({ key: "Escape", code: "Escape" }), "Escape");
  assert.equal(letterOf({ key: " ", code: "Space" }), " ");
  assert.equal(letterOf({ key: ",", code: "Comma" }), ",");
  assert.equal(letterOf({ key: "?", code: "Digit7" }), "?");
});
