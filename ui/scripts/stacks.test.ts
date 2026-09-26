/**
 * 一覧の重ね（`src/stacks.ts`、dev #32）。`npm --prefix ui test`。
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  closeOverStacks,
  countPhotos,
  filesOf,
  selectRangeOverTiles,
  selectedDaysIn,
  stackMembersIndex,
  pairAwareScope,
  stacksOfDay,
  viewerWalk,
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
  assert.equal(stacks[0].spanMs, 900);
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

test("RAW+JPEG を切って連写だけ: 連写でない組はファイルごとに分け、連写の中は組で数える（#160 のゲート2）", () => {
  const off = { ...burstsOn, rawJpeg: false };
  // 連写でない1組（両方が秒未満を持つ）: 間隔0で「連写 2」にしない。RAW と JPEG の2枚に分ける
  const lone = newestFirst([
    f(1, 1, true, "A.CR3", 100, { taken_subsec: true, body_key: 7 }),
    f(2, 1, false, "A.JPG", 100, { taken_subsec: true, body_key: 7 }),
  ]);
  assert.deepEqual(frames(stacksOfDay(lone, off)), [1, 1]);
  // 連写の中は組のまま: 3コマ・6ファイル。秒までしか無い CR3 も、組の JPEG と一緒に入る
  const pairs = newestFirst([
    f(10, 1, true, "A.CR3", 0, { body_key: 7 }),
    f(11, 1, false, "A.JPG", 100, { taken_subsec: true, body_key: 7 }),
    f(20, 2, true, "B.CR3", 0, { body_key: 7 }),
    f(21, 2, false, "B.JPG", 400, { taken_subsec: true, body_key: 7 }),
    f(30, 3, true, "C.CR3", 0, { body_key: 7 }),
    f(31, 3, false, "C.JPG", 700, { taken_subsec: true, body_key: 7 }),
  ]);
  const stacks = stacksOfDay(pairs, off);
  assert.deepEqual(frames(stacks), [3]);
  assert.equal(filesOf(stacks[0]).length, 6);
  assert.equal(stacks[0].spanMs, 600, "長さは鎖に使った秒未満の時刻で");
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

/** 読み込み済みの日（新しい順に並べた1日ずつ）から、範囲選択の材料を作る（App と同じ組み方） */
const tilesOf = (days: ReturnType<typeof stacksOfDay>[]) => {
  const pos = new Map<number, number>();
  let tile = 0;
  for (const stacks of days)
    for (const st of stacks) {
      for (const x of filesOf(st)) pos.set(x.id, tile);
      tile++;
    }
  return { pos, index: stackMembersIndex(days.flat()) };
};

test("範囲選択はタイルの並びで切る: 飛び飛びの連写を、範囲の外から引き込まない（#160 の codex）", () => {
  // 一覧（新しい順）: A2(機体7) B(機体8) A1(機体7) C(機体8、離れている) → タイル A, B, C
  const a2 = frame(4, 600, 7);
  const b = frame(3, 500, 8);
  const a1 = frame(2, 300, 7);
  const c = frame(1, -5_000, 8);
  const day = newestFirst([a1, a2, b, c]);
  const stacks = stacksOfDay(day, burstsOn);
  assert.deepEqual(members(stacks), [[4, 2], [3], [1]]);
  const { pos, index } = tilesOf([stacks]);
  // B〜C を選ぶ。DB の範囲（ファイルの並び）は B, A1, C——A1 は A のコマだが、A は範囲の外
  assert.deepEqual([...selectRangeOverTiles([3, 2, 1], 3, 1, pos, index)].sort(), [1, 3]);
  // 古い閉じ方なら A ぜんぶが入っていた（この升が見ている差）
  assert.deepEqual([...closeOverStacks([3, 2, 1], index)].sort(), [1, 2, 3, 4]);
  // A〜B（A のタイルは一覧で最初のコマ A2 の位置）: A ぜんぶと B
  assert.deepEqual([...selectRangeOverTiles([4, 3], 4, 3, pos, index)].sort(), [2, 3, 4]);
  // 逆向きに押しても同じ
  assert.deepEqual([...selectRangeOverTiles([3, 2, 1], 1, 3, pos, index)].sort(), [1, 3]);
});

test("範囲選択: 間に挟まる読み込んでいない日の id はそのまま入れ、端が読めていなければファイルの並びで閉じる", () => {
  const day1 = stacksOfDay(newestFirst([frame(10, 900), frame(11, 1200)]), burstsOn); // 連写 {11,10}
  const day3 = stacksOfDay([f(30, 30, false)], burstsOn);
  const { pos, index } = tilesOf([day1, day3]);
  // 10 から 30 まで。20・21 は読み込んでいない日（間に挟まる）
  assert.deepEqual(
    [...selectRangeOverTiles([10, 20, 21, 30], 10, 30, pos, index)].sort((x, y) => x - y),
    [10, 11, 20, 21, 30],
  );
  // 端（99）が読めていない: ファイルの並びの閉包に落とす
  assert.deepEqual(
    [...selectRangeOverTiles([10, 99], 10, 99, pos, index)].sort((x, y) => x - y),
    [10, 11, 99],
  );
});

