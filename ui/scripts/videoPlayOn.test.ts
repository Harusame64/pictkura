/**
 * 動画を続けて見る（pictkura-dev #75）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { afterVideoEnded, isVideoOnlyFullscreen, rememberDir } from "../src/videoPlayOn.ts";

const filtered = { playing: false, videosOnly: true, curtainUp: false };

test("with the video filter, a move towards newer items plays on towards newer videos (time order) and stops at the end", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: -1 }), { dir: -1, wrap: false, slideshow: false });
});

test("with the video filter, a move towards older items plays on towards older videos", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: 1 }), { dir: 1, wrap: false, slideshow: false });
});

test("opened and not moved yet: the same as next (one older)", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: null }), { dir: 1, wrap: false, slideshow: false });
});

test("without the video filter, a finished video stays (no jump to a photo)", () => {
  for (const lastDir of [null, 1, -1] as const) {
    assert.equal(afterVideoEnded({ ...filtered, videosOnly: false, lastDir }), null);
  }
});

test("behind a curtain (info, print, reject, settings…) the viewer does not change", () => {
  assert.equal(afterVideoEnded({ ...filtered, curtainUp: true, lastDir: -1 }), null);
});

test("during a slideshow: next, wrapping to the start, whatever the filter or the last direction", () => {
  for (const lastDir of [null, 1, -1] as const) {
    for (const videosOnly of [true, false]) {
      assert.deepEqual(afterVideoEnded({ playing: true, lastDir, videosOnly, curtainUp: false }), {
        dir: 1,
        wrap: true,
        slideshow: true,
      });
    }
  }
});

test("only the user's own moves change the remembered direction", () => {
  assert.equal(rememberDir(null, -1, false), -1);
  assert.equal(rememberDir(-1, 1, false), 1);
  //スライドショーの送り（いつも 1）で、前へ動かして決めた向きを上書きしない
  assert.equal(rememberDir(-1, 1, true), -1);
  assert.equal(rememberDir(null, 1, true), null);
});

test("only a video element in full screen is moved to the window's full screen", () => {
  assert.equal(isVideoOnlyFullscreen({ tagName: "VIDEO" }), true);
  for (const el of [{ tagName: "HTML" }, { tagName: "DIV" }, null, undefined]) {
    assert.equal(isVideoOnlyFullscreen(el), false, String(el?.tagName));
  }
});
