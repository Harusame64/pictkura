import { test } from "node:test";
import assert from "node:assert/strict";
import { changesLightLayout, decodeLightIndex, LIGHT_INDEX_MAGIC, LIGHT_RECORD_LEN, usableLightDays, withPicked } from "../src/lightIndex.ts";

/** Rust の `pack_light_index` の表どおりに 1 枚を書く（位置は表から写す。Rust の試験も同じ表を見る） */
function record(v: DataView, o: number, r: {
  id: number; taken: number; mtime: number; shot: number; body: number; rev: number;
  day: number; w: number; h: number; state: number; flags: number; tiny?: number[];
}) {
  v.setFloat64(o, r.id, true);
  v.setFloat64(o + 8, r.taken, true);
  v.setFloat64(o + 16, r.mtime, true);
  v.setFloat64(o + 24, r.shot, true);
  v.setFloat64(o + 32, r.body, true);
  v.setFloat64(o + 40, r.rev, true);
  v.setInt32(o + 48, r.day, true);
  v.setUint32(o + 52, r.w, true);
  v.setUint32(o + 56, r.h, true);
  v.setUint8(o + 60, r.state);
  v.setUint8(o + 61, r.flags);
  v.setUint8(o + 62, r.tiny ? 1 : 0);
  r.tiny?.forEach((b, i) => v.setUint8(o + 64 + i, b));
}

function indexOf(recs: Parameters<typeof record>[2][]): ArrayBuffer {
  const buf = new ArrayBuffer(8 + recs.length * LIGHT_RECORD_LEN);
  const v = new DataView(buf);
  v.setUint32(0, LIGHT_INDEX_MAGIC, true);
  v.setUint32(4, recs.length, true);
  recs.forEach((r, i) => record(v, 8 + i * LIGHT_RECORD_LEN, r));
  return buf;
}

const base = { taken: 1.7e12, mtime: 1.7e12 + 5, body: 0, rev: 3, w: 6000, h: 4000, state: 2, flags: 0 };

test("records come back in order, grouped by day, with every field from the documented layout", () => {
  const shot = 2 ** 53 - 1; // shot_key は 53 ビットまで: f64 で失わない
  const tinyRgb = Array.from({ length: 48 }, (_, i) => i);
  const idx = decodeLightIndex(
    indexOf([
      { ...base, id: 11, shot, day: 20261126, flags: 1 | 4 | 8 | 16 | 32, tiny: tinyRgb },
      { ...base, id: 10, shot: 1, day: 20261126, flags: 2 | 64 },
      { ...base, id: 9, shot: 2, day: 20261120, flags: 0, w: 0, h: 0, state: 0 },
    ]),
  )!;
  assert.deepEqual([...idx.days.keys()], [20261126, 20261120]);
  assert.deepEqual(idx.days.get(20261126)!.map((x) => x.id), [11, 10]);
  const a = idx.days.get(20261126)![0];
  assert.equal(a.shot_key, shot);
  assert.equal(a.taken_at_ms, base.taken);
  assert.equal(a.mtime_ms, base.mtime);
  assert.equal(a.rev, 3);
  assert.equal(a.day_key, 20261126);
  assert.equal(a.width, 6000);
  assert.equal(a.height, 4000);
  assert.equal(a.thumb_state, 2);
  assert.deepEqual(
    [a.is_raw, a.is_video, a.taken_at_known, a.taken_subsec, a.has_thumb, a.favorite, a.picked],
    [true, false, true, true, true, true, false],
  );
  const b = idx.days.get(20261126)![1];
  assert.deepEqual([b.is_raw, b.is_video, b.picked, b.favorite], [false, true, true, false]);
  // 色: RGB 48 バイトが RGBA 64 バイトに
  const px = idx.tiny.get(11)!;
  assert.deepEqual([...px.slice(0, 8)], [0, 1, 2, 255, 3, 4, 5, 255]);
  assert.equal(idx.tiny.has(10), false, "no colours: nothing kept");
});

test("a different mark or a wrong length is refused, not half-read", () => {
  const good = indexOf([{ ...base, id: 1, shot: 1, day: 20261126 }]);
  assert.ok(decodeLightIndex(good));
  const badMark = good.slice(0);
  new DataView(badMark).setUint32(0, LIGHT_INDEX_MAGIC + 1, true);
  assert.equal(decodeLightIndex(badMark), null);
  assert.equal(decodeLightIndex(good.slice(0, good.byteLength - 1)), null, "short");
  assert.equal(decodeLightIndex(new ArrayBuffer(3)), null);
  const empty = decodeLightIndex(indexOf([]))!;
  assert.equal(empty.days.size, 0);
});

test("only days whose count matches the summary are used", () => {
  const idx = decodeLightIndex(
    indexOf([
      { ...base, id: 3, shot: 1, day: 2 },
      { ...base, id: 2, shot: 2, day: 2 },
      { ...base, id: 1, shot: 3, day: 1 },
    ]),
  )!;
  const ok = usableLightDays(idx.days, [
    { day_key: 2, count: 2 },
    { day_key: 1, count: 2 }, // a photo arrived after the index was read
    { day_key: 0, count: 1 }, // not in the index at all
  ]);
  assert.deepEqual([...ok], [2]);
});

test("an update that changes the layout asks for a new index; a thumbnail-only update does not", () => {
  const idx = decodeLightIndex(indexOf([{ ...base, id: 5, shot: 9, day: 20261126, flags: 4 | 8 }]))!;
  const it = idx.byId.get(5)!;
  const thumbOnly = { ...it, thumb_state: 1, has_thumb: false };
  assert.equal(changesLightLayout(idx, thumbOnly), false, "thumbnail stage only");
  assert.equal(changesLightLayout(idx, { ...it, id: 6 }), false, "not in the index");
  assert.equal(changesLightLayout(null, it), false);
  for (const [k, v] of [
    ["rev", 4], ["day_key", 20261125], ["width", 4000], ["height", 6000], ["taken_at_ms", 1],
    ["taken_at_known", false], ["taken_subsec", false], ["shot_key", 1], ["body_key", 7],
    ["is_raw", true], ["is_video", true], ["picked", true],
  ] as const) {
    assert.equal(changesLightLayout(idx, { ...it, [k]: v }), true, k);
  }
});

test("a pick change is copied into the index without touching other days", () => {
  const idx = decodeLightIndex(
    indexOf([
      { ...base, id: 3, shot: 1, day: 2 },
      { ...base, id: 2, shot: 2, day: 2 },
      { ...base, id: 1, shot: 3, day: 1 },
    ]),
  )!;
  const next = withPicked(idx, [2, 99], true);
  assert.notEqual(next, idx);
  assert.deepEqual(next.days.get(2)!.map((x) => [x.id, x.picked]), [[3, false], [2, true]]);
  assert.equal(next.byId.get(2)!.picked, true);
  assert.equal(next.days.get(1), idx.days.get(1), "untouched day keeps its array");
  assert.equal(idx.byId.get(2)!.picked, false, "the old index is not mutated");
  assert.equal(withPicked(next, [2], true), next, "no change: same index");
});