test("連写の間隔の選択肢と既定は、Rust と UI で同じ（写しが食い違うと、UI が出す値を Rust が断る）", () => {
  const rust = readFileSync(new URL("../../crates/pictkura-core/src/config.rs", import.meta.url), "utf8");
  const api = readFileSync(new URL("../src/api.ts", import.meta.url), "utf8");
  const list = (src: string, re: RegExp) => {
    const m = re.exec(src);
    assert.ok(m, `見つからない: ${re}`);
    return m[1].split(",").map((x) => Number(x.trim()));
  };
  assert.deepEqual(
    list(rust, /pub const BURST_GAPS_MS: \[u32; 3\] = \[([^\]]*)\]/),
    list(api, /export const BURST_GAPS_MS = \[([^\]]*)\]/),
  );
  // 行末は台で変わる（Windows の CI は CRLF で取り出す）ので、改行を名指ししない
  const rustDefault = /burst_gap_ms: (\d+),\s*\}/.exec(rust);
  // CRLF で取り出した木でも同じものを読む（Windows の CI で落ちた形）
  assert.equal(/burst_gap_ms: (\d+),\s*\}/.exec(rust.replace(/\n/g, "\r\n"))?.[1], rustDefault?.[1]);
  const apiDefault = /export const DEFAULT_BURST_GAP_MS = (\d+);/.exec(api);
  assert.ok(rustDefault && apiDefault);
  assert.equal(rustDefault[1], apiDefault[1]);
});

test("RAW+JPEG を切ったとき、ばらした組のファイルは元の位置に戻す（間に挟まる写真の順を崩さない。#160 の codex）", () => {
  // 同じ秒に: A.JPG, B.JPG, A.CR3, C.JPG（新しい順）。B と C は別の名前の1枚
  const day = [
    f(1, 1, false, "A.JPG", 1000),
    f(2, 2, false, "B.JPG", 1000),
    f(3, 1, true, "A.CR3", 1000),
    f(4, 3, false, "C.JPG", 1000),
  ];
  const stacks = stacksOfDay(day, { ...burstsOn, rawJpeg: false });
  assert.deepEqual(leads(stacks), [1, 2, 3, 4]);
  // 組を重ねるなら、組は A.JPG の位置に1枚
  assert.deepEqual(members(stacksOfDay(day, burstsOn)), [[1, 3], [2], [4]]);
});

test("範囲選択: 起点が連写の途中のコマでも、見えている間のタイルを落とさない（#160 の codex、3周目）", () => {
  // 一覧: A2 B A1 C。起点 A1 は連写を入れる前に選んだ——いまは A のタイルの中
  const day = newestFirst([frame(4, 600, 7), frame(3, 500, 8), frame(2, 300, 7), frame(1, -5_000, 8)]);
  assert.deepEqual(day.map((x) => x.id), [4, 3, 2, 1], "ファイルの並びは A2 B A1 C");
  const stacks = stacksOfDay(day, burstsOn);
  assert.deepEqual(members(stacks), [[4, 2], [3], [1]]);
  const { pos, index } = tilesOf([stacks]);
  // A1(2) から C(1) まで。DB の範囲は A1, C だけ（B はファイルの並びでは A1 より前）
  assert.deepEqual([...selectRangeOverTiles([2, 1], 2, 1, pos, index)].sort(), [1, 2, 3, 4]);
});

// ---- ビューアの歩く列（2026-09-26: 組は JPEG だけ／RAW だけ／RAW→JPEG） ----

test("ビューア: 組は選んだ側だけを歩き、組でないファイルはそのまま", () => {
  // 並び（list_day の順）: JPG 1、別の写真 2、CR3 3（1 と組）、動画でない1枚 4
  const items = [f(1, 7, false, "A.JPG"), f(2, 8, false, "B.JPG"), f(3, 7, true, "A.CR3"), f(4, 9, true, "C.CR3")];
  const ids = (view: "jpeg" | "raw" | "both") => viewerWalk(items, view).walk.map((x) => x.id);
  assert.deepEqual(ids("jpeg"), [1, 2, 4]);
  // 組の位置は組の最初の1件（1）が居た位置。RAW だけのファイル（4）は組ではないので残る
  assert.deepEqual(ids("raw"), [3, 2, 4]);
  // 両方: RAW が先（list_day では JPG が先だった）
  assert.deepEqual(ids("both"), [3, 1, 2, 4]);
});

