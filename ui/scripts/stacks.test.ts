/**
 * 一覧の重ね（`src/stacks.ts`、dev #32）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  closeOverStacks,
  countPhotos,
  filesOf,
  stackMembersIndex,
  stacksOfDay,
  type Stackable,
} from "../src/stacks.ts";

/** id・組の鍵・RAW か・撮影日時。名前は読みやすさのため（関数は見ない） */
const f = (
  id: number,
  shot_key: number,
  is_raw: boolean,
  name = "",
  taken_at_ms = 1000,
  extra: Partial<Stackable> = {},
) =>
  ({
    id,
    shot_key,
    is_raw,
    name,
    taken_at_ms,
    taken_at_known: true,
    is_video: false,
    taken_subsec: false,
    body_key: 0,
    picked: false,
    ...extra,
  }) as Stackable & { name: string };

const leads = (stacks: ReturnType<typeof stacksOfDay>) =>
  stacks.map((s) => s.cover.lead.id);
const members = (stacks: ReturnType<typeof stacksOfDay>) =>
  stacks.map((s) => filesOf(s).map((x) => x.id));

test("RAW+JPEG の組は1枚に重ね、表紙は JPEG", () => {
  const day = [
    f(1, 10, true, "IMG_0001.CR3"),
    f(2, 10, false, "IMG_0001.JPG"),
    f(3, 20, false, "IMG_0002.JPG"),
  ];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [2, 3]);
  assert.deepEqual(members(stacks), [[1, 2], [3]]);
});

test("重ねの位置は組の最初の1件が居た位置（並びを崩さない）", () => {
  // 最初の1件が RAW（表紙ではない）でも、位置はその RAW のところ
  const day = [f(5, 1, false), f(2, 10, true), f(6, 2, false), f(1, 10, false)];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [5, 1, 6]);
  assert.deepEqual(members(stacks), [[5], [2, 1], [6]]);
});

test("名前が同じでも撮影日時が違えば別の写真（カメラ2台の IMG_0001）", () => {
  const day = [
    f(1, 10, true, "IMG_0001.CR3 (本体A)", 1000),
    f(2, 10, false, "IMG_0001.JPG (本体B)", 5000),
  ];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("撮影日時は秒の単位で比べる（JPEG にだけ秒未満が付いても組は割れない）", () => {
  // CR3 は秒まで、同じシャッターの JPEG は .82 秒——同じ秒なので組
  const same = [f(1, 10, true, "IMG_0001.CR3", 17_000), f(2, 10, false, "IMG_0001.JPG", 17_820)];
  assert.deepEqual(members(stacksOfDay(same, { rawJpeg: true })), [[1, 2]]);
  // 秒が違えば別
  const apart = [f(1, 10, true, "", 17_000), f(2, 10, false, "", 18_100)];
  assert.deepEqual(members(stacksOfDay(apart, { rawJpeg: true })), [[1], [2]]);
});

test("撮影日時が読めていない（mtime で埋めた）ものは重ねない", () => {
  const day = [
    f(1, 10, true, "IMG_0001.CR3", 1000, { taken_at_known: false }),
    f(2, 10, false, "IMG_0001.JPG", 1000, { taken_at_known: false }),
  ];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
  // 片方だけ読めていても重ねない
  const half = [f(1, 10, true), f(2, 10, false, "", 1000, { taken_at_known: false })];
  assert.deepEqual(members(stacksOfDay(half, { rawJpeg: true })), [[1], [2]]);
});

test("RAW と同じ名前の動画は重ねない（IMG_0001.CR3 と IMG_0001.MOV）", () => {
  const day = [f(1, 10, true, "IMG_0001.CR3"), f(2, 10, false, "IMG_0001.MOV", 1000, { is_video: true })];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
  // RAW+JPEG に同名の動画が混ざっても、動画は別のタイル
  const mixed = [f(1, 10, true), f(2, 10, false), f(3, 10, false, "", 1000, { is_video: true })];
  assert.deepEqual(members(stacksOfDay(mixed, { rawJpeg: true })), [[1, 2], [3]]);
});

test("RAW 同士（CR3 と書き出した DNG）は RAW+JPEG ではないので重ねない", () => {
  const day = [f(1, 10, true, "IMG_0001.CR3"), f(2, 10, true, "IMG_0001.DNG")];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("RAW を含まない同名の組（Live Photos の HEIC+MOV）は重ねない", () => {
  const day = [f(1, 10, false, "IMG_0001.HEIC"), f(2, 10, false, "IMG_0001.MOV")];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: true })), [[1], [2]]);
});

