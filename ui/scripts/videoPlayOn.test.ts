/**
 * 動画を続けて見る（pictkura-dev #75）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { afterVideoEnded, isVideoOnlyFullscreen, nextScopeIndex, scopeStartIndex, wrapScopeIndex, wrapsOntoItself } from "../src/videoPlayOn.ts";

const filtered = { playing: false, videosOnly: true, inScope: false, curtainUp: false, nextIsVideo: () => true };

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

test("a mixed selection plays on only when the next item that way is a video (never stops on a photo)", () => {
  const scoped = { ...filtered, videosOnly: false, inScope: true };
  // 撮った順の次（-1）が写真、古い側（1）が動画
  const nextIsVideo = (dir: 1 | -1) => dir === 1;
  assert.equal(afterVideoEnded({ ...scoped, nextIsVideo, lastDir: null }), null);
  assert.deepEqual(afterVideoEnded({ ...scoped, nextIsVideo, lastDir: 1 }), { dir: 1, wrap: false, slideshow: false });
  // 動画のフィルター中は訊かない（並ぶのはみな動画）
  assert.deepEqual(afterVideoEnded({ ...filtered, nextIsVideo: () => false, lastDir: null }), {
    dir: -1,
    wrap: false,
    slideshow: false,
  });
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
      assert.deepEqual(afterVideoEnded({ ...filtered, playing: true, lastDir, videosOnly }), {
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

test("walking a selection skips seats that are gone, the same for moving and for checking what comes next", () => {
  // 0: 動画, 1: 消えた動画, 2: 写真, 3: 動画
  const gone = (k: number) => k === 1;
  assert.equal(nextScopeIndex(4, 0, 1, gone), 2, "消えた席を飛ばして写真へ");
  assert.equal(nextScopeIndex(4, 2, -1, gone), 0);
  assert.equal(nextScopeIndex(4, 3, 1, gone), -1, "端");
  assert.equal(nextScopeIndex(4, 0, -1, gone), -1, "端");
  assert.equal(nextScopeIndex(4, 1, 1, () => true), -1, "みな消えた");
});

test("repeat all with the video filter: plays on and wraps at the end, in the same direction", () => {
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: null, repeatAll: true }), {
    dir: -1,
    wrap: true,
    slideshow: false,
  });
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: 1, repeatAll: true }), { dir: 1, wrap: true, slideshow: false });
});

test("repeat all in a selection: wraps only at the end, and only onto a video", () => {
  const scoped = { ...filtered, videosOnly: false, inScope: true, repeatAll: true };
  // まだ先がある: 今までどおり次へ（戻らない）
  assert.deepEqual(afterVideoEnded({ ...scoped, lastDir: null, atEnd: () => false, wrapIsVideo: () => true }), {
    dir: -1,
    wrap: false,
    slideshow: false,
  });
  // 端に着いた: 戻る先が動画なら最初へ
  const atEnd = { ...scoped, nextIsVideo: () => false, atEnd: () => true };
  assert.deepEqual(afterVideoEnded({ ...atEnd, lastDir: null, wrapIsVideo: () => true }), {
    dir: -1,
    wrap: true,
    slideshow: false,
  });
  // 戻る先が写真なら止まる（写真へ着いて止まる、にしない）
  assert.equal(afterVideoEnded({ ...atEnd, lastDir: null, wrapIsVideo: () => false }), null);
  // 次が写真で端でもない: 止まる（繰り返しでも写真を飛ばして先へは行かない）
  assert.equal(
    afterVideoEnded({ ...scoped, nextIsVideo: () => false, lastDir: null, atEnd: () => false, wrapIsVideo: () => true }),
    null,
  );
});

test("without repeat all, the end of the list still stops", () => {
  const scoped = { ...filtered, videosOnly: false, inScope: true };
  assert.equal(
    afterVideoEnded({ ...scoped, nextIsVideo: () => false, lastDir: null, atEnd: () => true, wrapIsVideo: () => true }),
    null,
  );
  assert.deepEqual(afterVideoEnded({ ...filtered, lastDir: null }), { dir: -1, wrap: false, slideshow: false });
});

test("wrapScopeIndex walks from the head for 1 and from the tail for -1, skipping gone seats", () => {
  const none = () => false;
  assert.equal(wrapScopeIndex(4, 1, none), 0);
  assert.equal(wrapScopeIndex(4, -1, none), 3);
  assert.equal(wrapScopeIndex(4, -1, (k) => k === 3), 2);
  assert.equal(wrapScopeIndex(4, 1, (k) => k === 0), 1);
  assert.equal(wrapScopeIndex(0, 1, none), -1);
  assert.equal(wrapScopeIndex(2, -1, () => true), -1);
});

test("wrapsOntoItself: only at the end, and only onto the video that just ended", () => {
  const none = () => false;
  const ids = [30, 20, 10]; // 列は新しい順。撮った順（-1）なら 10 → 20 → 30
  const scope = { length: ids.length, idAt: (k: number) => ids[k] };
  const base = { scope, gone: none, total: 99 };
  // codex: 動画の絞り込み中に選んだ3本を開く。いちばん古い（尻）から撮った順に流す途中は戻らない
  assert.equal(wrapsOntoItself({ ...base, from: 2, dir: -1, currentId: 10 }), false);
  assert.equal(wrapsOntoItself({ ...base, from: 1, dir: -1, currentId: 20 }), false);
  // 端（いちばん新しい）に着いた: 戻る先は 10 で、今の 30 ではない
  assert.equal(wrapsOntoItself({ ...base, from: 0, dir: -1, currentId: 30 }), false);
  // 1本だけ選んだ: 端で、戻る先が自分
  const one = { length: 1, idAt: () => 7 };
  assert.equal(wrapsOntoItself({ ...base, scope: one, from: 0, dir: -1, currentId: 7 }), true);
  // 消えて1本だけ残った（ほかの席は gone）
  assert.equal(wrapsOntoItself({ ...base, from: 1, dir: -1, gone: (k) => k !== 1, currentId: 20 }), true);
  // 列が無い（動画の絞り込みの一覧）: 一覧が1枚のときだけ
  assert.equal(wrapsOntoItself({ scope: null, from: undefined, dir: -1, gone: none, currentId: 1, total: 1 }), true);
  assert.equal(wrapsOntoItself({ scope: null, from: undefined, dir: -1, gone: none, currentId: 1, total: 3 }), false);
});
