import { test } from "node:test";
import assert from "node:assert/strict";
import { digitOf, letterOf } from "../src/keyLetter.ts";

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
  // ポーランド語 AltGr+A の `ą`・ハンガリー語 AltGr+K の `ł`・トルコ語 F 配列で KeyX にある `ö`
  assert.equal(letterOf({ key: "ą", code: "KeyA", altKey: true }), "ą");
  assert.equal(letterOf({ key: "ł", code: "KeyK", altKey: true }), "ł");
  assert.equal(letterOf({ key: "ö", code: "KeyX" }), "ö");
  // macOS の Option+P は `π`（Alt あり）。位置で `p` と読むと ⚑ が付く
  assert.equal(letterOf({ key: "π", code: "KeyP", altKey: true }), "π");
  // 字を生まないキー（デッドキー・IME の変換中）は、名前のまま
  assert.equal(letterOf({ key: "Dead", code: "KeyI", altKey: true }), "Dead");
  assert.equal(letterOf({ key: "Process", code: "KeyP" }), "Process");
  assert.equal(letterOf({ key: "Unidentified", code: "KeyX" }), "Unidentified");
  // タイ語・ヒンディー語は母音記号（結合文字）を出すキーがある。それも落とす
  assert.equal(letterOf({ key: "\u0e35", code: "KeyU" }), "u");
  assert.equal(letterOf({ key: "\u0902", code: "KeyX" }), "x");
  // 露語の `,` の位置は `б`
  assert.equal(letterOf({ key: "б", code: "Comma" }), ",");
  // ラテン文字の配列の `,` の位置にある別の字は、そのまま（AZERTY の `;`）
  assert.equal(letterOf({ key: ";", code: "Comma" }), ";");
  // ギリシャ文字（ラテンでない別の字）は、Alt 無しなら落とす
  assert.equal(letterOf({ key: "π", code: "KeyP" }), "p");
});

test("文字でないキーはそのまま（位置が文字キーでなければ落とさない）", () => {
  assert.equal(letterOf({ key: "Escape", code: "Escape" }), "Escape");
  assert.equal(letterOf({ key: " ", code: "Space" }), " ");
  assert.equal(letterOf({ key: ",", code: "Comma" }), ",");
  assert.equal(letterOf({ key: "?", code: "Digit7" }), "?");
});

test("digit keys read the printed digit, then the number row without modifiers", () => {
  assert.equal(digitOf({ key: "1", code: "Digit1" }), 1);
  assert.equal(digitOf({ key: "١", code: "Digit1" }), 1, "Arabic-Indic");
  assert.equal(digitOf({ key: "۰", code: "Digit0" }), 0, "Persian");
  assert.equal(digitOf({ key: "1", code: "Numpad1" }), 1, "numpad with NumLock");
  assert.equal(digitOf({ key: "&", code: "Digit1" }), 1, "French AZERTY 1 key");
  assert.equal(digitOf({ key: "à", code: "Digit0" }), 0, "French AZERTY 0 key");
  // 拾わない
  assert.equal(digitOf({ key: "!", code: "Digit1", shiftKey: true }), null, "US Shift+1");
  assert.equal(digitOf({ key: "¡", code: "Digit1", altKey: true }), null, "Alt+1");
  assert.equal(digitOf({ key: "1", code: "Digit1", ctrlKey: true }), null, "Ctrl+1");
  assert.equal(digitOf({ key: "1", code: "Digit1", metaKey: true }), null, "⌘1");
  assert.equal(digitOf({ key: "End", code: "Numpad1" }), null, "numpad with NumLock off");
  assert.equal(digitOf({ key: "x", code: "KeyX" }), null);
});
