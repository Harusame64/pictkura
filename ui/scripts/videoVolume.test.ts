/**
 * 動画の音量を覚える（pictkura-dev #54）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { parseVideoVolume } from "../src/videoVolume.ts";

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
