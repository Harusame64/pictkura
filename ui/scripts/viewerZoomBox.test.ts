/**
 * タイルから広がる先の箱を、ビューアの写真の収め方と揃える（pictkura-dev #52）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";
import {
  containBox,
  fitScaleFor,
  VIEWER_PAD,
  VIEWER_PAD_VIDEO,
  viewerBox,
  viewerPad,
} from "../src/viewerZoomBox.ts";

test("a large photo fits the window, centred", () => {
  // 余白なし: 幅の上限 1920、高さの上限 1080。横長 6000x4000 は高さで決まる（1620x1080）
  assert.deepEqual(viewerBox([6000, 4000], 0, false, 1920, 1080), {
    left: 150,
    top: 0,
    width: 1620,
    height: 1080,
  });
});

test("a small photo is not enlarged", () => {
  assert.deepEqual(viewerBox([400, 300], 0, false, 1920, 1080), {
    left: 760,
    top: 390,
    width: 400,
    height: 300,
  });
});

test("the info panel's reserve narrows the area and shifts the centre left", () => {
  // 残りの幅 1920-300=1620。横長 3000x1000 は幅で決まる（1620x540）。中央はパネルを除いた残りの中——右へ寄らない
  assert.deepEqual(viewerBox([3000, 1000], 300, false, 1920, 1080), {
    left: 0,
    top: 270,
    width: 1620,
    height: 540,
  });
  // 幅が余るとき（縦長）は、残りの中央に寄る: 1000x2000 は高さで決まり 540x1080、(1620-540)/2 = 540
  assert.deepEqual(viewerBox([1000, 2000], 300, false, 1920, 1080), {
    left: 540,
    top: 0,
    width: 540,
    height: 1080,
  });
});

test("right to left puts the panel on the left and the photo after it", () => {
  // パネル 300 が左。縦長 540x1080 は残り 1620 の中央 300 + (1620-540)/2 = 840——左→右の 540 を鏡に映した位置
  assert.deepEqual(viewerBox([1000, 2000], 300, true, 1920, 1080), {
    left: 840,
    top: 0,
    width: 540,
    height: 1080,
  });
  // パネルが閉じていれば左右どちらでも同じ
  assert.deepEqual(viewerBox([3000, 1000], 0, true, 1920, 1080), viewerBox([3000, 1000], 0, false, 1920, 1080));
});

test("unknown dimensions give no box (the viewer opens without the zoom)", () => {
  assert.equal(viewerBox([0, 0], 0, false, 1920, 1080), null);
  assert.equal(viewerBox([NaN, 100], 0, false, 1920, 1080), null);
});

test("containBox finds the drawn picture inside a box of the wrong shape", () => {
  // 下敷きの箱 1800x1000 に 6000x4000 を contain で描くと 1500x1000、左右に 150 ずつ
  assert.deepEqual(
    containBox({ left: 60, top: 40, width: 1800, height: 1000 }, 6000, 4000),
    { left: 210, top: 40, width: 1500, height: 1000 },
  );
});

test("fitScaleFor never enlarges", () => {
  assert.equal(fitScaleFor(400, 300, 0, 1920, 1080), 1);
  assert.equal(fitScaleFor(6000, 4000, 0, 1920, 1080), 0.27);
});

test("fitScaleFor uses the whole window, in a window and in full screen", () => {
  // 6000x4000 on 1920x1080: no margin, so height is the limit at 1080/4000 (with the old 120/80 margin it was 1000/4000)
  assert.equal(fitScaleFor(6000, 4000, 0, 1920, 1080), 0.27);
  // a wide panorama is limited by width: 1920/6000 (1800/6000 with the old margin)
  assert.equal(fitScaleFor(6000, 1000, 0, 1920, 1080), 0.32);
  // the info panel's share is still taken
  assert.equal(fitScaleFor(6000, 1000, 120, 1920, 1080), 0.3);
  // a 1280x800 window (mac, 11-27): the landscape shot was 81% with the margin, 90% without
  assert.equal(fitScaleFor(8272, 6200, 0, 1280, 800), Math.min(1280 / 8272, 800 / 6200));
});

test("a video keeps the caption band free, so the zoom label matches what is drawn", () => {
  // win: 1920x1080 video, full screen, drawn 1842x1036 (= 1036/1080); the label said 100%
  assert.equal(fitScaleFor(1920, 1080, 0, 1920, 1080, viewerPad(true)), 1036 / 1080);
  assert.deepEqual(viewerPad(true), VIEWER_PAD_VIDEO);
  assert.deepEqual(viewerPad(false), VIEWER_PAD);
  assert.deepEqual(VIEWER_PAD, { x: 0, y: 0 });
});

test("the box a tile grows into uses the same pad as the viewer", () => {
  const box = viewerBox([6000, 4000], 0, false, 1920, 1080);
  assert.deepEqual(box, { left: 150, top: 0, width: 1620, height: 1080 });
});

test("the pads match App.css", () => {
  const css = readFileSync(new URL("../src/App.css", import.meta.url), "utf8");
  const block = (sel: string) => {
    const i = css.indexOf(`\n${sel} {`);
    assert.ok(i >= 0, sel);
    return css.slice(i, css.indexOf("}", i));
  };
  const px = (b: string, name: string) => Number(new RegExp(`${name}:\\s*(\\d+)px`).exec(b)?.[1]);
  const viewer = block(".viewer");
  assert.equal(px(viewer, "--viewer-pad-x"), VIEWER_PAD.x);
  assert.equal(px(viewer, "--viewer-pad-y"), VIEWER_PAD.y);
  assert.equal(px(viewer, "--caption-h"), VIEWER_PAD_VIDEO.y);
  assert.match(block(".viewer .viewer-video"), /max-height:\s*calc\(100vh - var\(--caption-h\)\)/);
  // 窓と全画面で余白を分けない（分けると、表示倍率の札と描いた大きさが片方でずれる）。全画面だけに効く
  // 規則（`.fullscreen` を含む選択子）が、余白や動画の高さを書き換えていないか、CSS 全体を見る
  for (const m of css.matchAll(/([^{}]*)\{([^}]*)\}/g)) {
    if (!/fullscreen/.test(m[1])) continue;
    assert.doesNotMatch(m[2], /--viewer-pad-|--viewer-fit-|--caption-h/, m[1].trim());
    if (/viewer-video|viewer-image/.test(m[1])) assert.doesNotMatch(m[2], /max-(height|width)/, m[1].trim());
  }
});