test("ビューア: 組の相手は、列に居ない側の id からも引ける（RAW が先）", () => {
  const items = [f(1, 7, false, "A.JPG"), f(3, 7, true, "A.CR3"), f(2, 8, false, "B.JPG")];
  const { walk, pairOf } = viewerWalk(items, "raw");
  assert.deepEqual(walk.map((x) => x.id), [3, 2]);
  // 一覧のタイルは表紙の JPEG（1）で開く——そこから RAW（3）へ寄せられる
  assert.deepEqual(pairOf.get(1)?.map((x) => x.id), [3, 1]);
  assert.deepEqual(pairOf.get(3)?.map((x) => x.id), [3, 1]);
  assert.equal(pairOf.get(2), undefined);
});

test("ビューア: 同じ側が2つある組（CR3 と書き出した DNG）は、選んだ側を全部歩く", () => {
  const items = [f(1, 7, false, "A.JPG"), f(2, 7, true, "A.CR3"), f(3, 7, true, "A.DNG")];
  assert.deepEqual(viewerWalk(items, "raw").walk.map((x) => x.id), [2, 3]);
  assert.deepEqual(viewerWalk(items, "jpeg").walk.map((x) => x.id), [1]);
  assert.deepEqual(viewerWalk(items, "both").walk.map((x) => x.id), [2, 3, 1]);
});

test("ビューア: 組にならないもの（撮影日時が違う同名・日時が読めない・動画）は両方歩く", () => {
  const items = [
    f(1, 7, false, "A.JPG", 1000),
    f(2, 7, true, "A.CR3", 99_000), // 日時が違う: 別の写真
    f(3, 8, false, "B.JPG", 1000, { taken_at_known: false }),
    f(4, 8, true, "B.CR3", 1000, { taken_at_known: false }),
    f(5, 9, true, "C.CR3", 1000),
    f(6, 9, false, "C.MOV", 1000, { is_video: true }),
  ];
  for (const view of ["jpeg", "raw", "both"] as const) {
    const { walk, pairOf } = viewerWalk(items, view);
    assert.deepEqual(walk.map((x) => x.id), [1, 2, 3, 4, 5, 6], view);
    assert.equal(pairOf.size, 0, view);
  }
});

test("選んだ列: 組は最初の席に、見せる側だけを RAW が先で置く（#168 の codex）", () => {
  // 1 と 3 が組（JPG・CR3）。選択の列はファイルの並び: JPG 1、別の 2、CR3 3
  const items = [f(1, 7, false, "A.JPG"), f(2, 8, false, "B.JPG"), f(3, 7, true, "A.CR3")];
  const scope = [1, 2, 3].map((id) => ({ id, day_key: 20260926 }));
  const ids = (view: "jpeg" | "raw" | "both") =>
    pairAwareScope(scope, viewerWalk(items, view).pairOf, view).map((e) => e.id);
  assert.deepEqual(ids("jpeg"), [1, 2]);
  assert.deepEqual(ids("raw"), [3, 2]);
  assert.deepEqual(ids("both"), [3, 1, 2]);
  // 日は元の席のものを引き継ぐ
  assert.deepEqual(
    pairAwareScope(scope, viewerWalk(items, "raw").pairOf, "raw").map((e) => e.day_key),
    [20260926, 20260926],
  );
});

test("選んだ列: 組が分かっていない id（未読の日・組でない）はそのまま、同じ id は2度出さない", () => {
  const scope = [{ id: 9, day_key: 1 }, { id: 9, day_key: 1 }, { id: 4, day_key: 2 }];
  assert.deepEqual(pairAwareScope(scope, new Map(), "jpeg").map((e) => e.id), [9, 4]);
});

test("間引かない日: 選んだ写真が居る日だけ、上限を超えたら守らない", () => {
  const days = new Map([
    [1, [{ id: 10 }, { id: 11 }]],
    [2, [{ id: 20 }]],
    [3, [{ id: 30 }]],
  ]);
  assert.deepEqual([...selectedDaysIn(days, new Set([11]), 5)], [1]);
  assert.deepEqual([...selectedDaysIn(days, new Set([11, 30]), 5)], [1, 3]);
  assert.deepEqual([...selectedDaysIn(days, new Set(), 5)], []);
  // 読み込んでいない日の id だけなら守る日は無い
  assert.deepEqual([...selectedDaysIn(days, new Set([99]), 5)], []);
  // 上限（1日）を超えた: 全選択のまま端までスクロールした形——守らない
  assert.deepEqual([...selectedDaysIn(days, new Set([10, 20]), 1)], []);
  assert.deepEqual([...selectedDaysIn(days, new Set([10]), 1)], [1]);
});