test("RAW だけの組は1枚のまま、表紙は先頭（JPEG が無い）", () => {
  const day = [f(1, 10, true), f(2, 20, true)];
  assert.deepEqual(leads(stacksOfDay(day, { rawJpeg: true })), [1, 2]);
});

test("RAW と JPEG の2枚以上が同じ鍵でも1枚に（RAW+JPEG+HEIF 等）", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 10, false)];
  const stacks = stacksOfDay(day, { rawJpeg: true });
  assert.deepEqual(leads(stacks), [2]);
  assert.deepEqual(members(stacks), [[1, 2, 3]]);
});

test("設定で切ったら、1件1枚（重ねない）", () => {
  const day = [f(1, 10, true), f(2, 10, false)];
  assert.deepEqual(members(stacksOfDay(day, { rawJpeg: false })), [[1], [2]]);
});

test("空の日は空", () => {
  assert.deepEqual(stacksOfDay([], { rawJpeg: true }), []);
});

test("範囲選択の閉包: 組のどれか1つが入っていれば、組ぜんぶを入れる", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 20, false), f(4, 30, true), f(5, 30, false)];
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  // 重なっていない id は索引に入れない
  assert.equal(index.has(3), false);
  // 範囲が組 (1,2) の途中の 2 から始まり、組 (4,5) の途中の 4 で終わった
  assert.deepEqual([...closeOverStacks([2, 3, 4], index)].sort(), [1, 2, 3, 4, 5]);
  // 索引に無い id（読み込んでいない日）はそのまま通す
  assert.deepEqual([...closeOverStacks([99], index)], [99]);
});

test("見えているタイルの枚数: 重ねの組は1枚と数える", () => {
  const day = [f(1, 10, true), f(2, 10, false), f(3, 20, false)];
  const loaded = new Set([1, 2, 3]);
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  assert.equal(countPhotos([1, 2, 3], index, loaded), 2);
  assert.equal(countPhotos([1, 2], index, loaded), 1);
  assert.equal(countPhotos([2], index, loaded), 1, "組の片方だけでも1枚");
  assert.equal(countPhotos([3], index, loaded), 1);
  assert.equal(countPhotos([], index, loaded), 0);
  // 重ねを切ったら、ファイルの数そのもの
  const off = stackMembersIndex(stacksOfDay(day, { rawJpeg: false }));
  assert.equal(countPhotos([1, 2, 3], off, loaded), 3);
});

test("読み込んでいない日の id が混ざったら、枚数は数えられない（null）", () => {
  const day = [f(1, 10, true), f(2, 10, false)];
  const index = stackMembersIndex(stacksOfDay(day, { rawJpeg: true }));
  assert.equal(countPhotos([1, 2, 99], index, new Set([1, 2])), null);
});

// ---- 連写（dev #32 の PR 2、ADR の規則 A）----

/** 連写のコマ: 1ファイル・秒未満あり・機体 `body`。`at` は撮影時刻（ミリ秒） */
const frame = (id: number, at: number, body = 7, extra: Partial<Stackable> = {}) =>
  f(id, 1000 + id, false, "", at, { taken_subsec: true, body_key: body, ...extra });
/** `list_day` の順（**新しい順**）に並べる */
const newestFirst = <T extends Stackable>(xs: T[]) =>
  [...xs].sort((a, b) => b.taken_at_ms - a.taken_at_ms || b.id - a.id);
const burstsOn = { rawJpeg: true, bursts: true, burstGapMs: 1000 };
const frames = (stacks: ReturnType<typeof stacksOfDay>) => stacks.map((s) => s.shots.length);

test("同じ機体で1秒以内に続くコマは1枚に。位置は一覧で最初のコマ、表紙は撮り始め", () => {
  const day = newestFirst([frame(1, 10_000), frame(2, 10_300), frame(3, 10_900)]);
  const stacks = stacksOfDay(day, burstsOn);
  assert.deepEqual(frames(stacks), [3]);
  assert.deepEqual(leads(stacks), [1], "表紙は撮り始め（一覧では最後に並ぶコマ）");
  assert.deepEqual(members(stacks), [[3, 2, 1]], "コマは一覧の順");
});

