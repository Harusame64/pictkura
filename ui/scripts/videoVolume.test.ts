/**
 * 動画の音量を覚える（pictkura-dev #54）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { isSilent, loadVideoVolume, parseVideoVolume, saveVideoVolume } from "../src/videoVolume.ts";

test("a saved volume and mute come back as saved", () => {
  assert.deepEqual(parseVideoVolume('{"volume":0.3,"muted":true}'), { volume: 0.3, muted: true });
  assert.deepEqual(parseVideoVolume('{"volume":0,"muted":false}'), { volume: 0, muted: false });
});

test("nothing saved, or something unreadable, starts at full volume, not muted", () => {
  const def = { volume: 1, muted: false };
  for (const raw of [null, "", "x", "null", "[]", '{"volume":"0.5"}', '{"volume":2}', '{"volume":-0.1}']) {
    assert.deepEqual(parseVideoVolume(raw), def, String(raw));
  }
});

test("mute is on only when saved as true", () => {
  assert.equal(parseVideoVolume('{"volume":0.5,"muted":"yes"}').muted, false);
});

test("what is saved reads back the same (the stored shape)", () => {
  for (const v of [
    { volume: 0.25, muted: false },
    { volume: 1, muted: true },
  ]) {
    assert.deepEqual(parseVideoVolume(JSON.stringify(v)), v);
  }
});

test("muted, or volume 0, counts as silent (the viewer shows a mark)", () => {
  assert.equal(isSilent({ volume: 0.5, muted: true }), true);
  assert.equal(isSilent({ volume: 0, muted: false }), true);
  assert.equal(isSilent({ volume: 0.5, muted: false }), false);
});

test("a change that could not be stored still holds for the next video in this window", () => {
  // node には localStorage が無い＝書き込みも読み出しも失敗する窓と同じ
  saveVideoVolume({ volume: 0.2, muted: true });
  assert.deepEqual(loadVideoVolume(), { volume: 0.2, muted: true });
  // 返した値をいじっても、覚えた値は変わらない
  loadVideoVolume().volume = 1;
  assert.equal(loadVideoVolume().volume, 0.2);
});
