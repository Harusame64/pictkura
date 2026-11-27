import { test } from "node:test";
import assert from "node:assert/strict";
import { afterMuteToggle, afterVolumeKey, canSeek, formatTime, seekTarget, volumeTarget } from "../src/videoControls.ts";

test("formatTime shows m:ss, h:mm:ss past an hour, and --:-- when the length is unknown", () => {
  assert.equal(formatTime(0), "0:00");
  assert.equal(formatTime(3.9), "0:03");
  assert.equal(formatTime(65), "1:05");
  assert.equal(formatTime(3600 + 62), "1:01:02");
  assert.equal(formatTime(Infinity), "--:--"); // MediaRecorder の webm（win の audio5.webm）
  assert.equal(formatTime(NaN), "--:--");
});

test("a video with no known length cannot be sought, and seeking does not move it", () => {
  assert.equal(canSeek(Infinity), false);
  assert.equal(canSeek(NaN), false);
  assert.equal(canSeek(0), false);
  assert.equal(canSeek(4), true);
  assert.equal(seekTarget(2, 5, Infinity), 2);
});

test("seeking stays between 0 and the end", () => {
  assert.equal(seekTarget(2, -5, 10), 0);
  assert.equal(seekTarget(8, 5, 10), 10);
  assert.equal(seekTarget(3, 5, 10), 8);
});

test("volume steps stay in 0..1 without float drift, and turning it up unmutes", () => {
  assert.equal(volumeTarget(0.1, 0.05), 0.15);
  assert.equal(volumeTarget(0.98, 0.05), 1);
  assert.equal(volumeTarget(0.02, -0.05), 0);
  assert.deepEqual(afterVolumeKey({ volume: 0.5, muted: true }, 0.05), { volume: 0.55, muted: false });
  assert.deepEqual(afterVolumeKey({ volume: 0.5, muted: true }, -0.05), { volume: 0.45, muted: true });
});

test("afterMuteToggle treats volume 0 as silent, so one M after turning the volume down to 0 brings sound back", () => {
  assert.deepEqual(afterMuteToggle({ volume: 0.4, muted: false }), { volume: 0.4, muted: true });
  assert.deepEqual(afterMuteToggle({ volume: 0.4, muted: true }), { volume: 0.4, muted: false });
  // ↓ で 0 まで下げた形（afterVolumeKey は下げるとき muted を変えない）
  const down = afterVolumeKey({ volume: 0.05, muted: false }, -0.05);
  assert.deepEqual(down, { volume: 0, muted: false });
  assert.deepEqual(afterMuteToggle(down), { volume: 0.5, muted: false });
  assert.deepEqual(afterMuteToggle({ volume: 0, muted: true }), { volume: 0.5, muted: false });
});