test("間隔は「以下」でつなぐ（ちょうど設定値はつながり、1ミリ秒超えると割れる）", () => {
  const at = (gap: number) =>
    frames(stacksOfDay(newestFirst([frame(1, 0), frame(2, gap)]), burstsOn));
  assert.deepEqual(at(1000), [2]);
  assert.deepEqual(at(1001), [1, 1]);
});

test("間隔の設定で切れ目が動く（1.2 秒の間は 1 秒では割れ、2 秒ではつながる）", () => {
  const day = newestFirst([frame(1, 0), frame(2, 300), frame(3, 1500), frame(4, 1800)]);
  assert.deepEqual(frames(stacksOfDay(day, burstsOn)), [2, 2]);
  assert.deepEqual(frames(stacksOfDay(day, { ...burstsOn, burstGapMs: 2000 })), [4]);
  assert.deepEqual(frames(stacksOfDay(day, { ...burstsOn, burstGapMs: 500 })), [2, 2]);
});

test("秒までしか分からないコマ・機体が分からないコマ・動画は鎖に入れない", () => {
  for (const [what, odd] of [
    ["秒まで", { taken_subsec: false }],
    ["機体が分からない", { body_key: 0 }],
    ["撮影日時が読めていない", { taken_at_known: false }],
    ["動画", { is_video: true }],
  ] as const) {
    const day = newestFirst([frame(1, 0), frame(2, 300, 7, odd), frame(3, 600)]);
    const stacks = stacksOfDay(day, burstsOn);
    // 2 は鎖に入らない。1 と 3 は 600ms 離れているのでつながる（2 を飛ばしても同じ機体の直前）
    assert.deepEqual(members(stacks), [[3, 1], [2]].sort((a, b) => b[0] - a[0]), what);
  }
});

test("機体が分からないコマ同士は、間隔が短くてもつながない（0 を1台の機体と読まない）", () => {
  const day = newestFirst([frame(1, 0, 0), frame(2, 300, 0), frame(3, 600, 0)]);
  assert.deepEqual(frames(stacksOfDay(day, burstsOn)), [1, 1, 1]);
});

test("2台で同じ時間帯に撮っても、機体ごとの連写は割れず、混ざらない", () => {
  // 一覧では A と B が交互に並ぶ
  const day = newestFirst([
    frame(1, 0, 7),
    frame(2, 100, 8),
    frame(3, 400, 7),
    frame(4, 500, 8),
    frame(5, 800, 7),
  ]);
  const stacks = stacksOfDay(day, burstsOn);
  assert.deepEqual(members(stacks), [
    [5, 3, 1],
    [4, 2],
  ]);
});

test("表紙は ⚑ のコマ（いくつもあれば撮り始めに近いもの）", () => {
  const day = newestFirst([
    frame(1, 0),
    frame(2, 300, 7, { picked: true }),
    frame(3, 600, 7, { picked: true }),
  ]);
  assert.deepEqual(leads(stacksOfDay(day, burstsOn)), [2]);
});

test("RAW+JPEG で連写: コマは組の数で数え、時刻は秒未満を持つ JPEG から取る", () => {
  // CR3 は読み直していないので秒まで（#158）。JPEG が秒未満を持つ
  const pair = (n: number, at: number) => [
    f(10 * n, n, true, `IMG_${n}.CR3`, Math.floor(at / 1000) * 1000, { body_key: 7 }),
    f(10 * n + 1, n, false, `IMG_${n}.JPG`, at, { taken_subsec: true, body_key: 7 }),
  ];
  const day = newestFirst([...pair(1, 5_100), ...pair(2, 5_400), ...pair(3, 5_800)]);
  const stacks = stacksOfDay(day, burstsOn);
  assert.deepEqual(frames(stacks), [3]);
  assert.equal(filesOf(stacks[0]).length, 6);
  assert.deepEqual(
    stacks[0].shots.map((sh) => sh.files.map((x) => x.id).sort()),
    [
      [30, 31],
      [20, 21],
      [10, 11],
    ],
  );
  assert.deepEqual(leads(stacks), [11], "表紙は撮り始めのコマの JPEG");
});

test("連写の設定を切ると、コマのまま（RAW+JPEG の重ねは残る）", () => {
  const day = newestFirst([frame(1, 0), frame(2, 300)]);
  assert.deepEqual(frames(stacksOfDay(day, { ...burstsOn, bursts: false })), [1, 1]);
  assert.deepEqual(frames(stacksOfDay(day, { rawJpeg: true })), [1, 1], "省いたら重ねない");
});

