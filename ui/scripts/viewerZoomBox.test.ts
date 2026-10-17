/**
 * タイルから広がる先の箱を、ビューアの写真の収め方と揃える（pictkura-dev #52）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import { containBox, fitScaleFor, viewerBox } from "../src/viewerZoomBox.ts";

test("a large photo fits inside the margins, centred", () => {
  // 幅の上限 1920-120=1800、高さの上限 1080-80=1000。横長 6000x4000 は高さで決まる（1500x1000）
  assert.deepEqual(viewerBox([6000, 4000], 0, 1920, 1080), {
    left: 210,
    top: 40,
    width: 1500,
    height: 1000,
  });
});

test("a small photo is not enlarged", () => {
  assert.deepEqual(viewerBox([400, 300], 0, 1920, 1080), {
    left: 760,
    top: 390,
    width: 400,
    height: 300,
  });
});

test("the info panel's reserve narrows the area and shifts the centre left", () => {
  // 残りの幅 1920-300=1620、上限 1500。横長 3000x1000 は幅で決まる（1500x500）
  assert.deepEqual(viewerBox([3000, 1000], 300, 1920, 1080), {
    left: 60,
    top: 290,
    width: 1500,
    height: 500,
  });
});

test("unknown dimensions give no box (the viewer opens without the zoom)", () => {
  assert.equal(viewerBox([0, 0], 0, 1920, 1080), null);
  assert.equal(viewerBox([NaN, 100], 0, 1920, 1080), null);
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
  assert.equal(fitScaleFor(6000, 4000, 0, 1920, 1080), 0.25);
});
