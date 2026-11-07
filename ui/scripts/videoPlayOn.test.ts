/**
 * 動画を続けて見る（pictkura-dev #75）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { afterVideoEnded, isVideoOnlyFullscreen, rememberDir } from "../src/videoPlayOn.ts";

test("arriving with ← plays on towards newer videos (time order), and stops at the end", () => {
  assert.deepEqual(afterVideoEnded(false, -1), { dir: -1, wrap: false, slideshow: false });
});

test("arriving with → plays on towards older videos, as the slideshow does", () => {
  assert.deepEqual(afterVideoEnded(false, 1), { dir: 1, wrap: false, slideshow: false });
});

test("opened and not moved yet: the same as →", () => {
  assert.deepEqual(afterVideoEnded(false, null), { dir: 1, wrap: false, slideshow: false });
});

test("during a slideshow: next, wrapping to the start, whatever the last direction", () => {
  for (const last of [null, 1, -1] as const) {
    assert.deepEqual(afterVideoEnded(true, last), { dir: 1, wrap: true, slideshow: true });
  }
});

test("only the user's own moves change the remembered direction", () => {
  assert.equal(rememberDir(null, -1, false), -1);
  assert.equal(rememberDir(-1, 1, false), 1);
  //スライドショーの送り（いつも 1）で、← で決めた向きを上書きしない
  assert.equal(rememberDir(-1, 1, true), -1);
  assert.equal(rememberDir(null, 1, true), null);
});

test("only a video element in full screen is redirected", () => {
  assert.equal(isVideoOnlyFullscreen({ tagName: "VIDEO" }), true);
  for (const el of [{ tagName: "HTML" }, { tagName: "DIV" }, null, undefined]) {
    assert.equal(isVideoOnlyFullscreen(el), false, String(el?.tagName));
  }
});
