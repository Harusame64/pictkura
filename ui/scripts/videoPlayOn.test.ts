/**
 * 動画を続けて見る（pictkura-dev #75）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { afterVideoEnded, isVideoOnlyFullscreen, scopeStartIndex } from "../src/videoPlayOn.ts";

const filtered = { playing: false, videosOnly: true, inScope: false, curtainUp: false };

test("with the video filter, a move towards newer items plays on towards newer videos (time order) and stops at the end", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: -1 }), { dir: -1, wrap: false, slideshow: false });
});

test("with the video filter, a move towards older items plays on towards older videos", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: 1 }), { dir: 1, wrap: false, slideshow: false });
});

test("opened and not moved yet: shooting order (one newer, so act 1 plays on into act 2)", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: null }), { dir: -1, wrap: false, slideshow: false });
});

test("opened from a selection (a playlist): plays on in shooting order without the video filter", () => {
  const scoped = { ...filtered, videosOnly: false, inScope: true };
  assert.deepEqual(afterVideoEnded({ ...scoped, lastDir: null }), { dir: -1, wrap: false, slideshow: false });
  assert.deepEqual(afterVideoEnded({ ...scoped, lastDir: 1 }), { dir: 1, wrap: false, slideshow: false });
  assert.equal(afterVideoEnded({ ...scoped, curtainUp: true, lastDir: null }), null);
});

test("a selection of only videos opens at the oldest; a mixed selection at the newest", () => {
  const v = { is_video: true };
  const p = { is_video: false };
  assert.equal(scopeStartIndex([v, v, v]), 2);
  assert.equal(scopeStartIndex([v, p, v]), 0);
  assert.equal(scopeStartIndex([v]), 0);
  assert.equal(scopeStartIndex([]), 0);
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
      assert.deepEqual(afterVideoEnded({ playing: true, lastDir, videosOnly, inScope: false, curtainUp: false }), {
        dir: 1,
        wrap: true,
        slideshow: true,
      });
    }
  }
});

test("only a video element in full screen is moved to the window's full screen", () => {
  assert.equal(isVideoOnlyFullscreen({ tagName: "VIDEO" }), true);
  for (const el of [{ tagName: "HTML" }, { tagName: "DIV" }, null, undefined]) {
    assert.equal(isVideoOnlyFullscreen(el), false, String(el?.tagName));
  }
});