test("RAW+JPEG を切って連写だけ: RAW と JPEG は別のコマ。秒未満がそろえば間隔0でつながる", () => {
  // ADR の未決「それで良いかを升で固定する」。両方が秒未満を持てば1束・コマは2倍。
  // RAW が秒までなら、RAW は束の外に1枚ずつ残る（下の2つ目）
  const both = newestFirst([
    f(1, 1, true, "A.CR3", 100, { taken_subsec: true, body_key: 7 }),
    f(2, 1, false, "A.JPG", 100, { taken_subsec: true, body_key: 7 }),
    f(3, 2, true, "B.CR3", 400, { taken_subsec: true, body_key: 7 }),
    f(4, 2, false, "B.JPG", 400, { taken_subsec: true, body_key: 7 }),
  ]);
  const off = { ...burstsOn, rawJpeg: false };
  assert.deepEqual(frames(stacksOfDay(both, off)), [4]);
  const rawSecondsOnly = newestFirst([
    f(1, 1, true, "A.CR3", 0, { body_key: 7 }),
    f(2, 1, false, "A.JPG", 100, { taken_subsec: true, body_key: 7 }),
    f(3, 2, true, "B.CR3", 0, { body_key: 7 }),
    f(4, 2, false, "B.JPG", 400, { taken_subsec: true, body_key: 7 }),
  ]);
  assert.deepEqual(frames(stacksOfDay(rawSecondsOnly, off)).sort(), [1, 1, 2]);
});

/**
 * 見本 11 組（dev #32 の表、2026-09-25 に win が集めた実物）を**表の入力として**写す。
 * 表にあるのはコマ数・機体のシリアルの有無・秒未満の有無・間隔の最小と最大だけなので、
 * 間隔は最小と最大を交互に並べて作る。見るのは3つの設定それぞれで:
 * 最大 ≤ 設定なら1枚、最小 > 設定ならコマの数だけ、その間なら途中で割れる
 */
test("見本 11 組の実測（コマ数・秒未満・間隔の最小と最大）", () => {
  const samples: [string, number, boolean, number, number][] = [
    ["canon-m3-20160327", 13, true, 400, 600],
    ["canon-r8-20250719", 3, true, 350, 350],
    ["canon-r8-jpg-via-iphone-20260505-fast", 13, true, 20, 30],
    ["canon-r8-jpg-via-iphone-20260505", 25, true, 160, 480],
    ["canon-r8-rawjpg-burst-20260920", 4, true, 180, 800],
    ["canon-r8-rawjpg-burst-20260921", 8, true, 180, 850],
    ["iphone-se2-rapid-20220816", 21, true, 230, 270],
    ["iphone-se2-rapid-front-20220910", 23, true, 220, 330],
    ["iphone16-proraw-burstuuid-20260920", 9, true, 150, 11_500],
    ["iphone8-20190817", 3, true, 440, 460],
    ["xperia-so02g-20150920", 12, false, 0, 1000],
  ];
  for (const [name, n, subsec, min, max] of samples) {
    const xs = [];
    let at = 0;
    for (let k = 0; k < n; k++) {
      xs.push(frame(k + 1, at, 7, { taken_subsec: subsec }));
      at += k % 2 === 0 ? min : max;
    }
    const day = newestFirst(xs);
    for (const gap of [500, 1000, 2000]) {
      const tiles = stacksOfDay(day, { ...burstsOn, burstGapMs: gap }).length;
      const why = `${name} @ ${gap}ms`;
      if (!subsec || (n > 1 && min > gap)) assert.equal(tiles, n, why);
      else if (max <= gap || n < 3) assert.equal(tiles, min <= gap ? 1 : n, why);
      else assert.ok(tiles > 1 && tiles < n, why);
    }
  }
});

test("連写の閉包と数え方: 連写のどれか1つを選べば連写ぜんぶ、枚数は1枚", () => {
  const day = newestFirst([frame(1, 0), frame(2, 300), frame(3, 5_000)]);
  const stacks = stacksOfDay(day, burstsOn);
  const index = stackMembersIndex(stacks);
  assert.deepEqual([...closeOverStacks([1], index)].sort(), [1, 2]);
  assert.equal(countPhotos([1, 2, 3], index, new Set([1, 2, 3])), 2);
});
